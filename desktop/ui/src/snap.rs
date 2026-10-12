//! Snapping tolerances are expressed in screen pixels, independent of zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub value: f64,
    pub target: f64,
    pub distance_pixels: f64,
}
pub fn nearest(
    candidates: impl IntoIterator<Item = f64>,
    value: f64,
    pixels_per_unit: f64,
    tolerance_pixels: f64,
) -> Option<Snap> {
    if !value.is_finite()
        || !pixels_per_unit.is_finite()
        || pixels_per_unit <= 0.
        || !tolerance_pixels.is_finite()
        || tolerance_pixels < 0.
    {
        return None;
    }
    candidates
        .into_iter()
        .filter(|v| v.is_finite())
        .map(|target| Snap {
            value: target,
            target,
            distance_pixels: (target - value).abs() * pixels_per_unit,
        })
        .filter(|s| s.distance_pixels <= tolerance_pixels)
        .min_by(|a, b| {
            a.distance_pixels
                .total_cmp(&b.distance_pixels)
                .then(a.target.total_cmp(&b.target))
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screen_tolerance_stays_constant_and_ties_are_deterministic() {
        assert_eq!(nearest([10.], 9.3, 10., 8.).unwrap().value, 10.);
        assert!(nearest([10.], 9.3, 20., 8.).is_none());
        assert_eq!(nearest([12., 8.], 10., 2., 8.).unwrap().value, 8.);
        assert!(nearest([f64::NAN], 1., 10., 8.).is_none());
    }
}
