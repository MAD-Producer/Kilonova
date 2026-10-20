//! One pixel of geometry outside the original plane lets the fragment shader
//! integrate edge coverage even when a pixel center lies outside the rectangle.
use glam::{DMat4, DVec2, DVec4};

pub(crate) fn shader(source: &str, enabled: bool) -> String {
    let source = if enabled { source.to_owned() } else {
        source.replace("screen_uv:mat3x4<f32>,", "")
    };
    let coordinates = if enabled { include_str!("plane_coordinates.wgsl") } else {
        "struct PlaneCoordinates { uv:vec2<f32>, dx:vec2<f32>, dy:vec2<f32> };\nfn plane_coordinates(p:vec4<f32>,uv:vec2<f32>,dx:vec2<f32>,dy:vec2<f32>)->PlaneCoordinates{return PlaneCoordinates(uv,dx,dy);}"
    };
    format!(
        "const PLANE_ANTIALIAS:bool={enabled};\n{}\n{source}\n{coordinates}",
        include_str!("plane_coverage.wgsl")
    )
}

pub(crate) fn margin(matrix: DMat4, size: [f32; 2], offset: [f32; 2], viewport: [f32; 2]) -> DVec2 {
    let mut margin = DVec2::ZERO;
    let screen = DVec2::new(f64::from(viewport[0]), -f64::from(viewport[1])) * 0.5;
    let du = matrix.x_axis * f64::from(size[0]);
    let dv = -matrix.y_axis * f64::from(size[1]);
    for [u, v] in [[0., 0.], [0., 1.], [1., 1.], [1., 0.]] {
        let clip = matrix
            * DVec4::new(
                (u - 0.5) * f64::from(size[0]) + f64::from(offset[0]),
                (0.5 - v) * f64::from(size[1]) + f64::from(offset[1]),
                0.,
                1.,
            );
        // Near-plane crossings and singular projections keep ordinary hardware
        // clipping. Do not turn a degenerate plane into unbounded geometry.
        if !clip.is_finite() || clip.w <= 1.0e-8 {
            return DVec2::ZERO;
        }
        let derivative = |d: DVec4| {
            (DVec2::new(d.x, d.y) * clip.w - DVec2::new(clip.x, clip.y) * d.w) / (clip.w * clip.w)
                * screen
        };
        let x = derivative(du);
        let y = derivative(dv);
        let determinant = x.perp_dot(y);
        if !determinant.is_finite() || determinant.abs() < 1.0e-12 {
            return DVec2::ZERO;
        }
        let inverse_rows =
            DVec2::new(y.x.abs() + y.y.abs(), x.x.abs() + x.y.abs()) / determinant.abs();
        margin = margin.max(inverse_rows);
    }
    // Bound pathological subpixel/perspective cases; ordinary visible planes
    // require a very small fraction of one UV unit.
    let margin = margin.min(DVec2::splat(64.));
    for [u, v] in [
        [-margin.x, -margin.y],
        [-margin.x, 1. + margin.y],
        [1. + margin.x, -margin.y],
        [1. + margin.x, 1. + margin.y],
    ] {
        let w = matrix.x_axis.w * ((u - 0.5) * f64::from(size[0]) + f64::from(offset[0]))
            + matrix.y_axis.w * ((0.5 - v) * f64::from(size[1]) + f64::from(offset[1]))
            + matrix.w_axis.w;
        if !w.is_finite() || w <= 1.0e-8 {
            return DVec2::ZERO;
        }
    }
    margin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fringe_tracks_physical_pixels_and_stays_bounded_for_degenerate_projections() {
        let projection = DMat4::from_scale(glam::DVec3::new(2. / 256., 2. / 192., 1.));
        let regular = margin(projection, [144., 104.], [0.; 2], [256., 192.]);
        assert!((regular.x - 1. / 144.).abs() < 1.0e-12);
        assert!((regular.y - 1. / 104.).abs() < 1.0e-12);
        let high_dpi = margin(projection, [144., 104.], [0.; 2], [512., 384.]);
        assert!((high_dpi - regular * 0.5).length() < 1.0e-12);
        for matrix in [
            DMat4::ZERO,
            DMat4::from_scale(glam::DVec3::new(0., 1., 1.)),
            DMat4::from_cols(DVec4::X, DVec4::Y, DVec4::Z, -DVec4::W),
        ] {
            assert_eq!(
                margin(matrix, [144., 104.], [0.; 2], [256., 192.]),
                DVec2::ZERO
            );
        }
        let tiny = margin(projection, [1.0e-8; 2], [0.; 2], [256., 192.]);
        assert!(tiny.is_finite() && tiny.max_element() <= 64.);
    }
}
