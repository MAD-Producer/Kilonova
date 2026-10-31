//! A multi-key drag previews only painted marks. The authored document stays
//! untouched until one guarded atomic command is accepted on release.
use super::*;
use crate::panels::key_selection::transform::{self, Plan};

pub(super) struct ValueMotion {
    pub target: KeyframeTarget,
    pub component: usize,
    pub per_pixel: f32,
    pub limits: Option<[f32; 2]>,
}
pub(super) struct Drag {
    pub view: View,
    source: crate::layer_clipboard::KeyframeSource,
    plan: Plan,
    press: [f32; 2],
    motion: Option<ValueMotion>,
    value_limits: [f32; 2],
    candidates: BTreeSet<i64>,
    copy: bool,
    started: bool,
    delta: i32,
    value_delta: f32,
    valid: bool,
    guide: Option<f64>,
}
fn value_limits(
    plan: &Plan,
    state: &Value,
    motion: Option<&ValueMotion>,
) -> motion_core::Result<[f32; 2]> {
    let Some(motion) = motion else {
        return Ok([f32::NEG_INFINITY, f32::INFINITY]);
    };
    let Some(limits) = motion.limits else {
        return Ok([f32::NEG_INFINITY, f32::INFINITY]);
    };
    let (track, offset) = source::track(state, &motion.target)
        .ok_or_else(|| Error::Invalid("key drag value channel missing".into()))?;
    let mut range = [f32::NEG_INFINITY, f32::INFINITY];
    for key in track["keys"]
        .as_array()
        .ok_or_else(|| Error::Invalid("key drag value keys missing".into()))?
    {
        let frame = key["frame"]
            .as_i64()
            .and_then(|f| f.checked_add(offset))
            .and_then(|f| u32::try_from(f).ok())
            .ok_or_else(|| Error::Invalid("key drag authored time overflow".into()))?;
        if !plan.selected.contains(&Key {
            target: motion.target.clone(),
            frame,
        }) {
            continue;
        }
        let raw = if key["value"].is_array() {
            &key["value"][motion.component]
        } else {
            &key["value"]
        };
        let value = raw
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| v.is_finite())
            .ok_or_else(|| Error::Invalid("key drag numeric value missing".into()))?;
        range[0] = range[0].max(limits[0] - value);
        range[1] = range[1].min(limits[1] - value);
    }
    if range[0] > range[1] {
        return Err(Error::Invalid(
            "key drag value limits are inconsistent".into(),
        ));
    }
    Ok(range)
}
pub(super) fn arm(
    panels: &mut Panels,
    state: &Value,
    chosen: Set,
    view: View,
    press: [f32; 2],
    motion: Option<ValueMotion>,
    copy: bool,
    candidates: &[f64],
    out: &mut Vec<Action>,
) {
    if chosen.has_local() || chosen.has_spatial() && (copy || motion.is_some()) {
        out.extend(crate::panels::cancel_interaction(panels));
        transform::unavailable(panels);
        return;
    }
    let prepare = || -> motion_core::Result<_> {
        let plan = Plan::capture(&chosen, state)?;
        let limits = value_limits(&plan, state, motion.as_ref())?;
        let selected: BTreeSet<_> = chosen
            .selections()
            .into_iter()
            .flat_map(|s| s.frames)
            .map(i64::from)
            .collect();
        let candidates: BTreeSet<_> = candidates
            .iter()
            .copied()
            .chain(state["frame"].as_f64())
            .filter(|f| f.is_finite() && *f >= 0. && *f < f64::from(plan.total))
            .map(|f| f.round() as i64)
            .filter(|f| !selected.contains(f))
            .take(MAX_KEYFRAME_CLIPBOARD_BYTES / std::mem::size_of::<i64>() + 1)
            .collect();
        if candidates.len() > MAX_KEYFRAME_CLIPBOARD_BYTES / std::mem::size_of::<i64>() {
            return Err(Error::Invalid(
                "key drag snap candidates exceed metadata limit".into(),
            ));
        }
        Ok((plan, limits, candidates))
    };
    let (plan, value_limits, candidates) = match prepare() {
        Ok(v) => v,
        Err(error) => {
            out.extend(crate::panels::cancel_interaction(panels));
            panels.selected_keys.intent = true;
            project_primary(panels, state);
            transform::failed(panels, state, error);
            return;
        }
    };
    // Press has only armed the old single-key controller. Yield it before the
    // first movement, preserving Bézier-handle and numeric-field ownership.
    out.extend(crate::panels::cancel_interaction(panels));
    panels.selected_keys.intent = true;
    project_primary(panels, state);
    panels.key_gestures.drag = Some(Drag {
        view,
        source: crate::layer_clipboard::KeyframeSource::of(state),
        plan,
        press,
        motion,
        value_limits,
        candidates,
        copy,
        started: false,
        delta: 0,
        value_delta: 0.,
        valid: !copy,
        guide: None,
    });
}
fn snapped(drag: &Drag, delta: i32, tolerance: f64) -> Option<(i32, f64)> {
    let ppf = f64::from(drag.view.mapping[0]);
    let mut best: Option<(f64, i32, f64)> = None;
    for from in drag.plan.times() {
        let to = i64::from(from) + i64::from(delta);
        let candidates = drag
            .candidates
            .range(..=to)
            .next_back()
            .into_iter()
            .chain(drag.candidates.range(to..).next())
            .copied()
            .map(|f| f as f64);
        let Some(snap) = motion_ui::snap::nearest(candidates, to as f64, ppf, tolerance) else {
            continue;
        };
        let Ok(next) = i32::try_from(snap.target as i64 - i64::from(from)) else {
            continue;
        };
        if !drag.plan.allows(next, drag.copy) {
            continue;
        }
        if best.is_none_or(|b| {
            snap.distance_pixels < b.0 || snap.distance_pixels == b.0 && snap.target < b.2
        }) {
            best = Some((snap.distance_pixels, next, snap.target));
        }
    }
    best.map(|(_, delta, target)| (delta, target))
}
pub(super) fn advance(panels: &mut Panels, state: &Value, input: &Input, view: &View) {
    let Some(mut drag) = panels.key_gestures.drag.take() else {
        return;
    };
    if drag.source != crate::layer_clipboard::KeyframeSource::of(state)
        || drag.view.graph != view.graph
        || drag.view.mapping[..3] != view.mapping[..3]
        || !drag.plan.selected.writable(state)
    {
        transform::unavailable(panels);
        panels.timeline.snap_guide = None;
        return;
    }
    if input.pressed.is_some() || !input.dragging() && input.released.is_none() {
        panels.timeline.snap_guide = None;
        return;
    }
    let point = input.released.map_or(input.mouse, |(_, p)| p);
    let dx = point[0] - drag.press[0];
    let dy = drag.press[1] - point[1];
    let styles = panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    if !drag.started
        && motion_ui::widgets::numeric_behavior::started(
            dx.abs().max(dy.abs()),
            input.pointer_travel,
            styles.metric("motion.drag_threshold", 3.),
        )
    {
        drag.started = true;
        panels.selected_keys = drag.plan.selected.clone();
        panels.selected_keys.intent = true;
        project_primary(panels, state);
    }
    if drag.started {
        let value_only = drag.motion.is_some()
            && input.modifiers.shift
            && dy.abs() > dx.abs()
            && !input.modifiers.alt;
        let time_only =
            drag.motion.is_none() || input.modifiers.alt || input.modifiers.shift && !value_only;
        let raw = if value_only {
            0.
        } else {
            (dx / drag.view.mapping[0]).round()
        };
        drag.delta = raw.clamp(i32::MIN as f32, i32::MAX as f32) as i32;
        drag.guide = None;
        if !input.modifiers.alt && !value_only && drag.delta != 0 {
            if let Some((delta, target)) = snapped(
                &drag,
                drag.delta,
                f64::from(styles.metric("timeline.key.snap_tolerance", 8.)),
            ) {
                drag.delta = delta;
                drag.guide = Some(target);
            }
        }
        drag.value_delta = if time_only {
            0.
        } else {
            drag.motion.as_ref().map_or(0., |m| {
                (dy * m.per_pixel).clamp(drag.value_limits[0], drag.value_limits[1])
            })
        };
        drag.valid = raw.is_finite()
            && drag.value_delta.is_finite()
            && drag.plan.allows(drag.delta, drag.copy);
        panels.timeline.snap_guide = drag.guide;
        panels.cursor = Some(motion_ui::cursor::Cursor::ResizeEW);
        let axis = if value_only {
            if panels.locale == crate::i18n::Locale::Zh {
                "仅数值"
            } else {
                "Value only"
            }
        } else if time_only {
            if panels.locale == crate::i18n::Locale::Zh {
                "仅时间"
            } else {
                "Time only"
            }
        } else if panels.locale == crate::i18n::Locale::Zh {
            "时间与数值"
        } else {
            "Time + value"
        };
        let value = if time_only {
            String::new()
        } else {
            format!(" · Δ {:+.3}", drag.value_delta)
        };
        panels.drag_readout = Some((
            point,
            format!(
                "{:+} f · {axis}{value}{}{}",
                drag.delta,
                if drag.copy {
                    if panels.locale == crate::i18n::Locale::Zh {
                        " · 复制"
                    } else {
                        " · Copy"
                    }
                } else {
                    ""
                },
                if drag.valid {
                    ""
                } else if panels.locale == crate::i18n::Locale::Zh {
                    " · 目标不可用"
                } else {
                    " · Unavailable"
                }
            ),
        ));
    }
    panels.key_gestures.drag = Some(drag);
}
pub(super) fn finish(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    view: &View,
    out: &mut Vec<Action>,
) -> bool {
    let Some(drag) = panels.key_gestures.drag.take() else {
        return false;
    };
    if drag.view != *view {
        transform::unavailable(panels);
        panels.timeline.snap_guide = None;
        return true;
    }
    if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
        panels.timeline.snap_guide = None;
        if drag.started {
            if !drag.valid {
                transform::rejected(panels);
                return true;
            }
            if drag.delta != 0 || drag.value_delta != 0. {
                let value = drag
                    .motion
                    .map(|m| motion_core::KeyframeValueOffset {
                        target: m.target,
                        component: m.component,
                        delta: drag.value_delta,
                    })
                    .filter(|v| v.delta != 0.);
                match drag.plan.selected.translated(drag.delta) {
                    Ok(selected) => {
                        out.push(Action::EditorAction {
                            source: drag.source,
                            commands: drag.plan.commands(drag.delta, value, drag.copy),
                        });
                        panels.selected_keys = selected;
                        project_primary(panels, state);
                    }
                    Err(error) => transform::failed(panels, state, error),
                }
            }
        }
        return true;
    }
    panels.key_gestures.drag = Some(drag);
    true
}
pub(super) fn ghost(
    panels: &Panels,
    key: &Key,
    point: [f32; 2],
    component: Option<usize>,
) -> Option<[f32; 2]> {
    ghost_addressed(panels, &AddressedKey::Frame(key.clone()), point, component)
}
pub(super) fn ghost_addressed(
    panels: &Panels,
    key: &AddressedKey,
    point: [f32; 2],
    component: Option<usize>,
) -> Option<[f32; 2]> {
    let drag = panels.key_gestures.drag.as_ref()?;
    if !drag.started || !drag.valid || !drag.plan.selected.contains_addressed(key) {
        return None;
    }
    let dy = drag
        .motion
        .as_ref()
        .filter(|m| &m.target == key.target() && component == Some(m.component))
        .map_or(0., |m| drag.value_delta / m.per_pixel);
    Some([
        point[0] + drag.delta as f32 * drag.view.mapping[0],
        point[1] - dy,
    ])
}
