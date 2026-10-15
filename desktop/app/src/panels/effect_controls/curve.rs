//! Inline editor for curve-kind effect parameters (colour transfer curves).
//! The working copy pattern matches the numeric scrub: edits stream inside a
//! history transaction and the widget renders from its own copy until release.
use super::Action;
use motion_core::{Command, CurveObject};
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::Align,
};

/// Channel selector row + graph + bottom padding, below the parameter name.
pub const HEIGHT: f32 = 26. + GRAPH_HEIGHT + 8.;
const GRAPH_HEIGHT: f32 = 164.;
const HIT_RADIUS: f32 = 7.;
const POINT_LIMIT: usize = 64;
const CHANNELS: [&str; 5] = ["RGB", "R", "G", "B", "A"];

#[derive(Default)]
pub struct Editor {
    channel: usize,
    drag: Option<Drag>,
    cache: Option<(CurveObject, Vec<[f32; 2]>)>,
}
struct Drag {
    point: usize,
    value: CurveObject,
    moved: bool,
}
impl Editor {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    /// Drop an unfinished gesture; true when a history transaction needs
    /// cancellation by the caller.
    pub fn cancel(&mut self) -> bool {
        self.drag.take().is_some_and(|d| d.moved)
    }
    fn samples(&mut self, value: &CurveObject) -> &[[f32; 2]] {
        if !self
            .cache
            .as_ref()
            .is_some_and(|(cached, _)| cached == value)
        {
            let graph = value.graph();
            let samples = graph["channels"][self.channel]["samples"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some([p[0].as_f64()? as f32, p[1].as_f64()? as f32]))
                .collect();
            self.cache = Some((value.clone(), samples));
        }
        &self.cache.as_ref().unwrap().1
    }
    /// `command` wraps an edited curve object into the owning engine command.
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        input: &Input,
        enabled: bool,
        current: &CurveObject,
        mut command: impl FnMut(CurveObject) -> Command,
        out: &mut Vec<Action>,
        readout: &mut Option<([f32; 2], String)>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        let edit_enabled = enabled && self.drag.is_none();

