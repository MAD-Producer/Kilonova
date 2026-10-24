//! One activation transition for toolbar and keyboard. Authored edits remain host commands.
use super::{path_controls, transform_controls, Action, Panels};
use crate::{i18n::Locale, shortcuts::Command};
use motion_core::vector::ShapeKind;
use motion_ui::{
    cursor::Cursor,
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Active {
    #[default]
    Select,
    Pen,
    Mask,
    Rotate,
    Scale,
    Anchor,
    Rectangle,
    Ellipse,
    Text,
    Hand,
    Zoom,
    Camera,
    CameraPan,
    CameraDolly,
}
#[derive(Default)]
pub(crate) struct State {
    /// Path and transform controls retain ownership of their existing gestures.
    auxiliary: Option<Active>,
    camera: camera::State,
    shape: Option<ShapeGesture>,
    owner: Option<motion_ui::dock::Panel>,
    render_owner: Option<motion_ui::dock::Panel>,
    nudge_failed: bool,
    nudge_repeats: motion_render::observability::FailureCounter,
}
struct ShapeGesture {
    start: [f32; 2],
    local: [f64; 2],
    mapping: crate::viewer_navigation::Mapping,
    root: String,
    revision: u64,
    frame: f64,
    context: Option<u64>,
}
impl State {
    pub(crate) fn editing(&self) -> bool {
        self.shape.is_some() || self.camera.editing()
    }
    pub(crate) fn cancel(&mut self) {
        self.shape = None;
        self.camera.cancel();
        self.nudge_failed = false;
        self.nudge_repeats.success();
    }
}
/// Only an input-bearing window changes ownership; background redraws cannot
/// cancel a gesture in another native window.
pub(crate) fn prepare(panels: &mut Panels, owner: Option<motion_ui::dock::Panel>, input: &Input) {
    panels.tools.render_owner = owner;
    let event = input.pending
        || input.pressed.is_some()
        || input.released.is_some()
        || !input.keys.is_empty()
        || input.wheel != [0., 0.];
    if input.focus_lost && panels.tools.owner == owner
        || !input.focus_lost && event && panels.tools.owner != owner
    {
        panels.tools.cancel();
        if let Some((_, before)) = panels.composition.pan.take() {
            panels.composition.navigation = before;
        }
    }
    if !input.focus_lost && event {
        panels.tools.owner = owner;
    }
}
pub(crate) fn active(panels: &Panels) -> Active {
    // Inspector buttons use the same existing controls, so their state is authoritative.
    match panels.path_controls.tool {
        path_controls::Tool::Pen => Active::Pen,
        path_controls::Tool::Mask => Active::Mask,
        path_controls::Tool::Select => {
            panels
                .tools
                .auxiliary
                .unwrap_or(match panels.transform_controls.tool {
                    transform_controls::Tool::Move => Active::Select,
                    transform_controls::Tool::Rotate => Active::Rotate,
                    transform_controls::Tool::Scale => Active::Scale,
                    transform_controls::Tool::Anchor => Active::Anchor,
                })
        }
    }
}
pub(crate) fn choose(panels: &mut Panels, tool: Active, state: &Value, out: &mut Vec<Action>) {
    if camera::is_tool(tool) && state["observing"] != true {
        panels.message = Some(
            if panels.locale == Locale::Zh {
                "摄影机工具仅用于观察视图，请先切换到观察视图"
            } else {
                "Camera tool requires an observation view"
            }
            .into(),
        );
        return;
    }
    // Both owners are cancelled before installing the new tool; rollback is emitted once.
    let path = panels.path_controls.cancel();
    let transform = panels.transform_controls.cancel();
    if path || transform {
        out.push(Action::History(4));
    }
    panels.tools.cancel();
    camera::flush(panels, out);
    if let Some((_, before)) = panels.composition.pan.take() {
        panels.composition.navigation = before;
    }
    panels.path_controls.choose(
        match tool {
            Active::Pen => path_controls::Tool::Pen,
            Active::Mask => path_controls::Tool::Mask,
            _ => path_controls::Tool::Select,
        },
        out,
    );
    panels.transform_controls.choose(
        match tool {
            Active::Rotate => transform_controls::Tool::Rotate,
            Active::Scale => transform_controls::Tool::Scale,
            Active::Anchor => transform_controls::Tool::Anchor,
            _ => transform_controls::Tool::Move,
        },
        out,
    );
    panels.tools.auxiliary = matches!(
        tool,
        Active::Rectangle
            | Active::Ellipse
            | Active::Text
            | Active::Hand
            | Active::Zoom
            | Active::Camera | Active::CameraPan | Active::CameraDolly
    )
    .then_some(tool);
    panels.message = Some(hint(tool, panels));
}
pub(crate) fn command(
    panels: &mut Panels,
    command: Command,
    state: &Value,
    out: &mut Vec<Action>,
) -> bool {
    let tool = match command {
        Command::ToolSelect => Active::Select,
        Command::ToolPen => Active::Pen,
        Command::ToolAnchor => Active::Anchor,
        Command::ToolRotate => Active::Rotate,
        Command::ToolShape => {
            if active(panels) == Active::Rectangle {
                Active::Ellipse
            } else {
                Active::Rectangle
            }
        }
        Command::ToolText => Active::Text,
        Command::ToolHand => Active::Hand,
        Command::ToolZoom => Active::Zoom,
        Command::ToolCamera => camera::next(active(panels), state),
        _ => return false,
    };
    choose(panels, tool, state, out);
    true
}
pub(crate) fn title(tool: Active, locale: Locale) -> &'static str {
    let (zh, en) = match tool {
        Active::Select => ("选择工具", "Selection tool"),
        Active::Pen => ("钢笔工具", "Pen tool"),
        Active::Mask => ("蒙版工具", "Mask tool"),
        Active::Rotate => ("旋转工具", "Rotation tool"),
        Active::Scale => ("缩放变换工具", "Scale transform tool"),
        Active::Anchor => ("锚点工具", "Anchor tool"),
        Active::Rectangle => ("矩形工具", "Rectangle tool"),
        Active::Ellipse => ("椭圆工具", "Ellipse tool"),
        Active::Text => ("文字工具", "Text tool"),
        Active::Hand => ("抓手工具", "Hand tool"),
        Active::Zoom => ("视图缩放工具", "Viewer zoom tool"),
        Active::Camera => ("观察环绕工具", "Observation orbit tool"),
        Active::CameraPan => ("观察平移工具", "Observation pan tool"),
        Active::CameraDolly => ("观察推拉工具", "Observation dolly tool"),
    };
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
pub(crate) fn hint(tool: Active, panels: &Panels) -> String {
    let command = match tool {
        Active::Select => Some(Command::ToolSelect),
        Active::Pen => Some(Command::ToolPen),
        Active::Anchor => Some(Command::ToolAnchor),
        Active::Rotate => Some(Command::ToolRotate),
        Active::Rectangle | Active::Ellipse => Some(Command::ToolShape),
        Active::Text => Some(Command::ToolText),
        Active::Hand => Some(Command::ToolHand),
        Active::Zoom => Some(Command::ToolZoom),
        Active::Camera | Active::CameraPan | Active::CameraDolly => Some(Command::ToolCamera),
        _ => None,
    };
    let name = title(tool, panels.locale);
    let label = command.map_or_else(
        || name.to_string(),
        |command| {
            panels
                .shortcut_bindings
                .label(name, crate::shortcuts::Scope::Viewer, command)
        },
    );
    let (zh, en) = match tool {
        Active::Rectangle | Active::Ellipse => (
            "再次激活切换形状；拖动画布绘制，Shift 等比",
            "Activate again to cycle shapes; drag to draw, Shift constrains",
        ),
        Active::Text => ("点击画布后设置文字", "Click viewer to configure text"),
        Active::Hand => (
            "拖动平移；中键可临时平移",
            "Drag to pan; middle mouse pans temporarily",
        ),
        Active::Zoom => (
            "点击放大，Alt 点击缩小",
            "Click to zoom in, Alt-click to zoom out",
        ),
        Active::Camera | Active::CameraPan | Active::CameraDolly => ("再次激活切换；拖动观察视角，Esc 取消；此工具下撤销仅作用于视角", "Activate again to cycle; drag observation view, Esc cancels; undo in this tool affects view only"),
        _ => ("", ""),
    };
    let note = if panels.locale == Locale::Zh { zh } else { en };
    if note.is_empty() {
        label
    } else {
        format!("{label} · {note}")
    }
}
pub(crate) fn cursor(tool: Active) -> Cursor {
    match tool {
        Active::Hand => Cursor::Grab,
        Active::Text => Cursor::Text,
        Active::Select => Cursor::Default,
        _ => Cursor::Crosshair,
    }
}
/// Own auxiliary tools and temporary middle-button navigation before path hit testing.
pub(crate) fn viewport(
    panels: &mut Panels,
    list: &mut PaintList,
    view: Rect,
    mapping: crate::viewer_navigation::Mapping,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) -> bool {
    camera::flush(panels, out);
    if panels.tools.editing() && panels.tools.owner != panels.tools.render_owner { return true; }
    if camera::is_tool(active(panels)) && state["observing"] != true {
        choose(panels, Active::Select, state, out);
    }
    let tool = active(panels);
    if camera::viewport(panels, view, mapping, state, input, out) { return true; }
    if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
        panels.tools.cancel();
        return matches!(
            tool,
            Active::Rectangle
                | Active::Ellipse
                | Active::Text
                | Active::Hand
                | Active::Zoom
                | Active::Camera | Active::CameraPan | Active::CameraDolly
        );
    }
    if panels.tools.shape.as_ref().is_some_and(|g| {
        g.root != state["root"].as_str().unwrap_or("")
            || g.revision != state["revision"].as_u64().unwrap_or(0)
            || g.frame != state["frame"].as_f64().unwrap_or(0.)
            || g.mapping.image != mapping.image
            || g.context != panels.composition.context
    }) {
        panels.tools.cancel();
    }
    if view.contains(input.mouse) {
        panels.cursor = Some(cursor(tool));
    }
    let middle = input.held == Some(MouseButton::Middle)
        || input
            .released
            .is_some_and(|(b, _)| b == MouseButton::Middle);
    if middle || panels.composition.pan.is_some() {
        if panels.composition.pan.is_some() {
            panels.cursor = Some(Cursor::Grabbing);
        }
        panels.tools.cancel();
        return true;
    }
    let auxiliary = matches!(
        tool,
        Active::Rectangle
            | Active::Ellipse
            | Active::Text
            | Active::Hand
            | Active::Zoom
            | Active::Camera | Active::CameraPan | Active::CameraDolly
    );
    if !auxiliary {
        return false;
    }
    let press = input
        .pressed
        .filter(|(b, p)| *b == MouseButton::Left && view.contains(*p));
    if let Some((_, p)) = press {
        match tool {
            Active::Rectangle | Active::Ellipse => {
                panels.tools.shape = Some(ShapeGesture {
                    start: p,
                    local: mapping.local(p),
                    mapping,
                    root: state["root"].as_str().unwrap_or("").into(),
                    revision: state["revision"].as_u64().unwrap_or(0),
                    frame: state["frame"].as_f64().unwrap_or(0.),
                    context: panels.composition.context,
                })
            }
            Active::Text => out.push(Action::NewText),
            Active::Zoom => {
                let ratio = if input.modifiers.alt { 0.5 } else { 2. };
                if !panels.composition.navigation.zoom_at(
                    view,
                    [
                        state["project"]["width"].as_f64().unwrap_or(1920.),
                        state["project"]["height"].as_f64().unwrap_or(1080.),
                    ],
                    p,
                    mapping.scale * ratio,
                ) {
                    rejected(
                        panels,
                        "zoom",
                        state,
                        "finite viewer zoom mapping unavailable",
                    );
                }
            }
            _ => {}
        }
    }
    if let Some(g) = &panels.tools.shape {
        let end = input
            .released
            .filter(|(b, _)| *b == MouseButton::Left)
            .map_or(input.mouse, |(_, p)| p);
        let mut delta = [end[0] - g.start[0], end[1] - g.start[1]];
        if input.modifiers.shift {
            let side = delta[0].abs().max(delta[1].abs());
            for d in &mut delta {
                *d = side * if *d < 0. { -1. } else { 1. };
            }
        }
        let end = [g.start[0] + delta[0], g.start[1] + delta[1]];
        let rect = Rect::new(
            g.start[0].min(end[0]),
            g.start[1].min(end[1]),
            delta[0].abs(),
            delta[1].abs(),
        );
        let styles = list.styles();
        paint::stroke(
            list,
            rect,
            styles.color("focus"),
            styles.metric("border", 1.),
        );
        if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
            let last = g.mapping.local(end);
            let size = std::array::from_fn(|i| (last[i] - g.local[i]).abs() as f32);
            if rect.width() >= styles.metric("motion.drag_threshold", 3.)
                && rect.height() >= styles.metric("motion.drag_threshold", 3.)
                && size
                    .iter()
                    .all(|v| v.is_finite() && *v > 0. && *v <= 32768.)
            {
                out.push(Action::PlaceShape {
                    shape: if tool == Active::Rectangle {
                        ShapeKind::Rectangle
                    } else {
                        ShapeKind::Ellipse
                    },
                    size,
                    position: [
                        ((g.local[0] + last[0]) * 0.5) as f32,
                        ((g.local[1] + last[1]) * 0.5) as f32,
                        0.,
                    ],
                });
            } else if rect.width() >= styles.metric("motion.drag_threshold", 3.)
                && rect.height() >= styles.metric("motion.drag_threshold", 3.)
            {
                rejected(
                    panels,
                    "create_shape",
                    state,
                    "shape size is non-finite or exceeds 32768 source pixels",
                );
            }
            panels.tools.cancel();
        } else if input.held != Some(MouseButton::Left) {
            panels.tools.cancel();
        }
    }
    true
}

