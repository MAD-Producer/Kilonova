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
        self.inspector(list, atlas, area, input, language);
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
        let styles = list.styles();
        let colors = styles.colors();

        let mut panel = list.child();
        paint::rect(&mut panel, area, colors.PANEL);
        paint::rect(
            &mut panel,
            Rect::new(area.min[0], area.min[1], 1., area.height()),
            colors.BORDER,
        );
        let node = self
            .primary
            .as_ref()
            .and_then(|id| self.document.nodes.iter().find(|n| n.id == *id))
            .cloned();
        label(
            &mut panel,
            atlas,
            Rect::new(area.min[0] + 12., area.min[1] + 5., area.width() - 24., 26.),
            language.text("节点属性", "NODE PROPERTIES"),
            colors.TEXT,
            Align::Left,
        );
        let content = Rect::new(
            area.min[0] + 12.,
            area.min[1] + 37.,
            area.width() - 24.,
            (area.height() - 44.).max(0.),
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
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 27.),
                &operation_title(&node.operation, language),
                colors.TEXT,
                Align::Left,
            );
            y += 26.;
            label(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 23.),
                &node.id,
                colors.TEXT_MUTED,
                Align::Left,
            );
            y += 30.;
            if ui::button(
                &mut body,
                atlas,
                Rect::new(content.min[0], y, content.width(), 26.),
                language.text("删除选中节点", "Delete selected node"),
                &clipped_input(input, content),
                true,
            )
            .clicked
            {
                self.delete_selected();
            }
            y += 38.;
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
                        record_inspector_edit(self.set_operation(&node.id, NodeOp::Bool { value }), self.revision);
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
                        record_inspector_edit(self.set_operation(
                            &node.id,
                            NodeOp::Math {
                                operation: next,
                                value_type: *value_type,
                            },
                        ), self.revision);
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
                        record_inspector_edit(self.set_operation(
                            &node.id,
                            NodeOp::Math {
                                operation: *operation,
                                value_type: ty,
                            },
                        ), self.revision);
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
                        record_inspector_edit(self.set_operation(&node.id, NodeOp::ColorMath { operation: next }), self.revision);
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
                        record_inspector_edit(self.set_operation(
                            &node.id,
                            NodeOp::Splat {
                                value_type: if *value_type == ValueType::Vec2 {
                                    ValueType::Vec3
                                } else {
                                    ValueType::Vec2
                                },
                            },
                        ), self.revision);
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
                        record_inspector_edit(self.set_operation(&node.id, NodeOp::Select { value_type: next }), self.revision);
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
                        record_inspector_edit(self.set_operation(
                            &node.id,
                            NodeOp::Image {
                                source: if *source == ImageSource::Input {
                                    ImageSource::Source
                                } else {
                                    ImageSource::Input
                                },
                            },
                        ), self.revision);
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
                        record_inspector_edit(self.set_operation(
                            &node.id,
                            NodeOp::SampleImage {
                                coordinates: if *coordinates == SampleCoordinates::Pixels {
                                    SampleCoordinates::Uv
                                } else {
                                    SampleCoordinates::Pixels
                                },
                            },
                        ), self.revision);
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
                                record_inspector_edit(self
                                    .set_operation(&node.id, NodeOp::Parameter { parameter: next }), self.revision);
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
                                    record_inspector_edit(self.set_parameter_default(
                                        &p.id,
                                        0,
                                        if checked { 1. } else { 0. },
                                    ), self.revision);
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
            if let Ok(ports) = self.observed("inspector.ports", &node.id, node_ports(&node, &self.document.public_parameters)) {
                y += 13.;
                label(
                    &mut body,
                    atlas,
                    Rect::new(content.min[0], y, content.width(), 24.),
                    language.text("输入连接", "INPUT CONNECTIONS"),
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                y += 27.;
                for (port, ty) in ports.inputs {
                    let input_ref = PortRef::new(&node.id, &port);
                    let edge = self
                        .document
                        .connections
                        .iter()
                        .find(|c| c.to == input_ref)
                        .cloned();
                    if edge.is_none() {
                        if let Some(value) = node.input_defaults.get(&port) {
                            let height = styles.metric("row.height", 22.);
                            label(
                                &mut body,
                                atlas,
                                Rect::new(content.min[0], y, content.width(), height),
                                &format!(
                                    "{port}: {} · {}",
                                    super::super::input_defaults::value_text(value),
                                    language.text("默认值", "default")
                                ),
                                colors.TEXT_MUTED,
                                Align::Left,
                            );
                            y += height;
                            continue;
                        }
                    }
                    label(
                        &mut body,
                        atlas,
                        Rect::new(content.min[0], y, content.width(), 22.),
                        &format!("{port} · {}", type_label(ty)),
                        type_color(ty),
                        Align::Left,
                    );
                    y += 22.;
                    if let Some(edge) = edge {
                        let rect = Rect::new(content.min[0], y, content.width() - 31., 24.);
                        if ui::button(
                            &mut body,
                            atlas,
                            rect,
                            &format!("{}.{}", edge.from.node, edge.from.port),
                            &clipped_input(input, content),
                            true,
                        )
                        .clicked
                        {
                            self.select(Some(&edge.from.node), false);
                        }
                        if ui::button_icon(
                            &mut body,
                            Rect::new(content.max[0] - 27., y, 27., 24.),
                            motion_ui::icons::Icon::Close,
                            &clipped_input(input, content),
                            true,
                        )
                        .clicked
                        {
                            self.disconnect_input(&input_ref);
                        }
                    } else {
                        label(
                            &mut body,
                            atlas,
                            Rect::new(content.min[0], y, content.width(), 24.),
                            language
                                .text("拖动匹配类型的端口连接", "Connect a matching output socket"),
                            Color::rgb(235, 167, 102),
                            Align::Left,
                        );
                    }
                    y += 33.;
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
            if let Ok(specs) = self.observed("inspector.help_ports", &node.id, motion_graph::node_port_specs(node, &self.document.public_parameters))
            {
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

        let rect = Rect::new(area.min[0] + 68., *y, (area.width() - 68.).max(1.), 27.);
        let active = self.field.as_ref().is_some_and(|f| f.binding == binding);
        label(
            list,
            atlas,
            Rect::new(area.min[0], *y, 64., 27.),
            name,
            colors.TEXT_MUTED,
            Align::Left,
        );
        paint::rect(list, rect, colors.SUNKEN);
        if active {
            paint::stroke(
                list,
                rect,
                if self.field.as_ref().unwrap().error.is_some() {
                    Color::rgb(230, 128, 103)
                } else {
                    colors.ACCENT
                },
                1.,
            );
        }
        let shown = if active {
            &self.field.as_ref().unwrap().text
        } else {
            &value
        };
        if active && self.field.as_ref().unwrap().replace {
            paint::rect(list, rect.shrink(4.), colors.ACCENT.with_alpha(0.2));
        }
        label(
            list,
            atlas,
            rect.shrink(5.),
            shown,
            colors.TEXT,
            Align::Left,
        );
        if active && !self.field.as_ref().unwrap().replace {
            let cursor = self.field.as_ref().unwrap().cursor;
            let x = (rect.min[0] + 5. + atlas.measure(&shown[..cursor])).min(rect.max[0] - 5.);
            paint::rect(list, Rect::new(x, rect.min[1] + 6., 1., 15.), colors.ACCENT);
        }
        if area.contains(input.mouse) && hit(input, rect) {
            if !active && self.commit_field().is_ok() {
                self.field = Some(Field {
                    binding,
                    text: value.clone(),
                    cursor: value.len(),
                    replace: true,
                    error: None,
                });
                self.search_focus = false;
            }
        }
        *y += 31.;
    }
    pub(super) fn field_input(&mut self, input: &Input) {
        if input.composing
            && self
                .field
                .as_ref()
                .is_some_and(|field| matches!(field.binding, Binding::InputValue(..)))
        {
            return;
        }
        let mut commit = false;
        let mut cancel = false;
        if let Some(field) = &mut self.field {
            if matches!(field.binding, Binding::InputValue(..))
                && input.key_presses().any(|(key, modifiers, repeat)| {
                    key == Key::A
                        && modifiers.control
                        && !modifiers.alt
                        && !modifiers.shift
                        && !repeat
                })
            {
                field.replace = true;
                field.cursor = field.text.len();
            }
            if !input.text.is_empty() {
                if field.replace {
                    field.text.clear();
                    field.cursor = 0;
                    field.replace = false;
                }
                field.text.insert_str(field.cursor, &input.text);
                field.cursor += input.text.len();
                field.error = None;
            }
            for (key, _) in &input.keys {
                match key {
                    Key::Enter => commit = true,
                    Key::Escape => cancel = true,
                    Key::Backspace => {
                        if field.replace {
                            field.text.clear();
                            field.cursor = 0;
                            field.replace = false;
                        } else if field.cursor > 0 {
                            let previous = field.text[..field.cursor]
                                .char_indices()
                                .last()
                                .map_or(0, |(i, _)| i);
                            field.text.drain(previous..field.cursor);
                            field.cursor = previous;
                        }
                        field.error = None;
                    }
                    Key::Delete => {
                        if field.cursor < field.text.len() {
                            let next = field.cursor
                                + field.text[field.cursor..]
                                    .chars()
                                    .next()
                                    .unwrap()
                                    .len_utf8();
                            field.text.drain(field.cursor..next);
                        }
                    }
                    Key::Left => {
                        field.cursor = field.text[..field.cursor]
                            .char_indices()
                            .last()
                            .map_or(0, |(i, _)| i);
                        field.replace = false;
                    }
                    Key::Right => {
                        if field.cursor < field.text.len() {
                            field.cursor += field.text[field.cursor..]
                                .chars()
                                .next()
                                .unwrap()
                                .len_utf8();
                        }
                        field.replace = false;
                    }
                    Key::Home => {
                        field.cursor = 0;
                        field.replace = false;
                    }
                    Key::End => {
                        field.cursor = field.text.len();
                        field.replace = false;
                    }
                    _ => {}
                }
            }
        }
        if cancel {
            self.field = None;
        } else if commit {
            let _ = self.commit_field();
        }
    }
}
