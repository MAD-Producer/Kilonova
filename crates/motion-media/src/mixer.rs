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
        project.validate().map_err(crate::Error::from)?;
        let root = root.canonicalize().map_err(crate::Error::from)?;
        let mut sources = Vec::new();
        let voices=project.audio_voices().map_err(crate::Error::from)?;
        let mut tables = HashMap::new();
        for a in &project.audio_assets {
            if !voices.iter().any(|voice|voice.asset==a.id) {
                continue;
            }
            let cache = cache_path(&root, a)?;
            let path = cache.canonicalize().map_err(|error| {
                crate::Error::Context {
                    message: format!("audio cache unavailable: asset={} path={} kind={:?} reason={error}; call prepare_audio", a.id, cache.display(), error.kind()),
                    cause: Arc::new(error.into()),
                }
            })?;
            if !path.starts_with(&root) {
                return Err(format!("audio cache outside project: asset={} path={}", a.id, path.display()).into());
            }
            let file = File::open(&path).map_err(|error| {
                crate::Error::Context {
                    message: format!("audio cache open failed: asset={} path={} reason={error}", a.id, path.display()),
                    cause: Arc::new(error.into()),
                }
            })?;
            let actual_bytes = file.metadata().map_err(|error| {
                crate::Error::Context {
                    message: format!("audio cache metadata failed: asset={} path={} reason={error}", a.id, path.display()),
                    cause: Arc::new(error.into()),
                }
            })?.len();
            let expected_bytes = a.sample_frames * u64::from(a.channels) * 4;
            if actual_bytes != expected_bytes {
                return Err(format!(
                    "audio cache invalid for asset {}: path={} expected_bytes={expected_bytes} actual_bytes={actual_bytes}; call prepare_audio",
                    a.id, path.display()
                ).into());
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
                .map_err(|error| crate::Error::Context {
                    message: format!("audio cache seek failed: asset={} first_sample={first} reason={error}", asset.id),
                    cause: Arc::new(error.into()),
                })?;
            source
                .file
                .read_exact(&mut self.bytes)
                .map_err(|error| crate::Error::Context {
                    message: format!("audio cache read failed: asset={} first_sample={first} sample_frames={} kind={:?} reason={error}", asset.id, last - first, error.kind()),
                    cause: Arc::new(error.into()),
                })?;
            self.samples.clear();
            for (index, b) in self.bytes.chunks_exact(4).enumerate() {
                let v = f32::from_le_bytes(b.try_into().unwrap());
                if !v.is_finite() {
                    return Err(format!("audio cache contains a non-finite sample: asset={} sample={} channel={}", asset.id, first + (index / ch) as u64, index % ch).into());
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
        for (index, value) in out.iter_mut().enumerate() {
            // Finite source samples can still overflow while mixing; opposite
            // infinities then produce NaN, which clamp would pass to the device.
            // Keep the existing saturation of +/-infinity and report NaN before
            // a caller can publish this block to an audio backend.
            if value.is_nan() {
                let sample = start_sample + (index / 2) as u64;
                let frame = sample * u64::from(self.project.fps) / OUTPUT_RATE;
                return Err(format!(
                    "audio mix produced NaN at sample {sample}, frame {frame}, channel {}; finite PCM overflow or cancellation during mixing",
                    index % 2
                ).into());
            }
            *value = value.clamp(-1.0, 1.0);
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
    let root = root.canonicalize().map_err(crate::Error::from)?;
    let path = cache_path(&root, asset)?
        .with_extension("wave")
        .canonicalize()
        .map_err(|_| "waveform missing; call prepare_audio")?;
    if !path.starts_with(&root) {
        return Err("waveform cache outside project".into());
    }
    let mut f = File::open(path).map_err(crate::Error::from)?;
    if f.metadata().map_err(crate::Error::from)?.len() != buckets * 12 {
        return Err("waveform cache invalid; call prepare_audio".into());
    }
    f.seek(SeekFrom::Start(first_bucket * 12))
        .map_err(crate::Error::from)?;
    let mut bytes = vec![0; count * 12];
    f.read_exact(&mut bytes).map_err(crate::Error::from)?;
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
    let mut source = File::open(pcm).map_err(crate::Error::from)?;
    let mut out = std::io::BufWriter::new(
        File::create(pcm.with_extension("wave")).map_err(crate::Error::from)?,
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
            ).into());
        }
        bucket += 1;
        source
            .read_exact(&mut bytes[..count * 4])
            .map_err(crate::Error::from)?;
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
            out.write_all(&v.to_le_bytes()).map_err(crate::Error::from)?;
        }
        remaining -= count as u64;
    }
    out.flush().map_err(crate::Error::from)?;
    out.get_ref().sync_all().map_err(crate::Error::from)?;
    Ok(())
}

