use super::*;

fn record_inspector_edit(result: Result<(), Diagnostic>, revision: u64) {
    if let Err(error) = result {
        log::warn!("node inspector edit rejected: revision={revision} reason={error}");
    }
}

impl NodeEditor {
    pub(crate) fn draw_inspector(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) -> FrameResponse {
        let revision = self.revision;
        let semantic = self.semantic_revision;
        let neutral = Input::default();
        let input = if self.inline.busy() { &neutral } else { input };
        self.field_input(input);
        self.inspector_body(list, atlas, area, input, language, false);
        let mut result = FrameResponse::default();
        result.changed = revision != self.revision;
        result.semantic_changed = semantic != self.semantic_revision;
        if result.changed {
            result.actions.push(Action::Changed {
                revision: self.revision,
                semantic_revision: self.semantic_revision,
                semantic: result.semantic_changed,
            });
        }
        result
    }
    pub(super) fn inspector(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        self.inspector_body(list, atlas, area, input, language, true);
    }

    fn inspector_body(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
        embedded: bool,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("node.dropdown.row.height", 28.);
        let gap = styles.metric("padding", 5.);
        let header = if embedded {
            styles.metric("panel.header.height", 24.)
        } else {
            0.
        };

        let mut panel = list.child();
        paint::rect(&mut panel, area, colors.PANEL);
        paint::rect(
            &mut panel,
            Rect::new(
                area.min[0],
                area.min[1],
                styles.metric("border", 1.),
                area.height(),
            ),
            colors.BORDER,
        );
        let node = self
            .primary
            .as_ref()
            .and_then(|id| self.document.nodes.iter().find(|n| n.id == *id))
            .cloned();
        if embedded {
            label(
                &mut panel,
                atlas,
                Rect::new(
                    area.min[0] + pad,
                    area.min[1],
                    (area.width() - pad * 2.).max(0.),
                    header,
                ),
                language.text("节点属性", "NODE PROPERTIES"),
                colors.TEXT,
                Align::Left,
            );
        }
        let content = Rect::new(
            area.min[0] + pad,
            area.min[1] + header + pad,
            (area.width() - pad * 2.).max(0.),
            (area.height() - header - pad * 2.).max(0.),
        );
        if area.contains(input.mouse) && !input.modifiers.alt {
            self.inspector_scroll = (self.inspector_scroll + input.wheel[1]).max(0.);
        }
        let mut body = list.child();
        let mut y = content.min[1] - self.inspector_scroll;
        if let Some(node) = node {
            if let NodeOp::Unsupported { feature } = &node.operation {
                self.observe_failure(
                    "inspector.unsupported",
                    format_args!("node={:?} feature={feature:?}", node.id),
                );
            }
            let action_width = styles
                .metric("inspector.action.width", 112.)
                .min(content.width());
            label(
                &mut body,
                atlas,
                Rect::new(
                    content.min[0],
                    y,
                    (content.width() - action_width - gap).max(0.),
                    row,
                ),
                &node_title(&node, &self.document.public_parameters, language),
                colors.TEXT,
                Align::Left,
            );
            if ui::button_styled(
                &mut body,
                atlas,
                Rect::new(content.max[0] - action_width, y, action_width, row),
                language.text("删除选中节点", "Delete node"),
                &clipped_input(input, content),
                true,
                &styles,
                "button.icon",
            )
            .clicked
            {
                self.delete_selected();
            }
            y += row;
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), row),
                &format!("ID: {}", node.id),
                colors.TEXT_MUTED,
                Align::Left,
            );
            y += row + gap;
            match &node.operation {
                NodeOp::Float { value } => {
                    self.field_row(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Value",
                        Binding::Constant(node.id.clone(), 0),
                        value.to_string(),
                        input,
                    );
                }
                NodeOp::Vec2 { value } => {
                    for (i, value) in value.iter().enumerate() {
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            ["X", "Y"][i],
                            Binding::Constant(node.id.clone(), i),
                            value.to_string(),
                            input,
                        );
                    }
                }
                NodeOp::Vec3 { value } => {
                    for (i, value) in value.iter().enumerate() {
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            ["X", "Y", "Z"][i],
                            Binding::Constant(node.id.clone(), i),
                            value.to_string(),
                            input,
                        );
                    }
                }
                NodeOp::Color { value } => {
                    let swatch = Rect::new(content.min[0], y, content.width(), 28.);
                    paint::checkerboard(
                        &mut body,
                        swatch,
                        colors.TRANSPARENT_LIGHT,
                        colors.TRANSPARENT_DARK,
                        7.,
                    );
                    paint::rect(&mut body, swatch, Color(*value));
                    y += 34.;
                    for (i, value) in value.iter().enumerate() {
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            ["R", "G", "B", "Alpha"][i],
                            Binding::Constant(node.id.clone(), i),
                            value.to_string(),
                            input,
                        );
                    }
                }
                NodeOp::Bool { value } => {
                    let mut value = *value;
                    if ui::checkbox(
                        &mut body,
                        atlas,
                        Rect::new(content.min[0], y, content.width(), 27.),
                        language.text("启用", "Enabled"),
                        &mut value,
                        &clipped_input(input, content),
                    )
                    .clicked
                    {
                        record_inspector_edit(
                            self.set_operation(&node.id, NodeOp::Bool { value }),
                            self.revision,
                        );
                    }
                    y += 31.;
                }
                NodeOp::Math {
                    operation,
                    value_type,
                } => {
                    let operations = [
                        MathOp::Add,
                        MathOp::Subtract,
                        MathOp::Multiply,
                        MathOp::Divide,
                        MathOp::Min,
                        MathOp::Max,
                        MathOp::Abs,
                        MathOp::Sin,
                        MathOp::Cos,
                        MathOp::Sqrt,
                        MathOp::Floor,
                        MathOp::Fract,
                        MathOp::Clamp,
                        MathOp::Lerp,
                        MathOp::Smoothstep,
                        MathOp::Dot,
                        MathOp::Length,
                        MathOp::Normalize,
                    ];
                    let allowed: Vec<_> = operations
                        .into_iter()
                        .filter(|op| {
                            *value_type != ValueType::Float
                                || !matches!(op, MathOp::Dot | MathOp::Length | MathOp::Normalize)
                        })
                        .collect();
                    let label = operation_title(&node.operation, language);
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        language.text("运算", "Operation"),
                        &label,
                        input,
                    ) {
                        let next =
                            allowed[(allowed.iter().position(|o| o == operation).unwrap_or(0) + 1)
                                % allowed.len()];
                        record_inspector_edit(
                            self.set_operation(
                                &node.id,
                                NodeOp::Math {
                                    operation: next,
                                    value_type: *value_type,
                                },
                            ),
                            self.revision,
                        );
                    }
                    let types =
                        if matches!(operation, MathOp::Dot | MathOp::Length | MathOp::Normalize) {
                            vec![ValueType::Vec2, ValueType::Vec3]
                        } else {
                            vec![ValueType::Float, ValueType::Vec2, ValueType::Vec3]
                        };
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Type",
                        type_label(*value_type),
                        input,
                    ) {
                        let ty = types[(types.iter().position(|t| t == value_type).unwrap_or(0)
                            + 1)
                            % types.len()];
                        record_inspector_edit(
                            self.set_operation(
                                &node.id,
                                NodeOp::Math {
                                    operation: *operation,
                                    value_type: ty,
                                },
                            ),
                            self.revision,
                        );
                    }
                }
                NodeOp::ColorMath { operation } => {
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Operation",
                        &operation_title(&node.operation, language),
                        input,
                    ) {
                        let operations = [
                            ColorOp::Invert,
                            ColorOp::Mix,
                            ColorOp::Multiply,
                            ColorOp::Screen,
                            ColorOp::SrgbEncode,
                            ColorOp::SrgbDecode,
                        ];
                        let next =
                            operations[(operations.iter().position(|o| o == operation).unwrap()
                                + 1)
                                % operations.len()];
                        record_inspector_edit(
                            self.set_operation(&node.id, NodeOp::ColorMath { operation: next }),
                            self.revision,
                        );
                    }
                }
                NodeOp::Splat { value_type } => {
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Type",
                        type_label(*value_type),
                        input,
                    ) {
                        record_inspector_edit(
                            self.set_operation(
                                &node.id,
                                NodeOp::Splat {
                                    value_type: if *value_type == ValueType::Vec2 {
                                        ValueType::Vec3
                                    } else {
                                        ValueType::Vec2
                                    },
                                },
                            ),
                            self.revision,
                        );
                    }
                }
                NodeOp::Select { value_type } => {
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Type",
                        type_label(*value_type),
                        input,
                    ) {
                        let types = [
                            ValueType::Float,
                            ValueType::Vec2,
                            ValueType::Vec3,
                            ValueType::Color,
                            ValueType::Bool,
                            ValueType::DistancePixels,
                        ];
                        let next = types[(types.iter().position(|t| t == value_type).unwrap_or(0)
                            + 1)
                            % types.len()];
                        record_inspector_edit(
                            self.set_operation(&node.id, NodeOp::Select { value_type: next }),
                            self.revision,
                        );
                    }
                }
                NodeOp::Image { source } => {
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Source",
                        if *source == ImageSource::Input {
                            "Layer Input"
                        } else {
                            "Original Source"
                        },
                        input,
                    ) {
                        record_inspector_edit(
                            self.set_operation(
                                &node.id,
                                NodeOp::Image {
                                    source: if *source == ImageSource::Input {
                                        ImageSource::Source
                                    } else {
                                        ImageSource::Input
                                    },
                                },
                            ),
                            self.revision,
                        );
                    }
                    label(
                        &mut body,
                        atlas,
                        Rect::new(content.min[0], y, content.width(), 23.),
                        language.text("仅当前图层宿主图像", "Host image of the current layer"),
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                    y += 27.;
                }
                NodeOp::SampleImage { coordinates } => {
                    if self.choice(
                        &mut body,
                        atlas,
                        content,
                        &mut y,
                        "Coordinate",
                        if *coordinates == SampleCoordinates::Pixels {
                            "Pixels"
                        } else {
                            "UV"
                        },
                        input,
                    ) {
                        record_inspector_edit(
                            self.set_operation(
                                &node.id,
                                NodeOp::SampleImage {
                                    coordinates: if *coordinates == SampleCoordinates::Pixels {
                                        SampleCoordinates::Uv
                                    } else {
                                        SampleCoordinates::Pixels
                                    },
                                },
                            ),
                            self.revision,
                        );
                    }
                }
                NodeOp::Parameter { parameter } => {
                    if let Some(p) = self
                        .document
                        .public_parameters
                        .iter()
                        .find(|p| p.id == *parameter)
                        .cloned()
                    {
                        if self.choice(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            language.text("绑定", "Binding"),
                            &p.id,
                            input,
                        ) {
                            if !self.document.public_parameters.is_empty() {
                                let index = self
                                    .document
                                    .public_parameters
                                    .iter()
                                    .position(|v| v.id == p.id)
                                    .unwrap();
                                let next = self.document.public_parameters
                                    [(index + 1) % self.document.public_parameters.len()]
                                .id
                                .clone();
                                record_inspector_edit(
                                    self.set_operation(
                                        &node.id,
                                        NodeOp::Parameter { parameter: next },
                                    ),
                                    self.revision,
                                );
                            }
                        }
                        label(
                            &mut body,
                            atlas,
                            Rect::new(content.min[0], y, content.width(), 24.),
                            &format!(
                                "{} · {}",
                                type_label(p.value_type),
                                language.text("稳定参数ID", "Stable parameter ID")
                            ),
                            colors.TEXT_MUTED,
                            Align::Left,
                        );
                        y += 29.;
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            "中文名",
                            Binding::ParamName(p.id.clone(), true),
                            p.name.clone(),
                            input,
                        );
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            "Name",
                            Binding::ParamName(p.id.clone(), false),
                            p.english_name.clone(),
                            input,
                        );
                        for i in 0..dimensions(p.value_type) {
                            if p.value_type == ValueType::Bool {
                                let mut checked = p.default[0] > 0.5;
                                if ui::checkbox(
                                    &mut body,
                                    atlas,
                                    Rect::new(content.min[0], y, content.width(), 26.),
                                    language.text("默认启用", "Default enabled"),
                                    &mut checked,
                                    &clipped_input(input, content),
                                )
                                .clicked
                                {
                                    record_inspector_edit(
                                        self.set_parameter_default(
                                            &p.id,
                                            0,
                                            if checked { 1. } else { 0. },
                                        ),
                                        self.revision,
                                    );
                                }
                                y += 30.;
                            } else {
                                self.field_row(
                                    &mut body,
                                    atlas,
                                    content,
                                    &mut y,
                                    if dimensions(p.value_type) > 1 {
                                        ["X / R", "Y / G", "Z / B", "Alpha"][i]
                                    } else {
                                        language.text("默认值", "Default")
                                    },
                                    Binding::ParamDefault(p.id.clone(), i),
                                    p.default[i].to_string(),
                                    input,
                                );
                            }
                        }
                        if p.value_type != ValueType::Bool {
                            self.field_row(
                                &mut body,
                                atlas,
                                content,
                                &mut y,
                                "Min",
                                Binding::ParamMin(p.id.clone()),
                                p.min.to_string(),
                                input,
                            );
                            self.field_row(
                                &mut body,
                                atlas,
                                content,
                                &mut y,
                                "Max",
                                Binding::ParamMax(p.id.clone()),
                                p.max.to_string(),
                                input,
                            );
                            self.field_row(
                                &mut body,
                                atlas,
                                content,
                                &mut y,
                                "Step",
                                Binding::ParamStep(p.id.clone()),
                                p.step.to_string(),
                                input,
                            );
                        }
                        self.field_row(
                            &mut body,
                            atlas,
                            content,
                            &mut y,
                            language.text("单位", "Units"),
                            Binding::ParamUnits(p.id.clone()),
                            p.units.clone(),
                            input,
                        );
                        let mut animated = p.animatable;
                        if ui::checkbox(
                            &mut body,
                            atlas,
                            Rect::new(content.min[0], y, content.width(), 26.),
                            language.text("允许动画", "Animatable"),
                            &mut animated,
                            &clipped_input(input, content),
                        )
                        .clicked
                        {
                            let before = self.state(true);
                            self.document
                                .public_parameters
                                .iter_mut()
                                .find(|v| v.id == p.id)
                                .unwrap()
                                .animatable = animated;
                            self.finish_edit(before);
                        }
                        y += 32.;
                    }
                }
                _ => {}
            }
            if let Ok(ports) = self.observed(
                "inspector.ports",
                &node.id,
                motion_graph::node_port_specs(&node, &self.document.public_parameters),
            ) {
                paint::rect(
                    &mut body,
                    Rect::new(content.min[0], y, content.width(), row),
                    colors.HEADER,
                );
                label(
                    &mut body,
                    atlas,
                    Rect::new(
                        content.min[0] + pad,
                        y,
                        (content.width() - pad * 2.).max(0.),
                        row,
                    ),
                    language.text("输入连接", "Input connections"),
                    colors.TEXT,
                    Align::Left,
                );
                y += row;
                for (port, spec) in ports.inputs {
                    let input_ref = PortRef::new(&node.id, &port);
                    let edge = self
                        .document
                        .connections
                        .iter()
                        .find(|c| c.to == input_ref)
                        .cloned();
                    let columns = motion_ui::widgets::InspectorColumns::new(
                        &styles,
                        Rect::new(content.min[0] - pad, y, content.width() + pad * 2., row),
                        y,
                        row,
                        0.,
                        0.,
                        1,
                    );
                    let title = motion_graph::group_port(&node.operation, &port, false)
                        .map(|port| language.text(&port.name, &port.english_name))
                        .unwrap_or_else(|| language.text(spec.name, spec.english_name));
                    label(
                        &mut body,
                        atlas,
                        columns.label,
                        title,
                        colors.TEXT_MUTED,
                        Align::Left,
                    );
                    if let Some(edge) = edge {
                        let action = styles.metric("auxiliary.action.width", 28.);
                        let source = self.document.nodes.iter().find(|n| n.id == edge.from.node);
                        let caption = source
                            .map(|source| {
                                node_title(source, &self.document.public_parameters, language)
                            })
                            .unwrap_or_else(|| edge.from.node.clone());
                        let value = Rect::new(
                            columns.values.min[0],
                            y,
                            (content.max[0] - action - gap - columns.values.min[0])
                                .max(0.)
                                .min(columns.values.width()),
                            row,
                        );
                        let response = ui::button_styled(
                            &mut body,
                            atlas,
                            value,
                            "",
                            &clipped_input(input, content),
                            true,
                            &styles,
                            "button.icon",
                        );
                        label(
                            &mut body,
                            atlas,
                            value.shrink(gap),
                            &caption,
                            colors.ACCENT,
                            Align::Left,
                        );
                        if response.clicked {
                            self.select(Some(&edge.from.node), false);
                        }
                        if ui::button_icon(
                            &mut body,
                            Rect::new(content.max[0] - action, y, action, row),
                            motion_ui::icons::Icon::Close,
                            &clipped_input(input, content),
                            true,
                        )
                        .clicked
                        {
                            self.disconnect_input(&input_ref);
                        }
                    } else {
                        let shown = node
                            .input_defaults
                            .get(&port)
                            .map(super::super::input_defaults::value_text)
                            .unwrap_or_else(|| language.text("未连接", "Not connected").into());
                        label(
                            &mut body,
                            atlas,
                            columns.values,
                            &shown,
                            colors.TEXT_MUTED,
                            Align::Left,
                        );
                    }
                    y += row;
                }
            }
        } else {
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 27.),
                language.text("图与导出设置", "GRAPH & EXPORT"),
                colors.TEXT,
                Align::Left,
            );
            y += 33.;
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "中文名",
                Binding::GraphName(true),
                self.document.name.clone(),
                input,
            );
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "Name",
                Binding::GraphName(false),
                self.document.english_name.clone(),
                input,
            );
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "Plugin ID",
                Binding::PluginId,
                self.options.plugin_id.clone(),
                input,
            );
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "Version",
                Binding::Version,
                self.options.version.clone(),
                input,
            );
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "Author",
                Binding::Author,
                self.options.author.clone(),
                input,
            );
            self.field_row(
                &mut body,
                atlas,
                content,
                &mut y,
                "License",
                Binding::License,
                self.options.license.clone(),
                input,
            );
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y + 8., content.width(), 24.),
                &format!(
                    "SDK 6 · {} nodes · {} parameters",
                    self.document.nodes.len(),
                    self.document.public_parameters.len()
                ),
                colors.TEXT_MUTED,
                Align::Left,
            );
            y += 40.;
            let parameters = self.document.public_parameters.clone();
            for p in parameters {
                let used = self
                    .document
                    .nodes
                    .iter()
                    .find(
                        |n| matches!(&n.operation,NodeOp::Parameter{parameter} if *parameter==p.id),
                    )
                    .map(|n| n.id.clone());
                let rect = Rect::new(content.min[0], y, content.width() - 31., 26.);
                if ui::button(
                    &mut body,
                    atlas,
                    rect,
                    &format!("{} · {}", p.id, type_label(p.value_type)),
                    &clipped_input(input, content),
                    true,
                )
                .clicked
                {
                    if let Some(id) = used {
                        self.select(Some(&id), false);
                    } else {
                        let position = self.view.screen_to_world(
                            [
                                self.last_canvas.min[0] + self.last_canvas.width() * 0.5,
                                self.last_canvas.min[1] + self.last_canvas.height() * 0.5,
                            ],
                            self.last_canvas,
                        );
                        if let Err(error) = self.add_node(
                            NodeOp::Parameter {
                                parameter: p.id.clone(),
                            },
                            position,
                        ) {
                            self.notice = Some(error.to_string());
                        }
                    }
                }
                if !self.document.nodes.iter().any(
                    |n| matches!(&n.operation,NodeOp::Parameter{parameter} if *parameter==p.id),
                ) && ui::button_icon(
                    &mut body,
                    Rect::new(content.max[0] - 27., y, 27., 26.),
                    motion_ui::icons::Icon::Close,
                    &clipped_input(input, content),
                    true,
                )
                .clicked
                {
                    let before = self.state(true);
                    self.document
                        .public_parameters
                        .retain(|parameter| parameter.id != p.id);
                    self.finish_edit(before);
                }
                y += 30.;
            }
        }
        if let Some(node) = self
            .primary
            .as_ref()
            .and_then(|id| self.document.nodes.iter().find(|node| node.id == *id))
        {
            y += 14.;
            let (zh, en) = motion_graph::node_description(&node.operation);
            y += crate::node_examples::paragraph(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 80.),
                language.text(zh, en),
                colors.TEXT_MUTED,
                4,
            );
            if let Ok(specs) = self.observed(
                "inspector.help_ports",
                &node.id,
                motion_graph::node_port_specs(node, &self.document.public_parameters),
            ) {
                for (heading, ports) in [
                    (language.text("输入端口", "INPUT PORTS"), specs.inputs),
                    (language.text("输出端口", "OUTPUT PORTS"), specs.outputs),
                ] {
                    if ports.is_empty() {
                        continue;
                    }
                    label(
                        &mut body,
                        atlas,
                        Rect::new(content.min[0], y + 6., content.width(), 24.),
                        heading,
                        colors.TEXT,
                        Align::Left,
                    );
                    y += 34.;
                    for spec in ports.values() {
                        let (zh, en) = spec.semantic.label();
                        label(
                            &mut body,
                            atlas,
                            Rect::new(content.min[0], y, content.width(), 24.),
                            &format!(
                                "{} · {}",
                                language.text(spec.name, spec.english_name),
                                language.text(zh, en)
                            ),
                            colors.ACCENT,
                            Align::Left,
                        );
                        y += 26.;
                        y += crate::node_examples::paragraph(
                            &mut body,
                            atlas,
                            Rect::new(content.min[0], y, content.width(), 60.),
                            language.text(spec.description, spec.english_description),
                            colors.TEXT_MUTED,
                            3,
                        ) + 6.;
                    }
                }
            }
        }
        if let Some((revision, report)) = &self.compiled {
            y += 12.;
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 24.),
                if *revision == self.semantic_revision {
                    language.text("编译结果", "COMPILED")
                } else {
                    language.text("上次有效编译", "LAST VALID COMPILE")
                },
                colors.TEXT_MUTED,
                Align::Left,
            );
            y += 25.;
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 24.),
                &format!(
                    "{} pass · {} sample sites · loop {}",
                    report.passes, report.total_sample_sites, report.loop_work
                ),
                colors.TEXT_MUTED,
                Align::Left,
            );
            y += 27.;
        }
        let extent = y - (content.min[1] - self.inspector_scroll);
        self.inspector_scroll = self
            .inspector_scroll
            .min((extent - content.height()).max(0.));
        body.clip_to(content);
        panel.append(body);
        panel.clip_to(area);
        list.append(panel);
    }
    pub(super) fn choice(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        name: &str,
        value: &str,
        input: &Input,
    ) -> bool {
        let styles = list.styles();
        let colors = styles.colors();

        label(
            list,
            atlas,
            Rect::new(area.min[0], *y, 66., 27.),
            name,
            colors.TEXT_MUTED,
            Align::Left,
        );
        let clicked = ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + 68., *y, (area.width() - 68.).max(1.), 27.),
            &format!("{value}  ›"),
            &clipped_input(input, area),
            true,
        )
        .clicked;
        *y += 31.;
        clicked && self.commit_field().is_ok()
    }
    pub(super) fn field_row(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        name: &str,
        binding: Binding,
        value: String,
        input: &Input,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        let padding = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let row = styles.metric("node.dropdown.row.height", 28.);
        let columns = motion_ui::widgets::InspectorColumns::new(
            &styles,
            Rect::new(
                area.min[0] - padding,
                area.min[1],
                area.width() + padding * 2.,
                area.height(),
            ),
            *y,
            row,
            0.,
            0.,
            1,
        );
        let rect = columns.values;
        let active = self.field.as_ref().is_some_and(|f| f.binding == binding);
        label(
            list,
            atlas,
            columns.label,
            name,
            colors.TEXT_MUTED,
            Align::Left,
        );
        if active {
            let mut states =
                motion_ui::style::States::default().with(motion_ui::style::State::Focused);
            if self
                .field
                .as_ref()
                .is_some_and(|field| field.error.is_some() || field.editor.error.is_some())
            {
                states = states.with(motion_ui::style::State::Invalid);
            }
            motion_ui::style::box_style(list, rect, styles.resolve("field", states).unwrap());
        }
        let shown = if active {
            &self.field.as_ref().unwrap().text
        } else {
            &value
        };
        if active && self.field.as_ref().unwrap().replace {
            paint::rect(list, rect.shrink(gap), styles.color("selected"));
        }
        label(
            list,
            atlas,
            rect.shrink(gap),
            shown,
            colors.TEXT,
            Align::Left,
        );
        if active && !self.field.as_ref().unwrap().replace {
            let cursor = self.field.as_ref().unwrap().cursor;
            let x = (rect.min[0] + gap + atlas.measure(&shown[..cursor])).min(rect.max[0] - gap);
            paint::rect(
                list,
                Rect::new(
                    x,
                    rect.min[1] + gap,
                    styles.metric("border", 1.),
                    (rect.height() - gap * 2.).max(0.),
                ),
                colors.ACCENT,
            );
        }
        if area.contains(input.mouse) && hit(input, rect) {
            if !active && self.commit_field().is_ok() {
                self.field = Some(Field {
                    binding,
                    text: value.clone(),
                    cursor: value.len(),
                    replace: true,
                    editor: Default::default(),
                    error: None,
                });
                self.search_focus = false;
            }
        }
        *y += row;
    }
}
