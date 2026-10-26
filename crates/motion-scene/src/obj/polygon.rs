//! Bounded dominant-plane ear clipping. Concave faces are not converted with a fan.
use crate::{invalid, Result};
use glam::{DVec2, DVec3};

pub(super) const MAX_CORNERS: usize = 256;
pub(super) struct Polygon {
    pub triangles: Vec<[usize; 3]>,
    pub area_normal: DVec3,
    pub nonplanar: bool,
}
pub(super) fn triangulate(points: &[[f32; 3]], line: usize, work: &mut usize) -> Result<Polygon> {
    let fail = |reason| invalid("obj_polygon", line, reason);
    if !(3..=MAX_CORNERS).contains(&points.len()) {
        return Err(fail("polygon corner count outside supported range"));
    }
    let p: Vec<_> = points
        .iter()
        .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
        .collect();
    let mut normal = DVec3::ZERO;
    for i in 0..p.len() {
        normal += (p[i] - p[0]).cross(p[(i + 1) % p.len()] - p[0]);
    }
    let extent = p
        .iter()
        .map(|&v| (v - p[0]).abs().max_element())
        .fold(0., f64::max);
    if extent == 0. || normal.length() <= extent * extent * 1e-12 {
        return Err(fail("degenerate or cancelling polygon"));
    }
    let direction = normal.normalize();
    let nonplanar = p
        .iter()
        .any(|&v| (v - p[0]).dot(direction).abs() > extent * 1e-6);
    let axis = normal.abs();
    let q: Vec<_> = p
        .iter()
        .map(|&v| {
            let v = v - p[0];
            if axis.x >= axis.y && axis.x >= axis.z {
                DVec2::new(v.y, v.z)
            } else if axis.y >= axis.z {
                DVec2::new(v.z, v.x)
            } else {
                DVec2::new(v.x, v.y)
            }
        })
        .collect();
    let epsilon = extent * extent * 1e-12;
    let area = (0..q.len())
        .map(|i| q[i].perp_dot(q[(i + 1) % q.len()]))
        .sum::<f64>();
    let sign = area.signum();
    if area.abs() <= epsilon {
        return Err(fail("degenerate projected polygon"));
    }
    // Reject self intersections before choosing any diagonal, including touching nonadjacent edges.
    for i in 0..q.len() {
        for j in i + 1..q.len() {
            if j == i + 1 || (i == 0 && j == q.len() - 1) {
                continue;
            }
            charge(work, line)?;
            if intersects(
                q[i],
                q[(i + 1) % q.len()],
                q[j],
                q[(j + 1) % q.len()],
                epsilon,
            ) {
                return Err(fail("self-intersecting polygon"));
            }
        }
    }
    let mut pending: Vec<_> = (0..q.len()).collect();
    let mut triangles = Vec::with_capacity(q.len() - 2);
    while pending.len() > 3 {
        let mut ear = None;
        for n in 0..pending.len() {
            charge(work, line)?;
            let [a, b, c] = [
                pending[(n + pending.len() - 1) % pending.len()],
                pending[n],
                pending[(n + 1) % pending.len()],
            ];
            if cross(q[a], q[b], q[c]) * sign <= epsilon {
                continue;
            }
            let mut inside = false;
            for &index in &pending {
                charge(work, line)?;
                if [a, b, c].contains(&index) {
                    continue;
                }
                if cross(q[a], q[b], q[index]) * sign >= -epsilon
                    && cross(q[b], q[c], q[index]) * sign >= -epsilon
                    && cross(q[c], q[a], q[index]) * sign >= -epsilon
                {
                    inside = true;
                    break;
                }
            }
            if !inside {
                ear = Some((n, [a, b, c]));
                break;
            }
        }
        let (index, triangle) =
            ear.ok_or_else(|| fail("polygon cannot be triangulated without degeneracy"))?;
        triangles.push(triangle);
        pending.remove(index);
    }
    if cross(q[pending[0]], q[pending[1]], q[pending[2]]) * sign <= epsilon {
        return Err(fail("degenerate final triangle"));
    }
    triangles.push([pending[0], pending[1], pending[2]]);
    Ok(Polygon {
        triangles,
        area_normal: normal,
        nonplanar,
    })
}
fn charge(work: &mut usize, line: usize) -> Result<()> {
    *work = work
        .checked_add(1)
        .filter(|&n| n <= 16_000_000)
        .ok_or_else(|| invalid("obj_polygon", line, "triangulation work limit exceeded"))?;
    Ok(())
}
fn cross(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a)
}
fn intersects(a: DVec2, b: DVec2, c: DVec2, d: DVec2, epsilon: f64) -> bool {
    let values = [
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    ];
    let opposite = |a: f64, b: f64| (a > epsilon && b < -epsilon) || (a < -epsilon && b > epsilon);
    if opposite(values[0], values[1]) && opposite(values[2], values[3]) {
        return true;
    }
    let on = |p: DVec2, x: DVec2, y: DVec2| {
        p.x >= x.x.min(y.x) && p.x <= x.x.max(y.x) && p.y >= x.y.min(y.y) && p.y <= x.y.max(y.y)
    };
    (values[0].abs() <= epsilon && on(c, a, b))
        || (values[1].abs() <= epsilon && on(d, a, b))
        || (values[2].abs() <= epsilon && on(a, c, d))
        || (values[3].abs() <= epsilon && on(b, c, d))
}