fn validate_waveform_asset(asset: &AudioAsset) -> Result<()> {
    asset
        .validate()
        .map_err(|error| crate::Error::from(error).context(format!("audio asset {}", asset.id)))
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
            assert!(error.to_string().contains("audio asset 42"), "{sample_rate}: {error}");
            assert!(error.to_string().contains("8–192 kHz"), "{sample_rate}: {error}");
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
        assert!(error.to_string().contains("audio asset 42"), "{error}");
    }
}

#[cfg(test)]
mod source_interval_tests {
    use super::*;
    use motion_model::{AudioClip, Content, Layer, LayerTimeline};

    #[test]
    fn integer_frame_audio_tail_is_zero_padded_without_losing_source_samples() {
        let root = tempfile::tempdir().unwrap();
        let mut project = Project::new(64, 64, 30, 3).unwrap();
        let asset = AudioAsset {
            id: 1,
            path: "assets/short.wav".into(),
            mime: "audio/wav".into(),
            bytes: 64,
            track: 0,
            sample_rate: 48_000,
            channels: 1,
            sample_frames: 16,
            duration_us: 333,
        };
        let cache = cache_path(root.path(), &asset).unwrap();
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(cache, 0.25f32.to_le_bytes().repeat(16)).unwrap();
        project.audio_assets.push(asset);
        let mut layer = Layer::solid(1, "short audio", [0.; 2], [0.; 3], [1.; 4]);
        layer.content = Content::Audio {
            audio: AudioClip::new(1),
        };
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 1,
            offset_frame: 0,
        });
        project.layers.push(layer);
        project.validate().unwrap();
        let mut mixer = AudioMixer::new(project.clone(), root.path()).unwrap();

        // A source shorter than one timeline frame remains a valid clip. Its
        // actual PCM is preserved; only the unavailable tail is silent.
        let mut full_frame = vec![f32::NAN; 1_600 * 2];
        assert_eq!(mixer.mix(0, &mut full_frame).unwrap(), 1_600);
        assert_eq!(&full_frame[..16 * 2], vec![0.25; 16 * 2]);
        assert!(full_frame[16 * 2..].iter().all(|value| *value == 0.));

        // Seeking into that tail takes the empty source-window path and must
        // agree exactly with the same region from the original block.
        let mut tail = vec![f32::NAN; 128 * 2];
        assert_eq!(mixer.mix(64, &mut tail).unwrap(), 128);
        assert_eq!(&tail[..], &full_frame[64 * 2..192 * 2]);

        // This tolerance does not accept a further frame whose first sample
        // already lies outside the source; model validation rejects it.
        project.layers[0].timeline.as_mut().unwrap().out_frame = 2;
        let error = match AudioMixer::new(project, root.path()) {
            Ok(_) => panic!("an entirely unavailable audio frame must be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("audio clip exceeds recoverable source interval"),
            "{error}"
        );
    }
    fn project_with_pcm(root: &Path, values: &[f32]) -> Project {
        let mut project = Project::new(64, 64, 30, 3).unwrap();
        for (index, value) in values.iter().enumerate() {
            let id = index as u64 + 1;
            let asset = AudioAsset {
                id,
                path: format!("assets/source-{id}.wav"),
                mime: "audio/wav".into(),
                bytes: 64,
                track: 0,
                sample_rate: 48_000,
                channels: 1,
                sample_frames: 16,
                duration_us: 333,
            };
            let cache = cache_path(root, &asset).unwrap();
            std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
            std::fs::write(cache, value.to_le_bytes().repeat(16)).unwrap();
            project.audio_assets.push(asset);
            let mut layer = Layer::solid(id, "audio", [0.; 2], [0.; 3], [1.; 4]);
            let mut audio = AudioClip::new(id);
            audio.volume = 2.;
            layer.content = Content::Audio { audio };
            layer.timeline = Some(LayerTimeline {
                in_frame: 0,
                out_frame: 1,
                offset_frame: 0,
            });
            project.layers.push(layer);
        }
        project.validate().unwrap();
        project
    }

    #[test]
    fn cache_failures_keep_the_asset_and_original_filesystem_cause() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_pcm(root.path(), &[0.25]);
        let cache = cache_path(root.path(), &project.audio_assets[0]).unwrap();
        let mut mixer = AudioMixer::new(project.clone(), root.path()).unwrap();
        std::fs::write(&cache, 0.25f32.to_le_bytes().repeat(4)).unwrap();
        let error = mixer.mix(8, &mut [0.; 8]).unwrap_err();
        assert!(matches!(error.primary(), crate::Error::Io(cause) if cause.kind() == std::io::ErrorKind::UnexpectedEof));
        assert!(error.to_string().contains("audio cache read failed: asset=1"), "{error}");
        assert!(error.to_string().contains("kind=UnexpectedEof reason="), "{error}");
        drop(mixer);
        std::fs::remove_file(&cache).unwrap();
        let error = match AudioMixer::new(project, root.path()) {
            Ok(_) => panic!("a missing cache must fail preparation"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("audio cache unavailable: asset=1 path="), "{error}");
        assert!(matches!(error.primary(), crate::Error::Io(cause) if cause.kind() == std::io::ErrorKind::NotFound));
        assert!(error.to_string().contains("kind=NotFound reason="), "{error}");
        assert!(error.to_string().contains(cache.file_name().unwrap().to_str().unwrap()), "{error}");
    }

    #[test]
    fn corrupt_pcm_reports_the_source_sample_and_channel() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_pcm(root.path(), &[0.25]);
        let cache = cache_path(root.path(), &project.audio_assets[0]).unwrap();
        let mut samples = [0.25f32; 16];
        samples[6] = f32::NAN;
        let bytes: Vec<_> = samples.into_iter().flat_map(f32::to_le_bytes).collect();
        std::fs::write(cache, bytes).unwrap();
        let mut mixer = AudioMixer::new(project, root.path()).unwrap();
        let error = mixer.mix(8, &mut [0.; 8]).unwrap_err();
        assert!(error.to_string().contains("non-finite sample: asset=1 sample=6 channel=0"), "{error}");
    }

    #[test]
    fn opposite_finite_pcm_overflow_is_rejected_before_publishing_a_nan_block() {
        let root = tempfile::tempdir().unwrap();
        let project = project_with_pcm(root.path(), &[f32::MAX, -f32::MAX]);
        let mut mixer = AudioMixer::new(project, root.path()).unwrap();
        let mut output = [0.; 8];
        let error = mixer.mix(8, &mut output).unwrap_err();
        assert!(error.to_string().contains("NaN"), "{error}");
        assert!(error.to_string().contains("sample 8, frame 0, channel 0"), "{error}");
        assert!(error.to_string().contains("overflow or cancellation"), "{error}");
    }

    #[test]
    fn finite_clipping_and_infinity_saturation_keep_the_existing_pcm_contract() {
        for (values, expected) in [
            (vec![1., 1.], 1.),
            (vec![-1., -1.], -1.),
            (vec![f32::MAX], 1.),
            (vec![-f32::MAX], -1.),
            (vec![0.125, -0.0625], 0.125),
        ] {
            let root = tempfile::tempdir().unwrap();
            let project = project_with_pcm(root.path(), &values);
            let mut mixer = AudioMixer::new(project, root.path()).unwrap();
            let mut output = [0.; 8];
            assert_eq!(mixer.mix(0, &mut output).unwrap(), 4);
            assert_eq!(output, [expected; 8], "{values:?}");
        }
    }
}
