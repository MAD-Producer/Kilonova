//! A shared exposure phase preserves each layer's own midpoint sample weights.
//! Partitioning at rational bin boundaries avoids an unbounded least-common-
//! multiple schedule when layers request different sample counts.
use super::{Shutter, MAX_SHUTTER_SAMPLES, PREVIEW_SHUTTER_SAMPLES};
use crate::{ensure, Error, Result, MAX_LAYERS};
use std::collections::BTreeSet;

// The union of all i/n boundaries for 2 <= n <= 16 has 80 intervals.
pub const MAX_EXPOSURE_PHASES: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fraction(u32, u32);
impl Fraction {
    fn new(numerator: u32, denominator: u32) -> Self {
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self(numerator / a, denominator / a)
    }
    fn value(self) -> f64 {
        f64::from(self.0) / f64::from(self.1)
    }
}
impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0 * other.1).cmp(&(other.0 * self.1))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExposurePhase {
    /// Normalized shutter phase, shared by all enabled layers.
    pub start: f64,
    pub end: f64,
    /// Includes transparent samples: consumers must not renormalize coverage.
    pub weight: f64,
    midpoint_numerator: u32,
    midpoint_denominator: u32,
}

#[derive(Clone, Debug)]
pub struct ExposureClock {
    frame: f64,
    shutters: Vec<(u64, Shutter, u8)>,
    phases: Vec<ExposurePhase>,
}
impl ExposureClock {
    pub fn new(frame: f64, shutters: &[(u64, Shutter)], preview: bool) -> Result<Self> {
        ensure(frame.is_finite(), "nonfinite exposure time")?;
        ensure(
            shutters.len() <= MAX_LAYERS,
            "exposure layer count exceeds model limit",
        )?;
        let mut objects = BTreeSet::new();
        let mut boundaries = BTreeSet::from([Fraction(0, 1), Fraction(1, 1)]);
        let mut layers = Vec::with_capacity(shutters.len());
        for &(object, shutter) in shutters {
            ensure(
                object != 0 && objects.insert(object),
                "invalid or duplicate exposure layer id",
            )?;
            shutter
                .validate()
                .map_err(|error| Error::Invalid(format!("layer {object} shutter: {error}")))?;
            let count = if preview {
                shutter.samples.min(PREVIEW_SHUTTER_SAMPLES)
            } else {
                shutter.samples
            };
            for index in 1..count {
                boundaries.insert(Fraction::new(u32::from(index), u32::from(count)));
            }
            layers.push((object, shutter, count));
        }
        layers.sort_unstable_by_key(|(object, _, _)| *object);
        let boundaries: Vec<_> = boundaries.into_iter().collect();
        ensure(
            boundaries.len() <= MAX_EXPOSURE_PHASES + 1 && MAX_SHUTTER_SAMPLES == 16,
            "exposure phase schedule exceeds validated limit",
        )?;
        let mut phases: Vec<_> = boundaries
            .windows(2)
            .map(|bounds| {
                let (a, b) = (bounds[0], bounds[1]);
                ExposurePhase {
                    start: a.value(),
                    end: b.value(),
                    weight: f64::from(b.0 * a.1 - a.0 * b.1) / f64::from(a.1 * b.1),
                    midpoint_numerator: a.0 * b.1 + b.0 * a.1,
                    midpoint_denominator: 2 * a.1 * b.1,
                }
            })
            .collect();
        // Correct summation roundoff for the complete exposure only. Transparent
        // phases keep their weight; never renormalize the visible subset.
        let last = phases.len() - 1;
        phases[last].weight = 1. - phases[..last].iter().map(|phase| phase.weight).sum::<f64>();
        Ok(Self {
            frame,
            shutters: layers,
            phases,
        })
    }
    pub fn phases(&self) -> &[ExposurePhase] {
        &self.phases
    }
    /// An unblurred layer remains at the requested composition time. Enabled
    /// layers use their own shutter duration and exact midpoint bin, even when
    /// another layer asks for a different duration or sample count.
    pub fn layer_frame(&self, object: u64, phase: usize) -> Result<f64> {
        ensure(object != 0, "invalid exposure layer id")?;
        let phase = self.phases.get(phase).ok_or_else(|| {
            Error::Invalid(format!(
                "layer {object}: exposure phase index outside schedule"
            ))
        })?;
        let Ok(index) = self
            .shutters
            .binary_search_by_key(&object, |(id, _, _)| *id)
        else {
            return Ok(self.frame);
        };
        let (_, shutter, count) = &self.shutters[index];
        let index = phase.midpoint_numerator * u32::from(*count) / phase.midpoint_denominator;
        let time =
            self.frame + ((f64::from(index) + 0.5) / f64::from(*count) - 0.5) * shutter.frames;
        ensure(time.is_finite(), "exposure sample time overflow")?;
        Ok(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn marginal(clock: &ExposureClock, object: u64) -> Vec<(f64, f64)> {
        let mut result: Vec<(f64, f64)> = Vec::new();
        for (index, phase) in clock.phases().iter().enumerate() {
            let time = clock.layer_frame(object, index).unwrap();
            if let Some((_, weight)) = result.iter_mut().find(|(value, _)| *value == time) {
                *weight += f64::from(phase.weight);
            } else {
                result.push((time, f64::from(phase.weight)));
            }
        }
        result
    }
    #[test]
    fn all_supported_counts_keep_exact_marginal_weights_with_a_bounded_schedule() {
        let settings: Vec<_> = (2..=16)
            .map(|n| {
                (
                    u64::from(n),
                    Shutter {
                        frames: f64::from(n) / 8.,
                        samples: n,
                    },
                )
            })
            .collect();
        for preview in [false, true] {
            let mut unordered = settings.clone();
            unordered.reverse();
            let clock = ExposureClock::new(5.25, &unordered, preview).unwrap();
            assert_eq!(clock.phases().len(), if preview { 6 } else { 80 });
            assert!(
                (clock
                    .phases()
                    .iter()
                    .map(|p| f64::from(p.weight))
                    .sum::<f64>()
                    - 1.)
                    .abs()
                    < 1e-7
            );
            for &(object, shutter) in &settings {
                let samples = marginal(&clock, object);
                let expected = shutter.taps(5.25, preview).unwrap();
                assert_eq!(samples.len(), expected.len());
                for ((time, weight), tap) in samples.iter().zip(expected) {
                    assert_eq!(*time, tap.frame);
                    assert!((*weight - f64::from(tap.weight)).abs() < 1e-7);
                }
            }
            assert_eq!(marginal(&clock, 999), [(5.25, 1.)]);
        }
    }
    #[test]
    fn transparent_boundary_weights_and_invalid_inputs_are_preserved() {
        let clock = ExposureClock::new(
            0.,
            &[
                (
                    1,
                    Shutter {
                        frames: 2.,
                        samples: 3,
                    },
                ),
                (2, Shutter::default()),
            ],
            false,
        )
        .unwrap();
        let samples = marginal(&clock, 1);
        assert!(samples[0].0 < 0.);
        assert!((samples[0].1 - 1. / 3.).abs() < 1e-7);
        assert_eq!(clock.layer_frame(999, 0).unwrap(), 0.);
        assert!(clock.layer_frame(1, clock.phases().len()).is_err());
        assert!(clock.layer_frame(0, 0).is_err());
        assert!(ExposureClock::new(f64::NAN, &[], false).is_err());
        assert!(ExposureClock::new(
            0.,
            &[(1, Shutter::default()), (1, Shutter::default())],
            false
        )
        .is_err());
        assert!(ExposureClock::new(
            0.,
            &[(
                1,
                Shutter {
                    frames: 0.,
                    samples: 8
                }
            )],
            false
        )
        .is_err());
        let empty = ExposureClock::new(3., &[], false).unwrap();
        assert_eq!(empty.phases().len(), 1);
        assert_eq!(empty.phases()[0].weight, 1.);
        assert_eq!(empty.layer_frame(1, 0).unwrap(), 3.);
    }
}
