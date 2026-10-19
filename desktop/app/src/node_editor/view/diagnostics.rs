//! Read-only details for the current diagnostic. Navigation never edits a graph.
use super::*;
use motion_ui::style::StyleSheet;
use std::hash::{Hash, Hasher};

pub(super) struct DrawerLayout {
    pub area: Rect,
    pub content: Rect,
    pub locate: Rect,
}

/// Character wrapping also handles paths and IDs without whitespace, without losing text.
pub(super) fn wrapped_lines(atlas: &mut TextAtlas, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.;
        for character in paragraph.chars() {
            let advance = atlas.measure(character.encode_utf8(&mut [0; 4]));
            if !line.is_empty() && used + advance > width {
                lines.push(std::mem::take(&mut line));
                used = 0.;
            }
            line.push(character);
            used += advance;
        }
        lines.push(line);
    }
    lines
}

impl NodeEditor {
    pub(crate) fn diagnostic_toggle(&self, footer: Rect, styles: &StyleSheet) -> Rect {
        let width = styles.metric("row.height", 22.) * 4.;
        let padding = styles.metric("padding.panel", 8.);
        Rect::new(
            footer.max[0] - 64. - width,
            footer.min[1] + padding * 0.25,
            width,
            footer.height() - padding * 0.5,
        )
    }
    pub(crate) fn diagnostic_controls(&self, styles: &StyleSheet) -> (Rect, Option<Rect>) {
        (
            self.diagnostic_toggle(self.layout(self.last_area).footer, styles),
            self.diagnostic_drawer_layout(self.last_area, styles)
                .map(|drawer| drawer.locate),
        )
    }
    pub(super) fn diagnostic_drawer_layout(
        &self,
        area: Rect,
        styles: &StyleSheet,
    ) -> Option<DrawerLayout> {
        if !self.diagnostic_drawer.open || self.diagnostic.is_none() {
            return None;
        }
        let layout = self.layout(area);
        let padding = styles.metric("padding.panel", 8.);
        let header = styles.metric("panel.header.height", 24.);
        let row = styles.metric("row.height", 22.);
        let height = (header + row * 6. + padding * 2.)
            .min((layout.footer.min[1] - layout.toolbar.max[1]) * 0.55)
            .max(0.);
        let bounds = Rect::new(
            area.min[0],
            layout.footer.min[1] - height,
            area.width(),
            height,
        );
        let button_width = row * 5.;
        Some(DrawerLayout {
            area: bounds,
            content: Rect::new(
                bounds.min[0] + padding,
                bounds.min[1] + header + padding,
                (bounds.width() - padding * 3.).max(0.),
                (height - header - padding * 2.).max(0.),
            ),
            locate: Rect::new(
                bounds.max[0] - padding - button_width,
                bounds.min[1],
                button_width,
                header,
            ),
        })
    }
    pub(super) fn diagnostic_target(&self) -> Option<Vec<String>> {
        let id = self.diagnostic.as_ref()?.node.as_deref()?;
        // Match real IDs, including IDs containing '/'; never split an untrusted path blindly.
        fn find(nodes: &[Node], id: &str) -> Option<Vec<String>> {
            for node in nodes {
                if node.id == id {
                    return Some(vec![node.id.clone()]);
                }
                if let NodeOp::Group { graph } = &node.operation {
                    if let Some(rest) = id.strip_prefix(&format!("{}/", node.id)) {
                        if let Some(mut path) = find(&graph.nodes, rest) {
                            path.insert(0, node.id.clone());
                            return Some(path);
                        }
                    }
                }
            }
            None
        }
        find(&self.root_document().nodes, id)
    }
    pub(super) fn locate_diagnostic(&mut self, area: Rect, styles: &StyleSheet) -> bool {
        if self.editing() || self.popup_open() {
            return false;
        }
        let Some(mut path) = self.diagnostic_target() else {
            return false;
        };
        let id = path.pop().unwrap_or_default();
        // Reuse scope navigation to preserve edited children and parent views.
        let shared = self
            .parents
            .iter()
            .zip(&path)
            .take_while(|(parent, id)| parent.group == **id)
            .count();
        while self.parents.len() > shared {
            if !self.leave_group() {
                return false;
            }
        }
        for group in &path[shared..] {
            if !self.enter_group(group) {
                return false;
            }
        }
        let Some(node) = self.document.nodes.iter().find(|node| node.id == id) else {
            return false;
        };
        let mut visible = self.guided_layout(area, styles).canvas;
        if let Some(drawer) = self.diagnostic_drawer_layout(area, styles) {
            visible.max[1] = visible.max[1].min(drawer.area.min[1]);
        }
        // Keep collapsed state and document/history intact. The ring identifies hidden inputs too.
        let height = node_height(node, &self.document);
        let padding = styles.metric("padding.panel", 8.);
        self.view.zoom = ((visible.width() - padding * 4.) / NODE_WIDTH)
            .min((visible.height() - padding * 4.) / height)
            .clamp(0.1, 1.);
        self.view.pan = [
            visible.width() * 0.5 - (node.position[0] + NODE_WIDTH * 0.5) * self.view.zoom,
            visible.height() * 0.5 - (node.position[1] + height * 0.5) * self.view.zoom,
        ];
        self.selected = BTreeSet::from([id.clone()]);
        self.primary = Some(id);
        self.first_frame = false;
        self.search_focus = false;
        true
    }
    /// Missing sockets on the diagnostic's node only; defaults and connected inputs are valid.
    pub(super) fn diagnostic_missing_inputs(&self, node: &Node) -> BTreeSet<String> {
        if !self
            .diagnostic
            .as_ref()
            .is_some_and(|d| d.node.as_deref() == Some(self.qualified_node(&node.id).as_str()))
        {
            return BTreeSet::new();
        }
        motion_graph::node_port_specs(node, &self.document.public_parameters)
            .ok()
            .into_iter()
            .flat_map(|ports| ports.inputs.into_keys())
            .filter(|port| {
                !node.input_defaults.contains_key(port)
                    && !self
                        .document
                        .connections
                        .iter()
                        .any(|edge| edge.to == PortRef::new(&node.id, port))
            })
            .collect()
    }
    pub(super) fn diagnostics_input(
        &mut self,
        area: Rect,
        input: &Input,
        styles: &StyleSheet,
    ) -> bool {
        let Some(diagnostic) = &self.diagnostic else {
            self.diagnostic_drawer = Default::default();
            return false;
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            diagnostic.code as usize,
            &diagnostic.node,
            &diagnostic.port,
            &diagnostic.message,
        )
            .hash(&mut hash);
        let key = hash.finish();
        if key != self.diagnostic_drawer.key {
            self.diagnostic_drawer.key = key;
            self.diagnostic_drawer.scroll = Default::default();
        }
        if input.focus_lost || !self.focus {
            self.diagnostic_drawer.focused = false;
        }
        if self.add_menu.is_some() || self.context_menu.is_some() || self.inline.popup.is_some() {
            return false;
        }
        if self.editing() {
            let covered = self
                .diagnostic_drawer_layout(area, styles)
                .is_some_and(|drawer| drawer.area.contains(input.mouse));
            if covered && input.released.is_some() {
                self.cancel_canvas_interaction();
                return true;
            }
            if !input.keys.is_empty() || !input.text.is_empty() || input.focus_lost {
                // Pointer occlusion must not swallow an active editor's keys or cancellation.
                return false;
            }
            // Never complete a wire or numeric drag through an opaque drawer.
            return covered;
        }
        if hit(
            input,
            self.diagnostic_toggle(self.layout(area).footer, styles),
        ) {
            self.diagnostic_drawer.open = !self.diagnostic_drawer.open;
            self.diagnostic_drawer.focused = self.diagnostic_drawer.open;
            return true;
        }
        let Some(drawer) = self.diagnostic_drawer_layout(area, styles) else {
            return false;
        };
        if let Some((_, point)) = input.pressed {
            self.diagnostic_drawer.focused = drawer.area.contains(point);
        }
        if hit(input, drawer.locate) {
            self.locate_diagnostic(area, styles);
            return true;
        }
        if self.diagnostic_drawer.focused {
            let row = styles.metric("row.height", 22.);
            for (key, _, _) in input.key_presses() {
                match key {
                    Key::Escape => {
                        self.diagnostic_drawer.open = false;
                        self.diagnostic_drawer.focused = false;
                    }
                    Key::Enter => {
                        self.locate_diagnostic(area, styles);
                    }
                    Key::Up => self.diagnostic_drawer.scroll.offset -= row,
                    Key::Down => self.diagnostic_drawer.scroll.offset += row,
                    Key::PageUp => self.diagnostic_drawer.scroll.offset -= drawer.content.height(),
                    Key::PageDown => {
                        self.diagnostic_drawer.scroll.offset += drawer.content.height()
                    }
                    Key::Home => self.diagnostic_drawer.scroll.offset = 0.,
                    Key::End => {
                        self.diagnostic_drawer.scroll.offset = self.diagnostic_drawer.scroll.max
                    }
                    _ => {}
                }
            }
            self.diagnostic_drawer.scroll.offset = self
                .diagnostic_drawer
                .scroll
                .offset
                .clamp(0., self.diagnostic_drawer.scroll.max);
            return true;
        }
        drawer.area.contains(input.mouse)
    }
    pub(super) fn paint_diagnostics(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        let Some(diagnostic) = &self.diagnostic else {
            return;
        };
        let styles = list.styles();
        let colors = styles.colors();
        let padding = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.).max(atlas.line_height);
        let toggle = self.diagnostic_toggle(self.layout(area).footer, &styles);
        ui::button(
            list,
            atlas,
            toggle,
            if self.diagnostic_drawer.open {
                language.text("收起诊断", "Hide details")
            } else {
                language.text("诊断详情", "Details")
            },
            input,
            !self.editing() && !self.popup_open(),
        );
        let Some(drawer) = self.diagnostic_drawer_layout(area, &styles) else {
            return;
        };
        let text = format!(
            "{}: {:?}\n{}: {}\n{}: {}\n{}: {}",
            language.text("错误码", "Code"),
            diagnostic.code,
            language.text("节点", "Node"),
            diagnostic.node.as_deref().unwrap_or("—"),
            language.text("端口", "Port"),
            diagnostic.port.as_deref().unwrap_or("—"),
            language.text("原因", "Cause"),
            diagnostic.message
        );
        let lines = wrapped_lines(atlas, &text, drawer.content.width());
        let content_height = lines.len() as f32 * row;
        let wheel = if drawer.area.contains(input.mouse) && !self.popup_open() && !self.editing() {
            input.wheel[1]
        } else {
            0.
        };
        self.diagnostic_drawer
            .scroll
            .consume(wheel, drawer.content.height(), content_height);
        let mut overlay = list.child();
        paint::rect(&mut overlay, drawer.area, colors.PANEL);
        paint::stroke(
            &mut overlay,
            drawer.area,
            colors.BORDER,
            styles.metric("border", 1.),
        );
        label(
            &mut overlay,
            atlas,
            Rect::new(
                drawer.area.min[0] + padding,
                drawer.area.min[1],
                (drawer.locate.min[0] - drawer.area.min[0] - padding * 2.).max(0.),
                drawer.locate.height(),
            ),
            language.text("当前诊断", "Current diagnostic"),
            styles.color("error"),
            Align::Left,
        );
        ui::button(
            &mut overlay,
            atlas,
            drawer.locate,
            language.text("定位节点", "Locate node"),
            input,
            self.diagnostic_target().is_some() && !self.editing() && !self.popup_open(),
        );
        let mut content = list.child();
        for (index, line) in lines.iter().enumerate() {
            let y =
                drawer.content.min[1] + index as f32 * row - self.diagnostic_drawer.scroll.offset;
            if y + row > drawer.content.min[1] && y < drawer.content.max[1] {
                label(
                    &mut content,
                    atlas,
                    Rect::new(drawer.content.min[0], y, drawer.content.width(), row),
                    line,
                    colors.TEXT,
                    Align::Left,
                );
            }
        }
        content.clip_to(drawer.content);
        overlay.append(content);
        let (fraction, position) = self
            .diagnostic_drawer
            .scroll
            .bar(drawer.content.height(), content_height);
        if fraction > 0. {
            let width = styles.metric("border", 1.) * 3.;
            paint::rect(
                &mut overlay,
                Rect::new(
                    drawer.area.max[0] - padding,
                    drawer.content.min[1] + position * drawer.content.height() * (1. - fraction),
                    width,
                    drawer.content.height() * fraction,
                ),
                colors.TEXT_MUTED,
            );
        }
        overlay.clip_to(drawer.area);
        list.append(overlay);
    }
}
