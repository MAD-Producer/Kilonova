//! Authored effect channels in the timeline. Commands keep composition time;
//! sampling keeps the model's local clock. No synthetic transform channel.
use super::*;
use motion_core::{Ease, Easing, EffectAction, EffectParam};
use std::collections::BTreeSet;
pub(super) mod graph;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) enum PlannedRow {
    Effect { index: usize, open: bool },
    Parameter { index: usize, param: String },
}
pub(super) fn plan(row: &timeline::Row, state: &types::TimelineState) -> Vec<PlannedRow> {
    let mut rows = vec![];
    for (index, effect) in row.effects.iter().enumerate() {
        let open = !state
            .collapsed_effect_items
            .contains(&(row.id, effect["id"].as_u64().unwrap_or(0)));
        rows.push(PlannedRow::Effect { index, open });
        if open {
            let mut params: Vec<_> = effect["params"].as_object().into_iter().flatten().collect();
            params.sort_by_key(|(name, parameter)| (parameter["kind"] == "curve", *name));
            rows.extend(params.into_iter().map(|(param, _)| PlannedRow::Parameter {
                index,
                param: param.clone(),
            }));
        }
    }
    rows
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    root: String,
    composition: String,
    object: u64,
    effect: u64,
    param: String,
    hash: String,
}
impl Target {
    fn new(state: &Value, object: u64, effect: u64, param: &str) -> Self {
        Self {
            root: state["root"].as_str().unwrap_or("").into(),
            composition: state["composition"].as_str().unwrap_or("").into(),
            object,
            effect,
            param: param.into(),
            hash: state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|l| l["id"] == object)
                .and_then(|l| l["effects"].as_array())
                .into_iter()
                .flatten()
                .find(|e| e["id"] == effect)
                .and_then(|e| e["hash"].as_str())
                .unwrap_or("")
                .into(),
        }
    }
    fn command(&self, action: EffectAction) -> Command {
        Command::Effect {
            object: self.object,
            action,
        }
    }
    fn source<'a>(&self, state: &'a Value) -> Option<(&'a Value, &'a Value)> {
        if self.root != state["root"].as_str().unwrap_or("")
            || self.composition != state["composition"].as_str().unwrap_or("")
        {
            return None;
        }
        let layer = state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|l| l["id"] == self.object)?;
        let effect = layer["effects"]
            .as_array()?
            .iter()
            .find(|e| e["id"] == self.effect)?;
        if effect["hash"].as_str().unwrap_or("") != self.hash {
            return None;
        }
        Some((layer, effect["params"].get(&self.param)?))
    }
    fn parameter(&self, state: &Value) -> Option<EffectParam> {
        let (_, source) = self.source(state)?;
        observe!(
            "timeline.effect_parameter",
            serde_json::from_value(source.clone()),
            "object={} effect={} parameter={}",
            self.object,
            self.effect,
            self.param
        )
        .ok()
    }
    fn editable(&self, state: &Value) -> bool {
        self.source(state)
            .is_some_and(|(l, _)| l["locked"] != true && state["observing"] != true)
    }
}
struct Field {
    target: Target,
    component: usize,
    number: numeric::Field,
    rect: Rect,
}
struct KeyDrag {
    target: Target,
    origin: u32,
    current: u32,
    point: [f32; 2],
    value: [f32; 4],
    occupied: BTreeSet<u32>,
    copy: bool,
    copied: bool,
    begun: bool,
    ppf: f32,
    offset: f64,
    value_per_pixel: f32,
    component: usize,
    bounds: Option<[f32; 2]>,
}
struct HandleDrag {
    target: Target,
    frame: u32,
    index: usize,
    easing: Easing,
    start: f64,
    duration: f64,
    a: f32,
    b: f32,
    point: [f32; 2],
    plot: Rect,
    bounds: [f32; 2],
    ppf: f32,
    offset: f64,
    begun: bool,
}
struct Menu {
    target: Target,
    frame: u32,
    point: [f32; 2],
    opened: bool,
}
#[derive(Default)]
pub(crate) struct Controls {
    field: Option<Field>,
    key: Option<(Target, u32)>,
    drag: Option<KeyDrag>,
    handle: Option<HandleDrag>,
    menu: Option<Menu>,
    dropdowns: std::collections::HashMap<(u64, u64, String), motion_ui::widgets::Dropdown>,
    enum_owner: Option<(u64, u64, String)>,
    dropdown_paint: Option<PaintList>,
    pub(crate) probe: property_focus::HitProbe,
    probe_parameter: Option<(u64, String)>,
    #[cfg(test)]
    pub(super) rects: std::collections::HashMap<(u64, String, i32), Rect>,
}
impl Controls {
    pub(crate) fn begin_probe(&mut self, effect: u64, parameter: &str) {
        self.probe.start();
        self.probe_parameter = Some((effect, parameter.into()));
    }
    pub(crate) fn editing(&self) -> bool {
        self.field.is_some()
            || self.drag.is_some()
            || self.handle.is_some()
            || self.menu.is_some()
            || self.enum_owner.is_some()
    }
    pub(crate) fn gesture_active(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.begun)
            || self.handle.as_ref().is_some_and(|d| d.begun)
            || self
                .field
                .as_ref()
                .is_some_and(|f| f.number.editor.dragging())
    }
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }
    pub(crate) fn dropdown_open(&self) -> bool {
        self.enum_owner.is_some()
    }
    pub(crate) fn cancel(&mut self) -> bool {
        self.menu = None;
        self.enum_owner = None;
        self.dropdowns.clear();
        self.dropdown_paint = None;
        self.field.take().is_some_and(|mut f| f.number.cancel())
            | self.drag.take().is_some_and(|d| d.begun)
            | self.handle.take().is_some_and(|d| d.begun)
    }
}
fn selected(panels: &Panels, state: &Value) -> Option<Target> {
    let (object, effect, param) = panels
        .effect_controls
        .selected_parameter(state)
        .or_else(|| panels.properties.effects.selected_parameter(state))?;
    (panels.selected == Some(object)).then(|| Target::new(state, object, effect, param))
}
fn choose(panels: &mut Panels, state: &Value, target: &Target, frame: Option<u32>) {
    if panels.selected != Some(target.object) {
        panels.select_only(Some(target.object));
    }
    panels
        .effect_controls
        .choose_parameter(state, target.object, target.effect, &target.param);
    panels.effect_timeline.key = frame.map(|f| (target.clone(), f));
    property_focus::settle(panels);
}
fn frames(parameter: &EffectParam, offset: f64) -> Vec<f64> {
    if let Some(curve) = &parameter.curve {
        curve.keys.iter().map(|k| k.frame as f64 + offset).collect()
    } else {
        parameter
            .track
            .keys
            .iter()
            .map(|k| k.frame as f64 + offset)
            .collect()
    }
}
fn composition_frame(frame: f64, state: &Value) -> Option<u32> {
    key_navigation::editable_frame(frame, last_frame(state))
}
fn row_input(input: &Input, body: Rect) -> Input {
    let mut input = input.clone();
    if input.pressed.is_some_and(|(_, p)| !body.contains(p)) {
        input.pressed = None;
    }
    input
}
pub(super) fn prepare(panels: &mut Panels, state: &Value, input: &Input, out: &mut Vec<Action>) {
    panels.effect_timeline.dropdown_paint = None;
    let active = selected(panels, state);
    // Keep only the active dropdown between frames; passive visible anchors
    // may allocate their own entries during this paint, never deleted objects.
    panels
        .effect_timeline
        .dropdowns
        .retain(|(object, effect, param), _| {
            active
                .as_ref()
                .is_some_and(|t| t.object == *object && t.effect == *effect && t.param == *param)
        });
    let valid = |target: &Target| {
        active.as_ref() == Some(target)
            && target.editable(state)
            && composition_frame(state["frame"].as_f64().unwrap_or(0.), state).is_some()
            && (panels.timeline.graph_mode
                || (panels.timeline.expanded.contains(&target.object)
                    && panels.timeline.expanded_effects.contains(&target.object)
                    && !panels
                        .timeline
                        .collapsed_effect_items
                        .contains(&(target.object, target.effect))))
    };
    let invalid = input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        || panels
            .effect_timeline
            .field
            .as_ref()
            .is_some_and(|f| !valid(&f.target))
        || panels
            .effect_timeline
            .drag
            .as_ref()
            .is_some_and(|d| !valid(&d.target))
        || panels
            .effect_timeline
            .handle
            .as_ref()
            .is_some_and(|d| !valid(&d.target));
    let invalid = invalid
        || panels
            .effect_timeline
            .enum_owner
            .as_ref()
            .is_some_and(|(object, effect, param)| {
                active.as_ref().is_none_or(|target| {
                    target.object != *object
                        || target.effect != *effect
                        || target.param != *param
                        || !valid(target)
                        || target
                            .parameter(state)
                            .is_none_or(|p| p.kind != motion_effects::ParamKind::Enum)
                })
            });
    if invalid && panels.effect_timeline.cancel() {
        out.push(Action::History(4));
    }
    if panels
        .effect_timeline
        .key
        .as_ref()
        .is_some_and(|(t, _)| active.as_ref() != Some(t))
    {
        panels.effect_timeline.key = None;
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    body: Rect,
    track: Rect,
    y: f32,
    height: f32,
    ppf: f32,
    offset: f32,
    row: &timeline::Row,
    rows: &[PlannedRow],
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    original_input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let input = row_input(input, body);
    for (r, planned) in rows.iter().enumerate() {
        let py = y + r as f32 * height;
        if py < body.min[1] || py + height > body.max[1] {
            continue;
        }
        let (index, param) = match planned {
            PlannedRow::Effect { index, .. } => (*index, None),
            PlannedRow::Parameter { index, param } => (*index, Some(param.as_str())),
        };
        let effect = &row.effects[index];
        let id = effect["id"].as_u64().unwrap_or(0);
        let definition = panels.effect_controls.definition(effect);
        let layout = timeline::property_layout(&styles, area, py, height, 0, 1);
        if let PlannedRow::Effect { open, .. } = planned {
            let slot = styles.metric("icon.slot", 20.);
            let indent = styles.metric("tree.indent", 14.);
            let arrow = Rect::new(layout.watch.min[0] - slot + indent, py, slot, height);
            if ui::button_icon(
                list,
                arrow,
                if *open {
                    motion_ui::icons::Icon::ChevronDown
                } else {
                    motion_ui::icons::Icon::ChevronRight
                },
                &input,
                true,
            )
            .clicked
            {
                if !panels.timeline.collapsed_effect_items.remove(&(row.id, id)) {
                    panels.timeline.collapsed_effect_items.insert((row.id, id));
                }
            }
            if motion_ui::widgets::Checkbox::draw(
                list,
                atlas,
                Rect::new(
                    layout.watch.min[0] + indent,
                    py,
                    layout.watch.width(),
                    height,
                ),
                "",
                effect["enabled"] != false,
                &input,
                !row.locked && state["observing"] != true,
                false,
                None,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Effect {
                    object: row.id,
                    action: EffectAction::Enable {
                        effect: id,
                        enabled: effect["enabled"] == false,
                    },
                }));
            }
            let title = definition
                .as_ref()
                .and_then(|d| {
                    d[if panels.locale == Locale::Zh {
                        "name"
                    } else {
                        "english_name"
                    }]
                    .as_str()
                })
                .unwrap_or_else(|| effect["effect"].as_str().unwrap_or("Effect"));
            label(
                list,
                atlas,
                Rect::new(
                    layout.caption.min[0] + indent,
                    py,
                    (area.max[0] - layout.caption.min[0] - indent).max(0.),
                    height,
                ),
                title,
                false,
            );
            continue;
        }
        let param = param.unwrap();
        let target = Target::new(state, row.id, id, param);
        let Some(parameter) = target.parameter(state) else {
            continue;
        };
        let parameter_def = definition
            .as_ref()
            .and_then(|d| d["params"].as_array())
            .and_then(|ps| ps.iter().find(|p| p["id"] == param));
        let name = parameter_def
            .and_then(|p| p["name"].as_str())
            .unwrap_or(param);
        let indent = styles.metric("tree.indent", 14.) * 2.;
        let watch = Rect::new(
            layout.watch.min[0] + indent,
            py,
            styles.metric("icon.slot", 20.),
            height,
        );
        let caption = Rect::new(
            layout.caption.min[0] + indent,
            py,
            (layout.caption.width() - indent).max(0.),
            height,
        );
        let selected = selected(panels, state).as_ref() == Some(&target);
        animation_controls::highlight_row(
            list,
            Rect::new(area.min[0], py, area.width(), height),
            selected,
        );
        label(list, atlas, caption, name, !parameter.implemented);
        if caption.contains(input.mouse) {
            panels.drag_readout = Some((input.mouse, name.into()));
        }
        if panels
            .effect_timeline
            .probe_parameter
            .as_ref()
            .is_some_and(|(effect, name)| *effect == id && name == param)
        {
            panels.effect_timeline.probe.record(caption, body);
        }
        #[cfg(test)]
        panels
            .effect_timeline
            .rects
            .insert((id, param.into(), -1), caption);
        if clicked(&input, caption) {
            choose(panels, state, &target, None);
        }
        let at = state["frame"].as_f64().unwrap_or(0.);
        let local = at - row.mask_offset as f64;
        let frame = composition_frame(at, state);
        let enabled = target.editable(state) && parameter.implemented && frame.is_some();
        let times = frames(&parameter, row.mask_offset as f64);
        let neighbours = key_navigation::neighbours(times.iter().copied(), at, last_frame(state));
        if let Some(choice) = key_navigation::draw(
            list,
            key_navigation::rail(&styles, Rect::new(area.min[0], py, area.width(), height)),
            &input,
            neighbours,
            enabled && parameter.animatable,
        ) {
            choose(panels, state, &target, None);
            match choice {
                key_navigation::Choice::Seek(time) => out.push(Action::Seek(time)),
                key_navigation::Choice::Toggle => {
                    let frame = frame.unwrap();
                    if neighbours.current {
                        out.push(Action::Edit(target.command(EffectAction::DeleteKey {
                            effect: id,
                            param: param.into(),
                            frame,
                        })));
                    } else {
                        let mut commands = vec![target.command(EffectAction::Animate {
                            effect: id,
                            param: param.into(),
                            frame,
                            enabled: true,
                        })];
                        if parameter.curve.is_none() {
                            commands.push(target.command(EffectAction::Set {
                                effect: id,
                                param: param.into(),
                                frame,
                                value: parameter.sample(local),
                            }));
                        }
                        out.push(Action::EditBatch(commands));
                    }
                }
            }
        }
        let animated = !frames(&parameter, 0.).is_empty();
        let mut states = motion_ui::style::States::default();
        if animated {
            states = states.with(motion_ui::style::State::Selected);
        }
        if !enabled || !parameter.animatable {
            states = states.with(motion_ui::style::State::Disabled);
        }
        if motion_ui::icons::button(
            list,
            watch,
            motion_ui::icons::Icon::Stopwatch,
            &input,
            &styles,
            states,
        )
        .clicked
        {
            choose(panels, state, &target, None);
            out.push(Action::Edit(target.command(EffectAction::Animate {
                effect: id,
                param: param.into(),
                frame: frame.unwrap(),
                enabled: !animated,
            })));
        }
        if input.pressed.is_some_and(|(_, p)| {
            Rect::new(
                layout.number.min[0],
                py,
                area.max[0] - layout.number.min[0],
                height,
            )
            .contains(p)
        }) {
            choose(panels, state, &target, None);
        }
        draw_value(
            list,
            atlas,
            area,
            py,
            height,
            &layout,
            &target,
            &parameter,
            parameter_def,
            local,
            frame,
            &input,
            original_input,
            enabled,
            panels,
            out,
        );
        if panels.timeline.graph_mode {
            continue;
        }
        for time in frames(&parameter, row.mask_offset as f64) {
            let x = track.min[0] + (time as f32 - offset) * ppf;
            if x < track.min[0] || x >= track.max[0] {
                continue;
            }
            let point = [x, py + height * 0.5];
            let chosen = panels
                .effect_timeline
                .key
                .as_ref()
                .is_some_and(|(t, f)| t == &target && *f as f64 == time);
            paint::diamond(
                list,
                point,
                styles.metric("timeline.key.radius", 4.),
                styles.color(if chosen { "focus" } else { "keyframe" }),
            );
            #[cfg(test)]
            panels.effect_timeline.rects.insert(
                (id, param.into(), time as i32),
                Rect::new(x, point[1], 0., 0.),
            );
            let hit = styles.metric("timeline.key.hit_radius", 5.);
            let rect = Rect::new(x - hit, py, hit * 2., height);
            if clicked(&input, rect) {
                if let Some(frame) = composition_frame(time, state) {
                    begin_key(
                        panels,
                        state,
                        target.clone(),
                        frame,
                        point,
                        &input,
                        ppf,
                        offset as f64,
                        0.,
                        0,
                        out,
                    );
                }
            }
            if right_clicked(&input, rect) {
                if let Some(frame) = composition_frame(time, state) {
                    open_menu(panels, state, target.clone(), frame, input.mouse);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_value(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: f32,
    height: f32,
    layout: &timeline::PropertyLayout,
    target: &Target,
    parameter: &EffectParam,
    definition: Option<&Value>,
    local: f64,
    frame: Option<u32>,
    input: &Input,
    original_input: &Input,
    enabled: bool,
    panels: &mut Panels,
    out: &mut Vec<Action>,
) {
    use motion_effects::ParamKind;
    let styles = list.styles();
    let width = (area.max[0] - layout.number.min[0] - styles.metric("padding.panel", 8.)).max(0.);
    let values = Rect::new(layout.number.min[0], y, width, height);
    let mut value = parameter.sample(local);
    if parameter.kind == ParamKind::Curve {
        if ui::button_styled(
            list,
            atlas,
            values,
            if panels.locale == Locale::Zh {
                "编辑曲线"
            } else {
                "Edit curve"
            },
            input,
            true,
            &styles,
            "button.icon",
        )
        .clicked
        {
            out.push(Action::ShowPanel(Panel::EffectControls.dock()));
        }
        return;
    }
    if parameter.kind == ParamKind::Bool {
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            values,
            "",
            value[0] > 0.5,
            input,
            enabled,
            false,
            None,
        )
        .clicked
        {
            value[0] = if value[0] > 0.5 { 0. } else { 1. };
            out.push(Action::Edit(target.command(EffectAction::Set {
                effect: target.effect,
                param: target.param.clone(),
                frame: frame.unwrap(),
                value,
            })));
        }
        return;
    }
    if parameter.kind == ParamKind::Enum {
        let key = (target.object, target.effect, target.param.clone());
        let options: Vec<_> = definition
            .into_iter()
            .flat_map(|d| d["options"].as_array().into_iter().flatten())
            .map(|o| motion_ui::widgets::DropdownItem {
                label: o.as_str().unwrap_or(""),
                enabled: true,
            })
            .collect();
        let dropdown = panels
            .effect_timeline
            .dropdowns
            .entry(key.clone())
            .or_default();
        let mut overlay = list.child();
        let dropdown_input = if panels.effect_timeline.enum_owner.as_ref() == Some(&key) {
            original_input
        } else {
            input
        };
        let result = dropdown.draw_inline(
            &mut overlay,
            atlas,
            values,
            area,
            &options,
            (value[0] - parameter.min).max(0.) as usize,
            dropdown_input,
            enabled,
            false,
        );
        if dropdown.open {
            panels.effect_timeline.enum_owner = Some(key);
        } else if panels.effect_timeline.enum_owner.as_ref() == Some(&key) {
            panels.effect_timeline.enum_owner = None;
        }
        if let Some(paint) = &mut panels.effect_timeline.dropdown_paint {
            paint.append(overlay);
        } else {
            panels.effect_timeline.dropdown_paint = Some(overlay);
        }
        if let Some(index) = result {
            value[0] = parameter.min + index as f32;
            out.push(Action::Edit(target.command(EffectAction::Set {
                effect: target.effect,
                param: target.param.clone(),
                frame: frame.unwrap(),
                value,
            })));
        }
        return;
    }
    let count = parameter.kind.dimensions();
    let cell = (width / count as f32).min(styles.metric("value.min_width", 64.));
    for component in 0..count {
        let rect = Rect::new(
            values.min[0] + component as f32 * cell,
            y,
            (cell - styles.metric("border", 1.) * 2.).max(0.),
            height,
        );
        #[cfg(test)]
        panels.effect_timeline.rects.insert(
            (target.effect, target.param.clone(), -10 - component as i32),
            rect,
        );
        if clicked(input, rect) && enabled {
            if panels
                .effect_timeline
                .field
                .take()
                .is_some_and(|mut f| f.number.cancel())
            {
                out.push(Action::History(4));
            }
            panels.effect_timeline.field = Some(Field {
                target: target.clone(),
                component,
                number: Default::default(),
                rect,
            });
        }
        if let Some(field) = panels
            .effect_timeline
            .field
            .as_mut()
            .filter(|f| f.target == *target && f.component == component)
        {
            field.rect = rect;
            let events = field.number.draw(
                list,
                atlas,
                rect,
                value[component] as f64,
                motion_ui::widgets::NumberOptions {
                    min: parameter.min as f64,
                    max: parameter.max as f64,
                    speed: if parameter.max - parameter.min <= 2. {
                        0.005
                    } else {
                        0.5
                    },
                    decimals: 3,
                },
                input,
            );
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input,
                field.number.editor.dragging(),
            ));
            if let Some(readout) = field.number.readout() {
                panels.drag_readout = Some((input.mouse, readout));
            }
            if numeric::apply(events, out, |v| {
                value[component] = v;
                target.command(EffectAction::Set {
                    effect: target.effect,
                    param: target.param.clone(),
                    frame: frame.unwrap_or(0),
                    value,
                })
            }) {
                panels.effect_timeline.field = None;
            }
        } else {
            numeric::passive(list, atlas, rect, value[component] as f64, input, enabled);
            if enabled && rect.contains(input.mouse) {
                panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(input, false));
            }
        }
    }
}

