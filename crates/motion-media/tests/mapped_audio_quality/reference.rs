//! Direct trigonometric FIR oracle, independent of the production recurrence.
pub fn sample(position: f64, rate: f64, frames: u64, mut read: impl FnMut(u64) -> f32) -> f32 {
    let fixed = (position * 1024.).round() as i64;
    let native = fixed.div_euclid(1024);
    let phase = fixed.rem_euclid(1024);
    if phase == 0 && rate.abs() <= 1. {
        return if native >= 0 && (native as u64) < frames {
            read(native as u64)
        } else {
            0.
        };
    }
    let cutoff = 1. / rate.abs().max(1.);
    let radius = if rate.abs() > 1. { 32. } else { 16. };
    let coefficients: [f64; 64] = std::array::from_fn(|tap| {
        let x = tap as f64 - 31. - phase as f64 / 1024.;
        if x.abs() >= radius {
            return 0.;
        }
        let pi_x = std::f64::consts::PI * x;
        let sinc = if x.abs() < 1e-12 {
            cutoff
        } else {
            (pi_x * cutoff).sin() / pi_x
        };
        sinc * (0.5 + 0.5 * (pi_x / radius).cos())
    });
    let sum = coefficients.iter().sum::<f64>();
    let mut value = 0f32;
    for (tap, weight) in coefficients.into_iter().enumerate() {
        let index = native + tap as i64 - 31;
        if index >= 0 && (index as u64) < frames {
            value += read(index as u64) * (weight / sum) as f32;
        }
    }
    value
}
