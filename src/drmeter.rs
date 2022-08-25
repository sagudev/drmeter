use std::fmt;

use crate::block::Block;
use crate::utils::{decibel, sqr, Interleaved, Planar, Sample, Samples};
use crate::Error;

/// upper 20% histogram values
const LOUD_FRACTION: f64 = 0.2;
/// How many bins there are (2¹⁵)
const BINS: usize = 32768;
//const BINS: usize = 10_000;
const MAX_RATE: u32 = 2_822_400;
const MAX_CHANNELS: u32 = 64;

// There are apparently two possibilities for implementation
// one is like in ffmpeg where we do not know full number of blocks
// when starting as we are streaming data
// and other
// is like
// [DeaDBeeF DR Meter](https://github.com/dakeryas/deadbeef-dr-meter)
// which does know final number of blocks, but we do not
/// DR Meter instance
pub struct DRMeter {
    /* user passed options */
    /// The sample rate.
    rate: u32,

    /// The number of channels
    channels: u32,

    /// window length in ms
    ///
    /// Default 3000ms
    window: usize,

    /* Audio buffer */
    /// How many frames* are needed for a block.
    /// Per DR standard it corresponds to 3000ms = 3s
    ///
    /// *frames are generic over the number of channels
    /// (egg. stereo frame has two samples)
    needed_frames: usize,

    /// Block Worker
    block: Block,

    /* Results */
    /// number of blocks that are scanned
    block_number: usize,

    /// Peak bins per channel
    peaks: Box<[Box<[u32]>]>,

    /// RMS bins per channel
    rms: Box<[Box<[u32]>]>,

    /// cached exact dr scores per channel
    /// that are generated when the instance is finalized
    ///
    /// Other values are pretty easy to calculate from these,
    /// so these ones are the only we cache
    channel_dr: Option<Box<[f64]>>,
}

impl fmt::Debug for DRMeter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DRMeter")
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .field("window", &self.window)
            .field("needed_frames", &self.needed_frames)
            .field("block", &self.block)
            .field("block_number", &self.block_number)
            //.field("peaks", &self.peaks)
            //.field("rms", &self.rms)
            .finish()
    }
}

impl DRMeter {
    /// Allocate audio data buffer used by the filter and check if we can allocate enough memory
    /// for it.
    fn allocate_bin(channels: usize) -> Result<Box<[Box<[u32]>]>, Error> {
        let _total_mem = (BINS + 1).checked_mul(channels).ok_or(Error::NoMem)?;

        Ok(vec![vec![0; BINS + 1].into_boxed_slice(); channels as usize].into_boxed_slice())
    }

    /// Create a new instance with default window of 3s.
    pub fn new(channels: u32, rate: u32) -> Result<Self, Error> {
        Self::new_with_window(channels, rate, 3000)
    }

    /// Create a new instance with the given configuration.
    pub fn new_with_window(channels: u32, rate: u32, window: usize) -> Result<Self, Error> {
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(Error::NoMem);
        }

        if !(16..=MAX_RATE).contains(&rate) {
            return Err(Error::NoMem);
        }

        assert!(window >= 10);

        // TODO: some pushover +5
        // FFMPEG: samples = time_constant * sample_rate + .5
        let needed_frames = (rate as usize).checked_mul(window).ok_or(Error::NoMem)? / 1000;

        let data = Self::allocate_bin(channels as usize)?;

