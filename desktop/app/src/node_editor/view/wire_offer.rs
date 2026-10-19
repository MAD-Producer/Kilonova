use super::*;

impl NodeEditor {
    pub(super) fn paint_wire_offer(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        point: [f32; 2],
        offer: &wires::WireOffer<'_>,
        language: Language,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        let color = if offer.error.is_some() {
            styles.color("error")
        } else {
            colors.ACCENT
        };
        let border = styles.metric("border", 1.);
        let padding = styles.metric("padding.panel", 8.);
        if !offer.direct {
            paint::stroke(list, offer.card.rect, color, border);
        }
        let sockets = if offer.targets.is_empty() {
            offer.focus.into_iter().collect()
        } else {
            offer.targets.clone()
        };
        for socket in sockets {
            let radius = wires::socket_radius(self.view.zoom);
            paint::diamond(list, socket.point, radius + 3., color);
            paint::diamond(list, socket.point, radius, type_color(socket.ty));
        }
        let row = styles.metric("row.height", 22.);
        let width = styles
            .metric("dropdown.max_width", 320.)
            .min((area.width() - padding * 2.).max(1.));
        let height = (row * 2. + padding * 2.).min((area.height() - padding * 2.).max(1.));
        let tooltip = Rect::new(
            (point[0] + padding * 2.).clamp(
                area.min[0] + padding,
                (area.max[0] - width - padding).max(area.min[0] + padding),
            ),
            (point[1] + styles.metric("icon.slot", 20.)).clamp(
                area.min[1] + padding,
                (area.max[1] - height - padding).max(area.min[1] + padding),
            ),
            width,
            height,
        );
        list.retain_glyphs(|glyph| {
            !intersects(
                tooltip,
                Rect::new(
                    glyph.position[0],
                    glyph.position[1],
                    glyph.size[0],
                    glyph.size[1],
                ),
            )
        });
        paint::rect(list, tooltip, colors.CHROME);
        paint::stroke(list, tooltip, color, border);
        let message = if let Some(error) = &offer.error {
            if !offer.direct && error.code == ErrorCode::UnknownPort {
                if self
                    .document
                    .nodes
                    .iter()
                    .any(|node| node.id == offer.card.id && node.collapsed)
                {
                    language.text("展开节点后连接", "Expand the node before connecting")
                } else {
                    language.text(
                        "该节点没有匹配方向的端口",
                        "This node has no port in the required direction",
                    )
                }
            } else {
                wires::reason(error, language)
            }
        } else if offer.targets.len() > 1 {
            language.text("松开鼠标选择端口", "Release to choose a port")
        } else {
            language.text("松开鼠标连接", "Release to connect")
        };
        label(
            list,
            atlas,
            Rect::new(
                tooltip.min[0] + padding,
                tooltip.min[1] + padding,
                width - padding * 2.,
                row,
            ),
            message,
            color,
            Align::Left,
        );
        let detail = if let Some(error) = &offer.error {
            error.message.clone()
        } else if offer.targets.len() > 1 {
            format!(
                "{} · {}",
                offer.targets.len(),
                language.text("个兼容端口", "compatible ports")
            )
        } else {
            offer
                .focus
                .map(|socket| {
                    let (zh, en) = socket.spec.semantic.label();
                    language.text(zh, en).to_string()
                })
                .unwrap_or_default()
        };
        label(
            list,
            atlas,
            Rect::new(
                tooltip.min[0] + padding,
                tooltip.min[1] + padding + row,
                width - padding * 2.,
                row,
            ),
            &detail,
            colors.TEXT_MUTED,
            Align::Left,
        );
    }
}
