//! Animation menu and keys share the existing plans and command transactions.
use super::*;
use crate::shortcuts::{Command as Shortcut, Scope};
use motion_core::Ease;

#[derive(PartialEq, Eq)]
pub(crate) struct MenuScope {
    source: crate::layer_clipboard::KeyframeSource,
    objects: Vec<u64>,
    focused: Option<Panel>,
    keys: Vec<motion_core::KeyframeSelection>,
}
impl MenuScope {
    pub(super) fn of(panels: &Panels, state: &Value) -> Self {
        Self {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            objects: panels.selection_ids(),
            focused: panels.focused,
            keys: super::keyframe_clipboard::selection(panels, state)
                .map_or_else(Vec::new, |s| s.selections),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Shortcut(Shortcut),
    Interpolation(Ease),
    Sequence,
    ReverseTime,
}

fn edit_scope(panels: &Panels, state: &Value) -> bool {
    state["observing"] != true
        && super::property_focus::navigation_scope(panels.focused)
        && panels.timeline.moving.is_none()
        && panels.timeline.trimming.is_none()
        && panels.timeline.horizontal_drag.is_none()
        && !panels.timeline.scrubbing
        && panels.timeline.work_area_drag.is_none()
}

fn toggle_plan(panels: &Panels, state: &Value, property: Property) -> Option<(Vec<Command>, bool)> {
    if !matches!(
        panels.focused,
        Some(Panel::Timeline | Panel::Curves | Panel::Properties | Panel::Composition)
    ) {
        return None;
    }
    let frame = state["frame"].as_f64()?.floor();
    if !frame.is_finite() || frame < 0. || frame > f64::from(u32::MAX) {
        return None;
    }
    super::timeline_shortcuts::toggle_key_plan(
        panels,
        state,
        panels.selected?,
        property,
        frame as u32,
    )
}

fn interpolation_selection(
    panels: &Panels,
    state: &Value,
) -> Option<super::keyframe_clipboard::Selection> {
    let selected = super::keyframe_clipboard::selection(panels, state)?;
    (selected.writable
        && !selected.selections.is_empty()
        && selected.selections.iter().all(|s| !s.frames.is_empty()))
    .then_some(selected)
}

pub(super) fn enabled(panels: &Panels, state: &Value, operation: Operation) -> bool {
    if !edit_scope(panels, state) {
        return false;
    }
    match operation {
        Operation::Shortcut(Shortcut::ToggleKey(property)) => {
            toggle_plan(panels, state, property).is_some()
        }
        Operation::Shortcut(
            Shortcut::EaseBoth | Shortcut::EaseIncoming | Shortcut::EaseOutgoing,
        ) => {
            let Operation::Shortcut(command) = operation else {
                return false;
            };
            super::timeline_shortcuts::ease_plan(panels, state, command)
                .is_ok_and(|edits| !edits.is_empty())
        }
        Operation::Shortcut(Shortcut::Delete) => {
            if panels.selected_keys.owns_keys() {
                return matches!(panels.focused, Some(Panel::Timeline | Panel::Curves)) && panels.selected_keys.writable(state);
            }
            matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
                && (super::effect_timeline::delete_available(panels, state)
                    || panels
                        .animation_controls
                        .user_key_in(state, panels.selected)
                        .is_some_and(|key| {
                            state["project"]["layers"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|layer| layer["id"] == key.object && layer["locked"] != true)
                        }))
        }
        Operation::Interpolation(_) => interpolation_selection(panels, state).is_some(),
        Operation::Sequence => sequence_plan(panels, state).is_some_and(|edits| !edits.is_empty()),
        Operation::ReverseTime | Operation::Shortcut(_) => false,
    }
}

pub(super) fn apply(
    panels: &mut Panels,
    state: &Value,
    operation: Operation,
    out: &mut Vec<Action>,
) {
    // Recheck against the current snapshot; menu navigation never publishes an edit.
    if !enabled(panels, state, operation) {
        return;
    }
    let mut pending = Vec::new();
    match operation {
        Operation::Shortcut(command) => {
            let mut frame = state["frame"].as_f64().unwrap_or(0.);
            super::timeline_shortcuts::handle(
                command,
                Default::default(),
                panels,
                state,
                &mut frame,
                &mut pending,
            );
        }
        Operation::Interpolation(ease) => {
            if let Some(selected) = interpolation_selection(panels, state) {
                pending.push(Action::Edit(Command::KeyframeInterpolation {
                    selections: selected.selections,
                    ease,
                }));
            }
        }
        Operation::Sequence => {
            if let Some(edits) = sequence_plan(panels, state) {
                pending.push(Action::EditBatch(edits));
            }
        }
        Operation::ReverseTime => {}
    }
    let mut commands = Vec::new();
    for action in pending {
        match action {
            Action::Edit(command) => commands.push(command),
            Action::EditBatch(batch) => commands.extend(batch),
            other => out.push(other),
        }
    }
    if !commands.is_empty() {
        out.push(Action::EditorAction {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            commands,
        });
    }
}

pub(super) fn entries(panels: &Panels, state: &Value) -> Vec<(String, Operation, bool)> {
    let zh = panels.locale == Locale::Zh;
    let mut rows = Vec::new();
    for (property, en, cn) in [
        (Property::Anchor, "Anchor", "锚点"),
        (Property::Position, "Position", "位置"),
        (Property::Scale, "Scale", "缩放"),
        (Property::Rotation, "Rotation", "旋转"),
        (Property::Opacity, "Opacity", "不透明度"),
    ] {
        let command = Shortcut::ToggleKey(property);
        let removing = toggle_plan(panels, state, property).is_some_and(|(_, removing)| removing);
        let title = if zh {
            format!(
                "{}当前帧{}关键帧",
                if removing { "删除" } else { "添加" },
                cn
            )
        } else {
            format!(
                "{} {en} key at playhead",
                if removing { "Remove" } else { "Add" }
            )
        };
        let title = panels
            .shortcut_bindings
            .label(&title, Scope::Animation, command);
        rows.push((title, Operation::Shortcut(command)));
    }
    for (command, en, cn) in [
        (
            Shortcut::Delete,
            "Delete selected keyframe",
            "删除选中的关键帧",
        ),
        (Shortcut::EaseBoth, "Ease both sides", "双侧缓动"),
        (Shortcut::EaseIncoming, "Ease incoming side", "入侧缓动"),
        (Shortcut::EaseOutgoing, "Ease outgoing side", "出侧缓动"),
    ] {
        rows.push((
            panels
                .shortcut_bindings
                .label(if zh { cn } else { en }, Scope::Animation, command),
            Operation::Shortcut(command),
        ));
    }
    let count = interpolation_selection(panels, state).map_or(0, |keys| {
        keys.selections
            .iter()
            .map(|s| s.frames.len())
            .sum::<usize>()
    });
    for (ease, en, cn) in [
        (Ease::Linear, "Linear", "线性"),
        (Ease::Hold, "Hold", "保持"),
    ] {
        let title = if zh {
            format!("{cn}插值 · 当前属性 {count} 个关键帧")
        } else {
            format!("{en} interpolation · {count} keys in current property")
        };
        rows.push((title, Operation::Interpolation(ease)));
    }
    rows.push((
        (if zh {
            "序列图层 · 保留首层起点，整帧移动"
        } else {
            "Sequence layers · retain first layer start, whole-frame shifts"
        })
        .into(),
        Operation::Sequence,
    ));
    rows.push((
        (if zh {
            "图层时间反向 · 尚无时间重映射支持"
        } else {
            "Reverse layer time · requires time remapping"
        })
        .into(),
        Operation::ReverseTime,
    ));
    rows.into_iter()
        .map(|(label, op)| (label, op, enabled(panels, state, op)))
        .collect()
}

fn sequence_plan(panels: &Panels, state: &Value) -> Option<Vec<Command>> {
    let selected = panels.selection_ids();
    if selected.len() < 2 {
        return None;
    }
    let layers: Vec<_> = state["project"]["layers"]
        .as_array()?
        .iter()
        .rev()
        .filter(|layer| {
            layer["id"]
                .as_u64()
                .is_some_and(|id| selected.contains(&id))
        })
        .collect();
    if layers.len() != selected.len() {
        return None;
    }
    let mut cursor = None;
    let mut edits = Vec::new();
    for layer in layers {
        if layer["locked"] == true || layer["content"]["kind"] == "camera" {
            return None;
        }
        let object = layer["id"].as_u64()?;
        let [start, end] = super::timeline_shortcuts::layer_range(state, object)?;
        let next = cursor.unwrap_or(start);
        let delta = next - start;
        if !delta.is_finite() || delta.fract() != 0. {
            return None;
        }
        let base = layer["timeline"]["in_frame"].as_i64().unwrap_or(0) as f64 + delta;
        if base < -f64::from(motion_core::MAX_FRAMES)
            || end + delta > f64::from(motion_core::MAX_FRAMES)
        {
            return None;
        }
        if delta != 0. {
            edits.push(Command::MoveLayerClip {
                object,
                in_frame: base as i32,
            });
        }
        cursor = Some(end + delta);
    }
    Some(edits)
}

#[cfg(test)]
mod tests;
