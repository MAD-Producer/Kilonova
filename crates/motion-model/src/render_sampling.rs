//! Authored sampling settings and deterministic clocks shared by preview/export.
//! A temporal weight belongs to the shutter, even when its sample is transparent.
use crate::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const MAX_SHUTTER_SAMPLES: u8 = 16;
pub const PREVIEW_SHUTTER_SAMPLES: u8 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameBlend {
    #[default]
    Hold,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shutter {
    /// Exposure duration in composition frames, centred on the requested time.
    pub frames: f64,
    pub samples: u8,
}
impl Default for Shutter {
    fn default() -> Self {
        Self {
            frames: 0.5,
            samples: 8,
        }
    }
}
impl Shutter {
    pub fn validate(self) -> Result<()> {
        ensure(
            self.frames.is_finite() && self.frames > 0. && self.frames <= 2.,
            "shutter duration must be finite and within (0, 2] composition frames",
        )?;
        ensure(
            (2..=MAX_SHUTTER_SAMPLES).contains(&self.samples),
            "shutter sample count must be within 2..=16",
        )
    }
    pub fn taps(self, frame: f64, preview: bool) -> Result<Vec<TemporalTap>> {
        self.validate()?;
        ensure(frame.is_finite(), "nonfinite shutter time")?;
        let count = if preview {
            self.samples.min(PREVIEW_SHUTTER_SAMPLES)
        } else {
            self.samples
        };
        Ok((0..count)
            .map(|index| TemporalTap {
                frame: frame + ((f64::from(index) + 0.5) / f64::from(count) - 0.5) * self.frames,
                weight: 1. / f32::from(count),
            })
            .collect())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerSampling {
    #[serde(default)]
    pub frame_blend: FrameBlend,
    #[serde(default)]
    pub motion_blur: Option<Shutter>,
    #[serde(default)]
    pub continuous_rasterization: bool,
}
impl LayerSampling {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(self) -> Result<()> {
        if let Some(shutter) = self.motion_blur {
            shutter.validate()?;
        }
        Ok(())
    }
    pub fn validate_content(self, content: &crate::Content) -> Result<()> {
        self.validate()?;
        ensure(
            self.frame_blend == FrameBlend::Hold || matches!(content, crate::Content::Video { .. }),
            "frame blending requires a video layer",
        )?;
        ensure(
            !self.continuous_rasterization || matches!(content, crate::Content::Vector { .. }),
            "continuous rasterization currently requires authored vector geometry",
        )?;
        ensure(
            self.motion_blur.is_none()
                || !matches!(
                    content,
                    crate::Content::Null
                        | crate::Content::Camera { .. }
                        | crate::Content::Audio { .. }
                        | crate::Content::Adjustment
                ),
            "motion blur requires a visual source layer",
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalTap {
    pub frame: f64,
    pub weight: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceTap {
    pub pts_us: u64,
    pub weight: f32,
}

/// Bracket actual presentation timestamps, including variable-rate video.
/// Clip/source time outside the half-open interval is transparent. Within the
/// interval the last presented image holds until its end; it never wraps.
/// Callers validate the bounded, strictly increasing PTS index on import.
pub fn source_taps(
    pts: &[u64],
    interval: [u64; 2],
    time_us: f64,
    blend: FrameBlend,
) -> Result<Vec<SourceTap>> {
    ensure(time_us.is_finite(), "nonfinite frame blending source time")?;
    ensure(
        interval[0] < interval[1],
        "invalid frame blending source interval",
    )?;
    if time_us < interval[0] as f64 || time_us >= interval[1] as f64 {
        return Ok(vec![]);
    }
    let end = pts.partition_point(|pts| *pts < interval[1]);
    let start = pts[..end].partition_point(|pts| *pts < interval[0]);
    let pts = &pts[start..end];
    let right = pts.partition_point(|pts| (*pts as f64) <= time_us);
    if right == 0 {
        return Ok(vec![]);
    }
    let left = pts[right - 1];
    let Some(next) = pts
        .get(right)
        .copied()
        .filter(|_| blend == FrameBlend::Linear)
    else {
        return Ok(vec![SourceTap {
            pts_us: left,
            weight: 1.,
        }]);
    };
    ensure(
        next > left,
        "frame blending PTS must be strictly increasing",
    )?;
    let weight = ((time_us - left as f64) / (next - left) as f64) as f32;
    if weight == 0. {
        return Ok(vec![SourceTap {
            pts_us: left,
            weight: 1.,
        }]);
    }
    Ok(vec![
        SourceTap {
            pts_us: left,
            weight: 1. - weight,
        },
        SourceTap {
            pts_us: next,
            weight,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutter_preview_and_frozen_share_exposure_and_preserve_transparent_weights() {
        for frame in [0., 12.25, 119.99] {
            for preview in [false, true] {
                let taps = Shutter::default().taps(frame, preview).unwrap();
                assert_eq!(taps.len(), if preview { 4 } else { 8 });
                assert!((taps.iter().map(|t| t.weight).sum::<f32>() - 1.).abs() < 1e-6);
                assert!(
                    (taps
                        .iter()
                        .map(|t| t.frame * f64::from(t.weight))
                        .sum::<f64>()
                        - frame)
                        .abs()
                        < 1e-6
                );
                assert!(taps.iter().all(|tap| (tap.frame - frame).abs() < 0.25));
                if frame == 0. {
                    assert_eq!(
                        taps.iter()
                            .filter(|t| t.frame >= 0.)
                            .map(|t| t.weight)
                            .sum::<f32>(),
                        0.5
                    );
                }
            }
        }
        assert!(Shutter {
            frames: f64::NAN,
            ..Default::default()
        }
        .taps(0., false)
        .is_err());
        assert!(Shutter {
            samples: 17,
            ..Default::default()
        }
        .taps(0., false)
        .is_err());
        assert!(Shutter::default().taps(f64::INFINITY, false).is_err());
    }
    #[test]
    fn blending_uses_real_pts_not_nominal_fps_and_never_wraps_or_renormalizes() {
        let pts = [100, 140, 300];
        assert_eq!(
            source_taps(&pts, [100, 400], 220., FrameBlend::Linear).unwrap(),
            vec![
                SourceTap {
                    pts_us: 140,
                    weight: 0.5
                },
                SourceTap {
                    pts_us: 300,
                    weight: 0.5
                }
            ]
        );
        assert_eq!(
            source_taps(&pts, [100, 400], 220., FrameBlend::Hold).unwrap(),
            vec![SourceTap {
                pts_us: 140,
                weight: 1.
            }]
        );
        assert_eq!(
            source_taps(&pts, [100, 400], 399., FrameBlend::Linear).unwrap(),
            vec![SourceTap {
                pts_us: 300,
                weight: 1.
            }]
        );
        assert!(source_taps(&pts, [100, 400], 99., FrameBlend::Linear)
            .unwrap()
            .is_empty());
        assert!(source_taps(&pts, [100, 400], 400., FrameBlend::Linear)
            .unwrap()
            .is_empty());
        assert!(source_taps(&pts, [150, 400], 220., FrameBlend::Linear)
            .unwrap()
            .is_empty());
        assert!(source_taps(&pts, [100, 100], 100., FrameBlend::Hold).is_err());
    }
}
