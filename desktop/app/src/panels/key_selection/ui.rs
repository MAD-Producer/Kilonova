//! One selection gesture consumes authored identities recorded by the actual
//! painted key lanes. Capturing happens only on press, select-all or a marquee;
//! normal playback does not build a second hit map.
use super::*;
use crate::panels::{Action, Panels};
use motion_ui::{
    input::{Input, Key as InputKey, MouseButton},
    paint::{self, PaintList},
    Rect,
};

#[derive(Clone, PartialEq)]
struct View {
    area: Rect,
    mapping: [f32; 5],
    graph: bool,
}
#[derive(Default)]
struct Batch {
    channels: BTreeMap<String, Channel>,
    bytes: usize,
    last: Option<Key>,
    error: Option<Error>,
}
impl Batch {
    fn push(&mut self, key: Key) {
        if self.error.is_some() {
            return;
        }
        let address = match serde_json::to_string(&key.target) {
            Ok(address) => address,
            Err(error) => {
                self.error = Some(error.into());
                return;
            }
        };
        let channel = self.channels.entry(address.clone()).or_default();
        if channel.target.is_none() {
            self.bytes = self.bytes.saturating_add(Channel::bytes(&address, 0));
            channel.target = Some(key.target.clone());
        }
        if channel.frames.insert(key.frame) {
            self.bytes = self.bytes.saturating_add(11);
        }
        if self.bytes.saturating_add(2) > MAX_KEYFRAME_CLIPBOARD_BYTES {
            self.error = Some(Error::Invalid(
                "key selection exceeds metadata limit".into(),
            ));
            self.channels.clear();
        } else {
            self.last = Some(key);
        }
    }
    fn empty(&self) -> bool {
        self.channels.is_empty() && self.error.is_none()
    }
    fn keys(self) -> impl Iterator<Item = Key> {
        self.channels.into_values().flat_map(|c| {
            c.frames.into_iter().filter_map(move |frame| {
                Some(Key {
                    target: c.target.clone()?,
                    frame,
                })
            })
        })
    }
}
struct Frame {
    view: View,
    before: Option<Set>,
    press: Option<[f32; 2]>,
    pointer: [f32; 2],
    rectangle: Option<Rect>,
    mode: Mode,
    all: bool,
    incoming: Batch,
    hits: Batch,
    action_start: usize,
    background_owned: bool,
}
struct Marquee {
    view: View,
    scope: Scope,
    revision: Option<u64>,
    press: [f32; 2],
    mode: Mode,
    before: Set,
}
#[derive(Default)]
pub(crate) struct Controls {
    frame: Option<Frame>,
    marquee: Option<Marquee>,
    pub(super) select_all: bool,
    current: bool,
    pub(super) transaction: Option<Set>,
}
impl Controls {
    pub(crate) fn cancel(&mut self) {
        self.marquee = None;
        self.select_all = false;
    }
    pub(crate) fn active(&self) -> bool {
        self.marquee.is_some()
    }
    pub(crate) fn recording(&self) -> bool {
        self.frame.is_some()
    }
}
fn mode(input: &Input) -> Mode {
    if input.modifiers.control {
        Mode::Toggle
    } else if input.modifiers.shift {
        Mode::Add
    } else {
        Mode::Replace
    }
}
fn rectangle(a: [f32; 2], b: [f32; 2], area: Rect) -> Rect {
    let a = [
        a[0].clamp(area.min[0], area.max[0]),
        a[1].clamp(area.min[1], area.max[1]),
    ];
    let b = [
        b[0].clamp(area.min[0], area.max[0]),
        b[1].clamp(area.min[1], area.max[1]),
    ];
    Rect::new(
        a[0].min(b[0]),
        a[1].min(b[1]),
        (a[0] - b[0]).abs(),
        (a[1] - b[1]).abs(),
    )
}
pub(in crate::panels) fn begin(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    area: Rect,
    mapping: [f32; 5],
    action_start: usize,
) -> bool {
    panels.key_gestures.current = panels.selected_keys.status(state) == Status::Current;
    panels.key_gestures.frame = None;
    if input.focus_lost || input.keys.iter().any(|(k, _)| *k == InputKey::Escape) {
        panels.key_gestures.cancel();
        return true;
    }
    let press = input
        .pressed
        .filter(|(b, p)| *b == MouseButton::Left && area.contains(*p))
        .map(|(_, p)| p);
    let all = std::mem::take(&mut panels.key_gestures.select_all);
    if press.is_none() && !all && !panels.key_gestures.active() {
        if !panels.selected_keys.owns_keys() || Scope::of(state).is_some() {
            return true;
        }
    }
    let identity = observe!(
        "selection.gesture_scope",
        Scope::of(state).ok_or("key selection document identity missing"),
        "phase=begin revision={:?}",
        state["revision"].as_u64()
    );
    if identity.is_err() {
        panels.key_gestures.cancel();
        panels.selected_keys.claimed = true;
        panels.selected_keys.intent = true;
        panels.selected_keys.validated.set(None);
        panels.message = Some(
            panels
                .locale
                .t(crate::i18n::Text::KeySelectionUnavailable)
                .into(),
        );
        return false;
    }
    let view = View {
        area,
        mapping,
        graph: panels.timeline.graph_mode,
    };
    let rect = panels.key_gestures.marquee.as_ref().map(|d| {
        rectangle(
            d.press,
            input.released.map_or(input.mouse, |(_, p)| p),
            area,
        )
    });
    panels.key_gestures.frame = Some(Frame {
        view,
        before: press.map(|_| panels.selected_keys.clone()),
        press,
        pointer: input.released.map_or(input.mouse, |(_, p)| p),
        rectangle: rect,
        mode: mode(input),
        all,
        incoming: Batch::default(),
        hits: Batch::default(),
        action_start,
        background_owned: false,
    });
    true
}
pub(in crate::panels) fn plot(panels: &mut Panels, area: Rect, low: f32, high: f32) {
    if let Some(frame) = &mut panels.key_gestures.frame {
        frame.view.area = area;
        frame.view.mapping[3] = low;
        frame.view.mapping[4] = high;
        if let Some(drag) = &panels.key_gestures.marquee {
            frame.rectangle = Some(rectangle(drag.press, frame.pointer, area));
        }
    }
}
pub(in crate::panels) fn block_background(panels: &mut Panels) {
    if let Some(frame) = &mut panels.key_gestures.frame {
        frame.background_owned = true;
    }
}
pub(in crate::panels) fn selecting_all(panels: &Panels) -> bool {
    panels
        .key_gestures
        .frame
        .as_ref()
        .is_some_and(|frame| frame.all)
}
pub(in crate::panels) fn all(panels: &mut Panels, keys: impl IntoIterator<Item = Key>) {
    if let Some(frame) = &mut panels.key_gestures.frame {
        if frame.all {
            for key in keys {
                frame.incoming.push(key);
            }
        }
    }
}
pub(in crate::panels) fn failed(panels: &mut Panels, error: Error) {
    if let Some(frame) = &mut panels.key_gestures.frame {
        frame.incoming.channels.clear();
        if frame.incoming.error.is_none() {
            frame.incoming.error = Some(error);
        }
    }
}
pub(in crate::panels) fn all_channel(
    panels: &mut Panels,
    state: &Value,
    target: KeyframeTarget,
    source: &Value,
) {
    if selecting_all(panels) {
        all(
            panels,
            source["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k["frame"].as_u64())
                .filter(|time| *time as f64 <= crate::panels::last_frame(state))
                .map(|time| Key {
                    target: target.clone(),
                    frame: time as u32,
                }),
        );
    }
}
pub(in crate::panels) fn curve(
    panels: &mut Panels,
    state: &Value,
    target: KeyframeTarget,
    source: &Value,
) {
    let Some(frame) = &mut panels.key_gestures.frame else {
        return;
    };
    frame.background_owned = true;
    for time in source["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|k| k["frame"].as_u64())
    {
        if time as f64 <= crate::panels::last_frame(state) {
            frame.hits.push(Key {
                target: target.clone(),
                frame: time as u32,
            });
        }
    }
}
pub(in crate::panels) fn record(panels: &mut Panels, key: Key, point: [f32; 2], hit: Rect) {
    let Some(frame) = &mut panels.key_gestures.frame else {
        return;
    };
    if !frame.view.area.contains(point) {
        return;
    }
    if frame.press.is_some_and(|p| hit.contains(p)) {
        frame.hits.push(key.clone());
    }
    if frame.all || frame.rectangle.is_some_and(|r| r.contains(point)) {
        frame.incoming.push(key);
    }
}
/// Add a token-sized ring without changing the existing key or its hit target.
pub(in crate::panels) fn mark(
    list: &mut PaintList,
    panels: &mut Panels,
    key: Key,
    point: [f32; 2],
    radius: f32,
    hit: Rect,
) {
    if panels.key_gestures.current && panels.selected_keys.contains(&key) {
        let styles = list.styles();
        let border = styles.metric("border", 1.);
        paint::diamond(list, point, radius + border, styles.color("text"));
        paint::diamond(list, point, radius, styles.color("keyframe"));
    }
    record(panels, key, point, hit);
}
pub(in crate::panels) fn mark_round(
    list: &mut PaintList,
    panels: &mut Panels,
    key: Key,
    point: [f32; 2],
    radius: f32,
    tolerance: f32,
) {
    let outside = panels
        .key_gestures
        .frame
        .as_ref()
        .and_then(|f| f.press)
        .is_some_and(|p| (p[0] - point[0]).hypot(p[1] - point[1]) > tolerance);
    let size = if outside { 0. } else { tolerance * 2. };
    mark(
        list,
        panels,
        key,
        point,
        radius,
        Rect::new(point[0] - size * 0.5, point[1] - size * 0.5, size, size),
    );
}
pub(in crate::panels) fn mask_lane(
    list: &mut PaintList,
    panels: &mut Panels,
    state: &Value,
    object: u64,
    mask: &motion_core::masks::LayerMask,
    index: u8,
    offset: i32,
    lane: Rect,
    ppf: f32,
    view_offset: f32,
) {
    if !panels.selected_keys.owns_keys() && !panels.key_gestures.recording() {
        return;
    }
    let styles = list.styles();
    let radius = styles.metric("timeline.key.radius", 4.);
    let tolerance = radius + styles.metric("motion.drag_threshold", 3.);
    let channels: Vec<_> = if index == 3 {
        mask.path
            .nodes
            .iter()
            .map(|node| {
                (
                    motion_core::masks::MaskProperty::Node(node.id),
                    node.geometry
                        .keys
                        .iter()
                        .map(|k| k.frame)
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    } else {
        let property = match index {
            0 => motion_core::masks::MaskProperty::Opacity,
            1 => motion_core::masks::MaskProperty::Feather,
            _ => motion_core::masks::MaskProperty::Expansion,
        };
        vec![(
            property,
            crate::panels::path_controls::timeline::key_frames(mask, index),
        )]
    };
    for (property, frames) in channels {
        for local in frames {
            let time = i64::from(local) + i64::from(offset);
            if time < 0 || time as f64 > crate::panels::last_frame(state) {
                continue;
            }
            let point = [
                lane.min[0] + (time as f32 - view_offset) * ppf,
                lane.min[1] + lane.height() * 0.5,
            ];
            if lane.contains(point) {
                mark_round(
                    list,
                    panels,
                    Key {
                        target: KeyframeTarget::Mask {
                            object,
                            mask: mask.id,
                            property: property.clone(),
                        },
                        frame: time as u32,
                    },
                    point,
                    radius,
                    tolerance,
                );
            }
        }
    }
}
pub(in crate::panels) fn vector_lane(
    list: &mut PaintList,
    panels: &mut Panels,
    state: &Value,
    object: u64,
    item: Option<u64>,
    parameter: &crate::panels::path_controls::groups::parameters::Parameter,
    offset: i32,
    lane: Rect,
    ppf: f32,
    view_offset: f32,
) {
    if parameter.static_value
        || !panels.selected_keys.owns_keys() && !panels.key_gestures.recording()
    {
        return;
    }
    let styles = list.styles();
    let radius = styles.metric("graph.handle_radius", 4.);
    let tolerance = radius + styles.metric("motion.drag_threshold", 3.);
    for (local, _) in &parameter.keys {
        let time = i64::from(*local) + i64::from(offset);
        if time < 0 || time as f64 > crate::panels::last_frame(state) {
            continue;
        }
        let point = [
            lane.min[0] + (time as f32 - view_offset) * ppf,
            lane.min[1] + lane.height() * 0.5,
        ];
        if lane.contains(point) {
            mark_round(
                list,
                panels,
                Key {
                    target: KeyframeTarget::Vector {
                        object,
                        item,
                        parameter: parameter.id.clone(),
                    },
                    frame: time as u32,
                },
                point,
                radius,
                tolerance,
            );
        }
    }
}
fn apply(panels: &mut Panels, state: &Value, batch: Batch, mode: Mode) {
    if let Some(error) = batch.error {
        log::error!(
            "key selection failed operation=select phase=capture revision={:?} cause={error}",
            state["revision"].as_u64()
        );
        panels.message = Some(format!(
            "{}: {error}",
            if panels.locale == crate::i18n::Locale::Zh {
                "关键帧选择失败"
            } else {
                "Keyframe selection failed"
            }
        ));
        return;
    }
    let last = batch.last.clone();
    if !super::choose(panels, state, batch.keys(), mode) {
        return;
    }
    // Iteration order is stable, but the last actually hit key is the primary.
    if let Some(key) = last.filter(|k| panels.selected_keys.contains(k)) {
        panels.selected_keys.primary = Some(key);
    }
    project_primary(panels, state);
}
pub(super) fn project_primary(panels: &mut Panels, state: &Value) {
    let Some(key) = panels.selected_keys.primary().cloned() else {
        panels.animation_controls.clear_key();
        panels.path_controls.clear_mask_selection();
        panels.audio_controls.clear_selection();
        crate::panels::effect_timeline::clear_projected_key(panels);
        panels.timeline.graph_box.clear_selection();
        return;
    };
    panels.selected = Some(key.target.object());
    panels.selected_layers.clear();
    if key.target.object() != 0 {
        panels.selected_layers.push(key.target.object());
    }
    panels.selection_anchor = panels.selected.filter(|id| *id != 0);
    match key.target {
        KeyframeTarget::Property {
            object,
            property,
            axis,
        } => panels.animation_controls.select_key_in(
            state,
            object,
            property,
            axis,
            i64::from(key.frame),
        ),
        KeyframeTarget::Effect {
            object,
            effect,
            param,
        } => crate::panels::effect_timeline::project_key(
            panels, state, object, effect, &param, key.frame,
        ),
        target @ (KeyframeTarget::Mask { .. } | KeyframeTarget::Vector { .. }) => {
            panels.path_controls.project_key(state, &target, key.frame)
        }
        KeyframeTarget::Audio { object, channel } => panels
            .audio_controls
            .project_key(state, object, channel, key.frame),
    }
}
pub(in crate::panels) fn finish(
    list: &mut PaintList,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &[Action],
) {
    super::movement::follow(panels, state, out);
    let Some(mut frame) = panels.key_gestures.frame.take() else {
        return;
    };
    if let Some(drag) = panels.key_gestures.marquee.take() {
        if Some(&drag.scope) != Scope::of(state).as_ref()
            || drag.revision != state["revision"].as_u64()
            || drag.view != frame.view
        {
            panels.selected_keys = drag.before;
            panels.message = Some(
                panels
                    .locale
                    .t(crate::i18n::Text::KeySelectionUnavailable)
                    .into(),
            );
            return;
        }
        if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
            panels.selected_keys = drag.before;
            apply(panels, state, frame.incoming, drag.mode);
            return;
        }
        if !input.dragging() {
            return;
        }
        if let Some(rect) = frame.rectangle {
            let styles = list.styles();
            let color = styles.color("focus");
            paint::rect(
                list,
                rect,
                color.with_alpha(styles.metric("graph.selection.fill_opacity", 0.05)),
            );
            paint::stroke(list, rect, color, styles.metric("border", 1.));
        }
        panels.key_gestures.marquee = Some(drag);
        return;
    }
    if frame.all {
        apply(panels, state, frame.incoming, Mode::Replace);
        return;
    }
    let Some(press) = frame.press else {
        return;
    };
    if !frame.hits.empty() {
        if let Some(before) = frame.before.take() {
            panels.selected_keys = before;
        }
        apply(panels, state, frame.hits, frame.mode);
        return;
    }
    if !frame.background_owned && frame.view.area.contains(press) && out.len() == frame.action_start
    {
        let Some(scope) = Scope::of(state) else {
            return;
        };
        let before = frame.before.take().unwrap_or_default();
        panels.selected_keys = before.clone();
        panels.selected_keys.intent = true;
        project_primary(panels, state);
        panels.key_gestures.marquee = Some(Marquee {
            view: frame.view,
            scope,
            revision: state["revision"].as_u64(),
            press,
            mode: frame.mode,
            before,
        });
    }
}
