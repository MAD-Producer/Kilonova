//! Fixed 64-tap interpolation, using the composed source derivative as cutoff.
//! Inline storage and a one-key cache bound memory even for changing clocks.
pub(super) struct Filter {
    key: Option<(u64, usize)>,
    weights: [f32; 64],
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            key: None,
            weights: [0.; 64],
        }
    }
}
impl Filter {
    pub(super) fn weights(&mut self, rate: f64, phase: usize) -> &[f32; 64] {
        let key = (rate.to_bits(), phase);
        if self.key != Some(key) {
            let cutoff = 1. / rate.max(1.);
            let radius = if rate > 1. { 32. } else { 16. };
            let fraction = phase as f64 / 1024.;
            let origin = -31. - fraction;
            let pi = std::f64::consts::PI;
            // Two rotations evaluate the sinc and Hann window. Avoid 128
            // transcendental calls per output sample when rate/phase changes.
            let (mut signal_sin, mut signal_cos) = (pi * cutoff * origin).sin_cos();
            let (signal_step_sin, signal_step_cos) = (pi * cutoff).sin_cos();
            let (mut window_sin, mut window_cos) = (pi * origin / radius).sin_cos();
            let (window_step_sin, window_step_cos) = (pi / radius).sin_cos();
            let mut coefficients = [0.; 64];
            let mut sum = 0.;
            for (tap, coefficient) in coefficients.iter_mut().enumerate() {
                let x = tap as f64 + origin;
                if x.abs() < radius {
                    let sinc = if x.abs() < 1e-12 {
                        cutoff
                    } else {
                        signal_sin / (pi * x)
                    };
                    *coefficient = sinc * (0.5 + 0.5 * window_cos);
                    sum += *coefficient;
                }
                (signal_sin, signal_cos) = (
                    signal_sin * signal_step_cos + signal_cos * signal_step_sin,
                    signal_cos * signal_step_cos - signal_sin * signal_step_sin,
                );
                (window_sin, window_cos) = (
                    window_sin * window_step_cos + window_cos * window_step_sin,
                    window_cos * window_step_cos - window_sin * window_step_sin,
                );
            }
            // Normalize in f64 before casting: finite large derivatives must
            // not underflow the entire unnormalized bank to f32 zero.
            self.weights = coefficients.map(|weight| (weight / sum) as f32);
            self.key = Some(key);
        }
        &self.weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurrence_matches_independent_windowed_sinc_and_keeps_extreme_rates_finite() {
        let mut filter = Filter::default();
        for rate in [0., 0.25, 1., 1.5, 2., 4., 8., 64., 1e300] {
            for phase in [0, 1, 127, 512, 1023, 0] {
                let actual = *filter.weights(rate, phase);
                let cutoff = 1. / rate.max(1.);
                let radius = if rate > 1. { 32. } else { 16. };
                let mut expected = [0.; 64];
                for (tap, value) in expected.iter_mut().enumerate() {
                    let x = tap as f64 - 31. - phase as f64 / 1024.;
                    if x.abs() < radius {
                        let a = std::f64::consts::PI * x;
                        let sinc = if x.abs() < 1e-12 {
                            cutoff
                        } else {
                            (a * cutoff).sin() / a
                        };
                        *value = sinc * (0.5 + 0.5 * (a / radius).cos());
                    }
                }
                let sum = expected.iter().sum::<f64>();
                for (a, e) in actual.iter().zip(expected) {
                    assert!(a.is_finite());
                    assert!(
                        (f64::from(*a) - e / sum).abs() < 3e-8,
                        "rate={rate} phase={phase}"
                    );
                }
                assert!((actual.iter().sum::<f32>() - 1.).abs() < 1e-6);
                assert_eq!(actual, *filter.weights(rate, phase));
            }
        }
    }
}
