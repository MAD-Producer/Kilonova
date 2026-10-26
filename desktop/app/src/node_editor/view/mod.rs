//! Node view layout and shared geometry; each editor region is a reusable component.
mod add_menu;
mod canvas;
#[cfg(test)]
mod frame_cards_tests;
mod connection_menu;
mod context_menu;
mod control_popup;
#[cfg(test)]
mod control_tests;
mod controls;
#[cfg(test)]
mod compact_example_tests;
#[cfg(test)]
mod diagnostic_tests;
mod diagnostics;
mod field_text;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod input_default_tests;
mod inspector;
mod rename;
mod interaction;
mod library;
mod search;
#[cfg(test)]
mod search_tests;
pub(super) mod minimap;
#[cfg(test)]
mod nudge_tests;
#[cfg(test)]
mod observability_tests;
mod onboarding;
#[cfg(test)]
mod onboarding_tests;
#[cfg(test)]
mod view_preferences_tests;
#[cfg(test)]
mod tests;
mod toolbar;
mod wires;
mod wire_offer;
#[cfg(test)]
mod wire_target_tests;
use super::catalog::{operation_title, type_label};
use super::*;
use super::catalog::node_title;
use motion_ui::{
    input::{Key, MouseButton},
    paint,
    theme::Color,
    ui::{self, Align},
};

