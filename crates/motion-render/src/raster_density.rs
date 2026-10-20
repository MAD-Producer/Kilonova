//! Source pixels per local vector pixel, including parents and perspective.
//! Native mode preserves the legacy raster. Continuous mode resolves authored
//! geometry at its projected output density; allocation limits remain explicit.
pub(crate) fn vector_scale(
    layer: &crate::DrawLayer,
    width: u32,
    height: u32,
    native: f32,
) -> Result<f32, String> {
    if !layer.sampling.continuous_rasterization {
        return Ok(native);
    }
    let matrix = layer.view_projection * layer.model;
    let points = [[0., 0.], [-0.5, -0.5], [0.5, -0.5], [-0.5, 0.5], [0.5, 0.5]]
        .map(|[x, y]| matrix * glam::Vec4::new(x * layer.size[0], y * layer.size[1], 0., 1.));
    // Fully clipped sources need no density increase. This is ordinary culling.
    if points.iter().all(|p| p.is_finite() && p.w <= 0.) {
        return Ok(native);
    }
    let mut density = native;
    for point in points {
        if !point.is_finite() || point.w <= f32::EPSILON {
            return Err(format!(
                "layer {}: continuous vector raster crosses or is behind the camera plane",
                layer.id
            ));
        }
        for axis in [matrix.x_axis, matrix.y_axis] {
            let derivative =
                (axis.truncate() * point.w - point.truncate() * axis.w) / point.w.powi(2);
            let screen = glam::Vec2::new(
                derivative.x * width as f32 * 0.5,
                derivative.y * height as f32 * 0.5,
            );
            if !screen.is_finite() {
                return Err(format!(
                    "layer {}: nonfinite continuous vector raster derivative",
                    layer.id
                ));
            }
            density = density.max(screen.length());
        }
    }
    if !density.is_finite() {
        return Err(format!(
            "layer {}: nonfinite continuous vector raster density",
            layer.id
        ));
    }
    Ok(density)
}
