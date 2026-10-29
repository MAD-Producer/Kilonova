//! Native authoring preview. This renderer is independent of the host shell.
mod author;
mod layout;
mod preparation;
mod session;
#[cfg(test)]
mod tests;
mod window;

use layout::{Metrics, ViewState};
use motion_effects::{native_layout::*, ParamKind};
use motion_ui::{
    paint::{self, PaintList},
    ui::{self, Align},
    widgets::{Checkbox, Dropdown, DropdownItem, NumberEditor, NumberEvent, NumberOptions},
    Input, Key, MouseButton, Rect, TextAtlas,
};
use session::Session;
use std::collections::BTreeMap;
pub use window::run;
pub use window::run_author;

#[derive(Default)]
struct Field {
    numbers: [NumberEditor; 4],
    dropdown: Dropdown,
}
#[derive(Default)]
struct Scroll {
    offset: [f32; 2],
    drag: Option<(usize, f32, f32)>,
}
pub struct Preview {
    layout: NativeLayout,
    session: Session,
    view: ViewState,
    fields: BTreeMap<String, Field>,
    color: Option<(String, usize)>,
    scroll: Scroll,
    zh: bool,
    redraw: bool,
    cursor: winit::window::CursorIcon,
}
fn tr(zh: bool, chinese: &str, english: &str) -> String {
    if zh { chinese } else { english }.into()
}
fn press(input: &Input, r: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && r.contains(p))
}
fn neutral() -> Input {
    Input {
        mouse: [f32::NEG_INFINITY; 2],
        ..Default::default()
    }
}
impl Preview {
    fn has_preview(&self) -> bool {
        fn contains(node: &LayoutNode) -> bool {
            matches!(node.content, LayoutContent::Preview) || node.children().any(contains)
        }
        contains(&self.layout.root)
    }
    pub fn new(
        layout: NativeLayout,
        definitions: Vec<motion_effects::ParamDefinition>,
        zh: bool,
    ) -> Result<Self, String> {
        layout
            .validate_bindings(&definitions)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            layout,
            session: Session::new(definitions)?,
            view: Default::default(),
            fields: BTreeMap::new(),
            color: None,
            scroll: Default::default(),
            zh,
            redraw: false,
            cursor: winit::window::CursorIcon::Default,
        })
    }
    fn cancel(&mut self) {
        self.session.cancel();
        self.color = None;
        self.scroll.drag = None;
        for field in self.fields.values_mut() {
            field.dropdown.open = false;
            for editor in &mut field.numbers {
                let _cancelled = editor.cancel();
            }
        }
    }
    fn owner(&self) -> Option<(&str, Option<usize>)> {
        self.fields.iter().find_map(|(id, f)| {
            if f.dropdown.open {
                Some((id.as_str(), None))
            } else {
                f.numbers
                    .iter()
                    .position(NumberEditor::interacting)
                    .map(|axis| (id.as_str(), Some(axis)))
            }
        })
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        input: &Input,
    ) -> Result<Option<Rect>, String> {
        self.redraw = false;
        self.cursor = winit::window::CursorIcon::Default;
        if input.focus_lost
            || (!input.composing && input.keys.iter().any(|(k, _)| *k == Key::Escape))
        {
            self.cancel();
        }
        // Commands must not also edit a focused numeric field or an open popup.
        if self.owner().is_none() && self.color.is_none() && !input.composing && !input.ime_consumed
        {
            for (key, m, _) in input.key_presses() {
                if m.control {
                    match key {
                        Key::Z if m.shift => self.session.redo(),
                        Key::Z => self.session.undo(),
                        Key::Y => self.session.redo(),
                        _ => {}
                    }
                }
            }
        }
        let styles = list.styles();
        let colors = styles.colors();
        let metrics = Metrics::new(&styles);
        let mut content = PaintList {
            stylesheet: Some(styles.clone()),
            ..Default::default()
        };
        let size = metrics.measure(&self.layout.root, &self.view);
        let bar = styles.metric("plugin.preview.scrollbar.width", 8.);
        let viewport = Rect::new(
            area.min[0],
            area.min[1],
            (area.width() - bar).max(0.),
            (area.height() - bar).max(0.),
        );
        let extent = [
            size[0].max(viewport.width()),
            size[1].max(viewport.height()),
        ];
        let exclusive = self.owner().map(|(id, axis)| (id.to_owned(), axis));
        let mut clipped_input = input.clone();
        if clipped_input
            .pressed
            .is_some_and(|(_, p)| !viewport.contains(p))
        {
            clipped_input.pressed = None;
        }
        let no_input = neutral();
        let scrolling = exclusive.is_none() && self.color.is_none();
        if scrolling && viewport.contains(input.mouse) {
            if input.modifiers.shift {
                self.scroll.offset[0] += input.wheel[1];
            } else {
                self.scroll.offset[1] += input.wheel[1];
            }
            self.scroll.offset[0] += input.wheel[0];
        }
        for axis in 0..2 {
            let length = if axis == 0 {
                viewport.width()
            } else {
                viewport.height()
            };
            let max = (extent[axis] - length).max(0.);
            self.scroll.offset[axis] = self.scroll.offset[axis].clamp(0., max);
            if max <= 0. {
                continue;
            }
            let track = if axis == 0 {
                Rect::new(viewport.min[0], viewport.max[1], length, bar)
            } else {
                Rect::new(viewport.max[0], viewport.min[1], bar, length)
            };
            let thumb_length = (length * length / extent[axis])
                .max(styles.metric("row.height", 22.))
                .min(length);
            let start = track.min[axis] + (length - thumb_length) * self.scroll.offset[axis] / max;
            let mut thumb = track;
            thumb.min[axis] = start;
            thumb.max[axis] = start + thumb_length;
            paint::rect(list, track, colors.SUNKEN);
            paint::rect(list, thumb, colors.BORDER);
            if scrolling && press(input, track) {
                self.scroll.drag = Some((axis, input.mouse[axis], self.scroll.offset[axis]));
            }
            if let Some((a, origin, offset)) = self.scroll.drag {
                if a == axis {
                    if input.held == Some(MouseButton::Left) {
                        self.scroll.offset[axis] = (offset
                            + (input.mouse[axis] - origin) * max
                                / (length - thumb_length).max(f32::EPSILON))
                        .clamp(0., max);
                    } else {
                        self.scroll.drag = None;
                    }
                }
            }
        }
        let root = Rect::new(
            viewport.min[0] - self.scroll.offset[0],
            viewport.min[1] - self.scroll.offset[1],
            extent[0],
            extent[1],
        );
        let mut placed = Vec::new();
        metrics.place(&self.layout.root, root, &self.view, &mut placed);
        let mut preview = None;
        let mut popup = PaintList {
            stylesheet: Some(styles.clone()),
            ..Default::default()
        };
        for item in placed {
            let r = item.rect;
            let hit = r.intersection(viewport);
            if hit.width() <= 0. || hit.height() <= 0. {
                continue;
            }
            let allowed = self.color.is_none()
                && exclusive.as_ref().is_none_or(|(id, _)| *id == item.node.id);
            let event = if !allowed {
                &no_input
            } else if exclusive
                .as_ref()
                .is_some_and(|(id, _)| *id == item.node.id)
            {
                input
            } else if viewport.contains(input.mouse) {
                &clipped_input
            } else {
                &no_input
            };
            match &item.node.content {
                LayoutContent::Group {
                    title, collapsible, ..
                } => {
                    let header = Rect::new(r.min[0], r.min[1], r.width(), metrics.header);
                    paint::rect(&mut content, header, colors.HEADER);
                    let folded = self.view.folded.contains(&item.node.id);
                    let icon = if folded {
                        motion_ui::icons::Icon::ChevronRight
                    } else {
                        motion_ui::icons::Icon::ChevronDown
                    };
                    let slot = styles.metric("icon.slot", 20.);
                    if *collapsible {
                        motion_ui::icons::draw(
                            &mut content,
                            Rect::new(r.min[0], r.min[1], slot, metrics.header),
                            icon,
                            styles.metric("font.small", 11.),
                            colors.TEXT_MUTED,
                        );
                    }
                    ui::text(
                        &mut content,
                        atlas,
                        Rect::new(
                            r.min[0] + slot,
                            r.min[1],
                            (r.width() - slot).max(0.),
                            metrics.header,
                        ),
                        title,
                        colors.TEXT,
                        Align::Left,
                    );
                    if *collapsible && press(event, header.intersection(hit)) {
                        if folded {
                            self.view.folded.remove(&item.node.id);
                        } else {
                            self.view.folded.insert(item.node.id.clone());
                        }
                        self.redraw = true;
                    }
                }
                LayoutContent::Tabs { tabs } => {
                    let header = Rect::new(r.min[0], r.min[1], r.width(), metrics.header);
                    let active = self.view.tabs.entry(item.node.id.clone()).or_default();
                    if let Some(next) = ui::tab_strip(
                        &mut content,
                        atlas,
                        header,
                        &tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
                        *active,
                        event,
                    ) {
                        *active = next;
                        self.redraw = true;
                    }
                }
                LayoutContent::Parameter { parameter, .. } => {
                    let index = self
                        .session
                        .definitions
                        .iter()
                        .position(|p| p.id == *parameter)
                        .ok_or("validated layout lost parameter binding")?;
                    let definition = &self.session.definitions[index];
                    let value = self.session.values()[index];
                    let field = self.fields.entry(item.node.id.clone()).or_default();
                    let gap = styles.metric("effect.number.gap", 4.);
                    let field_height = styles.metric("field.height", 18.);
                    let label_width = styles.metric("label.column", 72.).max(
                        (r.width() * 0.5).min(styles.metric("inspector.value.max_width", 192.)),
                    );
                    ui::text(
                        &mut content,
                        atlas,
                        Rect::new(r.min[0], r.min[1], (label_width - gap).max(0.), metrics.row),
                        &definition.name,
                        colors.TEXT,
                        Align::Left,
                    );
                    let values = Rect::new(
                        r.min[0] + label_width,
                        r.min[1] + (metrics.row - field_height) * 0.5,
                        (r.width() - label_width).max(0.),
                        field_height,
                    );
                    match definition.kind {
                        ParamKind::Bool => {
                            if Checkbox::draw(
                                &mut content,
                                atlas,
                                values,
                                "",
                                value[0] != 0.,
                                event,
                                true,
                                false,
                                None,
                            )
                            .clicked
                            {
                                let mut next = value;
                                next[0] = if value[0] == 0. { 1. } else { 0. };
                                self.session.set(index, next)?;
                                self.session.commit();
                            }
                        }
                        ParamKind::Enum => {
                            let enum_area = Rect::new(
                                values.min[0],
                                values.min[1],
                                values
                                    .width()
                                    .min(styles.metric("effect.enum.max_width", 112.)),
                                values.height(),
                            );
                            let items: Vec<_> = definition
                                .options
                                .iter()
                                .map(|label| DropdownItem {
                                    label,
                                    enabled: true,
                                })
                                .collect();
                            // Menus are painted after the clipped body and own all input.
                            let target = if field.dropdown.open {
                                &mut popup
                            } else {
                                &mut content
                            };
                            if let Some(next) = field.dropdown.draw_inline(
                                target,
                                atlas,
                                enum_area,
                                bounds,
                                &items,
                                value[0] as usize,
                                event,
                                true,
                                false,
                            ) {
                                let mut value = value;
                                value[0] = next as f32;
                                self.session.set(index, value)?;
                                self.session.commit();
                            }
                        }
                        ParamKind::Color => {
                            let swatch = Rect::new(
                                values.min[0],
                                values.min[1],
                                styles.metric("effect.number.cell_width", 44.),
                                field_height,
                            );
                            crate::panels::color_well::swatch(
                                &mut content,
                                swatch,
                                &value,
                                event,
                                true,
                            );
                            ui::text(
                                &mut content,
                                atlas,
                                Rect::new(
                                    swatch.max[0] + gap,
                                    values.min[1],
                                    (values.max[0] - swatch.max[0] - gap).max(0.),
                                    field_height,
                                ),
                                &crate::panels::color_well::caption(&value),
                                colors.TEXT_MUTED,
                                Align::Left,
                            );
                            if press(event, swatch.intersection(hit)) {
                                self.session.begin();
                                self.color = Some((item.node.id.clone(), index));
                            }
                        }
                        _ => {
                            let cell = styles.metric("effect.number.cell_width", 44.);
                            let opts = NumberOptions {
                                speed: definition.step as f64,
                                min: definition.min as f64,
                                max: definition.max as f64,
                                decimals: 2,
                            };
                            let mut next = value;
                            for axis in 0..definition.kind.dimensions() {
                                let number = &mut field.numbers[axis];
                                number.compact = true;
                                number.align_left = true;
                                number.drag_threshold =
                                    Some(styles.metric("motion.drag_threshold", 3.));
                                let nr = Rect::new(
                                    values.min[0] + axis as f32 * (cell + gap),
                                    values.min[1],
                                    cell,
                                    field_height,
                                );
                                let axis_input = if exclusive
                                    .as_ref()
                                    .is_none_or(|(_, a)| a.is_none_or(|a| a == axis))
                                {
                                    event
                                } else {
                                    &no_input
                                };
                                for change in
                                    number.update(value[axis] as f64, opts, axis_input, nr)
                                {
                                    match change {
                                        NumberEvent::Begin { .. } => self.session.begin(),
                                        NumberEvent::Change { value } => {
                                            next[axis] = value as f32;
                                            self.session.set(index, next)?;
                                        }
                                        NumberEvent::Commit { value } => {
                                            next[axis] = value as f32;
                                            self.session.set(index, next)?;
                                            self.session.commit();
                                        }
                                        NumberEvent::Cancel { .. } => self.session.cancel(),
                                    }
                                }
                                let mut states = motion_ui::style::States::default();
                                if nr.contains(axis_input.mouse) {
                                    states = states.with(motion_ui::style::State::Hovered);
                                }
                                if nr.contains(axis_input.mouse) || number.dragging() {
                                    self.cursor = if number.text.active() {
                                        winit::window::CursorIcon::Text
                                    } else {
                                        winit::window::CursorIcon::EwResize
                                    };
                                }
                                number.draw_styled(
                                    &mut content,
                                    atlas,
                                    nr,
                                    self.session.values()[index][axis] as f64,
                                    opts,
                                    &styles,
                                    states,
                                );
                            }
                        }
                    }
                    let border = styles.metric("border", 1.);
                    paint::rect(
                        &mut content,
                        Rect::new(r.min[0], r.min[1] + metrics.row - border, r.width(), border),
                        colors.SUNKEN,
                    );
                }
                LayoutContent::Preview => preview = Some(r.intersection(viewport)),
                LayoutContent::Note { text } => {
                    ui::text(&mut content, atlas, r, text, colors.TEXT_MUTED, Align::Left)
                }
                LayoutContent::Timeline => ui::text(
                    &mut content,
                    atlas,
                    r,
                    &tr(
                        self.zh,
                        "时间轴需要连接宿主时间上下文",
                        "Timeline requires a host time context",
                    ),
                    colors.TEXT_MUTED,
                    Align::Left,
                ),
                _ => {}
            }
        }
        content.clip_to(viewport);
        list.append(content);
        list.append(popup);
        if let Some((id, index)) = self.color.clone() {
            let pad = metrics.pad;
            let width = styles.metric("plugin.preview.row.min_width", 288.);
            let height = metrics.header + metrics.row * 5. + pad * 2.;
            let r = Rect::new(
                bounds.min[0] + (bounds.width() - width).max(0.) * 0.5,
                bounds.min[1] + (bounds.height() - height).max(0.) * 0.5,
                width,
                height,
            );
            paint::rect(list, r, styles.color("raised"));
            paint::stroke(list, r, colors.BORDER, styles.metric("border", 1.));
            ui::text(
                list,
                atlas,
                Rect::new(
                    r.min[0] + pad,
                    r.min[1],
                    r.width() - pad * 2.,
                    metrics.header,
                ),
                &tr(self.zh, "颜色 · 实时预览", "Color · live preview"),
                colors.TEXT,
                Align::Left,
            );
            let field = self
                .fields
                .get_mut(&id)
                .ok_or("color preview control disappeared")?;
            let definition = &self.session.definitions[index];
            let opts = NumberOptions {
                speed: definition.step as f64,
                min: definition.min as f64,
                max: definition.max as f64,
                decimals: 2,
            };
            for axis in 0..4 {
                let row = Rect::new(
                    r.min[0] + pad,
                    r.min[1] + metrics.header + axis as f32 * metrics.row,
                    r.width() - pad * 2.,
                    metrics.row,
                );
                ui::text(
                    list,
                    atlas,
                    row,
                    ["R", "G", "B", "A"][axis],
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                let nr = Rect::new(
                    row.min[0] + metrics.min_width * 0.5,
                    row.min[1],
                    styles.metric("value.min_width", 64.),
                    metrics.row,
                );
                let number = &mut field.numbers[axis];
                number.drag_threshold = Some(styles.metric("motion.drag_threshold", 3.));
                for change in
                    number.update(self.session.values()[index][axis] as f64, opts, input, nr)
                {
                    if let NumberEvent::Change { value } | NumberEvent::Commit { value } = change {
                        let mut next = self.session.values()[index];
                        next[axis] = value as f32;
                        self.session.set(index, next)?;
                    }
                }
                number.draw(
                    list,
                    atlas,
                    nr,
                    self.session.values()[index][axis] as f64,
                    opts,
                );
            }
            let y = r.max[1] - metrics.row - pad;
            let half = (r.width() - pad * 3.) * 0.5;
            if ui::button(
                list,
                atlas,
                Rect::new(r.min[0] + pad, y, half, metrics.row),
                &tr(self.zh, "取消", "Cancel"),
                input,
                true,
            )
            .clicked
            {
                self.cancel();
            }
            if ui::button(
                list,
                atlas,
                Rect::new(r.min[0] + pad * 2. + half, y, half, metrics.row),
                &tr(self.zh, "应用", "Apply"),
                input,
                true,
            )
            .clicked
            {
                self.session.commit();
                self.color = None;
            }
        }
        Ok(preview)
    }
}
