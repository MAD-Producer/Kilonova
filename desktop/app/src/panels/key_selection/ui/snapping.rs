//! Adapt only a key gesture's horizontal pointer coordinate. All existing lane
//! controllers keep their own hit testing, values, collision and history logic.
//! This includes mask/vector/audio lanes without sending two edit commands.
use super::*;

pub(super) struct Gesture {
    scope: Scope,
    view: View,
    origin: u32,
    press: [f32; 2],
    candidates: BTreeSet<i64>,
    revision: Option<u64>,
}
pub(super) fn arm(
    panels: &mut Panels,
    state: &Value,
    chosen: &Set,
    view: View,
    press: [f32; 2],
    candidates: &[f64],
    out: &mut Vec<Action>,
) {
    let Some(primary) = chosen.primary() else {
        return;
    };
    let Some(scope) = Scope::of(state) else {
        return;
    }; // choose/writable already checked identity.
    if candidates.len().saturating_mul(std::mem::size_of::<i64>()) > MAX_KEYFRAME_CLIPBOARD_BYTES {
        out.extend(crate::panels::cancel_interaction(panels));
        panels.selected_keys.intent = true;
        project_primary(panels, state);
        crate::panels::key_selection::transform::failed(
            panels,
            state,
            Error::Invalid("key drag snap candidates exceed metadata limit".into()),
        );
        return;
    }
    let candidates = candidates
        .iter()
        .copied()
        .chain(state["frame"].as_f64())
        .filter(|f| f.is_finite() && *f >= 0. && *f <= crate::panels::last_frame(state))
        .map(|f| f.round() as i64)
        .collect();
    panels.key_gestures.snap = Some(Gesture {
        scope,
        view,
        origin: primary.frame,
        press,
        candidates,
        revision: state["revision"].as_u64(),
    });
}
pub(super) fn input(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    offset: f32,
    out: &mut Vec<Action>,
) -> Option<Input> {
    let snap = panels.key_gestures.snap.as_ref()?;
    if input.pressed.is_some() {
        out.extend(crate::panels::cancel_interaction(panels));
        panels.timeline.snap_guide = None;
        return None;
    }
    if Some(&snap.scope) != Scope::of(state).as_ref()
        || snap.view.mapping[..2] != [ppf, offset]
        || snap.view.graph != panels.timeline.graph_mode
        || state["revision"]
            .as_u64()
            .zip(snap.revision)
            .is_some_and(|(now, expected)| now > expected)
    {
        out.extend(crate::panels::cancel_interaction(panels));
        crate::panels::key_selection::transform::unavailable(panels);
        panels.timeline.snap_guide = None;
        return None;
    }
    if input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == InputKey::Escape)
        || !input.dragging() && input.released.is_none()
    {
        panels.key_gestures.snap = None;
        panels.timeline.snap_guide = None;
        return None;
    }
    let point = input.released.map_or(input.mouse, |(_, p)| p);
    let released_input = || {
        input
            .released
            .filter(|(b, p)| *b == MouseButton::Left && *p != input.mouse)
            .map(|(_, point)| {
                let mut adjusted = input.clone();
                adjusted.mouse = point;
                adjusted
            })
    };
    let dx = point[0] - snap.press[0];
    let dy = point[1] - snap.press[1];
    panels.timeline.snap_guide = None;
    if input.modifiers.alt || snap.view.graph && input.modifiers.shift && dy.abs() > dx.abs() {
        return released_input();
    }
    let styles = panels
        .styles
        .clone()
        .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
    if !motion_ui::widgets::numeric_behavior::started(
        dx.abs().max(dy.abs()),
        input.pointer_travel,
        styles.metric("motion.drag_threshold", 3.),
    ) {
        return released_input();
    }
    let raw = f64::from(snap.origin) + f64::from(dx) / f64::from(ppf);
    if (f64::from(dx) / f64::from(ppf)).round() == 0. {
        return released_input();
    }
    let current = panels.selected_keys.primary().map(|k| i64::from(k.frame));
    let Some(hit) = motion_ui::snap::nearest(
        snap.candidates
            .iter()
            .copied()
            .filter(|f| *f != i64::from(snap.origin) && Some(*f) != current)
            .map(|f| f as f64),
        raw,
        f64::from(ppf),
        f64::from(styles.metric("timeline.key.snap_tolerance", 8.)),
    ) else {
        return released_input();
    };
    let mut adjusted = input.clone();
    let x = snap.press[0] + (hit.target - f64::from(snap.origin)) as f32 * ppf;
    adjusted.mouse[0] = x;
    if let Some((_, point)) = &mut adjusted.released {
        point[0] = x;
    }
    panels.timeline.snap_guide = Some(hit.target);
    Some(adjusted)
}
pub(super) fn finish(panels: &mut Panels, input: &Input, out: &[Action]) {
    if input.released.is_some() {
        panels.key_gestures.snap = None;
        panels.timeline.snap_guide = None;
    }
    if let Some(snap) = &mut panels.key_gestures.snap {
        for action in out {
            if matches!(action, Action::Edit(_) | Action::EditBatch(_)) {
                snap.revision = snap.revision.and_then(|r| r.checked_add(1));
            }
        }
    }
}
