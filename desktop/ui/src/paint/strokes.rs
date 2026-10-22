//! Connected stroke quads share their joins, including the AA fringe. Clipping
//! interpolates the distance coordinates, rather than moving the centerline.
use super::{Rect, SolidVertex};

fn direction(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    [d[0] / length, d[1] / length]
}
fn normal(d: [f32; 2]) -> [f32; 2] {
    [-d[1], d[0]]
}
fn join(before: [f32; 2], after: [f32; 2]) -> [f32; 2] {
    let a = normal(before);
    let b = normal(after);
    let divisor = 1. + before[0] * after[0] + before[1] * after[1];
    if divisor <= 0.0001 {
        return b;
    }
    let m = [(a[0] + b[0]) / divisor, (a[1] + b[1]) / divisor];
    let factor = (4. / m[0].hypot(m[1])).min(1.);
    [m[0] * factor, m[1] * factor]
}

fn finite(vertex: &SolidVertex) -> bool {
    vertex
        .position
        .iter()
        .chain(vertex.geometry.iter())
        .chain(vertex.params.iter())
        .all(|x| x.is_finite())
}

pub(super) fn append(
    out: &mut Vec<SolidVertex>,
    points: &[[f32; 2]],
    color: [f32; 4],
    scale: [f32; 2],
    clip: Option<Rect>,
    width: f32,
) {
    if points.len() < 2 || !width.is_finite() || width <= 0. {
        return;
    }
    let fringe = 1. / scale[0].min(scale[1]).max(0.001);
    let half = width * 0.5;
    let extent = half + fringe;
    let closed = points.len() > 2 && points.first() == points.last();
    for (i, pair) in points.windows(2).enumerate() {
        let [a, b] = [pair[0], pair[1]];
        if (a[0] - b[0]).hypot(a[1] - b[1]) <= f32::EPSILON {
            continue;
        }
        let d = direction(a, b);
        let n = normal(d);
        let first = i == 0;
        let last = i == points.len() - 2;
        let start = if first && !closed {
            n
        } else {
            let previous = if first {
                points[points.len() - 2]
            } else {
                points[i - 1]
            };
            join(direction(previous, a), d)
        };
        let end = if last && !closed {
            n
        } else {
            let next = if last { points[1] } else { points[i + 2] };
            join(d, direction(b, next))
        };
        let start_cap = first && !closed;
        let end_cap = last && !closed;
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let caps = u8::from(start_cap) + 2 * u8::from(end_cap);
        let positions = [
            (a, start, 1., -f32::from(u8::from(start_cap)) * fringe),
            (b, end, 1., f32::from(u8::from(end_cap)) * fringe),
            (b, end, -1., f32::from(u8::from(end_cap)) * fringe),
            (a, start, -1., -f32::from(u8::from(start_cap)) * fringe),
        ];
        let quad = positions.map(|(p, m, side, along)| {
            let position = [
                p[0] + m[0] * extent * side + d[0] * along,
                p[1] + m[1] * extent * side + d[1] * along,
            ];
            let delta = [position[0] - a[0], position[1] - a[1]];
            let mut vertex = SolidVertex::plain(position, color);
            vertex.geometry = [
                delta[0] * n[0] + delta[1] * n[1],
                delta[0] * d[0] + delta[1] * d[1],
                0.,
                0.,
            ];
            vertex.params = [5., half, length, f32::from(caps)];
            vertex
        });
        // Finite API inputs can still overflow while expanding an extreme
        // segment or miter. Never send non-finite positions/distances to wgpu.
        if quad.iter().all(finite) {
            append_quad(out, quad, clip);
        }
    }
}

fn append_quad(out: &mut Vec<SolidVertex>, quad: [SolidVertex; 4], clip: Option<Rect>) {
    let inside = |p: [f32; 2], r: Rect| {
        p[0] >= r.min[0] && p[0] <= r.max[0] && p[1] >= r.min[1] && p[1] <= r.max[1]
    };
    if clip.is_none_or(|r| quad.iter().all(|v| inside(v.position, r))) {
        out.extend_from_slice(&[quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
        return;
    }
    // Very short/reversing segments can fold their join quad. Clip each of its
    // original convex triangles: triangle/rectangle intersection has at most
    // seven vertices. Two stack buffers avoid per-segment heap allocations.
    let clip = clip.unwrap();
    for triangle in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
        let mut polygon = [triangle[0]; 8];
        polygon[..3].copy_from_slice(&triangle);
        let mut count = 3;
        for (axis, bound, greater) in [
            (0, clip.min[0], true),
            (0, clip.max[0], false),
            (1, clip.min[1], true),
            (1, clip.max[1], false),
        ] {
            if count == 0 {
                break;
            }
            let mut next = [triangle[0]; 8];
            let mut written = 0;
            let inside = |v: SolidVertex| {
                if greater {
                    v.position[axis] >= bound
                } else {
                    v.position[axis] <= bound
                }
            };
            let mut previous = polygon[count - 1];
            for current in polygon[..count].iter().copied() {
                if inside(previous) != inside(current) {
                    let t = (bound - previous.position[axis])
                        / (current.position[axis] - previous.position[axis]);
                    let mut crossing = previous;
                    for a in 0..2 {
                        crossing.position[a] += (current.position[a] - previous.position[a]) * t;
                    }
                    for a in 0..4 {
                        crossing.geometry[a] += (current.geometry[a] - previous.geometry[a]) * t;
                    }
                    crossing.position[axis] = bound;
                    next[written] = crossing;
                    written += 1;
                }
                if inside(current) {
                    next[written] = current;
                    written += 1;
                }
                previous = current;
            }
            polygon = next;
            count = written;
        }
        // Clipping arithmetic can also overflow for extreme finite inputs.
        if !polygon[..count].iter().all(finite) {
            continue;
        }
        for i in 1..count.saturating_sub(1) {
            out.extend_from_slice(&[polygon[0], polygon[i], polygon[i + 1]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflowing_inputs_never_emit_non_finite_vertices() {
        for width in [0., f32::NAN, f32::INFINITY, 1., f32::MAX] {
            for points in [
                vec![[-f32::MAX, 0.], [f32::MAX, 0.]],
                vec![[0., 0.], [1., 0.], [0., 0.001]],
            ] {
                for clip in [None, Some(Rect::new(0., 0., 16., 16.))] {
                    let mut out = vec![];
                    append(&mut out, &points, [1.; 4], [1.; 2], clip, width);
                    assert!(out.iter().all(|v| v
                        .position
                        .iter()
                        .chain(v.geometry.iter())
                        .chain(v.params.iter())
                        .all(|x| x.is_finite())));
                }
            }
        }
    }
    #[test]
    fn short_reversing_and_folded_joins_clip_without_panicking_or_losing_distance_values() {
        let mut seed = 0x1234abcd_u32;
        for _ in 0..4096 {
            let points: Vec<_> = (0..6)
                .map(|_| {
                    std::array::from_fn(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        (seed >> 8) as f32 / 16777216. * 12. - 3.
                    })
                })
                .collect();
            let mut vertices = vec![];
            let clip = Rect::new(0., 0., 5., 5.);
            append(&mut vertices, &points, [1.; 4], [1.; 2], Some(clip), 1.);
            assert!(vertices.len() <= 5 * 30);
            for v in vertices {
                assert!(v
                    .position
                    .iter()
                    .chain(v.geometry.iter())
                    .all(|x| x.is_finite()));
                assert!(
                    v.position[0] >= 0.
                        && v.position[0] <= 5.
                        && v.position[1] >= 0.
                        && v.position[1] <= 5.
                );
            }
        }
    }
}