pub(super) const NODE_WIDTH: f32 = 204.;
const HEADER: f32 = 29.;
const ROW: f32 = 23.;
const INVALID_WIRE: Color = Color::rgb(240, 101, 106);
const TOOLBAR: [(usize, f32, &str, &str); 13] = [
    (0, 41., "添加", "Add"),
    (1, 58., "属性", "Props"),
    (2, 56., "打开", "Open"),
    (11, 76., "示例库", "Examples"),
    (3, 56., "保存", "Save"),
    (4, 64., "另存", "Save As"),
    (5, 62., "校验", "Check"),
    (6, 70., "编译", "Compile"),
    (7, 82., "导出FX", "Export FX"),
    (8, 45., "全览", "Frame"),
    (12, 56., "视图", "View"),
    (9, 48., "撤销", "Undo"),
    (10, 48., "重做", "Redo"),
];
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub toolbar: Rect,
    pub canvas: Rect,
    pub library: Option<Rect>,
    pub inspector: Option<Rect>,
    pub footer: Rect,
}
#[derive(Clone)]
struct Socket {
    connectable: bool,
    port: PortRef,
    output: bool,
    ty: ValueType,
    spec: motion_graph::PortSpec,
    point: [f32; 2],
}
struct Card {
    // Identity only: never dereferenced or retained beyond this frame's cards.
    source_node: *const Node,
    id: String,
    rect: Rect,
    sockets: Vec<Socket>,
}
pub(super) fn node_height(node: &Node, document: &GraphDocument) -> f32 {
    if node.collapsed {
        return HEADER;
    }
    let rows = node_ports(node, &document.public_parameters)
        .map(|p| port_row_count(node, p.inputs.len(), p.outputs.len()))
        .unwrap_or(1);
    expanded_node_height(node, document, rows)
}
fn expanded_node_height(node: &Node, document: &GraphDocument, rows: usize) -> f32 {
    let count = inline::control_count(node, document);
    HEADER
        + rows as f32 * ROW
        + if count == 0 {
            27.
        } else {
            14. + count as f32 * inline::CONTROL_ROW
        }
}
fn port_row_count(node: &Node, inputs: usize, outputs: usize) -> usize {
    if node.input_defaults.is_empty() {
        inputs.max(outputs)
    } else {
        inputs + outputs
    }
}
fn port_row_index(node: &Node, inputs: usize, output: bool, index: usize) -> usize {
    index
        + if output && !node.input_defaults.is_empty() {
            inputs
        } else {
            0
        }
}
fn type_color(ty: ValueType) -> Color {
    match ty {
        // 改进的类型颜色 - 更饱和，更易区分
        ValueType::Float => Color::rgb(103, 196, 255), // 亮蓝 #67C4FF
        ValueType::Vec2 | ValueType::Vec3 => Color::rgb(167, 139, 250), // 紫色 #A78BFA
        ValueType::Color => Color::rgb(251, 191, 36),  // 金色 #FBBF24
        ValueType::Bool => Color::rgb(251, 113, 133),  // 粉红 #FB7185
        ValueType::Image => Color::rgb(52, 211, 153),  // 翠绿 #34D399
        ValueType::DistancePixels => Color::rgb(251, 146, 60), // 橙色 #FB923C
    }
}
fn category_color(op: &NodeOp) -> Color {
    match op {
        // SDF 操作 - 棕褐色系（更明亮）
        NodeOp::SdfCircle
        | NodeOp::SdfRoundedRectangle
        | NodeOp::SdfLine
        | NodeOp::SdfCapsule
        | NodeOp::SdfUnion
        | NodeOp::SdfIntersection
        | NodeOp::SdfDifference
        | NodeOp::SdfSmoothUnion
        | NodeOp::SdfFill
        | NodeOp::SdfStroke => Color::rgb(68, 55, 41), // 棕色 #B47850 (原: 112,76,53)

        // 输出节点 - 青色系（更明显）
        NodeOp::ColorOutput => Color::rgb(42, 66, 63), // 青色 #3CB4B4 (原: 48,81,75)

        // 颜色操作 - 金橙色系（更饱和）
        NodeOp::Color { .. }
        | NodeOp::ColorMath { .. }
        | NodeOp::Gradient { .. }
        | NodeOp::Noise { .. }
        | NodeOp::Blend { .. }
        | NodeOp::CombineColor
        | NodeOp::SplitColor
        | NodeOp::ApplyMask => Color::rgb(73, 62, 39), // 金橙 #DC963C (原: 92,81,45)

        // 数学操作 - 蓝紫色系（更亮）
        NodeOp::Math { .. }
        | NodeOp::CombineVec2
        | NodeOp::CombineVec3
        | NodeOp::Splat { .. }
        | NodeOp::Select { .. } => Color::rgb(45, 57, 81), // 蓝紫 #6478C8 (原: 67,67,95)

        // 其他 - 中性灰蓝（稍亮）
        _ => Color::rgb(49, 56, 63), // 灰蓝 #505F78 (原: 51,64,83)
    }
}
fn label(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    color: Color,
    align: Align,
) {
    ui::text(list, atlas, area, text, color, align)
}
fn hit(input: &Input, area: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(button, p)| button == MouseButton::Left && area.contains(p))
}
fn intersects(a: Rect, b: Rect) -> bool {
    a.min[0] < b.max[0] && a.max[0] > b.min[0] && a.min[1] < b.max[1] && a.max[1] > b.min[1]
}
fn curve(a: [f32; 2], b: [f32; 2]) -> Vec<[f32; 2]> {
    let offset = ((b[0] - a[0]).abs() * 0.45).clamp(42., 200.);
    let c = [a[0] + offset, a[1]];
    let d = [b[0] - offset, b[1]];
    (0..=24)
        .map(|i| {
            let t = i as f32 / 24.;
            let q = 1. - t;
            [
                q * q * q * a[0] + 3. * q * q * t * c[0] + 3. * q * t * t * d[0] + t * t * t * b[0],
                q * q * q * a[1] + 3. * q * q * t * c[1] + 3. * q * t * t * d[1] + t * t * t * b[1],
            ]
        })
        .collect()
}
fn distance_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let v = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1])
        / (v[0] * v[0] + v[1] * v[1]).max(1.0e-6))
    .clamp(0., 1.);
    ((p[0] - a[0] - v[0] * t).powi(2) + (p[1] - a[1] - v[1] * t).powi(2)).sqrt()
}

