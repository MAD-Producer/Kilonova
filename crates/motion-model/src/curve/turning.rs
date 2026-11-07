use super::*;

pub(super) fn times(curve: Curve) -> Vec<f64> {
    match curve.definition() {
        CurveDefinition::Elastic { oscillations, .. } => {
            // The derivative is a nonzero scale times exp(-d*t)*sin(w*t).
            (1..16).map(|k| f64::from(k) / (2. * oscillations))
                .take_while(|&t| t < 1.).collect()
        }
        CurveDefinition::Bezier(x, y) => {
            let coefficients = match curve.space {
                CurveSpace::Progress => vec![y[1], 2. * y[2], 3. * y[3]],
                // Progress is integral(y dx)/area. Monotone x and the fixed
                // nonzero normalization area do not change the zero set.
                CurveSpace::Velocity => y.to_vec(),
            };
            let mut times: Vec<_> = roots(&coefficients).into_iter()
                .map(|u| polynomial(x, u)).filter(|&t| t > 0. && t < 1.).collect();
            times.sort_by(f64::total_cmp);
            times.dedup();
            times
        }
    }
}

fn value(coefficients: &[f64], u: f64) -> f64 {
    coefficients.iter().rev().fold(0., |result, coefficient| result * u + coefficient)
}

// Degree is at most three. Derivative roots partition the polynomial into
// monotone intervals, so bisection cannot overlook two closely spaced turns
// inside one sampling cell. No epsilon merges distinct authored roots.
fn roots(coefficients: &[f64]) -> Vec<f64> {
    let mut degree = coefficients.len();
    while degree > 0 && coefficients[degree - 1] == 0. { degree -= 1; }
    let coefficients = &coefficients[..degree];
    if degree <= 1 { return Vec::new(); }
    let derivative: Vec<_> = coefficients.iter().enumerate().skip(1)
        .map(|(i, c)| *c * i as f64).collect();
    let mut cuts = vec![0.];
    cuts.extend(roots(&derivative));
    cuts.push(1.);
    let mut result = Vec::new();
    for pair in cuts.windows(2) {
        let (mut lo, mut hi) = (pair[0], pair[1]);
        let (left, right) = (value(coefficients, lo), value(coefficients, hi));
        if left == 0. { result.push(lo); }
        if right == 0. { result.push(hi); }
        if (left < 0. && right > 0.) || (left > 0. && right < 0.) {
            for _ in 0..64 {
                let mid = lo + (hi - lo) / 2.;
                if mid == lo || mid == hi { break; }
                let at = value(coefficients, mid);
                if at == 0. { lo = mid; hi = mid; break; }
                if (at > 0.) == (left > 0.) { lo = mid; } else { hi = mid; }
            }
            result.push(lo + (hi - lo) / 2.);
        }
    }
    result.retain(|&u| u > 0. && u < 1.);
    result.sort_by(f64::total_cmp);
    result.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monotone(curve: Curve, expected_turns: usize) {
        curve.validate().unwrap();
        let turns = curve.progress_turning_times();
        assert_eq!(turns.len(), expected_turns, "{curve:?}: {turns:?}");
        let mut cuts = vec![0.];
        cuts.extend(turns);
        cuts.push(1.);
        for pair in cuts.windows(2) {
            let direction = curve.sample(pair[1]).progress - curve.sample(pair[0]).progress;
            for i in 1..1000 {
                let t = pair[0] + (pair[1] - pair[0]) * f64::from(i) / 1000.;
                assert!(curve.sample(t).velocity * direction >= -1e-9, "{curve:?}: {pair:?} at {t}");
            }
        }
    }

    #[test]
    fn progress_and_velocity_use_different_polynomials_including_nonuniform_x() {
        let shape = CurveShape::Cubic { control1: [0.1, 2.], control2: [0.9, -1.], start: 0., end: 1. };
        monotone(Curve { space: CurveSpace::Progress, shape }, 2);
        // Asymmetric endpoints preserve valid nonzero signed velocity area.
        let shape = CurveShape::Cubic { control1: [0.1, -2.], control2: [0.9, 2.], start: 1., end: -0.5 };
        monotone(Curve { space: CurveSpace::Velocity, shape }, 3);
    }

    #[test]
    fn elastic_partition_is_analytic_and_bounded_at_fractional_frequency() {
        for (oscillations, turns) in [(0.5, 0), (2.25, 4), (8., 15)] {
            for damping in [0.5, 20.] {
                for space in [CurveSpace::Progress, CurveSpace::Velocity] {
                    monotone(Curve { space, shape: CurveShape::Elastic { oscillations, damping } }, turns);
                }
            }
        }
    }

    #[test]
    fn root_isolation_keeps_close_turns_and_handles_repeated_and_constant_roots() {
        let (a, b, c) = (0.25, 0.250001, 0.75);
        let found = roots(&[-a*b*c, a*b+a*c+b*c, -a-b-c, 1.]);
        assert_eq!(found.len(), 3);
        for (got, expected) in found.iter().zip([a, b, c]) { assert!((got-expected).abs() < 1e-9); }
        assert_eq!(roots(&[0.25, -1., 1.]), vec![0.5]);
        assert!(roots(&[0.; 4]).is_empty());
        assert!(roots(&[1., 0., 0., 0.]).is_empty());
    }
}