        // Channel selector + reset.
        for (i, name) in CHANNELS.into_iter().enumerate() {
            let button = Rect::new(rect.min[0] + i as f32 * 44., rect.min[1], 40., 22.);
            let selected = i == self.channel;
            paint::rect(
                list,
                button,
                if selected {
                    colors.ACCENT.with_alpha(0.35)
                } else if edit_enabled && button.contains(input.mouse) {
                    colors.SUNKEN
                } else {
                    colors.CHROME
                },
            );
            motion_ui::ui::text(
                list,
                atlas,
                button.shrink(2.),
                name,
                if selected { colors.ACCENT } else { colors.TEXT },
                Align::Center,
            );
            if edit_enabled
                && input
                    .pressed
                    .is_some_and(|(b, p)| b == MouseButton::Left && button.contains(p))
            {
                self.channel = i;
                self.cache = None;
            }
        }
        let reset = Rect::new(rect.max[0] - 30., rect.min[1], 30., 22.);
        paint::rect(
            list,
            reset,
            if edit_enabled && reset.contains(input.mouse) {
                colors.SUNKEN
            } else {
                colors.CHROME
            },
        );
        motion_ui::icons::draw(
            list,
            reset,
            motion_ui::icons::Icon::Reset,
            14.,
            if enabled {
                colors.TEXT
            } else {
                colors.TEXT_MUTED.with_alpha(0.4)
            },
        );
        if edit_enabled
            && input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && reset.contains(p))
        {
            let mut value = current.clone();
            value.channels[self.channel] = vec![[0., 0.], [1., 1.]];
            value.channel_luts[self.channel] = None;
            value.sampled_lut = None;
            out.push(Action::Edit(command(value)));
        }

        let graph = Rect::new(
            rect.min[0],
            rect.min[1] + 26.,
            rect.width(),
            GRAPH_HEIGHT,
        );
        let gx = |x: f32| ((x - graph.min[0]) / graph.width()).clamp(0., 1.);
        let gy = |y: f32| ((graph.max[1] - y) / graph.height()).clamp(0., 1.);
        let px = |x: f32| graph.min[0] + x * graph.width();
        let py = |y: f32| graph.max[1] - y * graph.height();

        let value = self
            .drag
            .as_ref()
            .map_or_else(|| current.clone(), |d| d.value.clone());
        let samples = self.samples(&value).to_vec();

        // Background, grid, identity diagonal.
        paint::rect(list, graph, colors.SUNKEN.with_alpha(0.5));
        for i in 1..4 {
            let f = i as f32 / 4.;
            paint::line(
                list,
                vec![[px(f), graph.min[1]], [px(f), graph.max[1]]],
                colors.TEXT_MUTED.with_alpha(0.25),
            );
            paint::line(
                list,
                vec![[graph.min[0], py(f)], [graph.max[0], py(f)]],
                colors.TEXT_MUTED.with_alpha(0.25),
            );
        }
        paint::line(
            list,
            vec![
                [graph.min[0], graph.max[1]],
                [graph.max[0], graph.min[1]],
            ],
            colors.TEXT_MUTED.with_alpha(0.35),
        );
        paint::line_with_width(
            list,
            samples.iter().map(|p| [px(p[0]), py(p[1])]).collect(),
            colors.ACCENT,
            1.5,
        );
        paint::stroke(list, graph, colors.TEXT_MUTED.with_alpha(0.5), 1.);

        // Control points.
        let points = &value.channels[self.channel];
        let hovered = points.iter().position(|p| {
            let dx = px(p[0]) - input.mouse[0];
            let dy = py(p[1]) - input.mouse[1];
            dx * dx + dy * dy <= HIT_RADIUS * HIT_RADIUS
        });
        for (i, p) in points.iter().enumerate() {
            let center = [px(p[0]), py(p[1])];
            let active = self.drag.as_ref().is_some_and(|d| d.point == i);
            paint::diamond(
                list,
                center,
                if active || hovered == Some(i) { 5. } else { 4. },
                if active {
                    colors.ACCENT
                } else if hovered == Some(i) && enabled {
                    colors.TEXT
                } else {
                    colors.TEXT_MUTED
                },
            );
        }

        if !enabled {
            return;
        }
        // Begin / continue / finish gestures.
        if let Some((button, point)) = input.pressed {
            if graph.contains(point) {
                match button {
                    MouseButton::Left if self.drag.is_none() => {
                        if let Some(i) = hovered {
                            self.drag = Some(Drag {
                                point: i,
                                value: current.clone(),
                                moved: false,
                            });
                            out.push(Action::History(2));
                        } else if points.len() < POINT_LIMIT {
                            let mut value = current.clone();
                            let channel = &mut value.channels[self.channel];
                            // Quantize at insertion so the drag update below
                            // stays idempotent on the press frame.
                            let inserted = [
                                (gx(point[0]) * 1000.).round() / 1000.,
                                (gy(point[1]) * 1000.).round() / 1000.,
                            ];
                            let at = channel.partition_point(|p| p[0] < inserted[0]);
                            channel.insert(at, inserted);
                            value.channel_luts[self.channel] = None;
                            value.sampled_lut = None;
                            out.push(Action::History(2));
                            out.push(Action::Edit(command(value.clone())));
                            self.drag = Some(Drag {
                                point: at,
                                value,
                                moved: true,
                            });
                        }
                    }
                    MouseButton::Right => {
                        if let Some(i) = hovered {
                            let interior = i > 0 && i + 1 < points.len();
                            if interior && points.len() > 2 {
                                let mut value = current.clone();
                                value.channels[self.channel].remove(i);
                                value.channel_luts[self.channel] = None;
                                value.sampled_lut = None;
                                out.push(Action::Edit(command(value)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if input.focus_lost && self.drag.is_some() {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        }
        if let Some(drag) = self.drag.as_mut() {
            let i = drag.point;
            let channel = &mut drag.value.channels[self.channel];
            let (min_x, max_x) = if i == 0 {
                (0., 0.)
            } else if i + 1 == channel.len() {
                (1., 1.)
            } else {
                (channel[i - 1][0] + 0.004, channel[i + 1][0] - 0.004)
            };
            let next = [
                gx(input.mouse[0]).clamp(min_x, max_x),
                gy(input.mouse[1]),
            ];
            let next = [
                (next[0] * 1000.).round() / 1000.,
                (next[1] * 1000.).round() / 1000.,
            ];
            if channel[i] != next {
                channel[i] = next;
                drag.value.channel_luts[self.channel] = None;
                drag.value.sampled_lut = None;
                drag.moved = true;
                out.push(Action::Edit(command(drag.value.clone())));
            }
            *readout = Some((
                input.mouse,
                format!("{} ({:.3}, {:.3})", CHANNELS[self.channel], next[0], next[1]),
            ));
            if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
                self.drag = None;
                out.push(Action::History(3));
            }
        }
    }
}

/// Pick the editable curve object at a local frame: the static value when
/// unanimated, otherwise the key at or before the frame.
pub fn value_at(track: &motion_core::CurveTrack, local: f64) -> CurveObject {
    if track.keys.is_empty() {
        return track.value.clone();
    }
    let upper = track
        .keys
        .partition_point(|k| f64::from(k.frame) <= local);
    if upper == 0 {
        track.keys[0].value.clone()
    } else {
        track.keys[upper - 1].value.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use motion_core::EffectAction;
    use motion_ui::text::TextAtlas;

    fn input(x: f32, y: f32, pressed: bool, released: bool) -> Input {
        Input {
            mouse: [x, y],
            held: (!released).then_some(MouseButton::Left),
            pressed: pressed.then_some((MouseButton::Left, [x, y])),
            released: released.then_some((MouseButton::Left, [x, y])),
            ..Default::default()
        }
    }
    fn run(engine: &Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::History(op) => {
                    engine.history(op).unwrap();
                }
                Action::Edit(command) => {
                    engine
                        .edit(serde_json::to_string(&command).unwrap())
                        .unwrap();
                }
                _ => panic!("unexpected curve action"),
            }
        }
    }
    #[test]
    fn click_adds_point_and_drag_commits_one_undo() {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        engine
            .edit(
                serde_json::to_string(&Command::Add {
                    layer: motion_core::Layer::solid(
                        7,
                        "Curve",
                        [100., 100.],
                        [960., 540., 0.],
                        [1.; 4],
                    ),
                })
                .unwrap(),
            )
            .unwrap();
        let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
        let package = catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["manifest"]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["id"] == "curves")
            })
            .unwrap();
        let param = package["manifest"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "curves")
            .unwrap()["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["kind"] == "curve")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        engine
            .plugin(
                serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"curves"})
                    .to_string(),
            )
            .unwrap();
        let state = engine.state().unwrap();
        let effect = state["project"]["layers"][0]["effects"][0]["id"]
            .as_u64()
            .unwrap();
        let before = engine.state().unwrap()["project"].clone();

        let mut editor = Editor::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let rect = Rect::new(0., 0., 300., HEIGHT);
        let command = |value: CurveObject| Command::Effect {
            object: 7,
            action: EffectAction::SetCurveObject {
                effect,
                param: param.clone(),
                frame: 0,
                value,
            },
        };
        let mut draw = |editor: &mut Editor, input: &Input| {
            let mut actions = vec![];
            editor.draw(
                &mut PaintList::default(),
                &mut atlas,
                rect,
                input,
                true,
                &CurveObject::default(),
                command,
                &mut actions,
                &mut None,
            );
            actions
        };
        // Press on empty graph area adds a point inside a transaction.
        let actions = draw(&mut editor, &input(150., 100., true, false));
        assert!(matches!(actions.as_slice(), [Action::History(2), Action::Edit(Command::Effect { .. })]));
        run(&engine, actions);
        // Dragging streams edits, release commits the transaction.
        run(&engine, draw(&mut editor, &input(200., 80., false, false)));
        let actions = draw(&mut editor, &input(200., 80., false, true));
        assert!(matches!(actions.as_slice(), [Action::History(3)]));
        run(&engine, actions);

        let state = engine.state().unwrap();
        let points = &state["project"]["layers"][0]["effects"][0]["params"][param]["curve"]
            ["value"]["channels"][0];
        let points = points.as_array().unwrap();
        assert_eq!(points.len(), 3);
        assert!((points[1][0].as_f64().unwrap() - 0.667).abs() < 0.01);
        assert!((points[1][1].as_f64().unwrap() - 0.671).abs() < 0.01);
        // One undo restores the whole gesture.
        engine.history(0).unwrap();
        assert_eq!(engine.state().unwrap()["project"], before);
    }
    #[test]
    fn right_click_removes_interior_points_only() {
        let mut editor = Editor::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let rect = Rect::new(0., 0., 300., HEIGHT);
        let mut current = CurveObject::default();
        current.channels[0] = vec![[0., 0.], [0.5, 0.5], [1., 1.]];
        let mut actions = vec![];
        // Right-click the middle point at (0.5, 0.5) → graph pixel (150, 108).
        editor.draw(
            &mut PaintList::default(),
            &mut atlas,
            rect,
            &Input {
                mouse: [150., 108.],
                pressed: Some((MouseButton::Right, [150., 108.])),
                ..Default::default()
            },
            true,
            &current,
            |value| Command::Effect {
                object: 7,
                action: EffectAction::SetCurveObject {
                    effect: 1,
                    param: "p".into(),
                    frame: 0,
                    value,
                },
            },
            &mut actions,
            &mut None,
        );
        let [Action::Edit(Command::Effect {
            action: EffectAction::SetCurveObject { value, .. },
            ..
        })] = actions.as_slice()
        else {
            panic!("expected one SetCurveObject edit, got {actions:?}");
        };
        assert_eq!(value.channels[0], vec![[0., 0.], [1., 1.]]);
        // Right-click an endpoint → no edit.
        let mut actions = vec![];
        editor.draw(
            &mut PaintList::default(),
            &mut atlas,
            rect,
            &Input {
                mouse: [0., 190.],
                pressed: Some((MouseButton::Right, [0., 190.])),
                ..Default::default()
            },
            true,
            &current,
            |value| Command::Effect {
                object: 7,
                action: EffectAction::SetCurveObject {
                    effect: 1,
                    param: "p".into(),
                    frame: 0,
                    value,
                },
            },
            &mut actions,
            &mut None,
        );
        assert!(actions.is_empty());
    }
}