impl NodeEditor {
    /// Visible screen geometry from the most recent layout, also used by native acceptance.
    pub(crate) fn port_position(&self, node: &str, port: &str, output: bool) -> Option<[f32; 2]> {
        self.cards(self.last_canvas)
            .iter()
            .flat_map(|card| &card.sockets)
            .find(|socket| socket.port == PortRef::new(node, port) && socket.output == output)
            .filter(|socket| self.last_canvas.contains(socket.point))
            .map(|socket| socket.point)
    }
    pub(crate) fn node_header_center(&self, id: &str) -> Option<[f32; 2]> {
        self.cards(self.last_canvas)
            .iter()
            .find(|card| card.id == id)
            .map(|card| {
                [
                    (card.rect.min[0] + card.rect.max[0]) * 0.5,
                    card.rect.min[1] + HEADER * self.view.zoom * 0.5,
                ]
            })
            .filter(|point| self.last_canvas.contains(*point))
    }
    fn toolbar_items(&self)->impl Iterator<Item=(usize,f32,&'static str,&'static str)> {
        let bound=self.layer_target.is_some();
        TOOLBAR.into_iter().filter(move |(id,_,_,_)|!bound||!matches!(id,4|5|6|7))
    }
    pub fn layout(&self, area: Rect) -> Layout {
        let mut x = 6.;
        let mut toolbar_height = 38.;
        for (_, width, _, _) in self.toolbar_items() {
            if x + width > area.width() - 6. {
                x = 6.;
                toolbar_height += 34.;
            }
            x += width + 4.;
        }
        if !self.parents.is_empty() {
            toolbar_height += 28.;
        }
        let toolbar = Rect::new(area.min[0], area.min[1], area.width(), toolbar_height);
        let footer = Rect::new(area.min[0], area.max[1] - 27., area.width(), 27.);
        let body = Rect::new(
            area.min[0],
            toolbar.max[1],
            area.width(),
            (footer.min[1] - toolbar.max[1]).max(0.),
        );

        // 根据 floating_mode 决定侧边栏布局
        let (library_width, inspector_width, inspector_height) = {
            // 漂浮形态：显示侧边栏（左侧库，右侧属性面板）
            let lib_width = if self.library_open && area.width() >= 1024. {
                210.
            } else if self.library_open && area.width() >= 320. {
                (area.width() * 0.32).clamp(190., 202.)
            } else {
                0.
            };
            let insp_width = if self.inspector_open
                && !self.external_inspector
                && lib_width == 0.
                && area.width() >= 320.
            {
                230.
            } else if self.inspector_open && !self.external_inspector && area.width() >= 1024. {
                252.
            } else {
                0.
            };
            (lib_width, insp_width, 0.)
        };

        Layout {
            toolbar,
            footer,
            library: (library_width > 0.)
                .then(|| Rect::new(body.min[0], body.min[1], library_width, body.height())),
            inspector: if inspector_height > 0. {
                // 底部属性面板（嵌入模式）
                Some(Rect::new(
                    body.min[0],
                    body.max[1] - inspector_height,
                    body.width(),
                    inspector_height,
                ))
            } else if inspector_width > 0. {
                // 右侧属性面板（漂浮模式）
                Some(Rect::new(
                    body.max[0] - inspector_width,
                    body.min[1],
                    inspector_width,
                    body.height(),
                ))
            } else {
                None
            },
            canvas: Rect::new(
                body.min[0] + library_width,
                body.min[1],
                (body.width() - library_width - inspector_width).max(0.),
                (body.height() - inspector_height).max(0.),
            ),
        }
    }

