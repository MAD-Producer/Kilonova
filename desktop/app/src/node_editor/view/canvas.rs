use super::*;

impl NodeEditor {
    pub(super) fn cards(&self, canvas: Rect) -> Vec<Card> {
        self.document
            .nodes
            .iter()
            .map(|node| {
                let point = self.view.world_to_screen(node.position, canvas);
                let rect = Rect::new(
                    point[0],
                    point[1],
                    NODE_WIDTH * self.view.zoom,
                    node_height(node, &self.document) * self.view.zoom,
                );
                let mut sockets = vec![];
                if let Ok(ports) =
                    motion_graph::node_port_specs(node, &self.document.public_parameters)
                {
                    for (output, entries) in [(false, ports.inputs), (true, ports.outputs)] {
                        for (index, (port, spec)) in entries.into_iter().enumerate() {
                            sockets.push(Socket {
                                port: PortRef::new(&node.id, port),
                                output,
                                ty: spec.value_type,
                                spec,
                                point: [
                                    if output { rect.max[0] } else { rect.min[0] },
                                    rect.min[1]
                                        + (HEADER + ROW * (index as f32 + 0.5)) * self.view.zoom,
                                ],
                            });
                        }
                    }
                }
                Card {
                    id: node.id.clone(),
                    rect,
                    sockets,
                }
            })
            .collect()
    }
    pub(super) fn paint_canvas(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        cards: &[Card],
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        let mut canvas = PaintList::default();
        paint::rect(&mut canvas, area, palette::SUNKEN);

        // 改进的网格系统 - 双层网格（大网格 + 小网格）
        let minor_step = (20. * self.view.zoom).max(10.);
        let major_step = minor_step * 5.;

        // 计算网格原点（世界坐标 0,0 对应的屏幕位置）
        let world_origin = self.view.world_to_screen([0., 0.], area);

        // 绘制小网格（淡）
        if self.view.zoom >= 0.4 {
            let minor_origin = [
                (world_origin[0] - area.min[0]).rem_euclid(minor_step) + area.min[0],
                (world_origin[1] - area.min[1]).rem_euclid(minor_step) + area.min[1],
            ];
            let mut x = minor_origin[0];
            while x < area.max[0] {
                let mut y = minor_origin[1];
                while y < area.max[1] {
                    paint::rect(
                        &mut canvas,
                        Rect::new(x, y, 1., 1.),
                        palette::BORDER.with_alpha(0.15),
                    );
                    y += minor_step;
                }
                x += minor_step;
            }
        }

        // 绘制大网格（稍亮）
        let major_origin = [
            (world_origin[0] - area.min[0]).rem_euclid(major_step) + area.min[0],
            (world_origin[1] - area.min[1]).rem_euclid(major_step) + area.min[1],
        ];
        let mut x = major_origin[0];
        while x < area.max[0] {
            let mut y = major_origin[1];
            while y < area.max[1] {
                paint::rect(
                    &mut canvas,
                    Rect::new(x, y, 1., 1.),
                    palette::BORDER.with_alpha(0.35),
                );
                y += major_step;
            }
            x += major_step;
        }

        // 绘制原点标记（十字）
        if area.contains(world_origin) {
            let cross_size = 10.;
            paint::line(
                &mut canvas,
                vec![
                    [world_origin[0] - cross_size, world_origin[1]],
                    [world_origin[0] + cross_size, world_origin[1]],
                ],
                palette::BORDER.with_alpha(0.5),
            );
            paint::line(
                &mut canvas,
                vec![
                    [world_origin[0], world_origin[1] - cross_size],
                    [world_origin[0], world_origin[1] + cross_size],
                ],
                palette::BORDER.with_alpha(0.5),
            );
        }
        let find = |port: &PortRef, output: bool| {
            cards
                .iter()
                .flat_map(|c| c.sockets.iter())
                .find(|s| s.port == *port && s.output == output)
        };

        // 绘制连线 - 改进样式：更粗、更明显
        for edge in &self.document.connections {
            if let Some((a, b)) = find(&edge.from, true).zip(find(&edge.to, false)) {
                let points = curve(a.point, b.point);
                if points.iter().all(|p| p[0].is_finite() && p[1].is_finite()) {
                    let base_color = type_color(a.ty);

                    // 绘制连线（使用稍亮的颜色，alpha 从 0.65 提升到 0.8）
                    paint::line(&mut canvas, points.clone(), base_color.with_alpha(0.8));

                    // 如果连线被选中，绘制外发光效果
                    // TODO: 添加选中状态检测
                }
            }
        }
        if let Some(Drag::Wire { source, .. }) = &self.drag {
            if let Some(a) = find(source, true) {
                paint::line(&mut canvas, curve(a.point, input.mouse), palette::ACCENT);
            }
        }
        if let Some(Drag::WireInput { target }) = &self.drag {
            if let Some(b) = find(target, false) {
                paint::line(&mut canvas, curve(input.mouse, b.point), palette::ACCENT);
            }
        }
        for card in cards {
            if !intersects(card.rect, area)
                || card
                    .rect
                    .min
                    .iter()
                    .chain(card.rect.max.iter())
                    .any(|v| !v.is_finite())
            {
                continue;
            }
            let node = self
                .document
                .nodes
                .iter()
                .find(|n| n.id == card.id)
                .unwrap();
            let selected = self.selected.contains(&card.id);
            let invalid = self
                .diagnostic
                .as_ref()
                .is_some_and(|d| d.node.as_deref() == Some(&card.id));
            paint::rect(
                &mut canvas,
                Rect::new(
                    card.rect.min[0] + 3.,
                    card.rect.min[1] + 4.,
                    card.rect.width(),
                    card.rect.height(),
                ),
                Color::rgba(0, 0, 0, 0.22),
            );
            paint::rect(&mut canvas, card.rect, palette::PANEL);
            paint::rect(
                &mut canvas,
                Rect::new(
                    card.rect.min[0],
                    card.rect.min[1],
                    card.rect.width(),
                    HEADER * self.view.zoom,
                ),
                category_color(&node.operation),
            );
            paint::stroke(
                &mut canvas,
                card.rect,
                if selected {
                    palette::ACCENT
                } else if invalid {
                    Color::rgb(218, 118, 91)
                } else {
                    palette::BORDER
                },
                if selected { 2. } else { 1. },
            );
            let mut content = PaintList::default();
            let local = Rect::new(0., 0., NODE_WIDTH, node_height(node, &self.document));
            label(
                &mut content,
                atlas,
                Rect::new(10., 1., NODE_WIDTH - 20., HEADER - 2.),
                &operation_title(&node.operation, language),
                palette::TEXT,
                Align::Left,
            );
            if let Ok(ports) = motion_graph::node_port_specs(node, &self.document.public_parameters)
            {
                for (output, entries) in [(false, ports.inputs), (true, ports.outputs)] {
                    for (index, (_, spec)) in entries.into_iter().enumerate() {
                        let compatible = match &self.drag {
                            Some(Drag::Wire { source, .. }) if !output => cards
                                .iter()
                                .flat_map(|card| &card.sockets)
                                .find(|socket| socket.output && socket.port == *source)
                                .is_none_or(|socket| {
                                    motion_graph::ports_compatible(&socket.spec, &spec)
                                }),
                            _ => true,
                        };
                        label(
                            &mut content,
                            atlas,
                            Rect::new(
                                if output { NODE_WIDTH * 0.5 } else { 12. },
                                HEADER + index as f32 * ROW,
                                NODE_WIDTH * 0.5 - 14.,
                                ROW,
                            ),
                            language.text(spec.name, spec.english_name),
                            if compatible {
                                palette::TEXT_MUTED
                            } else {
                                palette::TEXT_MUTED.with_alpha(0.35)
                            },
                            if output { Align::Right } else { Align::Left },
                        );
                    }
                }
            }
            let preview = match &node.operation {
                NodeOp::Float { value } => ui::format_value(*value),
                NodeOp::Vec2 { value } => format!("{value:?}"),
                NodeOp::Vec3 { value } => format!("{value:?}"),
                NodeOp::Color { value } => format!(
                    "RGBA {:0.2}  {:0.2}  {:0.2}  {:0.2}",
                    value[0], value[1], value[2], value[3]
                ),
                NodeOp::Bool { value } => value.to_string(),
                NodeOp::Parameter { parameter } => self
                    .document
                    .public_parameters
                    .iter()
                    .find(|p| p.id == *parameter)
                    .map(|p| language.text(&p.name, &p.english_name).to_string())
                    .unwrap_or_else(|| parameter.clone()),
                NodeOp::Math { value_type, .. }
                | NodeOp::Splat { value_type }
                | NodeOp::Select { value_type } => type_label(*value_type).into(),
                _ => String::new(),
            };
            label(
                &mut content,
                atlas,
                Rect::new(12., local.max[1] - 24., NODE_WIDTH - 24., 22.),
                &preview,
                palette::TEXT_MUTED,
                Align::Left,
            );
            for glyph in &mut content.glyphs {
                glyph.position = [
                    card.rect.min[0] + glyph.position[0] * self.view.zoom,
                    card.rect.min[1] + glyph.position[1] * self.view.zoom,
                ];
                glyph.size = [
                    glyph.size[0] * self.view.zoom,
                    glyph.size[1] * self.view.zoom,
                ];
            }
            canvas.append(content);
            for socket in &card.sockets {
                let radius = (4.5 * self.view.zoom).max(2.5);
                paint::diamond(&mut canvas, socket.point, radius + 1., palette::SUNKEN);
                paint::diamond(&mut canvas, socket.point, radius, type_color(socket.ty));
            }
        }
        if let Some(Drag::Marquee {
            origin, current, ..
        }) = &self.drag
        {
            let selection = selection_rect(*origin, *current);
            paint::rect(&mut canvas, selection, palette::ACCENT.with_alpha(0.12));
            paint::stroke(&mut canvas, selection, palette::ACCENT, 1.);
        }
        if self.drag.is_none() && area.contains(input.mouse) {
            if let Some(socket) = cards.iter().flat_map(|card| &card.sockets).find(|socket| {
                (socket.point[0] - input.mouse[0]).abs() <= 9.
                    && (socket.point[1] - input.mouse[1]).abs() <= 9.
            }) {
                let width = 330_f32.min(area.width() - 12.);
                let height = 128_f32.min(area.height() - 12.);
                let tooltip = Rect::new(
                    (input.mouse[0] + 14.)
                        .min(area.max[0] - width - 6.)
                        .max(area.min[0] + 6.),
                    (input.mouse[1] + 16.)
                        .min(area.max[1] - height - 6.)
                        .max(area.min[1] + 6.),
                    width,
                    height,
                );
                canvas.glyphs.retain(|glyph| {
                    !intersects(
                        Rect::new(
                            glyph.position[0],
                            glyph.position[1],
                            glyph.size[0],
                            glyph.size[1],
                        ),
                        tooltip,
                    )
                });
                paint::rect(&mut canvas, tooltip, palette::PANEL);
                paint::stroke(&mut canvas, tooltip, palette::ACCENT, 1.);
                let name = language.text(socket.spec.name, socket.spec.english_name);
                let direction = if socket.output {
                    language.text("输出", "Output")
                } else {
                    language.text("输入", "Input")
                };
                label(
                    &mut canvas,
                    atlas,
                    Rect::new(tooltip.min[0] + 10., tooltip.min[1] + 6., width - 20., 23.),
                    &format!("{direction} · {name}"),
                    palette::TEXT,
                    Align::Left,
                );
                let (zh, en) = socket.spec.semantic.label();
                label(
                    &mut canvas,
                    atlas,
                    Rect::new(tooltip.min[0] + 10., tooltip.min[1] + 31., width - 20., 22.),
                    language.text(zh, en),
                    palette::ACCENT,
                    Align::Left,
                );
                crate::node_examples::paragraph(
                    &mut canvas,
                    atlas,
                    Rect::new(
                        tooltip.min[0] + 10.,
                        tooltip.min[1] + 57.,
                        width - 20.,
                        height - 63.,
                    ),
                    language.text(socket.spec.description, socket.spec.english_description),
                    palette::TEXT_MUTED,
                    3,
                );
            }
        }
        canvas.clip_to(area);
        list.append(canvas);
    }
}
