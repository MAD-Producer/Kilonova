//! Deterministic, bounded geometric tangents. Temporal interpolation remains
//! with the authored position tracks; node ordering is never inferred here.
use super::MAX_NODES;
use crate::{ensure, Result};
use glam::DVec2;

/// Replace relative handles using sampled positions, preserving point order.
/// Reject invalid input before modifying any node. Repeated points are normal:
/// a node adjacent to a zero-length edge receives zero handles.
pub fn apply(nodes: &mut [[f32; 6]], closed: bool) -> Result<()> {
    ensure(
        nodes.len() <= MAX_NODES,
        "automatic path tangents: node limit exceeded",
    )?;
    ensure(
        !closed || nodes.len() >= 3,
        "automatic path tangents: closed path requires at least three nodes",
    )?;
    for (index, node) in nodes.iter().enumerate() {
        if !node.iter().all(|v| v.is_finite() && v.abs() <= 32768.) {
            return Err(crate::Error::Invalid(format!(
                "automatic path tangents: invalid geometry node={index}"
            )));
        }
    }
    let count = nodes.len();
    let position = |node: &[f32; 6]| DVec2::new(f64::from(node[0]), f64::from(node[1]));
    for index in 0..count {
        let point = position(&nodes[index]);
        let previous = if index > 0 {
            Some(position(&nodes[index - 1]))
        } else {
            closed.then(|| position(&nodes[count - 1]))
        };
        let next = if index + 1 < count {
            Some(position(&nodes[index + 1]))
        } else {
            closed.then(|| position(&nodes[0]))
        };
        let (incoming, outgoing) = match (previous, next) {
            (None, Some(next)) => (DVec2::ZERO, (next - point) / 3.),
            (Some(previous), None) => ((previous - point) / 3., DVec2::ZERO),
            (Some(previous), Some(next)) => {
                let before = point.distance(previous);
                let after = point.distance(next);
                if before == 0. || after == 0. {
                    (DVec2::ZERO, DVec2::ZERO)
                } else {
                    let a = before.sqrt();
                    let b = after.sqrt();
                    let tangent =
                        (point - previous) / a - (next - previous) / (a + b) + (next - point) / b;
                    let length = tangent.length();
                    if length == 0. {
                        (DVec2::ZERO, DVec2::ZERO)
                    } else {
                        let direction = tangent / length;
                        (
                            -direction * (length * a / 3.).min(before / 3.),
                            direction * (length * b / 3.).min(after / 3.),
                        )
                    }
                }
            }
            (None, None) => (DVec2::ZERO, DVec2::ZERO),
        };
        nodes[index][2..].copy_from_slice(&[
            incoming.x as f32,
            incoming.y as f32,
            outgoing.x as f32,
            outgoing.y as f32,
        ]);
    }
    Ok(())
}
