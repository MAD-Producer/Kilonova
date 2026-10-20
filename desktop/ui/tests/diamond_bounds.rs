use motion_ui::{
    paint::{self, PaintList, PaintOp, Shape},
    Color, Rect,
};

fn assert_rejected(center: [f32; 2], radius: f32, color: Color) {
    let mut list = PaintList::default();
    paint::rect(&mut list, Rect::new(1., 2., 3., 4.), Color::grey(80));
    paint::diamond(&mut list, center, radius, color);
    assert_eq!(
        list.shapes.len(),
        1,
        "center={center:?}, radius={radius}, color={color:?}"
    );
    assert_eq!(list.colors.len(), 1);
    assert_eq!(list.order, [PaintOp::Shape(0)]);
    // A rejected command must neither erase preceding work nor prevent recovery.
    paint::diamond(&mut list, [10.25, 12.5], 3., Color::grey(220));
    assert_eq!(list.shapes.len(), 2);
    assert_eq!(list.colors.len(), 2);
    assert_eq!(list.order, [PaintOp::Shape(0), PaintOp::Shape(1)]);
    assert!(matches!(list.shapes[1], Shape::Procedural { .. }));
}

#[test]
fn diamonds_reject_nonfinite_centers_and_color_components() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..2 {
            let mut center = [10., 12.];
            center[axis] = invalid;
            assert_rejected(center, 3., Color::grey(220));
        }
        for component in 0..4 {
            let mut color = Color::grey(220);
            color.0[component] = invalid;
            assert_rejected([10., 12.], 3., color);
        }
    }
}

#[test]
fn diamonds_reject_overflowed_or_collapsed_derived_bounds() {
    for (center, radius) in [
        ([0., 0.], f32::MAX), // Diameter overflows despite finite inputs.
        ([f32::MAX * 0.9, 0.], f32::MAX * 0.2), // Right edge overflows.
        ([-f32::MAX * 0.9, 0.], f32::MAX * 0.2), // Left edge overflows.
        ([0., f32::MAX * 0.9], f32::MAX * 0.2),
        ([0., -f32::MAX * 0.9], f32::MAX * 0.2),
        ([f32::MAX, 12.], 3.), // Precision loses the entire positive width.
        ([10., -f32::MAX], 3.),
        ([10., 12.], f32::from_bits(1)),
    ] {
        assert_rejected(center, radius, Color::grey(220));
    }
}

#[test]
fn diamonds_keep_existing_empty_input_behavior() {
    for radius in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_rejected([10., 12.], radius, Color::grey(220));
    }
    for alpha in [0., -0.5] {
        assert_rejected([10., 12.], 3., Color::rgba(200, 100, 40, alpha));
    }
}

#[test]
fn valid_diamonds_preserve_analytic_bounds_and_antialiasing_fringe() {
    for center in [[10., 12.], [10.25, 12.4], [-2.25, -4.5]] {
        for radius in [0.125, 1., 3., 10.5] {
            let mut list = PaintList::default();
            let color = Color::rgba(200, 100, 40, 0.6);
            paint::diamond(&mut list, center, radius, color);
            let Shape::Procedural {
                area,
                clip,
                kind: paint::Pattern::Diamond,
            } = list.shapes[0]
            else {
                panic!("a valid diamond must retain its analytic representation");
            };
            let [x, y] = center;
            assert_eq!(
                area,
                Rect::new(x - radius, y - radius, 2. * radius, 2. * radius)
            );
            assert_eq!(
                clip,
                Rect::new(
                    x - radius - 1.,
                    y - radius - 1.,
                    2. * radius + 2.,
                    2. * radius + 2.
                )
            );
            assert_eq!(list.colors[0].0, color.0);
            assert_eq!(list.order, [PaintOp::Shape(0)]);
        }
    }
}
