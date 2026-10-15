//! Independently authored energy-rise references from the existing 100 Hz RMS cache.
//! These are editing cues, not tempo estimation or a guaranteed musical beat grid.
use crate::{read_waveform, Result};
use motion_model::AudioAsset;
use std::path::Path;

pub fn audio_references(
    root: &Path,
    asset: &AudioAsset,
    begin_seconds: f64,
    end_seconds: f64,
    check: &mut dyn FnMut(f64) -> Result<()>,
) -> Result<Vec<f64>> {
    asset.validate().map_err(|e| e.to_string())?;
    if !begin_seconds.is_finite() || !end_seconds.is_finite() || begin_seconds > end_seconds {
        return Err("invalid audio reference interval".into());
    }
    let duration = asset.sample_frames as f64 / f64::from(asset.sample_rate);
    let begin = begin_seconds.clamp(0., duration);
    let end = end_seconds.clamp(begin, duration);
    let first = ((begin * 100.).floor() as u64).saturating_sub(4);
    let last = ((end * 100.).ceil() as u64 + 4)
        .min((asset.sample_frames * 100).div_ceil(u64::from(asset.sample_rate)));
    let mut rms = Vec::with_capacity((last - first) as usize);
    let mut bucket = first;
    while bucket < last {
        check((bucket - first) as f64 / (last - first).max(1) as f64 * 0.7)?;
        let count = ((last - bucket) as usize).min(4096);
        rms.extend(
            read_waveform(root, asset, bucket, count)?
                .into_iter()
                .map(|b| b.rms),
        );
        bucket += count as u64;
    }
    let peaks = pick_references(&rms, check)?;
    check(1.)?;
    Ok(peaks
        .into_iter()
        .map(|i| (first as usize + i) as f64 / 100.)
        .filter(|t| *t >= begin && *t < end)
        .collect())
}

fn pick_references(rms: &[f32], check: &mut dyn FnMut(f64) -> Result<()>) -> Result<Vec<usize>> {
    if rms.len() < 3 {
        return Ok(vec![]);
    }
    if rms.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err("invalid RMS cache".into());
    }
    let peak = rms.iter().copied().fold(0f32, f32::max);
    if peak < 0.000_01 {
        return Ok(vec![]);
    }
    let mut novelty = vec![0.; rms.len()];
    for i in 1..rms.len() {
        novelty[i] = (rms[i] - rms[i - 1]).max(0.);
    }
    let mut candidates: Vec<(usize, f32)> = vec![];
    for i in 1..rms.len() - 1 {
        if i % 2048 == 0 {
            check(0.7 + 0.3 * i as f64 / rms.len() as f64)?;
        }
        let value = novelty[i];
        if value <= peak * 0.006 || value < 0.000_01 {
            continue;
        }
        let window = &novelty[i.saturating_sub(25)..(i + 26).min(rms.len())];
        let mean = window.iter().sum::<f32>() / window.len() as f32;
        let variance = window.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / window.len() as f32;
        if value <= mean + variance.sqrt() * 1.5 {
            continue;
        }
        // Keep a unique local peak and suppress multiple attack/ringing cues within 120 ms.
        let local = i.saturating_sub(3)..(i + 4).min(rms.len());
        if local
            .clone()
            .any(|j| novelty[j] > value || (j < i && novelty[j] == value))
        {
            continue;
        }
        if let Some((previous, score)) = candidates.last_mut().filter(|(p, _)| i - *p < 12) {
            if value > *score {
                *previous = i;
                *score = value;
            }
        } else {
            candidates.push((i, value));
        }
    }
    Ok(candidates.into_iter().map(|(i, _)| i).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_steady_tones_and_small_noise_fluctuations_have_no_references() {
        for samples in [
            vec![0.; 500],
            vec![0.2; 500],
            (0..500).map(|i| 0.2 + (i % 7) as f32 * 0.0001).collect(),
        ] {
            assert!(pick_references(&samples, &mut |_| Ok(()))
                .unwrap()
                .is_empty());
        }
    }
    #[test]
    fn strong_and_soft_attacks_are_detected_without_a_fixed_bpm_grid() {
        let mut rms = vec![0.001; 500];
        for (time, gain) in [(50, 0.8), (101, 0.2), (159, 0.04), (310, 0.6)] {
            for k in 0..10 {
                rms[time + k] += gain * (1. - k as f32 / 10.);
            }
        }
        assert_eq!(
            pick_references(&rms, &mut |_| Ok(())).unwrap(),
            [50, 101, 159, 310]
        );
        assert!(pick_references(&vec![0.1; 3000], &mut |_| Err("cancelled".into())).is_err());
    }
}
