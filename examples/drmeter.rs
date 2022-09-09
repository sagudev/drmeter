use drmeter::DRMeter;
use ffmpeg::format::sample::Type;
use ffmpeg::format::Sample;
use ffmpeg::util::frame::audio::Audio as FAudio;
use ffmpeg_next as ffmpeg;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Quiet);
    let mut ictx = ffmpeg::format::input(&args[1]).unwrap();
    let input = ictx
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or(ffmpeg::Error::StreamNotFound)
        .unwrap();
    let idx = input.index();
    let context_decoder =
        ffmpeg::codec::context::Context::from_parameters(input.parameters()).unwrap();
    let mut decoder = context_decoder.decoder().audio().unwrap();
    decoder.set_parameters(input.parameters()).unwrap();
    let mut sample_type = decoder.format();

    let req_resample = match sample_type {
        // empty
        Sample::None => panic!("No samples"),
        // our DR meter cannot handle them so we need to resample
        Sample::U8(_) | Sample::I64(_) => {
            sample_type = Sample::I16(Type::Packed);
            println!("Resampling will be used!");
            true
        }
        // it's fine
        Sample::I16(_) => false,
        Sample::I32(_) => false,
        Sample::F32(_) => false,
        Sample::F64(_) => false,
    };

    let mut dr = DRMeter::new(
        decoder.channel_layout().channels() as u32,
        decoder.rate() as u32,
    )
    .unwrap();

    println!(
        "Channels: {}, Sample rate: {}Hz",
        decoder.channels(),
        decoder.rate()
    );

    for (packet_stream, packet) in ictx.packets() {
        if packet_stream.index() == idx {
            if let Err(e) = decoder.send_packet(&packet) {
                println!("Error while sending a packet to the decoder {e}");
                break;
            }
            let mut decoded = FAudio::empty();
            while decoder.receive_frame(&mut decoded).is_ok() {
                if req_resample {
                    let mut resampler = decoder
                        .resampler(
                            Sample::I16(Type::Packed),
                            decoder.channel_layout(),
                            decoder.rate(),
                        )
                        .unwrap();
                    let mut resampled = FAudio::empty();
                    resampler.run(&decoded, &mut resampled).unwrap();
                    decoded = resampled;
                }

                let planes = decoded.planes();
                debug_assert_eq!(decoded.format(), sample_type);

                match sample_type {
                    Sample::I16(t) => match t {
                        Type::Packed => dr.add_frames_i16(plane(&decoded, 0)),
                        Type::Planar => {
                            let l: Vec<_> = (0..planes).map(|x| plane(&decoded, x)).collect();
                            dr.add_frames_planar_i16(&l)
                        }
                    },
                    Sample::I32(t) => match t {
                        Type::Packed => dr.add_frames_i32(plane(&decoded, 0)),
                        Type::Planar => {
                            let l: Vec<_> = (0..planes).map(|x| plane(&decoded, x)).collect();
                            dr.add_frames_planar_i32(&l)
                        }
                    },
                    Sample::F32(t) => match t {
                        Type::Packed => dr.add_frames_f32(plane(&decoded, 0)),
                        Type::Planar => {
                            let l: Vec<_> = (0..planes).map(|x| plane(&decoded, x)).collect();
                            dr.add_frames_planar_f32(&l)
                        }
                    },
                    Sample::F64(t) => match t {
                        Type::Packed => dr.add_frames_f64(plane(&decoded, 0)),
                        Type::Planar => {
                            let l: Vec<_> = (0..planes).map(|x| plane(&decoded, x)).collect();
                            dr.add_frames_planar_f64(&l)
                        }
                    },

                    Sample::None | Sample::U8(_) | Sample::I64(_) => panic!("should not be"),
                }
                .unwrap();
            }
        }
    }

    dr.finalize().unwrap();

    for ch in 0..dr.channels() {
        println!("---------- CHANNEL {ch} ----------");
        println!(
            "Score: DR{} ({})",
            dr.channel_dr_score(ch).unwrap(),
            dr.exact_channel_dr(ch).unwrap()
        );
    }

    println!("----------- GLOBAL -----------");
    println!(
        "Score: DR{} ({})",
        dr.dr_score().unwrap(),
        dr.exact_dr().unwrap()
    );
}

#[inline]
/// The equation to convert to dBTP is: 20 * log10(n)
pub fn lufs_to_dbtp(n: f64) -> f64 {
    20.0 * (n).log10()
}

/// Fix from https://github.com/zmwangx/rust-ffmpeg/pull/104
#[inline]
fn plane<T: ffmpeg::frame::audio::Sample>(ss: &FAudio, index: usize) -> &[T] {
    if index >= ss.planes() {
        panic!("out of bounds");
    }
    if !<T as ffmpeg::frame::audio::Sample>::is_valid(ss.format(), ss.channels() as u16) {
        panic!("unsupported type");
    }

    if ss.is_planar() {
        unsafe { std::slice::from_raw_parts((*ss.as_ptr()).data[index] as *const T, ss.samples()) }
    } else {
        unsafe {
            std::slice::from_raw_parts(
                (*ss.as_ptr()).data[0] as *const T,
                ss.samples() * usize::from(ss.channels()),
            )
        }
    }
}
