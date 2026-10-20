use super::super::input::Rect;
fn legacy_clip_triangles(points: &[[f32; 2]], area: Rect) -> Vec<[f32; 2]> {
    let mut result = Vec::with_capacity(points.len());
    if area.width() <= 0. || area.height() <= 0. {
        return result;
    }
    for triangle in points.chunks_exact(3) {
        if triangle.iter().all(|p| area.contains(*p)) {
            result.extend_from_slice(triangle);
            continue;
        }
        let mut polygon = triangle.to_vec();
        for (axis, bound, keep_greater) in [
            (0, area.min[0], true),
            (0, area.max[0], false),
            (1, area.min[1], true),
            (1, area.max[1], false),
        ] {
            let mut clipped = Vec::with_capacity(polygon.len() + 1);
            let Some(mut previous) = polygon.last().copied() else {
                break;
            };
            let inside = |p: [f32; 2]| {
                if keep_greater {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            for current in polygon {
                if inside(previous) != inside(current) {
                    let t = (bound - previous[axis]) / (current[axis] - previous[axis]);
                    let mut crossing = [
                        previous[0] + (current[0] - previous[0]) * t,
                        previous[1] + (current[1] - previous[1]) * t,
                    ];
                    crossing[axis] = bound;
                    clipped.push(crossing);
                }
                if inside(current) {
                    clipped.push(current);
                }
                previous = current;
            }
            polygon = clipped;
        }
        for i in 1..polygon.len().saturating_sub(1) {
            result.extend_from_slice(&[polygon[0], polygon[i], polygon[i + 1]]);
        }
    }
    result
}

#[test]
fn triangle_clipping_preserves_legacy_vertices_for_edges_and_random_crossings() {
    let mut seed = 0x129u32;
    let mut sample = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 80. - 30.
    };
    let mut inputs = vec![
        vec![[0., 0.], [10., 0.], [0., 10.]],
        vec![[0., 0.]; 3],
        vec![[10., -1.], [10., 11.], [10., 5.]],
        vec![[f32::NAN, 0.], [10., f32::INFINITY], [0., 10.]],
        vec![[2., 2.], [4., 2.]],
    ];
    for _ in 0..5000 {
        inputs.push((0..3).map(|_| [sample(), sample()]).collect());
    }
    for area in [
        Rect::new(0., 0., 10., 10.),
        Rect::new(2.25, 3.5, 0., 4.),
        Rect::new(-5.25, -2.5, 20.5, 30.75),
    ] {
        for input in &inputs {
            let actual = super::clip_triangles(input, area);
            let expected = legacy_clip_triangles(input, area);
            assert_eq!(actual.len(), expected.len(), "{input:?} {area:?}");
            for (a, b) in actual.iter().flatten().zip(expected.iter().flatten()) {
                assert_eq!(a.to_bits(), b.to_bits(), "{input:?} {area:?}");
            }
        }
    }
}
