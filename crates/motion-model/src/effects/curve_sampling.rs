//! Bounded snapshots of authored transfers, independent of a GPU LUT format.
use super::{CurveLut, CurveObject, CurveTrack};
use crate::{color_curves::Transfer, ensure, Error, Result};
use std::sync::Arc;

/// Sixteen subdivisions of each editor interval. The nested grid preserves
/// every 256-sample point exactly when freezing and resampling a curve.
pub const HIGH_PRECISION_CURVE_SAMPLES: usize = 255 * 16 + 1;

/// Retains at most the two selected keys, never a complete animation history.
/// Dense GPU tables are materialized separately, after resource admission.
#[derive(Clone, Debug, PartialEq)]
pub struct SampledColorCurve {
    first: Arc<CurveObject>,
    second: Option<Arc<CurveObject>>,
    amount: f32,
}

struct Prepared<'a> {
    object: &'a CurveObject,
    channels: [Transfer<'a>; 5],
}
impl<'a> Prepared<'a> {
    fn new(object: &'a CurveObject) -> Self {
        Self {
            object,
            channels: std::array::from_fn(|i| {
                Transfer::new(
                    &object.channels[i],
                    object.interpolation[i],
                    object.channel_luts[i].as_deref(),
                )
            }),
        }
    }
    fn at(&self, x: f32) -> [f32; 4] {
        if let Some(lut) = &self.object.sampled_lut {
            return frozen_at(lut, x);
        }
        let master = self.channels[0].at(x);
        [
            self.channels[1].at(master),
            self.channels[2].at(master),
            self.channels[3].at(master),
            self.channels[4].at(x),
        ]
    }
}

/// Inputs are validated before constructing an immutable snapshot or preview.
pub(super) fn frozen_at(lut: &[[f32; 4]], x: f32) -> [f32; 4] {
    let last = (lut.len() - 1) as f32;
    let position = x.clamp(0., 1.) * last;
    let nearest = position.round().clamp(0., last) as usize;
    if x == nearest as f32 / last {
        return lut[nearest];
    }
    let lower = position.floor().clamp(0., last) as usize;
    let upper = (lower + 1).min(lut.len() - 1);
    std::array::from_fn(|c| {
        lut[lower][c] + (lut[upper][c] - lut[lower][c]) * (position - lower as f32)
    })
}

fn object_bytes(object: &CurveObject) -> usize {
    object.channels.iter().map(|p| p.len() * 8).sum::<usize>()
        + object
            .channel_luts
            .iter()
            .flatten()
            .map(|p| p.len() * 4)
            .sum::<usize>()
        + object.sampled_lut.as_ref().map_or(0, |p| p.len() * 16)
}

impl SampledColorCurve {
    /// Logical retained source bytes. Cloned contexts share the backing Arcs.
    pub fn source_bytes(&self) -> usize {
        object_bytes(&self.first) + self.second.as_deref().map_or(0, object_bytes)
    }
    fn sample_grid(&self, count: usize, mut output: impl FnMut([f32; 4])) {
        let first = Prepared::new(&self.first);
        let second = self.second.as_deref().map(Prepared::new);
        for i in 0..count {
            let x = i as f32 / (count - 1) as f32;
            let a = first.at(x);
            output(if let Some(second) = &second {
                let b = second.at(x);
                std::array::from_fn(|c| a[c] + (b[c] - a[c]) * self.amount)
            } else {
                a
            });
        }
    }
    /// The 256-sample editor/UNORM8 view is not an authoritative freeze format.
    pub fn preview_lut(&self) -> CurveLut {
        let mut output = [[0.; 4]; 256];
        let mut i = 0;
        self.sample_grid(256, |row| {
            output[i] = row;
            i += 1;
        });
        output
    }
    /// F32 samples come directly from authored transfers or their stored grid.
    /// This finite grid cannot resolve arbitrarily narrow features between taps.
    pub fn high_precision_lut(&self) -> Vec<[f32; 4]> {
        let mut output = Vec::with_capacity(HIGH_PRECISION_CURVE_SAMPLES);
        self.sample_grid(HIGH_PRECISION_CURVE_SAMPLES, |row| output.push(row));
        output
    }
}

