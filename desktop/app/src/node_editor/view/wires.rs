//! Connection hit geometry and non-mutating feedback share the painted Bezier.
use super::*;

// Preserve the painted socket geometry and the old 9px hover area. Picking
// never becomes smaller than either that area or the visible outer diamond.
pub(super) fn socket_radius(zoom: f32) -> f32 {
    (4.5 * zoom).max(2.5)
}
pub(super) fn socket_at(
    cards: &[Card],
    canvas: Rect,
    point: [f32; 2],
    zoom: f32,
) -> Option<&Socket> {
    if !point.iter().all(|v| v.is_finite()) || !canvas.contains(point) {
        return None;
    }
    let visible = socket_radius(zoom) + 1.;
    let radius = 9f32.max(visible);
    // An opaque front card blocks sockets belonging to cards underneath it.
    let front = cards
        .iter()
        .rposition(|card| card.rect.contains(point))
        .unwrap_or(0);
    cards
        .iter()
        .enumerate()
        .skip(front)
        .flat_map(|(z, card)| card.sockets.iter().map(move |socket| (z, socket)))
        .filter(|(_, socket)| {
            socket.connectable
                && intersects(
                    canvas,
                    Rect::new(
                        socket.point[0] - visible,
                        socket.point[1] - visible,
                        visible * 2.,
                        visible * 2.,
                    ),
                )
                && (socket.point[0] - point[0]).abs() <= radius
                && (socket.point[1] - point[1]).abs() <= radius
        })
        .min_by(|(za, a), (zb, b)| {
            distance(a.point, point)
                .total_cmp(&distance(b.point, point))
                .then_with(|| zb.cmp(za))
        })
        .map(|(_, socket)| socket)
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

pub(super) struct WireOffer<'a> {
    pub card: &'a Card,
    pub targets: Vec<&'a Socket>,
    pub focus: Option<&'a Socket>,
    pub error: Option<Diagnostic>,
    pub direct: bool,
}
impl<'a> WireOffer<'a> {
    #[cfg(test)]
    pub fn feedback(&self) -> Option<(&'a Socket, Result<(), Diagnostic>)> {
        Some((self.focus?, self.error.clone().map_or(Ok(()), Err)))
    }
}

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
    pub(super) fn wire_candidate(&self, socket: &Socket) -> Result<Connection, Diagnostic> {
        let (from, to, old) = match &self.drag {
            Some(Drag::Wire { source, old_input }) if !socket.output => {
                (source.clone(), socket.port.clone(), old_input.as_ref())
            }
            Some(Drag::WireInput { target }) if socket.output => {
                (socket.port.clone(), target.clone(), None)
            }
            _ => {
                return Err(diag(
                    ErrorCode::UnknownPort,
                    None,
                    None,
                    "Choose a port in the opposite direction",
                ))
            }
        };
        self.candidate_connections(from.clone(), to.clone(), old)?;
        Ok(Connection { from, to })
    }

    pub(super) fn wire_offer<'a>(
        &self,
        cards: &'a [Card],
        canvas: Rect,
        point: [f32; 2],
    ) -> Option<WireOffer<'a>> {
        if !canvas.contains(point) || !point.iter().all(|v| v.is_finite()) {
            return None;
        }
        let (anchor, wanted_output) = match &self.drag {
            Some(Drag::Wire { source, .. }) => (source, false),
            Some(Drag::WireInput { target }) => (target, true),
            _ => return None,
        };
        if let Some(socket) = socket_at(cards, canvas, point, self.view.zoom) {
            if &socket.port == anchor {
                return None;
            }
            let card = cards.iter().find(|card| card.id == socket.port.node)?;
            let error = self.wire_candidate(socket).err();
            return Some(WireOffer {
                card,
                targets: if error.is_none() {
                    vec![socket]
                } else {
                    vec![]
                },
                focus: Some(socket),
                error,
                direct: true,
            });
        }
        let card = cards.iter().rev().find(|card| card.rect.contains(point))?;
        let anchor_socket = cards
            .iter()
            .flat_map(|card| &card.sockets)
            .find(|socket| socket.port == *anchor && socket.output != wanted_output)?;
        let mut sockets: Vec<_> = card
            .sockets
            .iter()
            .filter(|socket| socket.connectable && socket.output == wanted_output)
            .collect();
        sockets.sort_by(|a, b| distance(a.point, point).total_cmp(&distance(b.point, point)));
        let focus = sockets.first().copied();
        let has_sockets = !sockets.is_empty();
        let mut targets = vec![];
        let mut rejection = None;
        let mut mismatch = None;
        for socket in sockets {
            let compatible = if wanted_output {
                motion_graph::ports_compatible(&socket.spec, &anchor_socket.spec)
            } else {
                motion_graph::ports_compatible(&anchor_socket.spec, &socket.spec)
            };
            match self.wire_candidate(socket) {
                Ok(_) => targets.push(socket),
                Err(error) if compatible && rejection.is_none() => rejection = Some(error),
                Err(error) if !compatible && mismatch.is_none() => mismatch = Some(error),
                Err(_) => {}
            }
        }
        let error = targets.is_empty().then(|| {
            rejection.unwrap_or_else(|| {
                let direction = if wanted_output { "output" } else { "input" };
                if let Some(mut mismatch) = mismatch {
                    mismatch.message = format!(
                        "Node '{}' has no compatible {} {direction}: {}",
                        card.id,
                        type_label(anchor_socket.ty),
                        mismatch.message
                    );
                    mismatch
                } else {
                    diag(
                        ErrorCode::UnknownPort,
                        Some(&card.id),
                        None,
                        if has_sockets {
                            format!("Node '{}' has no compatible {direction}", card.id)
                        } else if self
                            .document
                            .nodes
                            .iter()
                            .any(|node| node.id == card.id && node.collapsed)
                        {
                            format!("Expand node '{}' before connecting", card.id)
                        } else {
                            format!("Node '{}' has no {direction} sockets", card.id)
                        },
                    )
                }
            })
        });
        let focus = targets.first().copied().or(focus);
        Some(WireOffer {
            card,
            targets,
            focus,
            error,
            direct: false,
        })
    }

    pub(super) fn finish_wire_drop(
        &mut self,
        cards: &[Card],
        canvas: Rect,
        point: [f32; 2],
        language: Language,
    ) {
        if !canvas.contains(point) {
            return;
        }
        let old_input = match &self.drag {
            Some(Drag::Wire { old_input, .. }) => old_input.clone(),
            _ => None,
        };
        if let Some(offer) = self.wire_offer(cards, canvas, point) {
            if let Some(error) = offer.error {
                self.notice = Some(error.to_string());
                return;
            }
            let connections = offer
                .targets
                .iter()
                .map(|socket| self.wire_candidate(socket))
                .collect::<Result<Vec<_>, _>>();
            match connections {
                Ok(mut connections) if connections.len() == 1 => {
                    let connection = connections.remove(0);
                    if let Err(error) =
                        self.connect_replacing(connection.from, connection.to, old_input.as_ref())
                    {
                        self.notice = Some(error.to_string());
                    }
                }
                Ok(connections) if connections.len() > 1 => {
                    self.context_menu = Some(ContextMenu {
                        position: point,
                        selected: 0,
                        kind: MenuKind::Connect {
                            connections,
                            old_input,
                            scroll: 0.,
                            pointer: point,
                        },
                    });
                }
                Err(error) => self.notice = Some(error.to_string()),
                _ => {}
            }
        } else if !cards.iter().any(|card| card.rect.contains(point)) {
            if let Some(input) = old_input {
                self.disconnect_input(&input);
            } else {
                self.notice = Some(
                    language
                        .text(
                            "未建立连接：请拖到匹配的端口或节点",
                            "No connection: drag to a matching port or node",
                        )
                        .into(),
                );
            }
        }
    }

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
    #[cfg(test)]
    pub(super) fn wire_feedback<'a>(
        &self,
        cards: &'a [Card],
        canvas: Rect,
        point: [f32; 2],
    ) -> Option<(&'a Socket, Result<(), Diagnostic>)> {
        self.wire_offer(cards, canvas, point)?.feedback()
    }
}
