use crate::utils::{Sample, Samples};

#[derive(Debug)]
pub struct Block {
    /// Number of channels
    channels: u32,

    /// Number of consumed frames
    consumed_frames: usize,

    /// Previously measured sample peak.
    sample_peak: Box<[f64]>,

    /// This is energy per channel
    sum2: Box<[f64]>,
}

impl Block {
    /// Creates a new [`Block`].
    pub fn new(channels: u32) -> Self {
        assert!(channels > 0);

        Self {
            channels,
            consumed_frames: 0,
            sample_peak: vec![0.0; channels as usize].into_boxed_slice(),
            sum2: vec![0.0; channels as usize].into_boxed_slice(),
        }
    }

    /// Number of consumed frames to compere with needed frames.
    pub const fn consumed_frames(&self) -> usize {
        self.consumed_frames
    }

    pub fn reset(&mut self) {
        self.sample_peak.fill(0.0);
        self.sum2.fill(0.0);
        self.consumed_frames = 0;
    }

    /// Return finalized block results
    ///
    /// NOTE: This does not finalize block, so you can still feed it,
    /// but you must use `reset` method to really finalize the block.
    pub fn finish(&mut self) -> (Box<[f64]>, Box<[f64]>) {
        (
            self.sample_peak.clone(),
            self.sum2
                .iter()
                .map(|sum| f64::sqrt(2.0 * *sum / self.consumed_frames as f64))
                .collect(),
        )
    }

    /// Process frames in current block.
    ///
    /// NOTE: This function does not know what is target size of block,
    /// so you must make sure you give =< than target size.
    pub fn process<'a, T: Sample + 'a, S: Samples<'a, T>>(&mut self, src: S) {
        assert!(src.channels() == self.channels as usize);

        assert!(self.sample_peak.len() == self.channels as usize);

        for (channel, sample_peak) in self.sample_peak.iter_mut().enumerate() {
            let mut max = 0.0;

            debug_assert!(channel < src.channels());

            src.foreach_sample(channel, |sample| {
                let v = sample.as_f64_raw().abs();
                if v > max {
                    max = v;
                }
            });

            max /= T::MAX_AMPLITUDE;
            if max > *sample_peak {
                *sample_peak = max;
            }
        }

        for (channel, sum2) in self.sum2.iter_mut().enumerate() {
            debug_assert!(channel < src.channels());

            src.foreach_sample(channel, |sample| {
                //*sum2 += sample.as_f64_raw() * sample.as_f64_raw();
                *sum2 += sample.to_sample::<f64>() * sample.to_sample::<f64>();
            });
        }

        self.consumed_frames += src.frames();
    }
}
