/// Fit a positive canvas into a positive viewport without rounding beyond its
/// bounds. GPU viewport validation rejects even a tiny negative origin.
pub fn fit_viewport([x, y, w, h]: [f32; 4], [cw, ch]: [u32; 2]) -> [f32; 4] {
    let scale = (f64::from(w) / f64::from(cw)).min(f64::from(h) / f64::from(ch));
    let width = ((f64::from(cw) * scale) as f32).min(w);
    let height = ((f64::from(ch) * scale) as f32).min(h);
    [x + (w - width) * 0.5, y + (h - height) * 0.5, width, height]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_preview_stays_inside_odd_sized_targets() {
        // 994 x 560 previously produced x = -0.000030517578 and width = 994.00006.
        for canvas in [[1920, 1080], [1080, 1920], [4096, 2160]] {
            for width in 1..=2048 {
                for height in [1, 279, 560, 721, 1080] {
                    let bounds = [width as f32, height as f32];
                    let [x, y, w, h] = fit_viewport([0., 0., bounds[0], bounds[1]], canvas);
                    assert!(x >= 0. && y >= 0. && w > 0. && h > 0.);
                    assert!(
                        x + w <= bounds[0] && y + h <= bounds[1],
                        "{canvas:?} {bounds:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn fitting_preserves_center_and_aspect_ratio() {
        assert_eq!(
            fit_viewport([10., 20., 100., 100.], [1920, 1080]),
            [10., 41.875, 100., 56.25]
        );
    }
}
