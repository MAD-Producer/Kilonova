//! Stateless row controls and the candidate rules shared with J/K navigation.
use super::*;
use motion_ui::{
    icons,
    style::{State, States},
};

const EPSILON: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Neighbours {
    pub previous: Option<f64>,
    pub current: bool,
    pub next: Option<f64>,
}

pub(super) fn neighbours(
    times: impl IntoIterator<Item = f64>,
    frame: f64,
    last: f64,
) -> Neighbours {
    let mut result = Neighbours::default();
    if !frame.is_finite() || !last.is_finite() || last < 0. {
        return result;
    }
    for key in times
        .into_iter()
        .filter(|key| key.is_finite() && *key >= 0. && *key <= last)
    {
        if key < frame - EPSILON {
            result.previous = Some(result.previous.map_or(key, |old| old.max(key)));
        } else if key > frame + EPSILON {
            result.next = Some(result.next.map_or(key, |old| old.min(key)));
        } else {
            result.current = true;
        }
    }
    result
}

/// Core key commands use whole composition frames. Viewing and navigation
/// retain subframes; a row never silently rounds an edit from another time.
pub(super) fn editable_frame(frame: f64, last: f64) -> Option<u32> {
    (frame.is_finite()
        && frame >= 0.
        && frame <= last
        && (frame - frame.round()).abs() <= EPSILON
        && frame.round() <= u32::MAX as f64)
        .then(|| frame.round() as u32)
}

pub(super) fn width(styles: &motion_ui::style::StyleSheet) -> f32 {
    styles.metric("icon.slot", 20.) * 3.
}

pub(super) fn inspector_width(styles: &motion_ui::style::StyleSheet, available: f32) -> f32 {
    if available < styles.metric("inspector.navigation.min_width", 320.) {
        0.
    } else {
        width(styles)
    }
}

#[cfg(test)]
pub(super) fn inspector_rail(styles: &motion_ui::style::StyleSheet, row: Rect) -> Rect {
    let mut rect = rail(styles, row);
    rect.max[0] = rect.min[0] + inspector_width(styles, row.width());
    rect
}

pub(super) fn rail(styles: &motion_ui::style::StyleSheet, row: Rect) -> Rect {
    Rect::new(
        row.min[0] + styles.metric("padding.panel", 8.),
        row.min[1],
        width(styles),
        row.height(),
    )
}

pub(super) fn follow(view: &mut types::TimelineState, total: f64, frame: f64) {
    if view.graph_mode && view.graph_ranges.locked_x() {return;}
    let span = total / f64::from(view.zoom.max(1.));
    if frame < f64::from(view.offset) || frame > f64::from(view.offset) + span {
        view.offset = (frame - span * 0.5).clamp(0., (total - span).max(0.)) as f32;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Choice {
    Seek(f64),
    Toggle,
}

/// Controls emit one intent. Their owner retains its typed channel and core
/// transaction semantics; row navigation never creates an independent clock.
pub(super) fn draw(
    list: &mut PaintList,
    area: Rect,
    input: &Input,
    neighbours: Neighbours,
    editable: bool,
) -> Option<Choice> {
    let styles = list.styles();
    if area.width() < width(&styles) {
        return None;
    }
    let neutral = Input::default();
    let input = if input.focus_lost || input.keys.iter().any(|(key, _)| *key == Key::Escape) {
        &neutral
    } else {
        input
    };
    let slot = styles.metric("icon.slot", 20.);
    let mut chosen = None;
    for (index, (icon, enabled, choice)) in [
        (
            icons::Icon::Previous,
            neighbours.previous.is_some(),
            neighbours.previous.map(Choice::Seek),
        ),
        (icons::Icon::Key, editable, Some(Choice::Toggle)),
        (
            icons::Icon::Next,
            neighbours.next.is_some(),
            neighbours.next.map(Choice::Seek),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            area.min[0] + index as f32 * slot,
            area.min[1],
            slot,
            area.height(),
        );
        let mut states = States::default();
        if !enabled {
            states = states.with(State::Disabled);
        }
        if index == 1 && neighbours.current {
            states = states.with(State::Selected);
        }
        if icons::button(list, rect, icon, input, &styles, states).clicked && chosen.is_none() {
            chosen = choice;
        }
    }
    chosen
}

pub(super) fn property_row(
    list: &mut PaintList,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    object: u64,
    property: Property,
    axis: Option<motion_core::Axis>,
    times: &[f64],
    out: &mut Vec<Action>,
) {
    if let Some(choice) = property_intent(list, area, state, input, object, property, axis, times) {
        apply_property_choice(panels, state, object, property, axis, choice, out);
    }
}

pub(super) fn property_intent(
    list: &mut PaintList,
    area: Rect,
    state: &Value,
    input: &Input,
    object: u64,
    property: Property,
    axis: Option<motion_core::Axis>,
    times: &[f64],
) -> Option<Choice> {
    let current = state["frame"].as_f64().unwrap_or(0.);
    let last = last_frame(state);
    let adjacent = neighbours(times.iter().copied(), current, last);
    let frame = editable_frame(current, last);
    let editable = frame.is_some()
        && state["observing"] != true
        && state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|layer| layer["id"].as_u64() == Some(object) && layer["locked"] != true)
        && super::animation_data::channel(state, object, property, axis).is_some();
    let rect = rail(&list.styles(), area);
    draw(list, rect, input, adjacent, editable)
}

pub(super) fn apply_property_choice(
    panels: &mut Panels,
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<motion_core::Axis>,
    choice: Choice,
    out: &mut Vec<Action>,
) {
    let current = state["frame"].as_f64().unwrap_or(0.);
    let last = last_frame(state);
    match choice {
        Choice::Seek(frame) => {
            panels.select_only(Some(object));
            panels
                .animation_controls
                .select_key_in(state, object, property, axis, frame as i64);
            follow(&mut panels.timeline, last + 1., frame);
            out.push(Action::Seek(frame));
        }
        Choice::Toggle => {
            if let Some(frame) = editable_frame(current, last) {
                let present = super::animation_data::channel(state, object, property, axis)
                    .is_some_and(|source| {
                        source["keys"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|key| key["frame"].as_u64() == Some(u64::from(frame)))
                    });
                if let Some(commands) = super::animation_data::key_commands(
                    state, object, property, axis, frame, present,
                )
                .filter(|commands| !commands.is_empty())
                {
                    panels.select_only(Some(object));
                    panels
                        .animation_controls
                        .choose_track_in(state, object, property, axis);
                    if present {
                        panels.animation_controls.clear_key();
                    } else {
                        panels.animation_controls.select_key_in(
                            state,
                            object,
                            property,
                            axis,
                            i64::from(frame),
                        );
                    }
                    out.push(Action::EditBatch(commands));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
