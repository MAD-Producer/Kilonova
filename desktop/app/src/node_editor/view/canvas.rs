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
                                connectable: !node.collapsed,
                                port: PortRef::new(&node.id, port),
                                output,
                                ty: spec.value_type,
                                spec,
                                point: [
                                    if output { rect.max[0] } else { rect.min[0] },
                                    rect.min[1]
                                        + if node.collapsed {
                                            HEADER * 0.5 * self.view.zoom
                                        } else {
                                            (HEADER + ROW * (index as f32 + 0.5)) * self.view.zoom
                                        },
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

        let world_origin = self.view.world_to_screen([0., 0.], area);
        let minor = 20. * self.view.zoom;
        let mut major = 100. * self.view.zoom;
        while major < 24. {
            major *= 5.;
        }
        paint::grid(
            &mut canvas,
            area,
            palette::BORDER.with_alpha(0.15),
            palette::BORDER.with_alpha(0.35),
            world_origin,
            if minor >= 8. { minor } else { 0. },
            major,
        );

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
                    let cutting = matches!(&self.drag, Some(Drag::Cut { inputs, .. }) if inputs.contains(&edge.to));
                    paint::line(
                        &mut canvas,
                        points.clone(),
                        if cutting {
                            INVALID_WIRE
                        } else {
                            base_color.with_alpha(0.8)
                        },
                    );

                    // 如果连线被选中，绘制外发光效果
                    // TODO: 添加选中状态检测
                }
            }
        }
        let feedback = self.wire_feedback(cards, area, input.mouse);
        let rubber_color = if feedback.as_ref().is_some_and(|(_, result)| result.is_err()) {
            INVALID_WIRE
        } else {
            palette::ACCENT
        };
        if let Some(Drag::Wire { source, .. }) = &self.drag {
            if let Some(a) = find(source, true) {
                paint::line(&mut canvas, curve(a.point, input.mouse), rubber_color);
            }
        }
        if let Some(Drag::WireInput { target }) = &self.drag {
            if let Some(b) = find(target, false) {
                paint::line(&mut canvas, curve(input.mouse, b.point), rubber_color);
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
            let previewing = self
                .preview_target
                .as_ref()
                .is_some_and(|target| target.node == card.id);
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
            let title = if previewing {
                format!(
                    "{} · {}",
                    operation_title(&node.operation, language),
                    language.text("预览", "Preview")
                )
            } else {
                operation_title(&node.operation, language)
            };
            label(
                &mut content,
                atlas,
                Rect::new(10., 1., NODE_WIDTH - 44., HEADER - 2.),
                &title,
                palette::TEXT,
                Align::Left,
            );
            let x = NODE_WIDTH - 18.;
            paint::line(
                &mut content,
                if node.collapsed {
                    vec![[x - 3., 11.], [x + 2., 15.], [x - 3., 19.]]
                } else {
                    vec![[x - 4., 12.], [x, 16.], [x + 4., 12.]]
                },
                palette::TEXT_MUTED,
            );
            if !node.collapsed {
                if let Ok(ports) =
                    motion_graph::node_port_specs(node, &self.document.public_parameters)
                {
                    for (output, entries) in [(false, ports.inputs), (true, ports.outputs)] {
                        for (index, (port, spec)) in entries.into_iter().enumerate() {
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
                                motion_graph::group_port(&node.operation, &port, output)
                                    .map(|port| language.text(&port.name, &port.english_name))
                                    .unwrap_or_else(|| language.text(spec.name, spec.english_name)),
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
            if !node.collapsed && inline::control_count(node, &self.document) == 0 {
                label(
                    &mut content,
                    atlas,
                    Rect::new(12., local.max[1] - 24., NODE_WIDTH - 24., 22.),
                    &preview,
                    palette::TEXT_MUTED,
                    Align::Left,
                );
            }
            self.paint_controls(&mut content, atlas, node, language);
            controls::transform_content(&mut content, card.rect.min, self.view.zoom);
            content.clip_to(card.rect);
            canvas.append(content);
            for socket in &card.sockets {
                if !socket.connectable {
                    continue;
                }
                let radius = (4.5 * self.view.zoom).max(2.5);
                if self.preview_target.as_ref() == Some(&socket.port) && socket.output {
                    paint::diamond(&mut canvas, socket.point, radius + 3., palette::ACCENT);
                }
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
        if let Some(Drag::Cut { points, .. }) = &self.drag {
            for segment in points.windows(2) {
                if let Some((a, b)) = wires::clip_segment(segment[0], segment[1], area) {
                    paint::line(&mut canvas, vec![a, b], INVALID_WIRE);
                }
            }
        }
        if let Some((socket, result)) = feedback {
            let color = if result.is_err() {
                INVALID_WIRE
            } else {
                palette::ACCENT
            };
            let radius = (4.5 * self.view.zoom).max(2.5);
            paint::diamond(&mut canvas, socket.point, radius + 3., color);
            paint::diamond(&mut canvas, socket.point, radius, type_color(socket.ty));
            let width = 280f32.min((area.width() - 12.).max(1.));
            let height = 58f32.min((area.height() - 12.).max(1.));
            let tooltip = Rect::new(
                (input.mouse[0] + 14.).clamp(area.min[0] + 6., area.max[0] - width - 6.),
                (input.mouse[1] + 16.).clamp(area.min[1] + 6., area.max[1] - height - 6.),
                width,
                height,
            );
            canvas.glyphs.retain(|g| {
                !intersects(
                    tooltip,
                    Rect::new(g.position[0], g.position[1], g.size[0], g.size[1]),
                )
            });
            paint::rect(&mut canvas, tooltip, palette::CHROME);
            paint::stroke(&mut canvas, tooltip, color, 1.);
            let message = match &result {
                Ok(()) => language.text("松开鼠标连接", "Release to connect"),
                Err(error) => wires::reason(error, language),
            };
            label(
                &mut canvas,
                atlas,
                Rect::new(tooltip.min[0] + 8., tooltip.min[1] + 3., width - 16., 24.),
                message,
                color,
                Align::Left,
            );
            let (zh, en) = socket.spec.semantic.label();
            label(
                &mut canvas,
                atlas,
                Rect::new(tooltip.min[0] + 8., tooltip.min[1] + 28., width - 16., 24.),
                language.text(zh, en),
                palette::TEXT_MUTED,
                Align::Left,
            );
        }
        if self.drag.is_none() && area.contains(input.mouse) {
            if let Some(socket) = cards
                .iter()
                .flat_map(|card| &card.sockets)
                .filter(|socket| socket.connectable)
                .find(|socket| {
                    (socket.point[0] - input.mouse[0]).abs() <= 9.
                        && (socket.point[1] - input.mouse[1]).abs() <= 9.
                })
            {
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
