use crate::{cache_path, Result, MAX_BLOCK_FRAMES, OUTPUT_RATE};
use motion_model::{AudioAsset, Project};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct WaveBucket {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}
struct Source {
    id: u64,
    file: File,
    weights: Arc<Vec<[f32; 64]>>,
}
/// Each instance owns a frozen project and independent file cursors. Preview
/// seeking and edits cannot change an export mixer's samples or position.
pub struct AudioMixer {
    project: Project,
    root: PathBuf,
    sources: Vec<Source>,
    bytes: Vec<u8>,
    samples: Vec<f32>,
    voices: Vec<motion_model::composition::AudioVoice>,
}
impl AudioMixer {
    pub fn new(project: Project, root: &Path) -> Result<Self> {
        project.validate().map_err(|e| e.to_string())?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut sources = Vec::new();
        let voices=project.audio_voices().map_err(|e|e.to_string())?;
        let mut tables = HashMap::new();
        for a in &project.audio_assets {
            if !voices.iter().any(|voice|voice.asset==a.id) {
                continue;
            }
            let path = cache_path(&root, a)?.canonicalize().map_err(|_| {
                format!("audio cache missing for asset {}; call prepare_audio", a.id)
            })?;
            if !path.starts_with(&root) {
                return Err("audio cache outside project".into());
            }
            let file = File::open(path).map_err(|e| e.to_string())?;
            if file.metadata().map_err(|e| e.to_string())?.len()
                != a.sample_frames * u64::from(a.channels) * 4
            {
                return Err(format!(
                    "audio cache invalid for asset {}; call prepare_audio",
                    a.id
                ));
            }
            sources.push(Source {
                id: a.id,
                file,
                weights: tables
                    .entry(a.sample_rate)
                    .or_insert_with(|| Arc::new(sinc_table(a.sample_rate)))
                    .clone(),
            });
        }
        Ok(Self {
            project,
            root,
            sources,
            bytes: Vec::with_capacity((MAX_BLOCK_FRAMES + 64) * 8),
            samples: Vec::with_capacity((MAX_BLOCK_FRAMES + 64) * 2),
            voices,
        })
    }
    pub fn total_frames(&self) -> u64 {
        u64::from(self.project.frames) * OUTPUT_RATE / u64::from(self.project.fps)
    }
    pub fn mix(&mut self, start_sample: u64, out: &mut [f32]) -> Result<usize> {
        if out.len() % 2 != 0
            || out.len() / 2 > MAX_BLOCK_FRAMES
            || start_sample > self.total_frames()
        {
            return Err("invalid PCM block range (maximum one second, stereo)".into());
        }
        out.fill(0.0);
        let count = (out.len() / 2).min((self.total_frames() - start_sample) as usize);
        for audio in &self.voices {
            let begin = start_sample.max(audio.begin_sample);
            let end = (start_sample + count as u64).min(audio.end_sample);
            if begin >= end {
                continue;
            }
            let asset = self
                .project
                .audio_assets
                .iter()
                .find(|a| a.id == audio.asset)
                .ok_or("audio asset missing")?;
            let rate = i128::from(asset.sample_rate);
            let den = i128::from(OUTPUT_RATE) * 1_000_000;
            let position = |sample: u64| {
                (i128::from(sample) - i128::from(audio.offset_sample))
                    * rate
                    * 1_000_000
                    + i128::from(audio.source_offset_us) * rate * i128::from(OUTPUT_RATE)
            };
            let first = (position(begin).div_euclid(den) - 32)
                .max(0)
                .min(i128::from(asset.sample_frames)) as u64;
            let last = (position(end - 1).div_euclid(den) + 34)
                .max(0)
                .min(i128::from(asset.sample_frames)) as u64;
            if last <= first {
                continue;
            }
            let ch = asset.channels as usize;
            let len = (last - first) as usize * ch;
            self.bytes.resize(len * 4, 0);
            let source = self
                .sources
                .iter_mut()
                .find(|s| s.id == asset.id)
                .ok_or("audio source cache is not prepared")?;
            source
                .file
                .seek(SeekFrom::Start(first * ch as u64 * 4))
                .map_err(|e| e.to_string())?;
            source
                .file
                .read_exact(&mut self.bytes)
                .map_err(|e| e.to_string())?;
            self.samples.clear();
            for b in self.bytes.chunks_exact(4) {
                let v = f32::from_le_bytes(b.try_into().unwrap());
                if !v.is_finite() {
                    return Err("audio cache contains a non-finite sample".into());
                }
                self.samples.push(v);
            }
            let animated_levels=audio.animated_levels();
            let constant_gain=audio.gain_at(begin);
            for sample in begin..end {
                let pos = position(sample);
                if pos < 0 || pos >= i128::from(asset.sample_frames) * den {
                    continue;
                }
                let n = pos.div_euclid(den) as i64;
                let fraction = pos.rem_euclid(den);
                let target = (sample - start_sample) as usize * 2;
                let gain=if animated_levels{audio.gain_at(sample)}else{constant_gain};
                for channel in 0..2 {
                    let channel = channel.min(ch - 1);
                    let value = if fraction == 0 && asset.sample_rate <= OUTPUT_RATE as u32 {
                        self.samples[(n as u64 - first) as usize * ch + channel]
                    } else {
                        let weights = &source.weights[(fraction * 1024 / den) as usize];
                        let mut value = 0.0;
                        for (tap, &weight) in weights.iter().enumerate() {
                            let index = n + tap as i64 - 31;
                            if index >= first as i64 && index < last as i64 {
                                value += self.samples
                                    [(index as u64 - first) as usize * ch + channel]
                                    * weight;
                            }
                        }
                        value
                    };
                    // Mono maps to both output channels; stereo preserves channel order.
                    let output_channel = if ch == 1 { 0 } else { channel };
                    if ch == 1 {
                        out[target] += value * gain[0];
                        out[target + 1] += value * gain[1];
                        break;
                    }
                    out[target + output_channel] += value * gain[output_channel];
                }
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        Ok(count)
    }
    /// Actual decoded peaks at a declared 10 ms source-time resolution.
    pub fn waveform(
        &self,
        asset_id: u64,
        first_bucket: u64,
        count: usize,
    ) -> Result<Vec<WaveBucket>> {
        if count > 4096 {
            return Err("waveform query exceeds 4096 buckets".into());
        }
        let asset = self
            .project
            .audio_assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or("audio asset missing")?;
        read_waveform(&self.root, asset, first_bucket, count)
    }
}
pub fn read_waveform(
    root: &Path,
    asset: &AudioAsset,
    first_bucket: u64,
    count: usize,
) -> Result<Vec<WaveBucket>> {
    validate_waveform_asset(asset)?;
    if count > 4096 {
        return Err("waveform query exceeds 4096 buckets".into());
    }
    let buckets = (asset.sample_frames * 100).div_ceil(u64::from(asset.sample_rate));
    if first_bucket > buckets {
        return Err("waveform range outside audio source".into());
    }
    let count = count.min((buckets - first_bucket) as usize);
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = cache_path(&root, asset)?
        .with_extension("wave")
        .canonicalize()
        .map_err(|_| "waveform missing; call prepare_audio")?;
    if !path.starts_with(&root) {
        return Err("waveform cache outside project".into());
    }
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() != buckets * 12 {
        return Err("waveform cache invalid; call prepare_audio".into());
    }
    f.seek(SeekFrom::Start(first_bucket * 12))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; count * 12];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    bytes
        .chunks_exact(12)
        .map(|b| {
            let values = [0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap()));
            if values.iter().any(|v| !v.is_finite()) {
                return Err("invalid waveform values".into());
            }
            Ok(WaveBucket {
                min: values[0],
                max: values[1],
                rms: values[2],
            })
        })
        .collect()
}
fn sinc_table(rate: u32) -> Vec<[f32; 64]> {
    let cutoff = (OUTPUT_RATE as f64 / f64::from(rate)).min(1.0);
    let radius = if rate > OUTPUT_RATE as u32 {
        32.0
    } else {
        16.0
    };
    (0..1024)
        .map(|phase| {
            let fraction = phase as f64 / 1024.0;
            let mut w = [0.0; 64];
            let mut sum = 0.0;
            for (tap, value) in w.iter_mut().enumerate() {
                let x = tap as f64 - 31.0 - fraction;
                if x.abs() >= radius {
                    continue;
                }
                let sinc = if x.abs() < 1e-12 {
                    cutoff
                } else {
                    (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                };
                *value = (sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / radius).cos())) as f32;
                sum += *value;
            }
            for value in &mut w {
                *value /= sum;
            }
            w
        })
        .collect()
}

