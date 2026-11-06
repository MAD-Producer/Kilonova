//! Independent coordinate controls for reference-depth registration.
use motion_model::depth::DepthGeometry;

fn geometry(turns: u8, mirror_x: bool) -> DepthGeometry {
    DepthGeometry {
        source_size: [12, 10],
        crop: [2, 3, 8, 4],
        quarter_turns: turns,
        mirror_x,
    }
}

fn close(actual: [f64; 2], expected: [f64; 2]) {
    for i in 0..2 {
        assert!(
            (actual[i] - expected[i]).abs() < 1e-14,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn oriented_depth_centres_map_to_independently_known_source_pixels() {
    // The source point (5,4) is (3/8,1/4) inside this asymmetric crop.
    // Expected oriented coordinates are specified independently of either API.
    for (turns, oriented) in [
        (0, [3. / 8., 1. / 4.]),
        (1, [3. / 4., 3. / 8.]),
        (2, [5. / 8., 3. / 4.]),
        (3, [1. / 4., 5. / 8.]),
    ] {
        for mirror in [false, true] {
            let g = geometry(turns, mirror);
            let uv = [
                if mirror {
                    1. - oriented[0]
                } else {
                    oriented[0]
                },
                oriented[1],
            ];
            close(
                g.depth_uv_to_source_uv(uv).unwrap().unwrap(),
                [5. / 12., 4. / 10.],
            );
            close(
                g.source_uv_to_depth_uv([5. / 12., 4. / 10.])
                    .unwrap()
                    .unwrap(),
                uv,
            );
        }
    }
}

#[test]
fn every_crop_corner_and_fractional_centre_survives_the_inverse_without_clamping() {
    for turns in 0..4 {
        for mirror in [false, true] {
            let g = geometry(turns, mirror);
            for uv in [
                [0., 0.],
                [1., 0.],
                [1., 1.],
                [0., 1.],
                [0.5, 0.5],
                [0.125, 0.875],
            ] {
                let source = g.depth_uv_to_source_uv(uv).unwrap().unwrap();
                close(g.source_uv_to_depth_uv(source).unwrap().unwrap(), uv);
                assert!((2. / 12. ..=10. / 12.).contains(&source[0]));
                assert!((3. / 10. ..=7. / 10.).contains(&source[1]));
            }
            for uv in [
                [-f64::EPSILON, 0.5],
                [1. + f64::EPSILON, 0.5],
                [0.5, -0.1],
                [0.5, 1.1],
            ] {
                assert_eq!(g.depth_uv_to_source_uv(uv).unwrap(), None);
            }
        }
    }
}

#[test]
fn nonfinite_coordinates_and_invalid_geometry_keep_explicit_causes() {
    let g = geometry(1, true);
    for uv in [
        [f64::NAN, 0.5],
        [0.5, f64::INFINITY],
        [f64::NEG_INFINITY, 0.5],
    ] {
        assert!(g
            .depth_uv_to_source_uv(uv)
            .unwrap_err()
            .to_string()
            .contains("nonfinite depth UV"));
    }
    let mut bad_crop = g.clone();
    bad_crop.crop[2] = 11;
    assert!(bad_crop
        .depth_uv_to_source_uv([0.5; 2])
        .unwrap_err()
        .to_string()
        .contains("crop exceeds source bounds"));
    let mut bad_turn = g;
    bad_turn.quarter_turns = 4;
    assert!(bad_turn
        .depth_uv_to_source_uv([0.5; 2])
        .unwrap_err()
        .to_string()
        .contains("unsupported depth rotation"));
}