        Ok(Self {
            rate,
            channels,
            needed_frames,
            peaks: data.clone(),
            rms: data,
            block_number: 0,
            window,
            block: Block::new(channels),
            channel_dr: None,
        })
    }

    /************
     *
     *  getters
     *
     ************/

    /// Returns the configured number of channels.
    pub const fn channels(&self) -> u32 {
        self.channels
    }

    /// Returns the configured sample rate.
    pub const fn rate(&self) -> u32 {
        self.rate
    }

    /// Returns the configured window.
    pub const fn window(&self) -> usize {
        self.window
    }

    /// Returns `true` if this instance is finalized.
    pub const fn finalized(&self) -> bool {
        // instance is finalized if we have cached values
        self.channel_dr.is_some()
    }

    /// Finalize current block
    fn finalize_block(&mut self) {
        debug_assert_ne!(self.block.consumed_frames(), 0);
        let (peak, rms) = self.block.finish();
        for ch in 0..(self.channels as usize) {
            //println!("[CH {ch}] {}", rms[ch]);
            let rms_bin = ((rms[ch] * BINS as f64).round() as usize).clamp(0, BINS);
            let peak_bin = ((peak[ch] * BINS as f64) as usize).clamp(0, BINS);
            self.rms[ch][rms_bin] += 1;
            self.peaks[ch][peak_bin] += 1;
        }
        self.block_number += 1;
        // finalize block
        self.block.reset();
    }

    /// Finalize instance (marking end of stream)
    ///
    /// For streaming, DR values are computed using only fully finished blocks,
    /// but if you reached the end of stream you can use this function
    /// to forces finalization of half block if such block even exist.
    ///
    /// As part of finalization exact channel DR scores are also calculated and cached
    ///
    /// After finalization you cannot add frames to the instance.
    pub fn finalize(&mut self) -> Result<(), Error> {
        if self.finalized() {
            return Err(Error::Finalized);
        }

        // finalize half block if exist
        if self.block.consumed_frames() != 0 {
            self.finalize_block()
        };

        // calculate and cache exact channel values
        self.channel_dr = Some(
            (0..self.channels)
                .map(|ch| self.exact_channel_dr(ch))
                .collect::<Result<Box<[f64]>, Error>>()?,
        );

        Ok(())
    }

    /***********************
     *
     *  Fill with data
     *
     ***********************/
    /// Process frames. This is the generic variant of the different public add_frames() functions
    /// that are defined below.
    fn add_frames<'a, T: Sample + 'a, S: Samples<'a, T>>(
        &mut self,
        mut src: S,
    ) -> Result<(), Error> {
        if self.finalized() {
            return Err(Error::Finalized);
        }

        if src.frames() == 0 {
            return Ok(());
        }

        if self.channels == 0 {
            return Err(Error::NoMem);
        }

        while src.frames() > 0 {
            let num_frames = src.frames();

            let frames_still_needed = self.needed_frames - self.block.consumed_frames();
            if num_frames >= frames_still_needed {
                let (current, next) = src.split_at(frames_still_needed);

                self.block.process(current);
                // one block is now finished
                self.finalize_block();

                src = next;
            } else {
                let (current, next) = src.split_at(num_frames);
                // currently read frames for block processor
                self.block.process(current);
                // we get unfinished block

                // next is empty?
                src = next;
            }
        }

        Ok(())
    }

    /// Add interleaved frames to be processed.
    pub fn add_frames_i16(&mut self, frames: &[i16]) -> Result<(), Error> {
        self.add_frames(Interleaved::new(frames, self.channels as usize)?)
    }

    /// Add interleaved frames to be processed.
    pub fn add_frames_i32(&mut self, frames: &[i32]) -> Result<(), Error> {
        self.add_frames(Interleaved::new(frames, self.channels as usize)?)
    }

    /// Add interleaved frames to be processed.
    pub fn add_frames_f32(&mut self, frames: &[f32]) -> Result<(), Error> {
        self.add_frames(Interleaved::new(frames, self.channels as usize)?)
    }

    /// Add interleaved frames to be processed.
    pub fn add_frames_f64(&mut self, frames: &[f64]) -> Result<(), Error> {
        self.add_frames(Interleaved::new(frames, self.channels as usize)?)
    }

    /// Add planar frames to be processed.
    pub fn add_frames_planar_i16(&mut self, frames: &[&[i16]]) -> Result<(), Error> {
        self.add_frames(Planar::new(frames)?)
    }

    /// Add planar frames to be processed.
    pub fn add_frames_planar_i32(&mut self, frames: &[&[i32]]) -> Result<(), Error> {
        self.add_frames(Planar::new(frames)?)
    }

    /// Add planar frames to be processed.
    pub fn add_frames_planar_f32(&mut self, frames: &[&[f32]]) -> Result<(), Error> {
        self.add_frames(Planar::new(frames)?)
    }

    /// Add planar frames to be processed.
    pub fn add_frames_planar_f64(&mut self, frames: &[&[f64]]) -> Result<(), Error> {
        self.add_frames(Planar::new(frames)?)
    }

    /************
     *
     *  Results
     *
     ************/

    /// Find bin index of first peak
    fn find_first_peak(&self, channel_number: u32) -> Result<usize, Error> {
        if channel_number >= self.channels {
            return Err(Error::InvalidChannelIndex);
        }

        Ok(BINS
            - self.peaks[channel_number as usize]
                .iter()
                .rev()
                .position(|&x| x != 0)
                .unwrap())
    }

    /// Get maximum sample peak from all frames that have been processed for channel.
    ///
    /// The equation to convert to dBFS is: 20 * log10(out)
    pub fn first_peak(&self, channel_number: u32) -> Result<f64, Error> {
        Ok(self.find_first_peak(channel_number)? as f64 / BINS as f64)
    }

    /// Find bin index of first peak
    fn find_second_peak(&self, channel_number: u32) -> Result<usize, Error> {
        let first_index = self.find_first_peak(channel_number)?;

        Ok(BINS
            - self.peaks[channel_number as usize][first_index..]
                .iter()
                .rev()
                .position(|&x| x != 0)
                .unwrap())
    }

    /// Get second sample peak from all frames that have been processed for channel.
    ///
    /// The equation to convert to dBFS is: 20 * log10(out)
    pub fn second_peak(&self, channel_number: u32) -> Result<f64, Error> {
        Ok(self.find_second_peak(channel_number)? as f64 / BINS as f64)
    }

    fn channel_rms_sum(&self, channel_number: u32) -> Result<f64, Error> {
        if channel_number >= self.channels {
            return Err(Error::InvalidChannelIndex);
        }

        let mut j: u32 = 0;
        let n = (LOUD_FRACTION * self.block_number as f64).round() as u32;
        let mut rms_sum = 0.0;
        for (i, rms) in self.rms[channel_number as usize].iter().enumerate().rev() {
            if *rms > 0 {
                rms_sum += sqr(i as f64 / BINS as f64);
                j += rms;
            }
            if j >= n {
                break;
            }
        }

        Ok(rms_sum)
    }

    /// Return exact channel DR
    ///
    /// NOTE: DR values are computed using only fully finished blocks,
    /// in case you reached the end of stream you should finalize instance
    /// before getting the results.
    pub fn exact_channel_dr(&self, channel_number: u32) -> Result<f64, Error> {
        if let Some(channel_dr) = &self.channel_dr {
            if channel_number >= self.channels {
                return Err(Error::InvalidChannelIndex);
            }
            Ok(channel_dr[channel_number as usize])
        } else {
            // channel checking inside
            Ok(decibel(
                self.second_peak(channel_number)?
                    / f64::sqrt(
                        self.channel_rms_sum(channel_number)?
                            / (LOUD_FRACTION * self.block_number as f64),
                    ),
            ))
        }
    }

    /// Return channel DR score
    ///
    /// NOTE: DR values are computed using only fully finished blocks,
    /// in case you reached the end of stream you should finalize instance
    /// before getting the results.
    pub fn channel_dr_score(&self, channel_number: u32) -> Result<u8, Error> {
        Ok(self.exact_channel_dr(channel_number)? as u8)
    }

    /// Return exact DR
    ///
    /// NOTE: DR values are computed using only fully finished blocks,
    /// in case you reached the end of stream you should finalize instance
    /// before getting the results.
    pub fn exact_dr(&self) -> Result<f64, Error> {
        let mut dr = 0.0;
        for ch in 0..self.channels {
            dr += self.exact_channel_dr(ch)?;
        }
        Ok(dr / self.channels as f64)
    }

    /// Return DR score
    ///
    /// NOTE: DR values are computed using only fully finished blocks,
    /// in case you reached the end of stream you should finalize instance
    /// before getting the results.
    pub fn dr_score(&self) -> Result<u8, Error> {
        Ok(self.exact_dr()? as u8)
    }

    /*pub fn rms(&self, channels: u32) -> Result<f64, Error> {
        self.channel_rms_sum(channel_number)
    }*/

    /// Get average exact DR score across multiple instances.
    /// This can be used to calculate Albums DR score
    pub fn exact_dr_multiple<'a>(iter: impl Iterator<Item = &'a Self>) -> Result<f64, Error> {
        let h = iter
            .map(|d| d.exact_dr())
            .collect::<Result<Vec<f64>, _>>()?;

        Ok(h.iter().sum::<f64>() / h.len() as f64)
    }

    /// Get average DR score across multiple instances.
    /// This can be used to calculate Albums DR score
    pub fn dr_score_multiple<'a>(iter: impl Iterator<Item = &'a Self>) -> Result<u8, Error> {
        Ok(Self::exact_dr_multiple(iter)? as u8)
    }
}
