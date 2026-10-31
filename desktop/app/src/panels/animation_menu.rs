//! Animation menu and keys share the existing plans and command transactions.
use super::*;
use crate::shortcuts::{Command as Shortcut, Scope};
use motion_core::Ease;
mod type_state;
mod roving;
pub(crate) use type_state::Cache as TypeCache;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MenuScope {
    source: crate::layer_clipboard::KeyframeSource,
    objects: Vec<u64>,
    focused: Option<Panel>,
    keys: Vec<motion_core::KeyframeSelection>,
    spatial_keys: Vec<(motion_core::KeyframeTarget, Vec<u64>)>,
    local_keys: Vec<(motion_core::KeyframeTarget, Vec<i32>)>,
}
impl MenuScope {
    pub(super) fn of(panels: &Panels, state: &Value) -> Self {
        Self {
            source: crate::layer_clipboard::KeyframeSource::of(state),
            objects: panels.selection_ids(),
            focused: panels.focused,
            local_keys: if panels.selected_keys.status(state)==super::key_selection::Status::Current {
                panels.selected_keys.local_selections()
            } else {vec![]},
            spatial_keys: if panels.selected_keys.status(state)==super::key_selection::Status::Current {
                panels.selected_keys.spatial_selections()
            } else {vec![]},
            keys: if panels.selected_keys.owns_keys() {
                if panels.selected_keys.status(state)==super::key_selection::Status::Current {
                    panels.selected_keys.selections()
                } else {vec![]}
            } else {
                super::keyframe_clipboard::selection(panels, state)
                    .map_or_else(Vec::new, |s| s.selections)
            },
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Shortcut(Shortcut),
    Interpolation(Ease),
    Automatic(bool),
    Roving(bool),
    TypeStatus,
    TimingSettings,
    CopyEasing,
    PasteEasing,
    CompareCurves,
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
    if matches!(operation,Operation::CompareCurves) {
        return super::property_focus::navigation_scope(panels.focused)
            && state["document_generation"].is_u64();
    }
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
                return matches!(panels.focused, Some(Panel::Timeline | Panel::Curves)) && super::key_selection::delete_available(panels, state);
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
        Operation::Interpolation(_) | Operation::TimingSettings => interpolation_selection(panels, state).is_some(),
        Operation::Automatic(enabled) => interpolation_selection(panels, state).is_some()
            && panels.timeline.graph_drag.is_none()
            && !panels.key_gestures.active()
            && !panels.effect_timeline.editing()
            && !panels.animation_controls.editing()
            && type_state::summary(panels, state).available(enabled),
        Operation::Roving(enabled) => panels.timeline.graph_drag.is_none()
            && !panels.key_gestures.active()
            && !panels.effect_timeline.editing()
            && !panels.animation_controls.editing()
            && roving::writable(panels,state)
            && roving::plans(panels,state).commands(enabled).is_some(),
        Operation::TypeStatus => false,
        Operation::CopyEasing => !panels.clipboard_busy && super::keyframe_clipboard::selection(panels, state)
            .is_some_and(|s| s.selections.len() == 1 && s.selections[0].frames.len() == 1),
        Operation::PasteEasing => !panels.clipboard_busy && interpolation_selection(panels, state).is_some(),
        Operation::CompareCurves => true,
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
        Operation::Automatic(enabled) => {
            if let Some(selected) = interpolation_selection(panels, state) {
                pending.push(Action::Edit(Command::KeyframeAutomatic {
                    selections: selected.selections,
                    enabled,
                }));
            }
        }
        Operation::Roving(enabled) => roving::apply(panels,state,enabled,&mut pending),
        Operation::TypeStatus => {}
        Operation::TimingSettings => {
            if let Some(selected) = interpolation_selection(panels, state) {
                out.push(Action::KeyframeTimingSettings {
                    source: crate::layer_clipboard::KeyframeSource::of(state),
                    selections: selected.selections,
                });
            }
        }
        Operation::CopyEasing | Operation::PasteEasing => {
            if let Some(selected) = super::keyframe_clipboard::selection(panels, state) {
                out.push(Action::CurveClipboard {
                    intent: if matches!(operation, Operation::CopyEasing) {
                        crate::layer_clipboard::controller::Intent::Copy
                    } else { crate::layer_clipboard::controller::Intent::Paste { in_place: true } },
                    source: crate::layer_clipboard::KeyframeSource::of(state),
                    selections: selected.selections,
                });
            }
        }
        Operation::CompareCurves => super::curve_comparison::open(panels,state,out),
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
    let types = type_state::summary(panels, state);
    let spatial = roving::plans(panels,state);
    rows.push((spatial.title(panels.locale).unwrap_or_else(||types.title(panels.locale)), Operation::TypeStatus));
    for (ease, en, cn) in [
        (Ease::Linear, "Linear", "线性"),
        (Ease::Hold, "Hold", "保持"),
    ] {
        let title = if zh {
            format!("{cn}插值 · 所选 {count} 个关键帧")
        } else {
            format!("{en} interpolation · {count} selected keys")
        };
        rows.push((title, Operation::Interpolation(ease)));
    }
    rows.push(((if zh {
        if types.mixed_automatic() { "自动贝塞尔 · 部分启用" } else { "自动贝塞尔" }
    } else if types.mixed_automatic() { "Automatic Bezier · partially enabled" } else { "Automatic Bezier" }).into(), Operation::Automatic(true)));
    rows.push(((if zh { "冻结自动贝塞尔" } else { "Freeze automatic Bezier" }).into(), Operation::Automatic(false)));
    rows.push(((if zh {
        if spatial.partial() {"Roving · 部分启用"} else {"Roving · 按路径距离解算时间"}
    } else if spatial.partial() {"Roving · partially enabled"} else {"Roving · solve time from path distance"}).into(), Operation::Roving(true)));
    rows.push(((if zh {"固定所选 Roving 时间"} else {"Freeze selected Roving times"}).into(), Operation::Roving(false)));
    rows.push((panels.shortcut_bindings.label(
        if zh { "关键帧插值与速度…" } else { "Keyframe interpolation and velocity…" },
        Scope::Animation, Shortcut::KeyframeTiming), Operation::TimingSettings));
    rows.push(((if zh {"多通道曲线比较"} else {"Compare multiple curves"}).into(),Operation::CompareCurves));
    rows.push(((if zh { "复制所选区间的缓动曲线" } else { "Copy easing from selected interval" }).into(), Operation::CopyEasing));
    rows.push(((if zh { "将缓动曲线粘贴到所选区间" } else { "Paste easing to selected intervals" }).into(), Operation::PasteEasing));
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

pub(super) fn checked(panels: &Panels, state: &Value, operation: Operation) -> bool {
    if matches!(operation,Operation::Roving(true)) {return roving::plans(panels,state).all_roving();}
    matches!(operation, Operation::Interpolation(_) | Operation::Automatic(true))
        && type_state::summary(panels, state).checked(operation)
}

pub(super) fn reconcile_roving(panels:&mut Panels,state:&Value) {
    roving::reconcile(panels,state);
}

pub(super) fn toggle_roving(panels:&mut Panels,state:&Value,out:&mut Vec<Action>) {
    let operation=Operation::Roving(!roving::plans(panels,state).all_roving());
    if enabled(panels,state,operation) {apply(panels,state,operation,out);}
    else {panels.message=Some(if panels.locale==Locale::Zh {
        "请选择可编辑的耦合位置、锚点或摄影机目标关键帧；首尾键、分离轴及不支持的混合选区不能启用 Roving。"
    } else {
        "Select editable coupled position, anchor or camera-target keys. Endpoints, separated axes and unsupported mixed selections cannot enable Roving."
    }.into());}
}

pub(super) fn toggle_automatic(panels: &mut Panels, state: &Value, out: &mut Vec<Action>) {
    let operation = Operation::Automatic(!type_state::summary(panels, state).checked(Operation::Automatic(true)));
    if enabled(panels, state, operation) {
        apply(panels, state, operation, out);
    } else {
        panels.message = Some(if panels.locale == Locale::Zh {
            "请选中支持自动贝塞尔的可编辑数值关键帧；至少需要两个键，手动空间路径、离散参数及速度或弹性曲线不支持。"
        } else {
            "Select editable numeric keys that support automatic Bezier. At least two keys are required; manual spatial paths, discrete parameters, velocity and elastic curves are unsupported."
        }.into());
    }
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