impl CurveTrack {
    pub fn sampled_curve(&self, frame: f64) -> Result<SampledColorCurve> {
        self.sampled_curve_reusing(frame, None)
    }
    pub fn sampled_curve_reusing(
        &self,
        frame: f64,
        previous: Option<&SampledColorCurve>,
    ) -> Result<SampledColorCurve> {
        self.sampled_curve_with_budget(frame, previous, usize::MAX)
    }
    /// Bound copied source data before cloning a selected key. The caller owns
    /// aggregate scene admission; no dense table is allocated by this method.
    pub fn sampled_curve_with_budget(
        &self,
        frame: f64,
        previous: Option<&SampledColorCurve>,
        source_budget: usize,
    ) -> Result<SampledColorCurve> {
        self.capture_curve(frame, previous, source_budget)
            .map_err(|source| Error::ColorCurveSampling {
                frame,
                source: Box::new(source),
            })
    }
    fn capture_curve(
        &self,
        frame: f64,
        previous: Option<&SampledColorCurve>,
        source_budget: usize,
    ) -> Result<SampledColorCurve> {
        let capture = |object: &CurveObject| -> Result<Arc<CurveObject>> {
            object.validate()?;
            Ok(previous
                .into_iter()
                .flat_map(|p| std::iter::once(&p.first).chain(p.second.iter()))
                .find(|p| p.as_ref() == object)
                .cloned()
                .unwrap_or_else(|| Arc::new(object.clone())))
        };
        let constant = |object: &CurveObject| -> Result<SampledColorCurve> {
            object.validate()?;
            ensure(
                object_bytes(object) <= source_budget,
                "color curve source snapshot exceeds CPU budget",
            )?;
            Ok(SampledColorCurve {
                first: capture(object)?,
                second: None,
                amount: 0.,
            })
        };
        if self.keys.is_empty() {
            return constant(&self.value);
        }
        let upper = self.keys.partition_point(|k| f64::from(k.frame) <= frame);
        if upper == 0 {
            return constant(
                &self
                    .keys
                    .first()
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "color curve sample frame={frame}: first key is missing"
                        ))
                    })?
                    .value,
            );
        }
        if upper >= self.keys.len() {
            return constant(
                &self
                    .keys
                    .last()
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "color curve sample frame={frame}: last key is missing"
                        ))
                    })?
                    .value,
            );
        }
        let a = upper
            .checked_sub(1)
            .and_then(|i| self.keys.get(i))
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "color curve sample frame={frame}: predecessor is missing"
                ))
            })?;
        let b = self.keys.get(upper).ok_or_else(|| {
            Error::Invalid(format!(
                "color curve sample frame={frame}: successor is missing"
            ))
        })?;
        ensure(
            a.frame < b.frame,
            "color curve sample interval must increase",
        )?;
        a.value.validate()?;
        b.value.validate()?;
        ensure(
            object_bytes(&a.value) + object_bytes(&b.value) <= source_budget,
            "color curve source snapshot exceeds CPU budget",
        )?;
        let t = ((frame - f64::from(a.frame)) / (f64::from(b.frame) - f64::from(a.frame))) as f32;
        let amount = a
            .curve
            .map_or_else(|| a.ease.map(t), |c| c.sample(f64::from(t)).progress as f32);
        Ok(SampledColorCurve {
            first: capture(&a.value)?,
            second: Some(capture(&b.value)?),
            amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_source_snapshots_share_backing_storage() {
        let track = CurveTrack::default();
        let first = track.sampled_curve(0.).unwrap();
        let next = track.sampled_curve_reusing(1., Some(&first)).unwrap();
        assert!(Arc::ptr_eq(&first.first, &next.first));
        assert!(next.second.is_none());
        assert_eq!(next.source_bytes(), 5 * 2 * 8);
    }
}
