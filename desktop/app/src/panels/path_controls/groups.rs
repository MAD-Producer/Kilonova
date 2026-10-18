//! Shape contents share descriptors, numeric editing and timeline key operations.
use super::*;
use motion_core::vector::groups::{
    Composite, GroupItem, ParameterValue, Repeater, StructureEdit, VectorGroup,
};
use motion_ui::{
    dock::Panel,
    icons::{self, Icon},
    widgets::{Checkbox, Dropdown, DropdownItem, NumberOptions},
};
use std::collections::{HashMap, HashSet};
mod color;
mod keys;
pub(in crate::panels) mod parameters;
pub(in crate::panels) mod timeline;
fn value(v: &[f32]) -> ParameterValue {
    match v {
        [r, g, b, a] => ParameterValue::Color([*r, *g, *b, *a]),
        [x, y] => ParameterValue::Vector([*x, *y]),
        _ => ParameterValue::Scalar(v[0]),
    }
}
fn structure(object: u64, edit: StructureEdit) -> Action {
    Action::Edit(Command::Vector {
        object,
        action: VectorAction::EditGroupStructure { edit },
    })
}
fn maximum(g: &VectorGroup) -> u64 {
    g.items
        .iter()
        .map(|i| match i {
            GroupItem::Group { group } => maximum(group),
            _ => i.id(),
        })
        .fold(g.id, u64::max)
}
fn group(vector: &VectorContent, size: [f32; 2]) -> VectorGroup {
    if let VectorSource::Group { group } = &vector.source {
        *group.clone()
    } else {
        let VectorSource::Group { group } = VectorGroup::wrap(vector.clone(), size).source else {
            unreachable!()
        };
        *group
    }
}
fn new_item(id: u64, kind: usize, locale: Locale) -> GroupItem {
    let name = |zh: &str, en: &str| format!("{} {id}", if locale == Locale::Zh { zh } else { en });
    match kind {
        0 => GroupItem::Group {
            group: Box::new(VectorGroup {
                id,
                name: name("组", "Group"),
                transform: Default::default(),
                items: vec![],
            }),
        },
        1 => GroupItem::Fill {
            id,
            name: name("填充", "Fill"),
            color: Track::constant([1.; 4]),
            fill_rule: FillRule::NonZero,
            composite: Composite::Above,
        },
        2 => GroupItem::Stroke {
            id,
            name: name("描边", "Stroke"),
            stroke: Stroke {
                color: Track::constant([1.; 4]),
                width: Track::constant(2.),
                cap: LineCap::Butt,
                join: LineJoin::Miter,
                miter_limit: 4.,
                dashes: None,
            },
            composite: Composite::Above,
        },
        3 => GroupItem::Trim {
            id,
            name: name("修剪路径", "Trim paths"),
            trim: Default::default(),
        },
        4 => GroupItem::Repeater {
            id,
            name: name("中继器", "Repeater"),
            repeater: Repeater::default(),
        },
        _ => {
            let mut vector = VectorContent::shape(if kind == 5 {
                motion_core::vector::ShapeKind::Rectangle
            } else {
                motion_core::vector::ShapeKind::Ellipse
            });
            vector.fill = None;
            vector.stroke = None;
            GroupItem::Geometry {
                id,
                name: name(
                    if kind == 5 {
                        "矩形路径"
                    } else {
                        "椭圆路径"
                    },
                    if kind == 5 { "Rectangle" } else { "Ellipse" },
                ),
                vector: Box::new(vector),
                size: Track::constant([100.; 2]),
                position: Track::constant([0.; 2]),
            }
        }
    }
}
struct Field {
    host: Panel,
    object: u64,
    item: u64,
    parameter: String,
    axis: usize,
    number: crate::panels::numeric::Field,
}
struct Menu {
    host: Panel,
    object: u64,
    parent: u64,
    item: Option<u64>,
    anchor: Rect,
    dropdown: Dropdown,
    opened: bool,
}
#[derive(Default)]
pub(in crate::panels) struct Controls {
    pub(in crate::panels) selection_probe: crate::panels::property_focus::HitProbe,
    context: Option<(String, String, u64, u32)>,
    field: Option<Field>,
    color: Option<color::Popup>,
    open: HashSet<(u64, u64, u8)>,
    initialized: HashSet<u64>,
    menu: Option<Menu>,
    dropdowns: HashMap<(u64, u64, parameters::OptionKind), Dropdown>,
    dropdown_host: Option<Panel>,
    pub(in crate::panels) cursor: Option<(Rect, bool)>,
    pub(super) keys: keys::Controls,
    popup_input: Input,
    overlays: Option<PaintList>,
    pub(crate) add_rect: Option<(Panel, Rect)>,
    #[cfg(test)]
    pub(in crate::panels) rects: HashMap<(u32, u64, u64, String, usize), (Rect, Rect)>,
    #[cfg(test)]
    pub(in crate::panels) color_rects: HashMap<(u32, u64, u64, String), Rect>,
    #[cfg(test)]
    option_rects: HashMap<(u32, u64, u64, parameters::OptionKind), Rect>,
}
fn icon(
    list: &mut PaintList,
    rect: Rect,
    kind: Icon,
    input: &Input,
    enabled: bool,
    active: bool,
) -> bool {
    let mut states = motion_ui::style::States::default();
    if !enabled {
        states = states.with(motion_ui::style::State::Disabled);
    }
    if active {
        states = states.with(motion_ui::style::State::Selected);
    }
    let sheet = list.styles();
    icons::button(list, rect, kind, input, sheet.as_ref(), states).clicked
}
pub(super) fn layout(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    depth: usize,
    axis: usize,
    axes: usize,
    host: Panel,
) -> crate::panels::timeline::PropertyLayout {
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let slot = styles.metric("icon.slot", 20.);
    if matches!(host, Panel::Timeline | Panel::Curves) {
        let mut f = crate::panels::timeline::property_layout(styles, area, y, row, axis, axes);
        let indent = ((depth + 1) as f32 * styles.metric("tree.indent", 14.))
            .min((f.number.min[0] - f.caption.min[0] - pad).max(0.));
        f.watch.min[0] += indent;
        f.watch.max[0] += indent;
        f.caption.min[0] += indent;
        f.caption.max[0] = f.caption.max[0].max(f.caption.min[0]);
        return f;
    }
    let navigation = crate::panels::key_navigation::width(styles);
    let base = area.min[0] + pad + navigation;
    let x = area.min[0]
        + (pad + navigation + styles.metric("label.column", 72.) + slot * 2.).min(area.width() * 0.7);
    let start = base + (depth as f32 * pad).min((x - base - slot - pad).max(0.));
    let field = styles.metric("field.height", 18.).min(row);
    let cell =
        ((area.max[0] - pad - x) / axes.max(1) as f32).min(styles.metric("value.min_width", 64.));
    crate::panels::timeline::PropertyLayout {
        watch: Rect::new(start, y, slot, row),
        caption: Rect::new(start + slot, y, (x - start - slot - pad).max(0.), row),
        number: Rect::new(
            x + axis as f32 * cell,
            y + (row - field) * 0.5,
            (cell - styles.metric("border", 1.) * 2.).max(0.),
            field,
        ),
    }
}
impl Controls {
    pub(crate) fn reveal_item(&mut self, object: u64, root: u64, item: u64) {
        self.open.retain(|(owner, _, _)| *owner != object);
        self.open.insert((object, root, 0));
        self.open.insert((object, item, 2));
        self.initialized.insert(object);
    }
    pub(in crate::panels) fn begin_frame(&mut self, list: &PaintList, input: &Input) {
        self.popup_input = input.clone();
        self.overlays = Some(list.child());
        self.cursor = None;
    }
    pub(in crate::panels) fn editing(&self) -> bool {
        self.field.is_some() || self.modal() || self.keys.editing()
    }
    pub(in crate::panels) fn modal(&self) -> bool {
        self.menu.is_some()
            || self.color.is_some()
            || self.dropdowns.values().any(|d| d.open)
            || self.keys.modal()
    }
    pub(in crate::panels) fn menu_in(&self, host: Panel) -> bool {
        self.menu.as_ref().is_some_and(|m| m.host == host)
            || self.color.as_ref().is_some_and(|c| c.host == host)
            || (self.dropdown_host == Some(host) && self.dropdowns.values().any(|d| d.open))
    }
    pub(in crate::panels) fn cancel(&mut self) -> bool {
        let field = self.field.take().is_some_and(|mut f| f.number.cancel());
        self.menu = None;
        self.color = None;
        for d in self.dropdowns.values_mut() {
            d.open = false;
        }
        let key = self.keys.cancel();
        field || key
    }
    fn finish(&mut self, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(3));
        }
    }
    pub(in crate::panels) fn height(
        &self,
        layer: &Layer,
        styles: &motion_ui::style::StyleSheet,
    ) -> f32 {
        fn count(n: &parameters::Node, object: u64, open: &HashSet<(u64, u64, u8)>) -> usize {
            1 + if open.contains(&(object, n.id, n.tag)) {
                n.parameters.len()
                    + n.settings.len()
                    + n.children
                        .iter()
                        .map(|c| count(c, object, open))
                        .sum::<usize>()
            } else {
                0
            }
        }
        let Content::Vector { vector } = &layer.content else {
            return 0.;
        };
        let tree = parameters::tree(vector, layer.size, 0., Locale::En);
        styles.metric("row.height", 22.) * (1 + count(&tree, layer.id, &self.open)) as f32
    }
    pub(in crate::panels) fn prepare(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            selected.unwrap_or(0),
            frame,
        );
        let changed_project = self
            .context
            .as_ref()
            .is_some_and(|c| c.0 != context.0 || c.1 != context.1);
        if changed_project
            || input.focus_lost
            || input.keys.iter().any(|(k, _)| *k == Key::Escape)
            || state["observing"] == true
        {
            if changed_project {
                self.open.clear();
                self.initialized.clear();
            }
            if self.cancel() {
                out.push(Action::History(4));
            }
        } else if self.context.as_ref() != Some(&context) {
            if self.field.take().is_some_and(|mut f| f.number.cancel()) {
                out.push(Action::History(4));
            }
        }
        self.context = Some(context);
        if self.color.as_ref().is_some_and(|c| {
            Some(c.object) != selected
                || keys::cached_layer(state, c.object, &self.keys.cache).is_none_or(|l| l.locked)
        }) {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        if self.menu.as_ref().is_some_and(|m| {
            Some(m.object) != selected
                || keys::cached_layer(state, m.object, &self.keys.cache).is_none_or(|l| l.locked)
        }) {
            self.menu = None;
        }
        for ((object, _, _), d) in &mut self.dropdowns {
            if d.open
                && (Some(*object) != selected
                    || keys::cached_layer(state, *object, &self.keys.cache)
                        .is_none_or(|l| l.locked))
            {
                d.open = false;
            }
        }
        if let Some(f) = &self.field {
            let valid=keys::cached_layer(state,f.object,&self.keys.cache).is_some_and(|l|!l.locked&&matches!(&l.content,Content::Vector{vector} if parameters::lookup_source(vector,l.size,f.item,&f.parameter,l.local_frame(f64::from(frame)),Locale::En).is_some()));
            if !valid && self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.keys.prepare(state, input, out);
    }
    pub(in crate::panels) fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        self.prepare(state, Some(layer.id), input, out);
        self.begin_frame(list, input);
        let Content::Vector { vector } = &layer.content else {
            return;
        };
        let styles = list.styles();
        let row = styles.metric("row.height", 22.);
        let pad = styles.metric("padding.panel", 8.);
        let slot = styles.metric("icon.slot", 20.);
        let modal = self.menu_in(host);
        let neutral = Input::default();
        let normal = if modal { &neutral } else { input };
        let locked = layer.locked || state["observing"] == true;
        let rect = Rect::new(
            area.min[0] + pad,
            *y,
            (area.width() - pad * 2. - slot).max(0.),
            row,
        );
        label(
            list,
            atlas,
            rect,
            if locale == Locale::Zh {
                "矢量内容"
            } else {
                "Vector contents"
            },
            styles.colors().TEXT,
        );
        let add = Rect::new(area.max[0] - pad - slot, *y, slot, row);
        self.add_rect = Some((host, add));
        if icon(list, add, Icon::Plus, normal, !locked, false) {
            self.open_menu(host, layer.id, group(vector, layer.size).id, None, add);
        }
        *y += row;
        let tree = parameters::tree(
            vector,
            layer.size,
            layer.local_frame(state["frame"].as_f64().unwrap_or(0.)),
            locale,
        );
        if self.initialized.insert(layer.id) {
            self.open.insert((layer.id, tree.id, tree.tag));
        }
        self.node(
            list, atlas, area, y, layer, &tree, state, normal, locale, host, 0, out,
        );
    }
    pub(super) fn open_menu(
        &mut self,
        host: Panel,
        object: u64,
        parent: u64,
        item: Option<u64>,
        anchor: Rect,
    ) {
        self.menu = Some(Menu {
            host,
            object,
            parent,
            item,
            anchor,
            dropdown: Dropdown::default(),
            opened: true,
        });
    }
    fn node(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        n: &parameters::Node,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let row = styles.metric("row.height", 22.);
        let slot = styles.metric("icon.slot", 20.);
        let pad = styles.metric("padding.panel", 8.);
        let x = area.min[0] + pad + depth as f32 * pad;
        let locked = layer.locked || state["observing"] == true;
        let heading = Rect::new(x, *y, (area.max[0] - pad - slot - x).max(0.), row);
        let open = self.open.contains(&(layer.id, n.id, n.tag));
        icons::draw(
            list,
            Rect::new(x, *y, slot, row),
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            slot,
            styles.colors().TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(x + slot, *y, (heading.width() - slot).max(0.), row),
            &n.name,
            styles.colors().TEXT,
        );
        if click(input, heading) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            if !self.open.remove(&(layer.id, n.id, n.tag)) {
                self.open.insert((layer.id, n.id, n.tag));
            }
        }
        let tail = Rect::new(area.max[0] - pad - slot, *y, slot, row);
        if n.tag != 1
            && icon(
                list,
                tail,
                if n.tag == 0 { Icon::Plus } else { Icon::Close },
                input,
                !locked,
                false,
            )
        {
            self.finish(out);
            if n.tag == 0 {
                self.open_menu(host, layer.id, n.id, None, tail);
            } else {
                out.push(structure(layer.id, StructureEdit::Remove { item: n.id }));
            }
        }
        if n.tag != 1
            && !(n.tag == 0 && depth == 0)
            && input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Right && heading.contains(p))
            && !locked
        {
            let p = input.mouse;
            self.open_menu(
                host,
                layer.id,
                n.id,
                Some(n.id),
                Rect::new(p[0], p[1], slot, styles.metric("border", 1.)),
            );
        }
        *y += row;
        if !open {
            return;
        }
        for p in &n.parameters {
            self.parameter(
                list,
                atlas,
                area,
                *y,
                layer.id,
                n.id,
                p,
                state,
                input,
                host,
                !locked,
                depth + 1,
                out,
            );
            *y += row;
        }
        for s in &n.settings {
            self.setting(
                list,
                atlas,
                area,
                *y,
                layer.id,
                n.id,
                s,
                state,
                input,
                host,
                !locked,
                depth + 1,
                out,
            );
            *y += row;
        }
        for child in &n.children {
            self.node(
                list,
                atlas,
                area,
                y,
                layer,
                child,
                state,
                input,
                locale,
                host,
                depth + 1,
                out,
            );
        }
    }
    pub(super) fn parameter(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        object: u64,
        item: u64,
        p: &parameters::Parameter,
        state: &Value,
        input: &Input,
        host: Panel,
        enabled: bool,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let f = layout(styles.as_ref(), area, y, depth, 0, p.values.len(), host);
        self.selection_probe.record(f.caption, area);
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let picked = click(input, f.caption) || click(input, f.watch)
            || (0..p.values.len()).any(|axis|
                click(input, layout(styles.as_ref(), area, y, depth, axis, p.values.len(), host).number));
        if picked {
            self.keys.prepare(state, input, out);
            self.keys.choose(object, item, p);
        }
        crate::panels::animation_controls::highlight_row(list,
            Rect::new(area.min[0], y, area.width(), styles.metric("row.height", 22.)),
            self.keys.selected_in(state, object, item, &p.id));
        self.keys.navigation(list, Rect::new(area.min[0], y, area.width(), styles.metric("row.height", 22.)), state, object, item, p, input, enabled, out);
        if matches!(host, Panel::Timeline | Panel::Curves) {
            crate::panels::timeline::property_branch(list, f.watch);
        }
        crate::panels::timeline::property_icon(
            list,
            f.watch,
            Icon::Stopwatch,
            p.animated(),
            enabled && !p.static_value,
        );
        if enabled && !p.static_value && click(input, f.watch) {
            self.finish(out);
            out.push(Action::Edit(p.set(
                object,
                item,
                frame,
                Some(!p.animated()),
            )));
        }
        label(list, atlas, f.caption, &p.name, styles.colors().TEXT_MUTED);
        if area.contains(input.mouse)
            && f.caption.contains(input.mouse)
            && atlas.measure(&p.name) > f.caption.width()
            && !self.modal()
        {
            let pad = styles.metric("padding.panel", 8.);
            let row = styles.metric("row.height", 22.);
            let width = (atlas.measure(&p.name) + pad * 2.).min(area.width());
            let rect = Rect::new(
                input.mouse[0].min(area.max[0] - width).max(area.min[0]),
                (input.mouse[1] + row)
                    .min(area.max[1] - row)
                    .max(area.min[1]),
                width,
                row,
            );
            let paint = self.overlays.get_or_insert_with(|| list.child());
            if let Some(style) = styles.resolve("tooltip", motion_ui::style::States::default()) {
                motion_ui::style::box_style(paint, rect, style);
                motion_ui::style::text_style(
                    paint,
                    atlas,
                    rect.shrink(styles.metric("padding", 5.)),
                    &p.name,
                    style,
                    Align::Left,
                );
            }
        }
        if p.color {
            self.color_row(
                list, atlas, area, y, object, item, p, input, host, enabled, depth, out,
            );
            return;
        }
        for axis in 0..p.values.len() {
            let rect = layout(styles.as_ref(), area, y, depth, axis, p.values.len(), host).number;
            #[cfg(test)]
            self.rects.insert(
                (host as u32, object, item, p.id.clone(), axis),
                (f.watch, rect),
            );
            self.component(
                list, atlas, rect, object, item, p, axis, state, input, host, enabled, out,
            );
        }
    }
    fn component(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        object: u64,
        item: u64,
        p: &parameters::Parameter,
        axis: usize,
        state: &Value,
        input: &Input,
        host: Panel,
        enabled: bool,
        out: &mut Vec<Action>,
    ) {
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let current = &p.values[axis];
        let active = self.field.as_ref().is_some_and(|f| {
            f.host == host
                && f.object == object
                && f.item == item
                && f.parameter == p.id
                && f.axis == axis
        });
        if enabled && click(input, rect) && !active {
            if self.color.is_some() {
                if self.field.take().is_some_and(|mut f| f.number.cancel()) {
                    out.push(Action::History(3));
                }
            } else {
                self.finish(out);
            }
            self.context = Some((
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                object,
                frame,
            ));
            self.field = Some(Field {
                host,
                object,
                item,
                parameter: p.id.clone(),
                axis,
                number: Default::default(),
            });
            out.push(Action::Seek(f64::from(frame)));
        }
        if self.field.as_ref().is_some_and(|f| {
            f.host == host
                && f.object == object
                && f.item == item
                && f.parameter == p.id
                && f.axis == axis
        }) {
            let f = self.field.as_mut().unwrap();
            let events = f.number.draw(
                list,
                atlas,
                rect,
                f64::from(*current),
                NumberOptions {
                    speed: if p.color { 0.01 } else { 1. },
                    min: p.range[0],
                    max: p.range[1],
                    decimals: if p.discrete { 0 } else { 2 },
                },
                input,
            );
            if f.number.editor.dragging() || rect.contains(input.mouse) {
                self.cursor = Some((rect, f.number.editor.dragging()));
            }
            let finished = crate::panels::numeric::apply(events, out, |v| {
                let mut p = p.clone();
                p.values[axis] = if p.discrete {
                    v.round() as f32
                } else {
                    v as f32
                };
                p.set(object, item, frame, None)
            });
            if finished {
                self.field = None;
            }
        } else {
            crate::panels::numeric::passive_with_precision(
                list,
                atlas,
                rect,
                f64::from(*current),
                input,
                enabled,
                if p.discrete { 0 } else { 2 },
            );
            if enabled && rect.contains(input.mouse) {
                self.cursor = Some((rect, false));
            }
        }
    }
    pub(super) fn setting(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        object: u64,
        item: u64,
        s: &parameters::Setting,
        _state: &Value,
        input: &Input,
        host: Panel,
        enabled: bool,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let f = layout(styles.as_ref(), area, y, depth, 0, 1, host);
        #[cfg(test)]
        self.option_rects
            .insert((host as u32, object, item, s.kind), f.number);
        label(list, atlas, f.caption, s.title, styles.colors().TEXT_MUTED);
        let original = self.popup_input.clone();
        let choice = if s.boolean {
            Checkbox::draw(
                list,
                atlas,
                f.number,
                "",
                s.selected != 0,
                input,
                enabled,
                false,
                None,
            )
            .clicked
            .then_some(usize::from(s.selected == 0))
        } else {
            let items: Vec<_> = s
                .labels
                .iter()
                .map(|label| DropdownItem {
                    label,
                    enabled: true,
                })
                .collect();
            let d = self.dropdowns.entry((object, item, s.kind)).or_default();
            let width = (area.max[0] - styles.metric("padding.panel", 8.) - f.number.min[0])
                .min(styles.metric("dropdown.menu_min_width", 160.));
            let rect = Rect::new(
                f.number.min[0],
                f.number.min[1],
                width.max(0.),
                f.number.height(),
            );
            let paint = self.overlays.get_or_insert_with(|| list.child());
            let mut idle = Dropdown::default();
            let other_host = d.open && self.dropdown_host != Some(host);
            let current = if other_host { &mut idle } else { d };
            let result = current.draw_inline(
                paint,
                atlas,
                rect,
                area,
                &items,
                s.selected,
                if current.open && self.dropdown_host == Some(host) {
                    &original
                } else {
                    input
                },
                enabled,
                self.dropdown_host == Some(host),
            );
            if current.open && !other_host {
                self.dropdown_host = Some(host);
            }
            result
        };
        if let Some(i) = choice.filter(|i| *i != s.selected) {
            self.finish(out);
            out.push(structure(
                object,
                StructureEdit::Options {
                    item,
                    edit: s.kind.edit(i),
                },
            ));
        }
    }
    pub(super) fn append_overlays(&mut self, list: &mut PaintList) {
        if let Some(paint) = self.overlays.take() {
            list.append(paint);
        }
    }
    pub(in crate::panels) fn finish_paint(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        self.append_overlays(list);
        self.overlay(list, atlas, area, state, input, locale, host, out);
    }
    pub(super) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        if self.color.as_ref().is_some_and(|c| c.host == host) {
            self.color_overlay(list, atlas, area, state, input, locale, host, out);
            return;
        }
        let Some(menu) = &mut self.menu else { return };
        if menu.host != host {
            return;
        }
        let add = if locale == Locale::Zh {
            [
                "组",
                "填充",
                "描边",
                "修剪路径",
                "中继器",
                "矩形路径",
                "椭圆路径",
            ]
        } else {
            [
                "Group",
                "Fill",
                "Stroke",
                "Trim paths",
                "Repeater",
                "Rectangle",
                "Ellipse",
            ]
        };
        let modify = if locale == Locale::Zh {
            ["上移", "下移", "删除"]
        } else {
            ["Move up", "Move down", "Delete"]
        };
        let labels: &[&str] = if menu.item.is_some() { &modify } else { &add };
        let items: Vec<_> = labels
            .iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let first = Input {
            mouse: input.mouse,
            pressed: Some((
                MouseButton::Left,
                [
                    menu.anchor.min[0] + list.styles().metric("border", 1.),
                    menu.anchor.min[1],
                ],
            )),
            ..Default::default()
        };
        let selected = menu.dropdown.draw_grouped(
            list,
            atlas,
            menu.anchor,
            area,
            &items,
            0,
            if menu.opened { &first } else { input },
            true,
            true,
            &[],
            true,
        );
        menu.opened = false;
        if let Some(index) = selected {
            if let Some(item) = menu.item {
                out.push(structure(
                    menu.object,
                    if index == 2 {
                        StructureEdit::Remove { item }
                    } else {
                        StructureEdit::Move {
                            item,
                            delta: if index == 0 { -1 } else { 1 },
                        }
                    },
                ));
            } else if let Some(l) = super::layer(state, menu.object).filter(|l| !l.locked) {
                if let Content::Vector { vector } = &l.content {
                    let g = group(vector, l.size);
                    if let Some(id) = maximum(&g).checked_add(1) {
                        out.push(structure(
                            l.id,
                            StructureEdit::Insert {
                                parent: menu.parent,
                                item: new_item(id, index, locale),
                            },
                        ));
                        self.open.insert((l.id, g.id, 0));
                        self.open.insert((l.id, id, if index == 0 { 0 } else { 2 }));
                    }
                }
            }
            self.menu = None;
        } else if !menu.dropdown.open {
            self.menu = None;
        }
    }
}

#[cfg(test)]
#[path = "groups/tests.rs"]
mod tests;
