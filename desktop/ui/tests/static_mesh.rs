use motion_ui::{
    paint::{self, Glyph, PaintList, Shape, StaticMesh},
    Color, Rect,
};

static TRIANGLES: [[[f32; 2]; 3]; 3] = [
    [[-4., 1.], [17., 2.], [6., 18.]],
    [[17., 2.], [-4., 1.], [6., 18.]],
    [[1., 1.], [2., 1.], [1., 2.]],
];

fn points(shape: &Shape) -> Vec<[f32; 2]> {
    match shape {
        Shape::Mesh(mesh) => mesh.points().collect(),
        Shape::Triangles { points } => points.clone(),
        Shape::Quad { points } => points.to_vec(),
        other => panic!("unexpected geometry: {other:?}"),
    }
}

fn transform(points: &mut [[f32; 2]], origin: [f32; 2], scale: f32) {
    for point in points {
        *point = [origin[0] + point[0] * scale, origin[1] + point[1] * scale];
    }
}

#[test]
fn deferred_and_fallback_transforms_keep_owned_coordinate_rounding() {
    for placement in [([0., 0.], 1.), ([7.125, -12.25], 0.75), ([1e6, -1e5], 0.1)] {
        for outer in [([19.25, 8.5], 1.75), ([-91.5, 121.25], 0.33)] {
            let mut expected: Vec<_> = TRIANGLES.iter().flatten().copied().collect();
            transform(&mut expected, placement.0, placement.1);
            let mesh = StaticMesh::new(&TRIANGLES, placement.0, placement.1);
            assert_eq!(mesh.points().collect::<Vec<_>>(), expected);
            let Shape::Mesh(mesh) = mesh.transformed(outer.0, outer.1) else {
                panic!("ordinary node transform must retain shared geometry")
            };
            transform(&mut expected, outer.0, outer.1);
            assert_eq!(mesh.points().collect::<Vec<_>>(), expected);
            let third = mesh.transformed([0.25, -0.75], 1.1);
            assert!(matches!(third, Shape::Triangles { .. }));
            transform(&mut expected, [0.25, -0.75], 1.1);
            assert_eq!(points(&third), expected);
        }
    }
}

#[test]
fn shared_mesh_clipping_matches_owned_triangles_and_preserves_draw_order() {
    for clips in [
        vec![Rect::new(-100., -100., 300., 300.)],
        vec![Rect::new(0., 0., 14., 12.)],
        vec![Rect::new(17., 2., 20., 20.)],
        vec![Rect::new(0., 0., 0., 20.)],
        vec![Rect::new(90., 90., 1., 1.)],
        vec![Rect::new(0., 0., 14., 12.), Rect::new(3., 4., 6., 6.)],
    ] {
        let mesh = StaticMesh::new(&TRIANGLES, [1.25, 2.5], 0.75);
        let mut expected_points: Vec<_> = TRIANGLES.iter().flatten().copied().collect();
        transform(&mut expected_points, [1.25, 2.5], 0.75);
        let mut actual = PaintList::default();
        let mut expected = PaintList::default();
        for list in [&mut actual, &mut expected] {
            paint::rect(list, Rect::new(1., 1., 2., 2.), Color::grey(80));
        }
        actual.push_shape(Shape::Mesh(mesh), Color::grey(220));
        expected.push_shape(
            Shape::Triangles {
                points: expected_points,
            },
            Color::grey(220),
        );
        for list in [&mut actual, &mut expected] {
            let mut child = list.child();
            child.push_glyph(Glyph {
                uv: [0., 0., 1., 1.],
                atlas_size: None,
                position: [4., 5.],
                size: [3., 4.],
                color: Color::grey(200),
            });
            paint::rect(&mut child, Rect::new(5., 5., 2., 2.), Color::grey(120));
            list.append(child);
        }
        for clip in clips {
            actual.clip_to(clip);
            expected.clip_to(clip);
            assert_eq!(actual.order, expected.order);
            assert_eq!(actual.colors, expected.colors);
            assert_eq!(actual.shapes.len(), expected.shapes.len());
            for (a, b) in actual.shapes.iter().zip(&expected.shapes) {
                assert_eq!(points(a), points(b));
            }
            assert_eq!(actual.glyphs.len(), expected.glyphs.len());
            for (a, b) in actual.glyphs.iter().zip(&expected.glyphs) {
                assert_eq!((a.position, a.size, a.uv), (b.position, b.size, b.uv));
            }
        }
    }
}

#[test]
fn empty_shared_meshes_disappear_without_stale_paint_indices() {
    let mut list = PaintList::default();
    list.push_shape(
        Shape::Mesh(StaticMesh::new(&[], [0.; 2], 1.)),
        Color::grey(220),
    );
    paint::rect(&mut list, Rect::new(1., 1., 2., 2.), Color::grey(80));
    list.clip_to(Rect::new(0., 0., 20., 20.));
    assert_eq!(list.shapes.len(), 1);
    assert_eq!(list.order, [paint::PaintOp::Shape(0)]);
    assert_eq!(list.colors, [Color::grey(80)]);
}