pub(crate) fn build_waveform(
    pcm: &Path,
    asset: &AudioAsset,
    mut check: impl FnMut() -> Result<()>,
) -> Result<()> {
    validate_waveform_asset(asset)?;
    let mut source = File::open(pcm).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(
        File::create(pcm.with_extension("wave")).map_err(|e| e.to_string())?,
    );
    use std::io::Write;
    let bucket_samples = (asset.sample_rate.div_ceil(100) * asset.channels) as usize;
    let mut bytes = vec![0; bucket_samples * 4];
    let mut remaining = asset.sample_frames * u64::from(asset.channels);
    let mut bucket = 0u64;
    while remaining > 0 {
        check()?;
        let from = bucket * u64::from(asset.sample_rate) / 100;
        let to = (bucket + 1) * u64::from(asset.sample_rate) / 100;
        let count = remaining.min((to - from) * u64::from(asset.channels)) as usize;
        if count == 0 {
            return Err(format!(
                "audio asset {} waveform builder made no progress",
                asset.id
            ));
        }
        bucket += 1;
        source
            .read_exact(&mut bytes[..count * 4])
            .map_err(|e| e.to_string())?;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut squared = 0.0f64;
        for b in bytes[..count * 4].chunks_exact(4) {
            let v = f32::from_le_bytes(b.try_into().unwrap());
            min = min.min(v);
            max = max.max(v);
            squared += f64::from(v) * f64::from(v);
        }
        for v in [min, max, (squared / count as f64).sqrt() as f32] {
            out.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        remaining -= count as u64;
    }
    out.flush().map_err(|e| e.to_string())?;
    out.get_ref().sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_waveform_asset(asset: &AudioAsset) -> Result<()> {
    asset
        .validate()
        .map_err(|error| format!("audio asset {}: {error}", asset.id))
}

#[cfg(test)]
mod waveform_tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    fn asset(sample_rate: u32, sample_frames: u64) -> AudioAsset {
        AudioAsset {
            id: 42,
            path: "assets/waveform.wav".into(),
            mime: "audio/wav".into(),
            bytes: 16,
            track: 0,
            sample_rate,
            channels: 1,
            sample_frames,
            duration_us: if sample_rate == 0 {
                0
            } else {
                sample_frames * 1_000_000 / u64::from(sample_rate)
            },
        }
    }

    #[test]
    fn invalid_sample_rates_return_with_asset_context_without_opening_pcm() {
        for sample_rate in [0, 1, 50, 99, 100, 7_999, 192_001, u32::MAX] {
            let (send, receive) = mpsc::channel();
            std::thread::spawn(move || {
                let result = build_waveform(
                    Path::new("this-file-must-not-be-opened.pcm"),
                    &asset(sample_rate, 1),
                    || Ok(()),
                );
                send.send(result).unwrap();
            });
            let error = receive
                .recv_timeout(Duration::from_secs(1))
                .expect("invalid waveform metadata must terminate")
                .unwrap_err();
            assert!(error.contains("audio asset 42"), "{sample_rate}: {error}");
            assert!(error.contains("8–192 kHz"), "{sample_rate}: {error}");
        }
    }

    #[test]
    fn valid_waveform_keeps_exact_min_max_rms_bytes_and_read_contract() {
        let root = tempfile::tempdir().unwrap();
        let pcm = root.path().join("decoded.pcm");
        let samples = [-1.0f32, -0.5, 0.5, 1.0];
        std::fs::write(
            &pcm,
            samples
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let valid = asset(8_000, 4);
        let mut checks = 0;
        build_waveform(&pcm, &valid, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(checks, 1);
        let bytes = std::fs::read(pcm.with_extension("wave")).unwrap();
        let expected = [-1.0f32, 1.0, (0.625f32).sqrt()]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected);

        let cache = crate::cache_path(root.path(), &valid).unwrap();
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::rename(pcm.with_extension("wave"), cache.with_extension("wave")).unwrap();
        let buckets = read_waveform(root.path(), &valid, 0, 1).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].min, -1.);
        assert_eq!(buckets[0].max, 1.);
        assert_eq!(buckets[0].rms, (0.625f32).sqrt());
    }

    #[test]
    fn public_waveform_reader_rejects_invalid_metadata_before_division_or_io() {
        let root = tempfile::tempdir().unwrap();
        let error = read_waveform(root.path(), &asset(0, 1), 0, 1).unwrap_err();
        assert!(error.contains("audio asset 42"), "{error}");
    }
}