fn rejected(panels: &mut Panels, operation: &str, state: &Value, cause: &str) {
    log::error!("viewer tool rejected operation={operation} revision={} frame={} selected={:?} phase=prepare cause={}",state["revision"],state["frame"],panels.selected,cause.chars().take(512).collect::<String>());
    panels.message = Some(format!(
        "{}: {cause}",
        if panels.locale == Locale::Zh {
            "工具操作失败"
        } else {
            "Tool operation failed"
        }
    ));
}
/// Source-pixel movement. It never aliases timeline seek or property reveal.
pub(crate) fn nudge(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) -> bool {
    if camera::is_tool(active(panels)) {return false;}
    let mut delta = [0.; 2];
    let mut pressed = false;
    for (key, m, _) in input.key_presses() {
        if m.control || m.alt {
            continue;
        }
        let step = if m.shift { 10. } else { 1. };
        match key {
            Key::Left => delta[0] -= step,
            Key::Right => delta[0] += step,
            Key::Up => delta[1] -= step,
            Key::Down => delta[1] += step,
            _ => continue,
        };
        pressed = true;
    }
    if !pressed {
        return false;
    }
    if delta == [0., 0.] {
        panels.tools.nudge_failed = false;
        panels.tools.nudge_repeats.success();
        return true;
    }
    if let Some(result) = panels.path_controls.nudge(state, panels.selected, delta) {
        match result {
            Ok(commands) => {
                panels.tools.nudge_failed = false;
                panels.tools.nudge_repeats.success();
                if !commands.is_empty() {
                    out.push(Action::EditBatch(commands));
                }
            }
            Err(error) => nudge_rejected(panels, "nudge_path", state, &error.to_string(), input),
        }
        return true;
    }
    let mut commands = vec![];
    for object in panels.editable_selection(state) {
        let Some(position) = state["sampledLayers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l["id"].as_u64() == Some(object))
            .and_then(|l| l["position"].as_array())
            .filter(|v| v.len() == 3)
        else {
            nudge_rejected(
                panels,
                "nudge_layers",
                state,
                &format!("sampled position missing for layer {object}"),
                input,
            );
            return true;
        };
        let mut value = [0.; 3];
        for i in 0..3 {
            let Some(v) = position[i].as_f64().filter(|v| v.is_finite()) else {
                nudge_rejected(
                    panels,
                    "nudge_layers",
                    state,
                    &format!("sampled position invalid for layer {object} axis {i}"),
                    input,
                );
                return true;
            };
            value[i] = v as f32 + if i < 2 { delta[i] } else { 0. };
        }
        let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
        let separated = state["timeline_layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|l| {
                l["object"].as_u64() == Some(object)
                    && l["properties"]["position"]["separated"] == true
            });
        if separated {
            for i in 0..2 {
                if delta[i] != 0. {
                    commands.push(motion_core::Command::SetComponent {
                        object,
                        property: motion_core::Property::Position,
                        axis: [motion_core::Axis::X, motion_core::Axis::Y][i],
                        frame,
                        value: value[i],
                    });
                }
            }
        } else {
            commands.push(motion_core::Command::SetVector {
                object,
                property: motion_core::Property::Position,
                frame,
                value,
            });
        }
    }
    panels.tools.nudge_failed = false;
    panels.tools.nudge_repeats.success();
    if !commands.is_empty() {
        out.push(Action::EditBatch(commands));
    }
    true
}
fn nudge_rejected(panels: &mut Panels, operation: &str, state: &Value, cause: &str, input: &Input) {
    if !panels.tools.nudge_failed || input.key_presses().any(|(_, _, repeat)| !repeat) {
        rejected(panels, operation, state, cause);
        panels.tools.nudge_failed = true;
    } else {
        panels.tools.nudge_repeats.failure(
            "viewer.tool.nudge",
            format_args!(
                "operation={operation} revision={} frame={} selected={:?} cause={}",
                state["revision"],
                state["frame"],
                panels.selected,
                cause.chars().take(512).collect::<String>()
            ),
        );
    }
}

pub(crate) mod camera;
#[cfg(test)]
mod tests;
