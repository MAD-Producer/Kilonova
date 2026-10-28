//! Bounded display reduction of the prepared 100 Hz source waveform. Call on a
//! worker: even a small output can require reading a long source interval.
use crate::{read_waveform, Result, WaveBucket};
use motion_model::AudioAsset;
use std::path::Path;

pub const MAX_WAVEFORM_COLUMNS: usize = 2048;

pub fn waveform_view(
    root: &Path,
    asset: &AudioAsset,
    begin_seconds: f64,
    end_seconds: f64,
    columns: usize,
    check: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<WaveBucket>> {
    asset.validate().map_err(crate::Error::from)?;
    if columns == 0
        || columns > MAX_WAVEFORM_COLUMNS
        || !begin_seconds.is_finite()
        || !end_seconds.is_finite()
        || end_seconds <= begin_seconds
    {
        return Err("invalid waveform display interval or column count".into());
    }
    let duration = asset.sample_frames as f64 / f64::from(asset.sample_rate);
    let step = (end_seconds - begin_seconds) / columns as f64;
    if !step.is_finite() || step <= 0. {
        return Err("invalid waveform display step".into());
    }
    let mut result = Vec::with_capacity(columns);
    for column in 0..columns {
        check()?;
        let start = begin_seconds + step * column as f64;
        let end = if column + 1 == columns {
            end_seconds
        } else {
            start + step
        };
        let from = start.max(0.);
        let to = end.min(duration);
        let mut value = WaveBucket {
            min: 0.,
            max: 0.,
            rms: 0.,
        };
        if to > from {
            let mut bucket = (from * 100.).floor() as u64;
            let last = (to * 100.).ceil() as u64;
            let mut energy = 0.;
            let mut initialized = false;
            while bucket < last {
                check()?;
                let count = (last - bucket).min(4096) as usize;
                let batch = read_waveform(root, asset, bucket, count)?;
                if batch.is_empty() {
                    return Err("waveform display read made no progress".into());
                }
                for (index, b) in batch.iter().enumerate() {
                    if b.min > b.max || b.rms < 0. {
                        return Err("invalid waveform display values".into());
                    }
                    let at = (bucket + index as u64) as f64 / 100.;
                    let weight = (to.min(at + 0.01) - from.max(at)).max(0.);
                    if weight <= 0. {
                        continue;
                    }
                    if !initialized && start >= 0. && end <= duration {
                        value.min = b.min;
                        value.max = b.max;
                    } else {
                        value.min = value.min.min(b.min);
                        value.max = value.max.max(b.max);
                    }
                    initialized = true;
                    energy += f64::from(b.rms).powi(2) * weight;
                }
                bucket += batch.len() as u64;
            }
            value.rms = (energy / (end - start)).sqrt() as f32;
        }
        result.push(value);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> (tempfile::TempDir, AudioAsset) {
        let root = tempfile::tempdir().unwrap();
        let path = root
            .path()
            .join(format!("cache/audio-v{}/tone", crate::PCM_VERSION));
        std::fs::create_dir_all(&path).unwrap();
        let mut bytes = vec![];
        for i in 0..100 {
            let value = if i < 50 { 0.25f32 } else { 0.75f32 };
            for v in [-value, value, value] {
                bytes.extend(v.to_le_bytes());
            }
        }
        std::fs::write(path.join("decoded.wave"), bytes).unwrap();
        let asset = AudioAsset {
            id: 7,
            path: "assets/tone.wav".into(),
            bytes: 100,
            mime: "audio/wav".into(),
            track: 0,
            sample_rate: 192_000,
            channels: 1,
            sample_frames: 192_000,
            duration_us: 1_000_000,
        };
        (root, asset)
    }
    #[test]
    fn waveform_display_keeps_source_trim_offset_and_silence_padding_with_bounded_columns() {
        let (root, asset) = source();
        let values = waveform_view(root.path(), &asset, -0.5, 1.5, 4, &mut || Ok(())).unwrap();
        assert_eq!(values.len(), 4);
        for (value, expected) in values.iter().zip([0., 0.25, 0.75, 0.]) {
            assert!((value.min + expected).abs() < 1e-6);
            assert!((value.max - expected).abs() < 1e-6);
            assert!((value.rms - expected).abs() < 1e-6);
        }
        let trim = waveform_view(root.path(), &asset, 0.25, 0.75, 1, &mut || Ok(())).unwrap();
        assert_eq!(trim[0].max, 0.75);
        assert!((trim[0].rms - ((0.25f32.powi(2) + 0.75f32.powi(2)) / 2.).sqrt()).abs() < 1e-6);
        assert!(waveform_view(
            root.path(),
            &asset,
            0.,
            1.,
            MAX_WAVEFORM_COLUMNS + 1,
            &mut || Ok(())
        )
        .is_err());
    }
    #[test]
    fn waveform_display_cancel_and_invalid_cache_preserve_original_failure() {
        let (root, asset) = source();
        let mut checks = 0;
        let error = waveform_view(root.path(), &asset, 0., 1., 20, &mut || {
            checks += 1;
            if checks == 4 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "cancelled");
        std::fs::write(
            root.path().join(format!(
                "cache/audio-v{}/tone/decoded.wave",
                crate::PCM_VERSION
            )),
            b"broken",
        )
        .unwrap();
        assert!(
            waveform_view(root.path(), &asset, 0., 1., 20, &mut || Ok(()))
                .unwrap_err()
                .to_string()
                .contains("waveform cache invalid")
        );
    }
}
