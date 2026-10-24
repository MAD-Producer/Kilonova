//! Camera gestures affect only the host observer, never an authored camera.
use super::*;
use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    gesture: Option<Drag>,
    cancelled: Option<u64>,
}
struct Drag {
    id: u64,
    button: MouseButton,
    mode: CameraMode,
    start: [f64; 2],
    mapping: crate::viewer_navigation::Mapping,
    root: String,
    revision: u64,
    frame: f64,
    view: String,
    context: Option<u64>,
}
impl State {
    pub(crate) fn editing(&self) -> bool {
        self.gesture.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        if let Some(g) = self.gesture.take() {
            self.cancelled = Some(g.id);
        }
    }
}
pub(crate) fn cancel(p: &mut Panels, out: &mut Vec<Action>) {
    p.tools.camera.cancel();
    flush(p, out);
}
pub(crate) fn flush(p: &mut Panels, out: &mut Vec<Action>) {
    if let Some(id) = p.tools.camera.cancelled.take() {
        out.push(Action::CameraGesture(Op::End { id, commit: false }));
    }
}
pub(crate) fn is_tool(t: Active) -> bool {
    matches!(t, Active::Camera | Active::CameraPan | Active::CameraDolly)
}
pub(crate) fn next(t: Active, state: &Value) -> Active {
    match t {
        Active::Camera => Active::CameraPan,
        Active::CameraPan => Active::CameraDolly,
        _ if state["observationView"] == "top" || state["observationView"] == "side" => {
            Active::CameraPan
        }
        _ => Active::Camera,
    }
}
pub(crate) fn owns_history(p: &Panels) -> bool {
    p.focused == Some(motion_ui::dock::Panel::Composition) && is_tool(active(p))
}
pub(crate) fn history_enabled(p: &Panels, state: &Value, redo: bool) -> bool {
    if owns_history(p) && p.tools.camera.editing() {
        return false;
    }
    state[if owns_history(p) {
        if redo {
            "canRedoObservation"
        } else {
            "canUndoObservation"
        }
    } else if redo {
        "canRedo"
    } else {
        "canUndo"
    }]
    .as_bool()
    .unwrap_or(false)
}
pub(crate) fn history(p: &Panels, command: Command, out: &mut Vec<Action>) -> bool {
    if !owns_history(p) {
        return false;
    }
    let op = match command {
        Command::Undo => Op::Undo,
        Command::Redo => Op::Redo,
        _ => return false,
    };
    out.push(Action::CameraGesture(op));
    true
}
pub(crate) fn button(
    list: &mut PaintList,
    rect: Rect,
    p: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    use motion_ui::{
        icons::{self, Icon},
        style::{State, States},
    };
    let styles = list.styles();
    let enabled = state["observing"] == true;
    let tool = if is_tool(active(p)) {
        active(p)
    } else {
        next(active(p), state)
    };
    let states = if !enabled {
        States::default().with(State::Disabled)
    } else if is_tool(active(p)) {
        States::default().with(State::Selected)
    } else {
        States::default()
    };
    let icon = match tool {
        Active::CameraPan => Icon::Move,
        Active::CameraDolly => Icon::Search,
        _ => Icon::Camera,
    };
    if icons::button(list, rect, icon, input, &styles, states).clicked && enabled {
        choose(p, next(active(p), state), state, out);
    }
    if rect.contains(input.mouse) {
        p.drag_readout = Some((
            input.mouse,
            if enabled {
                hint(tool, p)
            } else if p.locale == Locale::Zh {
                "观察摄影机工具：先选择自由、顶视或侧视视图".into()
            } else {
                "Observation camera: choose Free, Top or Side view first".into()
            },
        ));
    }
}
pub(crate) fn viewport(
    p: &mut Panels,
    view: Rect,
    mapping: crate::viewer_navigation::Mapping,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) -> bool {
    let tool = active(p);
    let right = input.pressed.is_some_and(|(b, _)| b == MouseButton::Right)
        || input.held == Some(MouseButton::Right)
        || input.released.is_some_and(|(b, _)| b == MouseButton::Right);
    if !is_tool(tool) && !(right && state["observing"] == true) && p.tools.camera.gesture.is_none()
    {
        return false;
    }
    if tool == Active::Camera
        && (state["observationView"] == "top" || state["observationView"] == "side")
    {
        choose(p, Active::CameraPan, state, out);
        return true;
    }
    let changed = p.tools.camera.gesture.as_ref().is_some_and(|g| {
        g.root != state["root"].as_str().unwrap_or("")
            || g.revision != state["revision"].as_u64().unwrap_or(0)
            || g.frame != state["frame"].as_f64().unwrap_or(0.)
            || g.view != state["observationView"].as_str().unwrap_or("free")
            || g.mapping.image != mapping.image
            || g.context != p.composition.context
    });
    if input.focus_lost
        || changed
        || input.composing
        || input.ime_consumed
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        || input
            .pressed
            .is_some_and(|(b, _)| b != MouseButton::Left && b != MouseButton::Right)
    {
        p.tools.camera.cancel();
        flush(p, out);
        return true;
    }
    if view.contains(input.mouse) {
        p.cursor = Some(cursor(tool));
    }
    // Temporary middle-button panning remains owned by the independent canvas.
    if input.held == Some(MouseButton::Middle)
        || input
            .released
            .is_some_and(|(b, _)| b == MouseButton::Middle)
        || p.composition.pan.is_some()
    {
        return true;
    }
    let press = input.pressed.filter(|(b, pos)| {
        (*b == MouseButton::Left && is_tool(tool) || *b == MouseButton::Right)
            && view.contains(*pos)
            && p.tools.camera.gesture.is_none()
    });
    if let Some((button, position)) = press {
        let c = &mut p.tools.camera;
        c.next = c.next.wrapping_add(1).max(1);
        let id = c.next;
        let mode = if button == MouseButton::Right {
            if state["observationView"] == "top" || state["observationView"] == "side" {
                CameraMode::Pan
            } else {
                CameraMode::Orbit
            }
        } else {
            match tool {
                Active::CameraPan => CameraMode::Pan,
                Active::CameraDolly => CameraMode::Dolly,
                _ => CameraMode::Orbit,
            }
        };
        c.gesture = Some(Drag {
            id,
            button,
            mode,
            start: mapping.local(position),
            mapping,
            root: state["root"].as_str().unwrap_or("").into(),
            revision: state["revision"].as_u64().unwrap_or(0),
            frame: state["frame"].as_f64().unwrap_or(0.),
            view: state["observationView"].as_str().unwrap_or("free").into(),
            context: p.composition.context,
        });
        out.push(Action::CameraGesture(Op::Begin { id }));
    }
    if let Some(g) = p.tools.camera.gesture.as_ref() {
        let release = input.released.filter(|(b, _)| *b == g.button);
        if input.held != Some(g.button) && release.is_none() {
            p.tools.camera.cancel();
            flush(p, out);
            return true;
        }
        let end = g.mapping.local(release.map_or(input.mouse, |(_, pos)| pos));
        let delta = std::array::from_fn(|i| (end[i] - g.start[i]) as f32);
        let viewport = [
            state["project"]["width"].as_u64().unwrap_or(0) as u32,
            state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        out.push(Action::CameraGesture(Op::Update {
            id: g.id,
            mode: g.mode,
            delta,
            viewport,
        }));
        p.cursor = Some(Cursor::Grabbing);
        if release.is_some() {
            out.push(Action::CameraGesture(Op::End {
                id: g.id,
                commit: true,
            }));
            p.tools.camera.gesture = None;
        }
    }
    true
}
#[cfg(test)]
mod tests;
