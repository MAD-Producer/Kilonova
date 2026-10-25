use super::*;
mod node_index;
use node_index::NodeIndex;

impl NodeEditor {
    pub(super) fn cards(&self, canvas: Rect) -> Vec<Card> {
        #[cfg(test)]
        super::frame_cards_tests::record_build();
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
                    self.observed("canvas.sockets", &node.id, motion_graph::node_port_specs(node, &self.document.public_parameters))
                {
                    let inputs = ports.inputs.len();
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
                                            (HEADER
                                                + ROW
                                                    * (port_row_index(node, inputs, output, index)
                                                        as f32
                                                        + 0.5))
                                                * self.view.zoom
                                        },
                                ],
                            });
                        }
                    }
                }
                Card {
                    source_node: node,
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
        #[cfg(test)]
        super::frame_cards_tests::record_paint(cards);
        let styles = list.styles();
        let colors = styles.colors();

        let mut canvas = list.child();
        paint::rect(&mut canvas, area, colors.SUNKEN);

        let world_origin = self.view.world_to_screen([0., 0.], area);
        let minor = 20. * self.view.zoom;
        let mut major = 100. * self.view.zoom;
        while major < 24. {
            major *= 5.;
        }
        if self.view_preferences.grid { paint::grid(
            &mut canvas,
            area,
            colors.BORDER.with_alpha(0.15),
            colors.BORDER.with_alpha(0.35),
            world_origin,
            if minor >= 8. { minor } else { 0. },
            major,
        ); }

        // 绘制原点标记（十字）
        if self.view_preferences.grid && area.contains(world_origin) {
            let cross_size = 10.;
            paint::line(
                &mut canvas,
                vec![
                    [world_origin[0] - cross_size, world_origin[1]],
                    [world_origin[0] + cross_size, world_origin[1]],
                ],
                colors.BORDER.with_alpha(0.5),
            );
            paint::line(
                &mut canvas,
                vec![
                    [world_origin[0], world_origin[1] - cross_size],
                    [world_origin[0], world_origin[1] + cross_size],
                ],
                colors.BORDER.with_alpha(0.5),
            );
        }
        let sockets = (!self.document.connections.is_empty()
            || matches!(self.drag, Some(Drag::Wire { .. } | Drag::WireInput { .. })))
            .then(|| wires::SocketIndex::new(cards));
        let find = |port: &PortRef, output: bool| {
            sockets.as_ref().and_then(|sockets| sockets.find(port, output))
        };
        let source_socket = match &self.drag {
            Some(Drag::Wire { source, .. }) => find(source, true),
            _ => None,
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
        let offer = self.wire_offer(cards, area, input.mouse);
        let rubber_color = if offer.as_ref().is_some_and(|offer| offer.error.is_some()) {
            INVALID_WIRE
        } else {
            colors.ACCENT
        };
        let endpoint = offer
            .as_ref()
            .filter(|offer| offer.targets.len() == 1 && offer.error.is_none())
            .and_then(|offer| offer.targets.first())
            .map_or(input.mouse, |socket| socket.point);
        if let Some(Drag::Wire { source, .. }) = &self.drag {
            if let Some(a) = find(source, true) {
                paint::line(&mut canvas, curve(a.point, endpoint), rubber_color);
            }
        }
        if let Some(Drag::WireInput { target }) = &self.drag {
            if let Some(b) = find(target, false) {
                paint::line(&mut canvas, curve(endpoint, b.point), rubber_color);
            }
        }
        let mut nodes = NodeIndex::new(&self.document.nodes);
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
            let node = nodes.find(&card.id).unwrap();
            let selected = self.selected.contains(&card.id);
            let previewing = self
                .preview_target
                .as_ref()
                .is_some_and(|target| target.node == card.id);
            let invalid = self
                .diagnostic
                .as_ref()
                .is_some_and(|d| d.node.as_deref() == Some(self.qualified_node(&card.id).as_str()));
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
            paint::rect(&mut canvas, card.rect, colors.PANEL);
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
                    colors.ACCENT
                } else if invalid {
                    styles.color("error")
                } else {
                    colors.BORDER
                },
                if selected { 2. } else { 1. },
            );
            let mut content = list.child();
            let local = Rect::new(0., 0., NODE_WIDTH, node_height(node, &self.document));
            let title = if previewing {
                format!(
                    "{} · {}",
                    node_title(node, &self.document.public_parameters, language),
                    language.text("预览", "Preview")
                )
            } else {
                node_title(node, &self.document.public_parameters, language)
            };
            label(
                &mut content,
                atlas,
                Rect::new(10., 1., NODE_WIDTH - 44., HEADER - 2.),
                &title,
                colors.TEXT,
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
                colors.TEXT_MUTED,
            );
            if !node.collapsed {
                // Cards already own the current frame's resolved port metadata.
                // Empty cards retain the original fallible path and observations.
                let reuse_specs = !card.sockets.is_empty()
                    && std::ptr::eq(card.source_node, node);
                let resolved = if !reuse_specs {
                    Some(self.observed("canvas.labels", &node.id,
                        motion_graph::node_port_specs(node, &self.document.public_parameters)))
                } else {
                    None
                };
                let fallback = match &resolved {
                    Some(Ok(ports)) => Some(ports),
                    Some(Err(_)) | None => None,
                };
                let inputs = fallback.map_or_else(
                    || card.sockets.iter().filter(|socket| !socket.output).count(),
                    |ports| ports.inputs.len(),
                );
                for output in [false, true] {
                    let cached = card.sockets.iter().filter(|socket| reuse_specs && socket.output == output)
                        .map(|socket| (socket.port.port.as_str(), &socket.spec));
                    let fresh = fallback.into_iter().flat_map(|ports| {
                        if output { ports.outputs.iter() } else { ports.inputs.iter() }
                    }).map(|(port, spec)| (port.as_str(), spec));
                    for (index, (port, spec)) in cached.chain(fresh).enumerate() {
                        let compatible = match &self.drag {
                            Some(Drag::Wire { .. }) if !output => source_socket
                                .is_none_or(|socket| {
                                    motion_graph::ports_compatible(&socket.spec, spec)
                                }),
                            _ => true,
                        };
                        label(
                            &mut content,
                            atlas,
                            Rect::new(
                                if output { NODE_WIDTH * 0.5 } else { 12. },
                                HEADER
                                    + port_row_index(node, inputs, output, index) as f32 * ROW,
                                if !output && node.input_defaults.contains_key(port) {
                                    (controls::input_value_rect(
                                        Rect::new(0., 0., NODE_WIDTH, ROW),
                                        &styles,
                                    )
                                    .min[0]
                                        - styles.metric("padding.panel", 8.)
                                        - 12.)
                                        .max(0.)
                                } else {
                                    NODE_WIDTH * 0.5 - 14.
                                },
                                ROW,
                            ),
                            motion_graph::group_port(&node.operation, port, output)
                                .map(|port| language.text(&port.name, &port.english_name))
                                .unwrap_or_else(|| language.text(spec.name, spec.english_name)),
                            if compatible {
                                colors.TEXT_MUTED
                            } else {
                                colors.TEXT_MUTED.with_alpha(0.35)
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
            if !node.collapsed && inline::control_count(node, &self.document) == 0 {
                label(
                    &mut content,
                    atlas,
                    Rect::new(12., local.max[1] - 24., NODE_WIDTH - 24., 22.),
                    &preview,
                    colors.TEXT_MUTED,
                    Align::Left,
                );
            }
            self.paint_controls(&mut content, atlas, node, language);
            controls::transform_content(&mut content, card.rect.min, self.view.zoom);
            content.clip_to(card.rect);
            canvas.append(content);
            let missing = self.diagnostic_missing_inputs(node);
            for socket in &card.sockets {
                if !socket.output && missing.contains(&socket.port.port) {
                    let radius =
                        wires::socket_radius(self.view.zoom) + styles.metric("border", 1.) * 2.;
                    let points = (0..=24)
                        .map(|i| {
                            let angle = std::f32::consts::TAU * i as f32 / 24.;
                            [
                                socket.point[0] + radius * angle.cos(),
                                socket.point[1] + radius * angle.sin(),
                            ]
                        })
                        .collect();
                    paint::line_with_width(
                        &mut canvas,
                        points,
                        styles.color("error"),
                        styles.metric("border", 1.),
                    );
                    continue;
                }
                if !socket.connectable {
                    continue;
                }
                let radius = wires::socket_radius(self.view.zoom);
                if self.preview_target.as_ref() == Some(&socket.port) && socket.output {
                    paint::diamond(&mut canvas, socket.point, radius + 3., colors.ACCENT);
                }
                paint::diamond(&mut canvas, socket.point, radius + 1., colors.SUNKEN);
                paint::diamond(&mut canvas, socket.point, radius, type_color(socket.ty));
            }
        }
        if let Some(Drag::Marquee {
            origin, current, ..
        }) = &self.drag
        {
            let selection = selection_rect(*origin, *current);
            paint::rect(&mut canvas, selection, colors.ACCENT.with_alpha(0.12));
            paint::stroke(&mut canvas, selection, colors.ACCENT, 1.);
        }
        if let Some(Drag::Cut { points, .. }) = &self.drag {
            for segment in points.windows(2) {
                if let Some((a, b)) = wires::clip_segment(segment[0], segment[1], area) {
                    paint::line(&mut canvas, vec![a, b], INVALID_WIRE);
                }
            }
        }
        if let Some(offer) = offer {
            self.paint_wire_offer(&mut canvas, atlas, area, input.mouse, &offer, language);
        }
        if self.drag.is_none() && area.contains(input.mouse) {
            if let Some(socket) = wires::socket_at(cards, area, input.mouse, self.view.zoom) {
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
                canvas.retain_glyphs(|glyph| {
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
                paint::rect(&mut canvas, tooltip, colors.PANEL);
                paint::stroke(&mut canvas, tooltip, colors.ACCENT, 1.);
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
                    colors.TEXT,
                    Align::Left,
                );
                let (zh, en) = socket.spec.semantic.label();
                label(
                    &mut canvas,
                    atlas,
                    Rect::new(tooltip.min[0] + 10., tooltip.min[1] + 31., width - 20., 22.),
                    language.text(zh, en),
                    colors.ACCENT,
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
                    colors.TEXT_MUTED,
                    3,
                );
            }
        }
        canvas.clip_to(area);
        list.append(canvas);
    }
}