fn begin_key(
    panels: &mut Panels,
    state: &Value,
    target: Target,
    frame: u32,
    point: [f32; 2],
    input: &Input,
    ppf: f32,
    offset: f64,
    value_per_pixel: f32,
    component: usize,
    out: &mut Vec<Action>,
) {
    out.extend(cancel_interaction(panels));
    choose(panels, state, &target, Some(frame));
    out.push(Action::Seek(frame as f64));
    let Some(parameter) = target.parameter(state) else {
        return;
    };
    if !target.editable(state) || !parameter.implemented || !parameter.animatable {
        return;
    }
    let local = frame as f64
        - target.source(state).unwrap().0["timeline"]["offset_frame"]
            .as_f64()
            .unwrap_or(0.);
    let value = parameter.sample(local);
    let occupied = frames(&parameter, frame as f64 - local)
        .into_iter()
        .filter_map(|f| composition_frame(f, state))
        .collect();
    panels.effect_timeline.drag = Some(KeyDrag {
        target,
        origin: frame,
        current: frame,
        point: input.pressed.map_or(point, |(_, p)| p),
        value,
        occupied,
        copy: input.modifiers.control,
        copied: false,
        begun: false,
        ppf,
        offset,
        value_per_pixel,
        component,
        bounds: None,
    });
}
pub(super) fn update(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    offset: f64,
    out: &mut Vec<Action>,
) {
    let Some(mut drag) = panels.effect_timeline.drag.take() else {
        return;
    };
    if !drag.target.editable(state)
        || selected(panels, state).as_ref() != Some(&drag.target)
        || drag.ppf != ppf
        || drag.offset != offset
        || input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
    {
        if drag.begun {
            out.push(Action::History(4));
        }
        return;
    }
    if panels.timeline.navigation_paint_only {
        panels.effect_timeline.drag = Some(drag);
        return;
    }
    let release = input.released.filter(|(b, _)| *b == MouseButton::Left);
    if input.dragging() || release.is_some() {
        let point = release.map_or(input.mouse, |(_, p)| p);
        let threshold = list_styles(panels).metric("motion.drag_threshold", 3.);
        let travel = (point[0] - drag.point[0])
            .abs()
            .max((point[1] - drag.point[1]).abs());
        if !drag.begun && travel >= threshold {
            drag.begun = true;
            out.push(Action::History(2));
        }
        if drag.begun {
            let next = (drag.origin as f64 + (point[0] - drag.point[0]) as f64 / ppf as f64)
                .round()
                .clamp(0., last_frame(state)) as u32;
            if next == drag.current || !drag.occupied.contains(&next) {
                if next != drag.current {
                    let action = if drag.copy && !drag.copied {
                        EffectAction::CopyKey {
                            effect: drag.target.effect,
                            param: drag.target.param.clone(),
                            from: drag.origin,
                            to: next,
                        }
                    } else {
                        EffectAction::MoveKey {
                            effect: drag.target.effect,
                            param: drag.target.param.clone(),
                            from: drag.current,
                            to: next,
                        }
                    };
                    out.push(Action::Edit(drag.target.command(action)));
                    if !drag.copy || drag.copied {
                        drag.occupied.remove(&drag.current);
                    }
                    drag.occupied.insert(next);
                    drag.current = next;
                    drag.copied = true;
                }
                if drag.value_per_pixel != 0. && !input.modifiers.alt && (!drag.copy || drag.copied)
                {
                    if let Some(parameter) = drag.target.parameter(state) {
                        let mut value = drag.value;
                        value[drag.component] = (value[drag.component]
                            + (drag.point[1] - point[1]) * drag.value_per_pixel)
                            .clamp(parameter.min, parameter.max);
                        out.push(Action::Edit(drag.target.command(EffectAction::Set {
                            effect: drag.target.effect,
                            param: drag.target.param.clone(),
                            frame: drag.current,
                            value,
                        })));
                    }
                }
                panels.effect_timeline.key = Some((drag.target.clone(), drag.current));
            }
        }
    }
    if release.is_some() {
        if drag.begun {
            out.push(Action::History(3));
        }
    } else {
        panels.effect_timeline.drag = Some(drag);
    }
}
fn list_styles(panels: &Panels) -> std::sync::Arc<motion_ui::style::StyleSheet> {
    panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc)
}
fn open_menu(panels: &mut Panels, state: &Value, target: Target, frame: u32, point: [f32; 2]) {
    choose(panels, state, &target, Some(frame));
    panels.effect_timeline.menu = Some(Menu {
        target,
        frame,
        point,
        opened: true,
    });
}
pub(super) fn ease(
    panels: &mut Panels,
    state: &Value,
    command: crate::shortcuts::Command,
    out: &mut Vec<Action>,
) -> bool {
    use crate::shortcuts::Command as Shortcut;
    let Some(target) = selected(panels, state) else {
        return false;
    };
    let Some(parameter) = target.parameter(state) else {
        return true;
    };
    if !target.editable(state) || !parameter.implemented || parameter.kind.discrete() {
        return true;
    }
    let at = panels
        .effect_timeline
        .key
        .as_ref()
        .filter(|(t, _)| *t == target)
        .map(|(_, f)| *f)
        .or_else(|| composition_frame(state["frame"].as_f64().unwrap_or(0.), state));
    let Some(frame) = at else {
        return true;
    };
    let offset = target.source(state).unwrap().0["timeline"]["offset_frame"]
        .as_f64()
        .unwrap_or(0.);
    let source = &target.source(state).unwrap().1[if parameter.curve.is_some() {
        "curve"
    } else {
        "track"
    }];
    let keys = source["keys"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let Some(index) = keys.iter().position(|k| {
        k["frame"]
            .as_f64()
            .is_some_and(|f| f + offset == frame as f64)
    }) else {
        return true;
    };
    let mut edits = vec![];
    for (i, incoming, outgoing) in [
        (
            index.checked_sub(1),
            command != Shortcut::EaseOutgoing,
            false,
        ),
        (
            (index + 1 < keys.len()).then_some(index),
            false,
            command != Shortcut::EaseIncoming,
        ),
    ] {
        let Some(i) = i.filter(|_| incoming || outgoing) else {
            continue;
        };
        let Some(frame) = keys[i]["frame"]
            .as_f64()
            .and_then(|f| composition_frame(f + offset, state))
        else {
            continue;
        };
        let original = if let Some(curve) = &parameter.curve {
            Easing {
                ease: curve.keys[i].ease,
                curve: curve.keys[i].curve,
            }
        } else {
            Easing {
                ease: parameter.track.keys[i].ease,
                curve: parameter.track.keys[i].curve,
            }
        };
        let easing = timeline_shortcuts::ease_sides(original, incoming, outgoing);
        edits.push(target.command(EffectAction::Curve {
            effect: target.effect,
            param: target.param.clone(),
            frame,
            easing,
        }));
    }
    if !edits.is_empty() {
        out.push(Action::EditBatch(edits));
    }
    true
}
pub(super) fn delete(panels: &mut Panels, state: &Value, out: &mut Vec<Action>) -> bool {
    let Some(target) = selected(panels, state) else {
        return false;
    };
    if let Some((_, frame)) = panels
        .effect_timeline
        .key
        .as_ref()
        .filter(|(t, _)| *t == target)
    {
        if target.editable(state) {
            out.push(Action::Edit(target.command(EffectAction::DeleteKey {
                effect: target.effect,
                param: target.param.clone(),
                frame: *frame,
            })));
        }
        panels.effect_timeline.key = None;
    }
    true
}
pub(super) fn overlay(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if let Some(paint) = panels.effect_timeline.dropdown_paint.take() {
        list.append(paint);
    }
    let Some(menu) = panels.effect_timeline.menu.as_mut() else {
        return;
    };
    let styles = list.styles();
    let width = styles
        .metric("context.menu.width", 220.)
        .min(bounds.width());
    let height = styles.metric("context.row.height", 20.);
    let card = Rect::new(
        menu.point[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        menu.point[1].clamp(
            bounds.min[1],
            (bounds.max[1] - height * 4.).max(bounds.min[1]),
        ),
        width,
        height * 4.,
    );
    let just_opened = std::mem::take(&mut menu.opened);
    let target = menu.target.clone();
    let frame = menu.frame;
    if target.source(state).is_none()
        || selected(panels, state).as_ref() != Some(&target)
        || input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        || !just_opened && input.pressed.is_some_and(|(_, p)| !card.contains(p))
    {
        panels.effect_timeline.menu = None;
        return;
    }
    motion_ui::style::box_style(
        list,
        card,
        styles.resolve("popup", Default::default()).unwrap(),
    );
    let parameter = target.parameter(state);
    let enabled = target.editable(state)
        && parameter
            .as_ref()
            .is_some_and(|p| p.animatable && p.implemented && !p.kind.discrete());
    let offset = target.source(state).unwrap().0["timeline"]["offset_frame"]
        .as_f64()
        .unwrap_or(0.);
    let times = parameter
        .as_ref()
        .map(|p| frames(p, offset))
        .unwrap_or_default();
    let index = times.iter().position(|f| *f == frame as f64);
    for (i, (zh, en, command)) in [
        (
            "缓动",
            "Easy ease",
            Some(crate::shortcuts::Command::EaseBoth),
        ),
        (
            "缓入",
            "Ease incoming",
            Some(crate::shortcuts::Command::EaseIncoming),
        ),
        (
            "缓出",
            "Ease outgoing",
            Some(crate::shortcuts::Command::EaseOutgoing),
        ),
        ("删除关键帧", "Delete keyframe", None),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(card.min[0], card.min[1] + i as f32 * height, width, height);
        if ui::button_styled(
            list,
            atlas,
            rect,
            if panels.locale == Locale::Zh { zh } else { en },
            input,
            if command.is_some() {
                enabled
                    && index.is_some_and(|index| match command.unwrap() {
                        crate::shortcuts::Command::EaseIncoming => index > 0,
                        crate::shortcuts::Command::EaseOutgoing => index + 1 < times.len(),
                        _ => times.len() > 1,
                    })
            } else {
                target.editable(state)
            },
            &styles,
            "button.icon",
        )
        .clicked
            && !just_opened
        {
            if let Some(command) = command {
                ease(panels, state, command, out);
            } else {
                out.push(Action::Edit(target.command(EffectAction::DeleteKey {
                    effect: target.effect,
                    param: target.param.clone(),
                    frame,
                })));
                panels.effect_timeline.key = None;
            }
            panels.effect_timeline.menu = None;
            break;
        }
    }
}
