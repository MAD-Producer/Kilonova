//! Canonical source-plane UVs, independent of BSP triangle tessellation.
use glam::{DMat3, DMat4, DVec3, DVec4};

pub(crate) fn clip_map(matrix: DMat4, size: [f32; 2], offset: [f32; 2]) -> DMat3 {
    let u = matrix.x_axis * f64::from(size[0]);
    let v = -matrix.y_axis * f64::from(size[1]);
    let origin = matrix
        * DVec4::new(
            f64::from(offset[0]) - f64::from(size[0]) * 0.5,
            f64::from(offset[1]) + f64::from(size[1]) * 0.5,
            0.,
            1.,
        );
    let xyw = |p: DVec4| DVec3::new(p.x, p.y, p.w);
    DMat3::from_cols(xyw(u), xyw(v), xyw(origin))
}

pub(crate) fn screen_map(
    clip: DMat3,
    viewport: [f32; 4],
    object: u64,
) -> crate::renderer::Result<[[f32; 4]; 3]> {
    let [x, y, width, height] = viewport.map(f64::from);
    let screen = DMat3::from_cols(
        DVec3::new(width * 0.5, 0., 0.),
        DVec3::new(0., -height * 0.5, 0.),
        DVec3::new(x + width * 0.5, y + height * 0.5, 1.),
    ) * clip;
    // Collapsed/edge-on geometry has no invertible visible source plane.
    if screen.determinant() == 0. {
        return Ok([[0.; 4]; 3]);
    }
    let inverse = screen.inverse();
    if !inverse.is_finite() {
        return Err(crate::RenderError::Invalid(format!(
            "object={object} phase=plane_coordinates: nonfinite inverse projection"
        )));
    }
    let rows = inverse.transpose().to_cols_array_2d();
    // Homogeneous coordinates allow common normalization without changing UV.
    let scale = rows.iter().flatten().fold(0f64, |n, v| n.max(v.abs()));
    let mapped = rows.map(|r| {
        [
            (r[0] / scale) as f32,
            (r[1] / scale) as f32,
            (r[2] / scale) as f32,
            1.,
        ]
    });
    if mapped[2][..3].iter().all(|v| *v == 0.) {
        return Err(crate::RenderError::Invalid(format!(
            "object={object} phase=plane_coordinates: projection exceeds GPU coordinate range"
        )));
    }
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screen_coordinates_roundtrip_perspective_letterboxing_and_asymmetric_bounds() {
        let projection = DMat4::perspective_rh(0.9, 1.4, 0.5, 10000.)
            * DMat4::look_at_rh(DVec3::new(40., 15., 350.), DVec3::ZERO, DVec3::Y);
        let model = DMat4::from_rotation_y(0.4);
        let size = [144., 96.];
        let offset = [-12., 7.];
        let clip = clip_map(projection * model, size, offset);
        let viewport = [13., 27., 620., 440.];
        let map = screen_map(clip, viewport, 7).unwrap();
        for [u, v] in [[0., 0.], [1., 1.], [0.125, 0.67], [-0.01, 1.01]] {
            // Independently project the authored local point; do not use clip_map.
            let point = projection
                * model
                * DVec4::new((u - 0.5) * 144. - 12., (0.5 - v) * 96. + 7., 0., 1.);
            let x = (13. + (point.x / point.w + 1.) * 310.) as f32;
            let y = (27. + (1. - point.y / point.w) * 220.) as f32;
            let n = map.map(|row| row[0] * x + row[1] * y + row[2]);
            assert!((f64::from(n[0] / n[2]) - u).abs() < 1e-5);
            assert!((f64::from(n[1] / n[2]) - v).abs() < 1e-5);
        }
        assert_eq!(screen_map(DMat3::ZERO, viewport, 7).unwrap(), [[0.; 4]; 3]);
        let error =
            screen_map(DMat3::from_diagonal(DVec3::splat(f64::NAN)), viewport, 7).unwrap_err();
        assert!(error
            .to_string()
            .contains("object=7 phase=plane_coordinates: nonfinite inverse projection"));
        // Finite CPU projection that cannot preserve reciprocal W in f32.
        let error = screen_map(
            DMat3::from_diagonal(DVec3::new(1e-100, 1., 1.)),
            viewport,
            7,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("projection exceeds GPU coordinate range"));
        assert!(
            screen_map(clip, viewport, 7).is_ok(),
            "a refusal must not poison later maps"
        );
    }
}
