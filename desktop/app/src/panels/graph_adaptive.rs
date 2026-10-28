//! Bounded screen-error subdivision. Callers supply key/shape boundaries.
use std::cell::Cell;
use std::fmt;

// A clamped derivative can jump between adjacent f64 composition times.
// Keep the stack finite while allowing that one-sided boundary to be retained.
const MAX_DEPTH: u8 = 64;
const MAX_POINTS: usize = 65_536;

/// Merge existing view-density seeds with authored shape/discontinuity seeds.
/// Only adjacent equal times are removed; malformed ordering is never sorted
/// into an apparently valid track. Sampling still enforces its input budget.
pub(super) fn merge_knots(
    first: impl IntoIterator<Item = f64>,
    second: impl IntoIterator<Item = f64>,
) -> impl Iterator<Item = f64> {
    let mut first = first.into_iter().peekable();
    let mut second = second.into_iter().peekable();
    std::iter::from_fn(
        move || match (first.peek().copied(), second.peek().copied()) {
            (Some(a), Some(b)) => {
                if a <= b {
                    first.next()
                } else {
                    second.next()
                }
            }
            (Some(_), None) => first.next(),
            (None, Some(_)) => second.next(),
            (None, None) => None,
        },
    )
    .scan(None, |previous: &mut Option<f64>, frame| {
        let duplicate = previous.replace(frame) == Some(frame);
        Some((!duplicate).then_some(frame))
    })
    .flatten()
}

/// Bounded range preparation for an explicit fit operation. This is not a
/// successful partial result when evaluation or the input budget fails.
pub(super) fn extrema<E>(
    knots: impl IntoIterator<Item = f64>,
    mut value: impl FnMut(f64) -> Result<f64, E>,
) -> Result<Option<[f64; 2]>, Failure<E>> {
    let mut bounds: Option<[f64; 2]> = None;
    let mut previous = None;
    for (index, frame) in knots.into_iter().enumerate() {
        if index >= MAX_POINTS {
            return Err(Failure::Budget {
                evaluations: index,
                points: 0,
            });
        }
        if !frame.is_finite() || previous.is_some_and(|last| frame < last) {
            return Err(Failure::InvalidOrder { frame });
        }
        previous = Some(frame);
        let next = value(frame).map_err(|cause| Failure::Sample { frame, cause })?;
        if !next.is_finite() {
            return Err(Failure::NonFinite { frame });
        }
        bounds = Some(bounds.map_or([next, next], |[low, high]| [low.min(next), high.max(next)]));
    }
    Ok(bounds)
}

#[derive(Debug)]
pub(super) enum Failure<E> {
    Sample { frame: f64, cause: E },
    NonFinite { frame: f64 },
    InvalidGeometry,
    InvalidOrder { frame: f64 },
    Budget { evaluations: usize, points: usize },
    Precision { frame: f64 },
}
impl<E: fmt::Display> fmt::Display for Failure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sample { frame, cause } => write!(f, "graph sample frame={frame} cause={cause}"),
            Self::NonFinite { frame } => write!(f, "graph sample is not finite frame={frame}"),
            Self::InvalidGeometry => f.write_str("graph sampling geometry is invalid"),
            Self::InvalidOrder { frame } => write!(f, "graph knots are not ordered frame={frame}"),
            Self::Budget {
                evaluations,
                points,
            } => write!(
                f,
                "graph sampling budget exhausted evaluations={evaluations} points={points}"
            ),
            Self::Precision { frame } => {
                write!(f, "graph sampling precision exhausted frame={frame}")
            }
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for Failure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sample { cause, .. } => Some(cause),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Point {
    frame: f64,
    value: f64,
}
#[derive(Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
    depth: u8,
}

fn distance(point: Point, a: Point, b: Point, scale: [f64; 2]) -> f64 {
    let p = [
        (point.frame - a.frame) * scale[0],
        (point.value - a.value) * scale[1],
    ];
    let line = [
        (b.frame - a.frame) * scale[0],
        (b.value - a.value) * scale[1],
    ];
    let length = line[0].hypot(line[1]);
    if length == 0. {
        return p[0].hypot(p[1]);
    }
    let direction = [line[0] / length, line[1] / length];
    let along = (p[0] * direction[0] + p[1] * direction[1]).clamp(0., length);
    (p[0] - along * direction[0]).hypot(p[1] - along * direction[1])
}

