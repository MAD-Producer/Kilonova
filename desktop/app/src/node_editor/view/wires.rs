//! Connection hit geometry and non-mutating feedback share the painted Bezier.
use super::*;

pub(super) fn wire_curve(cards: &[Card], edge: &Connection) -> Vec<[f32; 2]> {
    let find = |port: &PortRef, output| {
        cards
            .iter()
            .flat_map(|c| &c.sockets)
            .find(|s| s.output == output && &s.port == port)
    };
    find(&edge.from, true)
        .zip(find(&edge.to, false))
        .map(|(a, b)| curve(a.point, b.point))
        .unwrap_or_default()
}

pub(super) fn clip_segment(a: [f32; 2], b: [f32; 2], area: Rect) -> Option<([f32; 2], [f32; 2])> {
    if !a.iter().chain(b.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let d = [b[0] - a[0], b[1] - a[1]];
    let mut lo = 0f32;
    let mut hi = 1f32;
    for (p, q) in [
        (-d[0], a[0] - area.min[0]),
        (d[0], area.max[0] - a[0]),
        (-d[1], a[1] - area.min[1]),
        (d[1], area.max[1] - a[1]),
    ] {
        if p.abs() < 1e-6 {
            if q < 0. {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0. {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
        if lo > hi {
            return None;
        }
    }
    Some((
        [a[0] + d[0] * lo, a[1] + d[1] * lo],
        [a[0] + d[0] * hi, a[1] + d[1] * hi],
    ))
}

fn near_segments(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2], tolerance: f32) -> bool {
    let cross = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let overlap = (0..2).all(|axis| {
        a[axis].min(b[axis]) <= c[axis].max(d[axis]) && c[axis].min(d[axis]) <= a[axis].max(b[axis])
    });
    (overlap && cross(a, b, c) * cross(a, b, d) <= 0. && cross(c, d, a) * cross(c, d, b) <= 0.)
        || [
            distance_segment(a, c, d),
            distance_segment(b, c, d),
            distance_segment(c, a, b),
            distance_segment(d, a, b),
        ]
        .into_iter()
        .any(|distance| distance <= tolerance)
}

pub(super) fn reason(error: &Diagnostic, language: Language) -> &str {
    match error.code {
        ErrorCode::Cycle => language.text("这条连接会形成循环", "This connection creates a cycle"),
        ErrorCode::Budget => language.text("连线数量已达上限", "Connection budget reached"),
        ErrorCode::TypeMismatch => {
            language.text("端口类型或单位不匹配", "Port types or units do not match")
        }
        ErrorCode::UnknownPort => language.text(
            "请选择相反方向的端口",
            "Choose a port in the opposite direction",
        ),
        _ => language.text("无法连接这个端口", "Cannot connect this port"),
    }
}

impl NodeEditor {
    pub(super) fn wire_at(&self, cards: &[Card], canvas: Rect, point: [f32; 2]) -> Option<PortRef> {
        self.document
            .connections
            .iter()
            .rev()
            .find(|edge| {
                wire_curve(cards, edge).windows(2).any(|p| {
                    clip_segment(p[0], p[1], canvas)
                        .is_some_and(|(a, b)| distance_segment(point, a, b) <= 6.)
                })
            })
            .map(|edge| edge.to.clone())
    }
    pub(super) fn collect_cut_inputs(
        &self,
        cards: &[Card],
        canvas: Rect,
        a: [f32; 2],
        b: [f32; 2],
        hits: &mut BTreeSet<PortRef>,
    ) {
        let Some((a, b)) = clip_segment(a, b, canvas) else {
            return;
        };
        for edge in &self.document.connections {
            if wire_curve(cards, edge).windows(2).any(|p| {
                clip_segment(p[0], p[1], canvas).is_some_and(|(c, d)| near_segments(a, b, c, d, 3.))
            }) {
                hits.insert(edge.to.clone());
            }
        }
    }
    pub(super) fn wire_feedback<'a>(
        &self,
        cards: &'a [Card],
        canvas: Rect,
        point: [f32; 2],
    ) -> Option<(&'a Socket, Result<(), Diagnostic>)> {
        if !canvas.contains(point) {
            return None;
        }
        let hovered = cards
            .iter()
            .rev()
            .flat_map(|c| &c.sockets)
            .filter(|socket| socket.connectable)
            .find(|s| (s.point[0] - point[0]).abs() <= 8. && (s.point[1] - point[1]).abs() <= 8.)?;
        let (from, to, old) = match &self.drag {
            Some(Drag::Wire { source, old_input }) if &hovered.port != source => {
                if hovered.output {
                    return Some((
                        hovered,
                        Err(diag(
                            ErrorCode::UnknownPort,
                            None,
                            None,
                            "Connect to an input port",
                        )),
                    ));
                }
                (source.clone(), hovered.port.clone(), old_input.as_ref())
            }
            Some(Drag::WireInput { target }) if &hovered.port != target => {
                if !hovered.output {
                    return Some((
                        hovered,
                        Err(diag(
                            ErrorCode::UnknownPort,
                            None,
                            None,
                            "Connect to an output port",
                        )),
                    ));
                }
                (hovered.port.clone(), target.clone(), None)
            }
            _ => return None,
        };
        Some((
            hovered,
            self.candidate_connections(from, to, old).map(|_| ()),
        ))
    }
}
