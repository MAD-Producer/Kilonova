//! Bounded output chunks and the union of their actual 64-tap source windows.
//! A nonmonotonic clock must never turn its distant extrema into one huge read.
use super::*;
use motion_model::composition::AudioVoice;

const OUTPUT_CHUNK: usize = 256;
const PHASES: i64 = 1024;
// Adjacent direct samples (e.g. 2x speed) share a read. Even with every gap,
// each chunk reads at most OUTPUT_CHUNK * (64 + 32) native frames.
const COALESCE_GAP: u64 = 32;

#[derive(Clone, Copy)]
pub(super) struct Point {
    output: usize,
    native: u64,
    phase: usize,
    gain: [f32; 2],
}
#[derive(Clone, Copy)]
pub(super) struct Window {
    first: u64,
    last: u64,
    offset: usize,
}
#[derive(Default)]
pub(super) struct Scratch {
    pub points: Vec<Point>,
    pub windows: Vec<Window>,
}
impl Scratch {
    pub fn mix_voice(
        &mut self,
        voice: &AudioVoice,
        asset: &AudioAsset,
        source: &mut Source,
        bytes: &mut Vec<u8>,
        samples: &mut Vec<f32>,
        begin: u64,
        end: u64,
        start_sample: u64,
        out: &mut [f32],
    ) -> Result<()> {
        let channels = asset.channels as usize;
        for chunk in (begin..end).step_by(OUTPUT_CHUNK) {
            self.points.clear();
            self.windows.clear();
            let finish = end.min(chunk + OUTPUT_CHUNK as u64);
            for sample in chunk..finish {
                let Some(mapped) = voice
                    .mapped_sample_at(sample, asset.sample_rate)
                    .map_err(crate::Error::from)
                    .map_err(|error| {
                        error.context(format!(
                            "audio mapped source: asset={} output_sample={sample} phase=map",
                            asset.id
                        ))
                    })?
                else {
                    continue;
                };
                // Quantize once at the leaf, to the nearest existing polyphase
                // bank position. This also carries phase 1024 into the next
                // integer sample rather than selecting the previous sample.
                // The unmapped rational/floor resampler remains unchanged.
                let fixed = (mapped.position * PHASES as f64).round() as i64;
                if fixed < 0 || fixed as u64 >= asset.sample_frames * PHASES as u64 {
                    continue;
                }
                let native = fixed.div_euclid(PHASES) as u64;
                let phase = fixed.rem_euclid(PHASES) as usize;
                let direct = phase == 0 && asset.sample_rate <= OUTPUT_RATE as u32;
                self.points.push(Point {
                    output: (sample - start_sample) as usize * 2,
                    native,
                    phase,
                    gain: mapped.gain,
                });
                self.windows.push(Window {
                    first: if direct {
                        native
                    } else {
                        native.saturating_sub(31)
                    },
                    last: if direct {
                        native + 1
                    } else {
                        (native + 33).min(asset.sample_frames)
                    },
                    offset: 0,
                });
            }
            self.windows.sort_unstable_by_key(|window| window.first);
            let mut merged = 0;
            for index in 0..self.windows.len() {
                let window = self.windows[index];
                if merged > 0 && window.first <= self.windows[merged - 1].last + COALESCE_GAP {
                    self.windows[merged - 1].last = self.windows[merged - 1].last.max(window.last);
                } else {
                    self.windows[merged] = window;
                    merged += 1;
                }
            }
            self.windows.truncate(merged);
            samples.clear();
            for window in &mut self.windows {
                window.offset = samples.len();
                let len = (window.last - window.first) as usize * channels;
                bytes.resize(len * 4, 0);
                source.file.seek(SeekFrom::Start(window.first * channels as u64 * 4))
                    .map_err(|error| crate::Error::from(error).context(format!(
                        "audio mapped cache seek: asset={} first_sample={} output_begin={chunk} output_end={finish}",
                        asset.id, window.first)))?;
                source.file.read_exact(bytes)
                    .map_err(|error| crate::Error::from(error).context(format!(
                        "audio mapped cache read: asset={} first_sample={} sample_frames={} output_begin={chunk} output_end={finish}",
                        asset.id, window.first, window.last - window.first)))?;
                for (index, bytes) in bytes.chunks_exact(4).enumerate() {
                    let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    if !value.is_finite() {
                        return Err(format!(
                            "audio mapped cache non-finite sample: asset={} sample={} channel={} output_begin={chunk} output_end={finish}",
                            asset.id, window.first + (index / channels) as u64, index % channels).into());
                    }
                    samples.push(value);
                }
            }
            for point in &self.points {
                let index = self
                    .windows
                    .partition_point(|window| window.last <= point.native);
                let window = self.windows.get(index).ok_or_else(|| {
                    crate::Error::from(format!(
                        "audio mapped window missing: asset={} source_sample={} phase=interpolate",
                        asset.id, point.native
                    ))
                })?;
                for channel in 0..2 {
                    let source_channel = channel.min(channels - 1);
                    let value = if point.phase == 0 && asset.sample_rate <= OUTPUT_RATE as u32 {
                        samples[window.offset
                            + (point.native - window.first) as usize * channels
                            + source_channel]
                    } else {
                        let weights = &source.weights[point.phase];
                        let mut value = 0.;
                        for (tap, weight) in weights.iter().enumerate() {
                            let native = point.native as i64 + tap as i64 - 31;
                            if native >= window.first as i64 && native < window.last as i64 {
                                value += samples[window.offset
                                    + (native as u64 - window.first) as usize * channels
                                    + source_channel]
                                    * weight;
                            }
                        }
                        value
                    };
                    out[point.output + channel] += value * point.gain[channel];
                }
            }
        }
        Ok(())
    }
}