/// Coordinates remain in model units. Only flatness is measured in logical px.
/// Duplicate knots are allowed, but neither iterator consumption nor model
/// evaluation can exceed the budget. No partial curve is returned on failure.
pub(super) fn sample<E>(
    knots: impl IntoIterator<Item = f64>,
    scale: [f64; 2],
    tolerance: f64,
    mut value: impl FnMut(f64) -> Result<f64, E>,
) -> Result<Vec<(f64, f64)>, Failure<E>> {
    if scale.iter().any(|v| !v.is_finite() || *v <= 0.) || !tolerance.is_finite() || tolerance <= 0.
    {
        return Err(Failure::InvalidGeometry);
    }
    let evaluations = Cell::new(0usize);
    let emitted = Cell::new(0usize);
    let mut evaluate = |frame: f64| {
        if evaluations.get() >= MAX_POINTS * 8 {
            return Err(Failure::Budget {
                evaluations: evaluations.get(),
                points: emitted.get(),
            });
        }
        evaluations.set(evaluations.get() + 1);
        let result = value(frame).map_err(|cause| Failure::Sample { frame, cause })?;
        if !result.is_finite() {
            return Err(Failure::NonFinite { frame });
        }
        Ok(Point {
            frame,
            value: result,
        })
    };
    let mut previous: Option<Point> = None;
    let mut output = Vec::new();
    let mut stack = Vec::with_capacity(usize::from(MAX_DEPTH) + 1);
    for (index, frame) in knots.into_iter().enumerate() {
        if index >= MAX_POINTS {
            return Err(Failure::Budget {
                evaluations: evaluations.get(),
                points: output.len(),
            });
        }
        if !frame.is_finite() {
            return Err(Failure::InvalidOrder { frame });
        }
        let Some(a) = previous else {
            let point = evaluate(frame)?;
            output.push((point.frame, point.value));
            emitted.set(output.len());
            previous = Some(point);
            continue;
        };
        if frame < a.frame {
            return Err(Failure::InvalidOrder { frame });
        }
        if frame == a.frame {
            continue;
        }
        if !(frame - a.frame).is_finite() {
            return Err(Failure::InvalidGeometry);
        }
        let b = evaluate(frame)?;
        stack.push(Segment { a, b, depth: 0 });
        while let Some(segment) = stack.pop() {
            let Segment { a, b, depth } = segment;
            let middle = a.frame + (b.frame - a.frame) * 0.5;
            // Adjacent representable times describe the discontinuity itself;
            // there is no representable sample between these endpoints.
            if middle == a.frame || middle == b.frame {
                if output.len() >= MAX_POINTS {
                    return Err(Failure::Budget {
                        evaluations: evaluations.get(),
                        points: output.len(),
                    });
                }
                output.push((b.frame, b.value));
                emitted.set(output.len());
                continue;
            }
            let m = evaluate(middle)?;
            let left = evaluate(a.frame + (b.frame - a.frame) * 0.25)?;
            let right = evaluate(a.frame + (b.frame - a.frame) * 0.75)?;
            // Neighboring samples also define quadratic control envelopes.
            // Their control values expose a narrow turning peak between probes
            // even when every sampled ordinate appears almost collinear.
            let envelope = |before: Point, probe: Point, after: Point| Point {
                frame: probe.frame,
                value: probe.value
                    + (probe.value - before.value) * 0.5
                    + (probe.value - after.value) * 0.5,
            };
            let maximum_error = |points: [Point; 3]| {
                points
                    .into_iter()
                    .map(|p| distance(p, a, b, scale))
                    .try_fold(0f64, |maximum, error| {
                        if error.is_finite() {
                            Ok(maximum.max(error))
                        } else {
                            Err(Failure::InvalidGeometry)
                        }
                    })
            };
            let error = maximum_error([left, m, right])?;
            let envelope_error = maximum_error([
                envelope(a, left, m),
                envelope(left, m, right),
                envelope(m, right, b),
            ])?;
            // Probes are finite samples, not a continuous supremum. Reserve
            // half the error budget for inter-probe extrema and step edges.
            // Do not accept a merely narrow time bracket: immediately after a
            // clamp jump the actual ordinate may exceed both endpoint values.
            // The adjacent-time branch above retains that one-sided limit.
            if error <= tolerance * 0.5 && envelope_error <= tolerance * 0.5 {
                if output.len() >= MAX_POINTS {
                    return Err(Failure::Budget {
                        evaluations: evaluations.get(),
                        points: output.len(),
                    });
                }
                output.push((b.frame, b.value));
                emitted.set(output.len());
            } else {
                if depth >= MAX_DEPTH {
                    return Err(Failure::Precision { frame: middle });
                }
                stack.push(Segment {
                    a: m,
                    b,
                    depth: depth + 1,
                });
                stack.push(Segment {
                    a,
                    b: m,
                    depth: depth + 1,
                });
            }
        }
        previous = Some(b);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn oracle(points: &[(f64, f64)], scale: [f64; 2], value: impl Fn(f64) -> f64) -> f64 {
        let mut maximum = 0f64;
        for segment in points.windows(2) {
            for i in 0..=128 {
                let frame = segment[0].0 + (segment[1].0 - segment[0].0) * f64::from(i) / 128.;
                maximum = maximum.max(distance(
                    Point {
                        frame,
                        value: value(frame),
                    },
                    Point {
                        frame: segment[0].0,
                        value: segment[0].1,
                    },
                    Point {
                        frame: segment[1].0,
                        value: segment[1].1,
                    },
                    scale,
                ));
            }
        }
        maximum
    }
    #[test]
    fn vertical_magnification_adds_geometry_and_follows_a_dense_screen_oracle() {
        let curve = |t: f64| t * t * t;
        let mut counts = Vec::new();
        for scale in [[800., 100.], [800., 6400.], [800., 1000000.]] {
            let points = sample((0..=4).map(|i| f64::from(i) / 4.), scale, 0.35, |t| {
                Ok::<_, String>(curve(t))
            })
            .unwrap();
            assert!(oracle(&points, scale, curve) <= 0.36);
            counts.push(points.len());
        }
        assert!(counts[1] > counts[0]);
        assert!(counts.iter().all(|count| *count < 1024));
    }
    #[test]
    fn periodic_knots_prevent_midpoint_aliasing_of_the_valid_elastic_frequency() {
        let curve = |t: f64| 1. - (-0.5 * t).exp() * (std::f64::consts::TAU * 8. * t).cos();
        let scale = [1200., 20000.];
        let points = sample((0..=32).map(|i| f64::from(i) / 32.), scale, 0.35, |t| {
            Ok::<_, String>(curve(t))
        })
        .unwrap();
        assert!(oracle(&points, scale, curve) <= 0.36);
        assert!(points.iter().any(|p| p.1 > 1.8));
    }

    #[test]
    fn quadratic_envelopes_refine_narrow_velocity_peaks_between_ordinary_probes() {
        let w = std::f64::consts::TAU * 8.;
        let damping = 0.5f64;
        let norm = 1. - (-damping).exp();
        let curve = |frame: f64| {
            if !(17. ..=77.).contains(&frame) {
                return 0.;
            }
            let t = (frame - 17.) / 60.;
            let progress =
                (1. - (-damping * t).exp() * ((w * t).cos() + damping / w * (w * t).sin())) / norm;
            if !(0. ..=1.).contains(&(0.25 + 0.5 * progress)) {
                return 0.;
            }
            0.25 * (-damping * t).exp() * (w + damping * damping / w) * (w * t).sin() / norm
        };
        let seeds = merge_knots(
            (0..=256).map(|i| 10. + 80. * f64::from(i) / 256.),
            (0..=32).map(|i| 17. + 60. * f64::from(i) / 32.),
        );
        let scale = [10., 500.];
        let points = sample(seeds, scale, 0.35, |frame| Ok::<_, String>(curve(frame))).unwrap();
        if oracle(&points, scale, curve) > 0.36 {
            let mut worst = (0f64, (0., 0.), (0., 0.), 0., 0.);
            for segment in points.windows(2) {
                for i in 0..=128 {
                    let frame = segment[0].0 + (segment[1].0 - segment[0].0) * f64::from(i) / 128.;
                    let value = curve(frame);
                    let error = distance(
                        Point { frame, value },
                        Point {
                            frame: segment[0].0,
                            value: segment[0].1,
                        },
                        Point {
                            frame: segment[1].0,
                            value: segment[1].1,
                        },
                        scale,
                    );
                    if error > worst.0 {
                        worst = (error, segment[0], segment[1], frame, value);
                    }
                }
            }
            eprintln!("dense worst={worst:?} points={}", points.len());
        }
        assert!(oracle(&points, scale, curve) <= 0.36);
        assert!(points.len() < 65536);
    }
    #[test]
    fn key_boundaries_preserve_hold_steps_without_diagonal_interpolation() {
        let before = f64::from_bits(1f64.to_bits() - 1);
        let points = sample([0., before, 1., 2.], [800., 800.], 0.35, |t| {
            Ok::<_, String>(if t < 1. { 0. } else { 1. })
        })
        .unwrap();
        assert_eq!(points, vec![(0., 0.), (before, 0.), (1., 1.), (2., 1.)]);
    }
    #[test]
    fn errors_keep_the_original_cause_and_budgets_do_not_return_partial_success() {
        let error = sample([0., 1.], [1., 1.], 0.35, |t| {
            if t == 0.5 {
                Err("original cause")
            } else {
                Ok(t)
            }
        })
        .unwrap_err();
        assert!(matches!(
            error,
            Failure::Sample {
                frame: 0.5,
                cause: "original cause"
            }
        ));
        assert!(matches!(
            sample(std::iter::repeat(0.), [1., 1.], 0.35, |t| Ok::<_, String>(
                t
            )),
            Err(Failure::Budget { .. })
        ));
        assert!(matches!(
            sample([1., 0.], [1., 1.], 0.35, |t| Ok::<_, String>(t)),
            Err(Failure::InvalidOrder { .. })
        ));
        assert!(matches!(
            sample([0., 1.], [1., 1.], 0.35, |_| Ok::<_, String>(f64::NAN)),
            Err(Failure::NonFinite { .. })
        ));
    }
    #[test]
    fn overflowing_screen_geometry_cannot_be_misreported_as_a_flat_curve() {
        assert!(matches!(
            sample([0., 1.], [f64::MAX, f64::MAX], 0.35, |t| Ok::<_, String>(
                t * 2.
            )),
            Err(Failure::InvalidGeometry)
        ));
    }
    #[test]
    fn duplicate_knot_budget_preserves_the_available_evaluation_and_output_counts() {
        assert!(matches!(
            sample(std::iter::repeat(0.), [1., 1.], 0.35, |t| Ok::<_, String>(
                t
            )),
            Err(Failure::Budget {
                evaluations: 1,
                points: 1
            })
        ));
    }
}