    /// Call once for the active node panel with its own logical-coordinate Input.
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) -> FrameResponse {
        let styles = list.styles();
        let colors = styles.colors();

        self.last_area = area;
        let revision = self.revision;
        let semantic = self.semantic_revision;
        let preview_revision = self.preview_revision;
        let preferences = self.view_preferences;
        let mut response = FrameResponse::default();
        self.settle_nudge(input);
        if area.width() < 320. || area.height() < self.layout(area).toolbar.height() + 77. {
            self.cancel_canvas_interaction();
            response.changed = revision != self.revision;
            response.semantic_changed = semantic != self.semantic_revision;
            if response.changed {
                response.actions.push(Action::Changed {
                    revision: self.revision,
                    semantic_revision: self.semantic_revision,
                    semantic: response.semantic_changed,
                });
            }
            paint::rect(list, area, colors.PANEL);
            label(
                list,
                atlas,
                area.shrink(8.),
                language.text("扩大节点面板以编辑", "Expand the node panel"),
                colors.TEXT_MUTED,
                Align::Left,
            );
            return response;
        }
        let mut layout = self.guided_layout(area, &styles);
        self.last_canvas = layout.canvas;
        if self.first_frame {
            if let Some(id) = &self.primary {
                if let Some(node) = self.document.nodes.iter().find(|n| n.id == *id) {
                    self.view.zoom = 0.85;
                    self.view.pan = [
                        layout.canvas.width() * 0.5
                            - (node.position[0] + NODE_WIDTH * 0.5) * self.view.zoom,
                        layout.canvas.height() * 0.5
                            - (node.position[1] + node_height(node, &self.document) * 0.5)
                                * self.view.zoom,
                    ];
                }
            } else {
                self.fit_all(layout.canvas);
            }
            self.first_frame = false;
        }
        if self.inline.busy() || input.pressed.is_some_and(|(_, p)| area.contains(p)) {
            self.focus = true;
        } else if input.pressed.is_some() && self.drag.is_none() && self.minimap_drag.is_none() {
            self.finish_nudge();
            let _ = self.commit_field();
            self.search_focus = false;
            self.focus = false;
        }
        let had_menu = self.popup_open();
        let diagnostic_input = self.diagnostics_input(area, input, &styles);
        let welcome_input = !diagnostic_input
            && self.welcome_input(area, input, &styles, &mut response.actions);
        if !had_menu && !diagnostic_input && !welcome_input {
            self.keyboard_language(input, &mut response.actions, language);
        }
        layout = self.guided_layout(area, &styles);
        self.last_canvas = layout.canvas;
        let geometry_stamp = |editor: &Self| {
            (editor.epoch, editor.revision, editor.preview_revision, editor.view.pan, editor.view.zoom)
        };
        let mut cards = self.cards(layout.canvas);
        let mut geometry = geometry_stamp(self);
        let numeric_file = if self.inline.popup.is_none()
            && self
                .inline
                .number
                .as_ref()
                .is_some_and(|edit| edit.editor.text.active())
        {
            input.key_presses().find_map(|(key, modifiers, repeat)| {
                use crate::shortcuts::{Command, Scope};
                match self.shortcut_bindings.resolve(Scope::Nodes, key, modifiers, repeat) {
                    Some(Command::Open) => Some(Request::Open),
                    Some(Command::Save) => Some(Request::Save),
                    Some(Command::SaveAs) => Some(Request::SaveAs),
                    _ => None,
                }
            })
        } else {
            None
        };
        let commit_input = numeric_file.as_ref().map(|_| {
            let mut local = input.clone();
            local.keys.push((Key::Enter, Default::default()));
            local.key_repeats.push(false);
            local
        });
        let controls_consumed = if !diagnostic_input
            && !welcome_input
            && self.add_menu.is_none()
            && self.context_menu.is_none()
            && (self.inline.busy() || !self.minimap_captures(layout.canvas, input, &styles))
        {
            self.inline_input(
                &cards,
                layout.canvas,
                commit_input.as_ref().unwrap_or(input),
                language,
                styles.as_ref(),
            )
        } else {
            false
        };
        if let Some(request) = numeric_file.filter(|_| self.inline.number.is_none()) {
            match self.request(request) {
                Ok(action) => response.actions.push(action),
                Err(error) => self.notice = Some(error.to_string()),
            }
        }
        let neutral = Input::default();
        let body_input = if had_menu
            || self.popup_open()
            || controls_consumed
            || diagnostic_input
            || welcome_input
        {
            &neutral
        } else {
            input
        };
        self.toolbar(
            list,
            atlas,
            layout.toolbar,
            body_input,
            language,
            &mut response.actions,
        );
        if !self.parents.is_empty() {
            let back = Rect::new(
                layout.toolbar.min[0] + 6.,
                layout.toolbar.max[1] - 27.,
                85.,
                25.,
            );
            if ui::button(
                list,
                atlas,
                back,
                language.text("返回父图", "Parent"),
                body_input,
                true,
            )
            .clicked
            {
                self.leave_group();
            }
            let path = std::iter::once(language.text("主图", "Root"))
                .chain(self.parents.iter().map(|parent| parent.group.as_str()))
                .collect::<Vec<_>>()
                .join(" / ");
            label(
                list,
                atlas,
                Rect::new(
                    back.max[0] + 8.,
                    back.min[1],
                    (layout.toolbar.width() - back.width() - 25.).max(0.),
                    25.,
                ),
                &path,
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        if let Some(library) = layout.library {
            self.library(list, atlas, library, body_input, language);
        }
        if let Some(inspector) = layout.inspector {
            self.inspector(list, atlas, inspector, body_input, language);
        }
        let current = geometry_stamp(self);
        if current != geometry {
            cards = self.cards(layout.canvas);
            geometry = current;
        }
        let moving_nodes = matches!(self.drag, Some(Drag::Nodes { .. }));
        if !self.minimap_input(layout.canvas, body_input, &styles) {
            self.canvas_input(&cards, layout.canvas, body_input, language);
        }
        // Node positions can change during a held drag before history is committed.
        if moving_nodes || matches!(self.drag, Some(Drag::Nodes { .. }))
            || geometry != geometry_stamp(self)
        {
            cards = self.cards(layout.canvas);
        }
        self.paint_canvas(list, atlas, &cards, layout.canvas, input, language);
        self.paint_minimap(list, layout.canvas);
        self.paint_welcome(list, atlas, area, input, language);
        let mut footer = list.child();
        paint::rect(&mut footer, layout.footer, colors.CHROME);
        paint::rect(
            &mut footer,
            Rect::new(
                layout.footer.min[0],
                layout.footer.min[1],
                layout.footer.width(),
                1.,
            ),
            colors.BORDER,
        );
        let status = self
            .notice
            .clone()
            .or_else(|| self.diagnostic.as_ref().map(|d| d.to_string()))
            .unwrap_or_else(|| {
                self.shortcut_bindings.label(language.text("空白框选 · 中键平移 · Ctrl+滚轮缩放 · 添加节点", "Drag to select · MMB pan · Ctrl+wheel zoom · Add node"),crate::shortcuts::Scope::Nodes,crate::shortcuts::Command::AddNode)
            });
        label(
            &mut footer,
            atlas,
            Rect::new(
                layout.footer.min[0] + 9.,
                layout.footer.min[1],
                (layout.footer.width()
                    - 75.
                    - if self.diagnostic.is_some() {
                        self.diagnostic_toggle(layout.footer, &styles).width()
                    } else {
                        0.
                    })
                .max(0.),
                27.,
            ),
            &status,
            if self.diagnostic.is_some() {
                styles.color("error")
            } else {
                colors.TEXT_MUTED
            },
            Align::Left,
        );
        label(
            &mut footer,
            atlas,
            Rect::new(layout.footer.max[0] - 64., layout.footer.min[1], 58., 27.),
            &format!("{:0.0}%", self.view.zoom * 100.),
            colors.TEXT_MUTED,
            Align::Right,
        );
        footer.clip_to(layout.footer);
        list.append(footer);
        self.paint_diagnostics(
            list,
            atlas,
            area,
            if had_menu { &neutral } else { input },
            language,
        );
        self.paint_add_menu(
            list,
            atlas,
            area,
            if had_menu { input } else { &neutral },
            language,
        );
        self.paint_context_menu(
            list,
            atlas,
            area,
            if had_menu { input } else { &neutral },
            language,
        );
        response.wants_pointer = area.contains(input.mouse) || self.drag.is_some() || self.minimap_drag.is_some();
        self.paint_control_popup(
            list,
            atlas,
            area,
            if had_menu { input } else { &neutral },
            language,
        );
        self.paint_rename(list, atlas, area, input, language);
        response.wants_keyboard = self.focus;
        response.changed = revision != self.revision;
        response.semantic_changed = semantic != self.semantic_revision;
        if response.changed {
            response.actions.push(Action::Changed {
                revision: self.revision,
                semantic_revision: self.semantic_revision,
                semantic: response.semantic_changed,
            });
        }
        if preview_revision != self.preview_revision && !response.semantic_changed {
            response.actions.push(Action::PreviewChanged);
        }
        if preferences != self.view_preferences {
            response.actions.push(Action::ViewPreferencesChanged(self.view_preferences));
        }
        if std::mem::take(&mut self.recent_dirty) {
            response.actions.push(Action::RecentNodesChanged(self.recent_nodes.clone()));
        }
        response
    }

    pub(super) fn commit_field(&mut self) -> Result<(), Diagnostic> {
        let Some(field) = self.field.as_ref() else {
            return Ok(());
        };
        let binding = field.binding.clone();
        let text = field.text.clone();
        let result = self.apply_field(binding, &text);
        match result {
            Ok(()) => {
                self.field = None;
                Ok(())
            }
            Err(error) => {
                self.field.as_mut().unwrap().error = Some(error.to_string());
                self.notice = Some(error.to_string());
                Err(error)
            }
        }
    }
    pub(super) fn apply_field(&mut self, binding: Binding, text: &str) -> Result<(), Diagnostic> {
        if let Binding::ParamName(id, _)
        | Binding::ParamMin(id)
        | Binding::ParamMax(id)
        | Binding::ParamStep(id)
        | Binding::ParamUnits(id) = &binding
        {
            if !self.document.public_parameters.iter().any(|p| p.id == *id) {
                log::error!("node field binding missing before apply epoch={} revision={} binding={binding:?} parameter={id:?}", self.epoch, self.revision);
            }
        }
        let number = || {
            text.trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| diag(ErrorCode::InvalidValue, None, None, "Enter a finite number"))
        };
        match &binding {
            Binding::InputValue(id, port) => return self.set_input_text(id, port, text),
            Binding::Constant(id, component) => {
                return self.set_constant(id, *component, number()?)
            }
            Binding::ParamDefault(id, component) => {
                return self.set_parameter_default(id, *component, number()?)
            }
            _ => {}
        }
        let before = self.state(true);
        let mut document = self.document.clone();
        let mut options = self.options.clone();
        match binding {
            Binding::ParamName(id, chinese) => {
                let p = document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap();
                if chinese {
                    p.name = text.into();
                } else {
                    p.english_name = text.into();
                }
            }
            Binding::ParamMin(id) => {
                document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .min = number()?
            }
            Binding::ParamMax(id) => {
                document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .max = number()?
            }
            Binding::ParamStep(id) => {
                document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .step = number()?
            }
            Binding::ParamUnits(id) => {
                document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == id)
                    .unwrap()
                    .units = text.into()
            }
            Binding::GraphName(chinese) => {
                if chinese {
                    document.name = text.into();
                } else {
                    document.english_name = text.into();
                }
            }
            Binding::PluginId => {
                if !text.is_empty()
                    && text.len() <= 128
                    && text
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
                {
                    options.plugin_id = text.into();
                } else {
                    return Err(diag(
                        ErrorCode::InvalidValue,
                        None,
                        None,
                        "Invalid stable plugin ID",
                    ));
                }
            }
            Binding::Version => {
                if text.is_empty() || text.len() > 128 {
                    return Err(diag(
                        ErrorCode::InvalidValue,
                        None,
                        None,
                        "Enter a SemVer version",
                    ));
                }
                options.version = text.into();
            }
            Binding::Author => options.author = text.into(),
            Binding::License => options.license = text.into(),
            _ => unreachable!(),
        }
        if let Err(error) = motion_graph::validate(&self.root_with(&document), &options) {
            let diagnostic = error.diagnostic();
            if matches!(
                diagnostic.code,
                ErrorCode::InvalidValue
                    | ErrorCode::DuplicateParameter
                    | ErrorCode::Budget
                    | ErrorCode::DocumentVersion
            ) {
                return Err(diagnostic);
            }
        }
        if document == self.document
            && serde_json::to_value(&options).unwrap()
                == serde_json::to_value(&self.options).unwrap()
        {
            return Ok(());
        }
        self.document = document;
        self.options = options;
        self.finish_edit(before);
        Ok(())
    }
}
fn selection_rect(origin: [f32; 2], current: [f32; 2]) -> Rect {
    Rect::new(
        origin[0].min(current[0]),
        origin[1].min(current[1]),
        (origin[0] - current[0]).abs(),
        (origin[1] - current[1]).abs(),
    )
}
fn clipped_input(input: &Input, area: Rect) -> motion_ui::InputView<'_> {
    let mut input = motion_ui::InputSource::input_view(input);
    if !area.contains(input.mouse) {
        input.pressed = None;
    }
    input
}
