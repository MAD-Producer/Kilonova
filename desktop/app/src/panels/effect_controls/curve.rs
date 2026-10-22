//! Inline editor for curve-kind effect parameters (colour transfer curves).
//! The working copy pattern matches the numeric scrub: edits stream inside a
//! history transaction and the widget renders from its own copy until release.
use super::Action;
use motion_core::{Command, CurveObject};
use motion_ui::{
    input::{Input, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
};

pub fn height(styles: &motion_ui::style::StyleSheet, width: f32) -> f32 {
    styles
        .metric("effect.row.height", 26.)
        .max(styles.metric("row.height", 22.))
        + width.min(styles.metric("effect.curve.graph_height", 320.))
        + styles.metric("padding.panel", 8.)
}
const POINT_LIMIT: usize = 64;
const CHANNELS: [&str; 5] = ["RGB", "R", "G", "B", "A"];

#[cfg(test)]
mod sampling_failure_tests;

#[cfg(test)]
mod observation_tests {
    use super::*;
    #[test]
    fn zero_extent_and_retained_curve_drag_emit_bounded_real_draw_evidence() {
        const CHILD: &str = "MOTION_CURVE_DRAW_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let _logger = crate::logging::init(root.clone()).unwrap();
            let mut editor = Editor::default();
            let mut current = CurveObject::default();
            current.channels[0] = vec![[0., 0.], [0.5, 0.5], [1., 1.]];
            let command = |value| Command::Effect {
                object: 7,
                action: motion_core::EffectAction::SetCurveObject {
                    effect: 1,
                    param: "curve".into(),
                    frame: 0,
                    value,
                },
            };
            let rect = Rect::new(0., 0., 300., height(&PaintList::default().styles(), 300.));
            let mut draw = |editor: &mut Editor, rect: Rect, input: &Input| {
                let mut actions = Vec::new();
                editor.draw(
                    &mut PaintList::default(),
                    &mut atlas,
                    rect,
                    rect,
                    None,
                    input,
                    true,
                    &current,
                    command,
                    &mut actions,
                    &mut None,
                );
                actions
            };
            assert!(draw(&mut editor, rect, &Input::default()).is_empty());
            assert!(std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .is_empty());
            for _ in 0..120 {
                assert!(
                    draw(&mut editor, Rect::new(0., 0., 0., 100.), &Input::default()).is_empty()
                );
                assert!(!editor.dragging());
            }
            draw(&mut editor, rect, &Input::default());
            let graph = editor.test_graph.unwrap();
            let point = [(graph.min[0] + graph.max[0]) * 0.5, (graph.min[1] + graph.max[1]) * 0.5];
            let actions = draw(
                &mut editor,
                rect,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            assert!(actions.iter().any(|a| matches!(a, Action::History(2))));
            assert!(editor.dragging());
            for _ in 0..120 {
                assert!(draw(
                    &mut editor,
                    rect,
                    &Input {
                        mouse: point,
                        ..Default::default()
                    }
                )
                .is_empty());
                assert!(editor.dragging());
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            for site in ["effects.curve_geometry", "effects.curve_release"] {
                assert_eq!(
                    text.lines()
                        .filter(|line| line.contains(&format!("site={site} ")))
                        .count(),
                    2,
                    "{text}"
                );
            }
            let actions = draw(
                &mut editor,
                rect,
                &Input {
                    mouse: point,
                    released: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
            );
            assert!(matches!(actions.as_slice(), [Action::History(3)]));
            assert!(!editor.dragging());
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::effect_controls::curve::observation_tests::zero_extent_and_retained_curve_drag_emit_bounded_real_draw_evidence"])
            .env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("persistent panel degradation site=effects.curve_geometry"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
}

#[derive(Default)]
pub struct Editor {
    channel: usize,
    channel_menu: motion_ui::widgets::Dropdown,
    drag: Option<Drag>,
    cache: Option<(CurveObject, Vec<[f32; 2]>)>,
    #[cfg(test)]
    pub(crate) test_graph: Option<Rect>,
}
struct Drag {
    point: usize,
    value: CurveObject,
    moved: bool,
}
impl Editor {
    pub fn modal(&self) -> bool {
        self.channel_menu.open
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    /// Drop an unfinished gesture; true when a history transaction needs
    /// cancellation by the caller.
    pub fn cancel(&mut self) -> bool {
        self.channel_menu.open = false;
        self.drag.take().is_some()
    }
    fn samples(&mut self, value: &CurveObject) -> motion_core::Result<&[[f32; 2]]> {
        if !self
            .cache
            .as_ref()
            .is_some_and(|(cached, _)| cached == value)
        {
            let graph = value.graph()?;
            let samples = graph["channels"][self.channel]["samples"]
                .as_array()
                .ok_or_else(|| motion_core::Error::Invalid(format!(
                    "color curve channel={}: sample array is missing", self.channel)))?
                .iter().enumerate().map(|(index, point)| {
                    let coordinate = |axis: usize| point[axis].as_f64().filter(|v| v.is_finite())
                        .map(|v| v as f32).ok_or_else(|| motion_core::Error::Invalid(format!(
                            "color curve channel={} sample={index} axis={axis}: finite coordinate is missing", self.channel)));
                    Ok([coordinate(0)?, coordinate(1)?])
                }).collect::<motion_core::Result<Vec<_>>>()?;
            self.cache = Some((value.clone(), samples));
        }
        self.cache.as_ref().map(|(_, samples)| samples.as_slice())
            .ok_or_else(|| motion_core::Error::Invalid("color curve sample cache is missing".into()))
    }
    /// `command` wraps an edited curve object into the owning engine command.
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        bounds: Rect,
        channel: Option<usize>,
        input: &Input,
        enabled: bool,
        current: &CurveObject,
        mut command: impl FnMut(CurveObject) -> Command,
        out: &mut Vec<Action>,
        readout: &mut Option<([f32; 2], String)>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        if let Some(channel) = channel {
            let channel = channel.min(CHANNELS.len() - 1);
            if self.channel != channel {
                self.channel = channel;
                self.cache = None;
            }
            self.channel_menu.open = false;
        }
        let original_input = input;
        let neutral = Input::default();
        let input = if self.channel_menu.open {
            &neutral
        } else {
            input
        };
        let edit_enabled = enabled && self.drag.is_none();
        let row = styles
            .metric("effect.row.height", 26.)
            .max(styles.metric("row.height", 22.));
        let field = row;
        let gap = styles.metric("effect.curve.selector.gap", 4.);
        let reset_width = styles.metric("effect.curve.reset.width", 30.);
        let selector = Rect::new(
            rect.min[0],
            rect.min[1],
            (rect.width() - reset_width - gap).max(0.),
            field,
        );
        let items: Vec<_> = CHANNELS
            .iter()
            .map(|name| motion_ui::widgets::DropdownItem {
                label: *name,
                enabled: true,
            })
            .collect();
        let mut overlay = list.child();
        if channel.is_some() {
            motion_ui::ui::text(
                list,
                atlas,
                selector,
                CHANNELS[self.channel],
                colors.TEXT_MUTED,
                motion_ui::ui::Align::Left,
            );
        } else {
            if let Some(channel) = self.channel_menu.draw(
                &mut overlay,
                atlas,
                selector,
                bounds,
                &items,
                self.channel,
                original_input,
                edit_enabled,
                false,
            ) {
                self.channel = channel;
                self.cache = None;
            }
        }
        let reset = Rect::new(rect.max[0] - reset_width, rect.min[1], reset_width, field);
        if motion_ui::ui::button_icon(
            list,
            reset,
            motion_ui::icons::Icon::Reset,
            input,
            edit_enabled,
        )
        .clicked
        {
            let mut value = current.clone();
            value.channels[self.channel] = vec![[0., 0.], [1., 1.]];
            value.channel_luts[self.channel] = None;
            value.sampled_lut = None;
            out.push(Action::Edit(command(value)));
        }
        let top = rect.min[1] + row;
        let graph = Rect::new(
            rect.min[0],
            top,
            rect.width(),
            styles
                .metric("effect.curve.graph_height", 320.)
                .min(rect.width())
                .min((rect.max[1] - top).max(styles.metric("border", 1.))),
        );
        #[cfg(test)]
        {
            self.test_graph = Some(graph);
        }
        let _ = observe!(
            "effects.curve_geometry",
            (graph.width().is_finite()
                && graph.width() > 0.
                && graph.height().is_finite()
                && graph.height() > 0.)
                .then_some(())
                .ok_or("invalid graph extent before coordinate division"),
            "width={} height={} phase={}",
            graph.width(),
            graph.height(),
            if self.drag.is_some() {
                "dragging"
            } else {
                "idle"
            }
        );
        let gx = |x: f32| ((x - graph.min[0]) / graph.width()).clamp(0., 1.);
        let gy = |y: f32| ((graph.max[1] - y) / graph.height()).clamp(0., 1.);
        let px = |x: f32| graph.min[0] + x * graph.width();
        let py = |y: f32| graph.max[1] - y * graph.height();

        if self.drag.is_some() {
            let _ = observe!(
                "effects.curve_release",
                (input.dragging() || input.released.is_some_and(|(b, _)| b == MouseButton::Left))
                    .then_some(())
                    .ok_or("curve gesture retained without held button or release"),
                "phase=dragging channel={}",
                self.channel
            );
        }
        let value = self
            .drag
            .as_ref()
            .map_or_else(|| current.clone(), |d| d.value.clone());
        let channel = self.channel;
        let samples = match observe!("effects.curve_samples", self.samples(&value),
            "phase=graph channel={channel}") {
            Ok(samples) => samples.to_vec(),
            Err(_) => {
                if self.cancel() { out.push(Action::History(4)); }
                return;
            }
        };

        // Background, grid, identity diagonal.
        paint::rect(
            list,
            graph,
            colors
                .SUNKEN
                .with_alpha(styles.metric("effect.curve.background_opacity", 0.5)),
        );
        for i in 1..4 {
            let f = i as f32 / 4.;
            paint::line(
                list,
                vec![[px(f), graph.min[1]], [px(f), graph.max[1]]],
                colors
                    .TEXT_MUTED
                    .with_alpha(styles.metric("effect.curve.grid_opacity", 0.25)),
            );
            paint::line(
                list,
                vec![[graph.min[0], py(f)], [graph.max[0], py(f)]],
                colors
                    .TEXT_MUTED
                    .with_alpha(styles.metric("effect.curve.grid_opacity", 0.25)),
            );
        }
        paint::line(
            list,
            vec![[graph.min[0], graph.max[1]], [graph.max[0], graph.min[1]]],
            colors
                .TEXT_MUTED
                .with_alpha(styles.metric("effect.curve.diagonal_opacity", 0.35)),
        );
        paint::line_with_width(
            list,
            samples.iter().map(|p| [px(p[0]), py(p[1])]).collect(),
            colors.ACCENT,
            styles.metric("effect.curve.stroke_width", 1.5),
        );
        paint::stroke(
            list,
            graph,
            colors
                .TEXT_MUTED
                .with_alpha(styles.metric("effect.curve.background_opacity", 0.5)),
            styles.metric("border", 1.),
        );

        // Control points.
        let points = &value.channels[self.channel];
        let hovered = points.iter().position(|p| {
            let dx = px(p[0]) - input.mouse[0];
            let dy = py(p[1]) - input.mouse[1];
            dx * dx + dy * dy <= styles.metric("effect.curve.hit_radius", 7.).powi(2)
        });
        for (i, p) in points.iter().enumerate() {
            let center = [px(p[0]), py(p[1])];
            let active = self.drag.as_ref().is_some_and(|d| d.point == i);
            paint::diamond(
                list,
                center,
                if active || hovered == Some(i) {
                    styles.metric("graph.key_radius", 5.)
                } else {
                    styles.metric("graph.handle_radius", 4.)
                },
                if active {
                    colors.ACCENT
                } else if hovered == Some(i) && enabled {
                    colors.TEXT
                } else {
                    colors.TEXT_MUTED
                },
            );
        }

        list.append(overlay);
        if !enabled
            || input.focus_lost
            || input
                .keys
                .iter()
                .any(|(key, _)| *key == motion_ui::Key::Escape)
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
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
            let next = [gx(input.mouse[0]).clamp(min_x, max_x), gy(input.mouse[1])];
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
                format!(
                    "{} ({:.3}, {:.3})",
                    CHANNELS[self.channel], next[0], next[1]
                ),
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
    let upper = track.keys.partition_point(|k| f64::from(k.frame) <= local);
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
        let rect = Rect::new(0., 0., 300., height(&PaintList::default().styles(), 300.));
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
                rect,
                None,
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
        assert!(matches!(
            actions.as_slice(),
            [Action::History(2), Action::Edit(Command::Effect { .. })]
        ));
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
        let graph = editor.test_graph.unwrap();
        assert!(
            (points[1][1].as_f64().unwrap() - f64::from((graph.max[1] - 80.) / graph.height()))
                .abs()
                < 0.01
        );
        // One undo restores the whole gesture.
        engine.history(0).unwrap();
        assert_eq!(engine.state().unwrap()["project"], before);
    }
    #[test]
    fn channel_dropdown_is_observational_and_escape_cancels_an_unmoved_gesture() {
        let mut editor = Editor::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let rect = Rect::new(0., 0., 220., height(&PaintList::default().styles(), 220.));
        let mut draw = |editor: &mut Editor, input: &Input| {
            let mut actions = vec![];
            editor.draw(
                &mut PaintList::default(),
                &mut atlas,
                rect,
                rect,
                None,
                input,
                true,
                &CurveObject::default(),
                |value| Command::Effect {
                    object: 7,
                    action: EffectAction::SetCurveObject {
                        effect: 1,
                        param: "curve".into(),
                        frame: 0,
                        value,
                    },
                },
                &mut actions,
                &mut None,
            );
            actions
        };
        assert!(draw(&mut editor, &input(10., 10., true, false)).is_empty());
        assert!(editor.modal());
        assert!(draw(
            &mut editor,
            &Input {
                keys: vec![(motion_ui::Key::Down, Default::default())],
                ..Default::default()
            }
        )
        .is_empty());
        assert!(draw(
            &mut editor,
            &Input {
                keys: vec![(motion_ui::Key::Enter, Default::default())],
                ..Default::default()
            }
        )
        .is_empty());
        assert!(!editor.modal());
        assert_eq!(editor.channel, 1);
        let graph = editor.test_graph.unwrap();
        assert_eq!(graph.width(), graph.height());
        assert!(matches!(
            draw(&mut editor, &input(graph.min[0] + 0.001, graph.max[1] - 0.001, true, false)).as_slice(),
            [Action::History(2)]
        ));
        let actions = draw(
            &mut editor,
            &Input {
                keys: vec![(motion_ui::Key::Escape, Default::default())],
                ..Default::default()
            },
        );
        assert!(matches!(actions.as_slice(), [Action::History(4)]));
        assert!(!editor.dragging());
    }
    #[test]
    fn right_click_removes_interior_points_only() {
        let mut editor = Editor::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let rect = Rect::new(0., 0., 300., height(&PaintList::default().styles(), 300.));
        let mut current = CurveObject::default();
        current.channels[0] = vec![[0., 0.], [0.5, 0.5], [1., 1.]];
        let mut actions = vec![];
        let middle_y = 26. + rect.width().min(320.) * 0.5;
        // Right-click the actual middle point of the rendered graph.
        editor.draw(
            &mut PaintList::default(),
            &mut atlas,
            rect,
            rect,
            None,
            &Input {
                mouse: [150., middle_y],
                pressed: Some((MouseButton::Right, [150., middle_y])),
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
            rect,
            None,
            &Input {
                mouse: [0., 26. + rect.width().min(320.)],
                pressed: Some((MouseButton::Right, [0., 26. + rect.width().min(320.)])),
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
