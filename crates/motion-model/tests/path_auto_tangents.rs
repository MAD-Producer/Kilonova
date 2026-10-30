use motion_model::vector::auto_tangents;

fn point(x: f32, y: f32) -> [f32; 6] {
    [x, y, 0., 0., 0., 0.]
}

#[test]
fn straight_path_has_one_sided_endpoints_and_exact_thirds() {
    let mut nodes = [point(0., 0.), point(9., 0.), point(18., 0.)];
    auto_tangents::apply(&mut nodes, false).unwrap();
    assert_eq!(
        nodes,
        [
            [0., 0., 0., 0., 3., 0.],
            [9., 0., -3., 0., 3., 0.],
            [18., 0., -3., 0., 0., 0.]
        ]
    );
}

#[test]
fn closed_seam_rotation_preserves_geometry_and_order() {
    let nodes = vec![
        point(0., 0.),
        point(120., 0.),
        point(130., 10.),
        point(0., 90.),
    ];
    let mut expected = nodes.clone();
    auto_tangents::apply(&mut expected, true).unwrap();
    assert!(
        expected.iter().any(|n| n[2..].iter().any(|v| *v != 0.)),
        "negative control: untouched linear handles cannot pass"
    );
    for rotation in 0..nodes.len() {
        let mut actual = nodes.clone();
        actual.rotate_left(rotation);
        auto_tangents::apply(&mut actual, true).unwrap();
        actual.rotate_right(rotation);
        assert_eq!(actual, expected);
    }
}

#[test]
fn unequal_spacing_is_smooth_and_handles_cannot_exceed_adjacent_thirds() {
    let mut nodes = [point(-20., 0.), point(0., 0.), point(1., 10.)];
    auto_tangents::apply(&mut nodes, false).unwrap();
    let n = nodes[1];
    assert!((n[2] * n[5] - n[3] * n[4]).abs() < 0.00001);
    assert!(n[2] * n[4] + n[3] * n[5] < 0.);
    assert!(n[2].hypot(n[3]) <= 20. / 3. + 0.00001);
    assert!(n[4].hypot(n[5]) <= 101_f32.sqrt() / 3. + 0.00001);
    assert_eq!([n[0], n[1]], [0., 0.]);
}

#[test]
fn duplicates_cusps_empty_and_extreme_coordinates_are_finite() {
    for mut nodes in [
        vec![],
        vec![point(0., 0.)],
        vec![point(0., 0.); 3],
        vec![point(0., 0.), point(1., 0.), point(0., 0.)],
        vec![
            point(-32768., -32768.),
            point(32768., 32768.),
            point(0., 0.),
        ],
    ] {
        auto_tangents::apply(&mut nodes, false).unwrap();
        assert!(nodes
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 32768.));
    }
    let mut duplicate = [point(0., 0.), point(0., 0.), point(1., 1.)];
    auto_tangents::apply(&mut duplicate, false).unwrap();
    assert_eq!(duplicate[1][2..], [0.; 4]);
}

#[test]
fn invalid_geometry_and_topology_are_rejected_before_any_mutation() {
    for bad in [f32::NAN, f32::INFINITY, 32769.] {
        let mut nodes = [point(0., 0.), point(2., 3.), point(4., bad)];
        let before: Vec<_> = nodes.iter().flatten().map(|v| v.to_bits()).collect();
        let cause = auto_tangents::apply(&mut nodes, false)
            .unwrap_err()
            .to_string();
        assert!(cause.contains("automatic path tangents") && cause.contains("node=2"));
        assert_eq!(
            nodes
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            before
        );
    }
    let mut nodes = [point(0., 0.), point(2., 3.)];
    let before = nodes;
    assert!(auto_tangents::apply(&mut nodes, true).is_err());
    assert_eq!(nodes, before);
}

#[test]
fn maximum_path_is_bounded_and_reversing_order_swaps_handles() {
    let original: Vec<_> = (0..motion_model::vector::MAX_NODES)
        .map(|i| {
            let angle = i as f32 * 0.01;
            point(angle.cos() * 100., angle.sin() * 100.)
        })
        .collect();
    let mut forward = original.clone();
    auto_tangents::apply(&mut forward, true).unwrap();
    let mut reverse = original.clone();
    reverse.reverse();
    auto_tangents::apply(&mut reverse, true).unwrap();
    reverse.reverse();
    for (a, b) in forward.iter().zip(&reverse) {
        assert_eq!(a[..2], b[..2]);
        for (a, b) in a[2..4]
            .iter()
            .chain(&a[4..])
            .zip(b[4..].iter().chain(&b[2..4]))
        {
            assert!((a - b).abs() < 0.00001);
        }
    }
    let mut oversized = original;
    oversized.push(point(0., 0.));
    let before = oversized.clone();
    assert!(auto_tangents::apply(&mut oversized, false)
        .unwrap_err()
        .to_string()
        .contains("node limit"));
    assert_eq!(oversized, before);
}
