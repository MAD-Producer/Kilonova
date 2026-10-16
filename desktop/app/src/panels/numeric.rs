//! One numeric gesture contract for property providers and effect parameters.
use super::Action;
use motion_core::Command;
use motion_ui::{
    paint::{self, PaintList},
    text::TextAtlas,
    ui::Align,
    widgets::{NumberEditor, NumberEvent, NumberOptions},
    Input, Rect,
};
#[derive(Default)]
pub struct Field {
    pub editor: NumberEditor,
}
impl Field {
    pub fn readout(&self) -> Option<String> {
        self.editor
            .drag_values()
            .map(|(start, value)| format!("{value:.3}  ({:+.3})", value - start))
    }
    pub fn cancel(&mut self) -> bool {
        !self.editor.cancel().is_empty()
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        value: f64,
        options: NumberOptions,
        input: &Input,
    ) -> Vec<NumberEvent> {
        let styles = list.styles();
        let colors = styles.colors();

        self.editor.drag_threshold = Some(styles.metric("motion.drag_threshold", 3.));
        let events = self.editor.update(value, options, input, rect);
        self.editor.draw(list, atlas, rect, value, options);
        if self.editor.dragging() {
            paint::stroke(list, rect, colors.ACCENT, 1.);
        }
        let shown = self.editor.display_value(value);
        if options.min.is_finite() && options.max.is_finite() && options.max > options.min {
            let fraction =
                ((shown - options.min) / (options.max - options.min)).clamp(0., 1.) as f32;
            paint::rect(
                list,
                Rect::new(rect.min[0], rect.max[1] - 1., rect.width() * fraction, 1.),
                colors.ACCENT.with_alpha(0.45),
            );
        }
        events
    }
}
pub fn passive(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    rect: Rect,
    value: f64,
    input: &Input,
    enabled: bool,
) {
    passive_with_precision(list, atlas, rect, value, input, enabled, 3);
}
pub fn passive_with_precision(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    rect: Rect,
    value: f64,
    input: &Input,
    enabled: bool,
    decimals: usize,
) {
    let styles = list.styles();

    use motion_ui::style::{box_style, text_style, State, States};
    let mut states = States::default();
    if enabled && rect.contains(input.mouse) {
        states = states.with(State::Hovered);
    }
    if !enabled {
        states = states.with(State::Disabled);
    }
    let style = styles.as_ref().resolve("field.number", states).unwrap();
    box_style(list, rect, style);
    text_style(
        list,
        atlas,
        rect.shrink(style.padding),
        &format!("{:.*}", decimals.min(8), value),
        style,
        Align::Left,
    );
}
/// Return true when the provider may release the field. The command adapter
/// only handles values; this helper owns identical transaction boundaries.
pub fn apply(
    events: Vec<NumberEvent>,
    out: &mut Vec<Action>,
    mut command: impl FnMut(f32) -> Command,
) -> bool {
    let mut finished = false;
    for event in events {
        match event {
            NumberEvent::Begin { .. } => out.push(Action::History(2)),
            NumberEvent::Change { value } => out.push(Action::Edit(command(value as f32))),
            NumberEvent::Commit { .. } => {
                out.push(Action::History(3));
                finished = true;
            }
            NumberEvent::Cancel { .. } => {
                out.push(Action::History(4));
                finished = true;
            }
        }
    }
    finished
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::Engine,
        i18n::Locale,
        panels::camera_controls,
    };
    use motion_ui::MouseButton;

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
                _ => panic!("unexpected numeric action"),
            }
        }
    }
    fn input(x: f32, y: f32, pressed: bool, released: bool) -> Input {
        Input {
            mouse: [x, y],
            held: (!released).then_some(MouseButton::Left),
            pressed: pressed.then_some((MouseButton::Left, [x, y])),
            released: released.then_some((MouseButton::Left, [x, y])),
            ..Default::default()
        }
    }
    #[test]
    fn effect_parameter_scrub_uses_real_catalogue_and_one_undo() {
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
                        "Numeric",
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
                    .any(|e| e["id"] == "brightness_contrast")
            })
            .unwrap();
        engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"brightness_contrast"}).to_string()).unwrap();
        let before = engine.state().unwrap();
        let mut panels = crate::panels::Panels::default();
        panels.selected = Some(7);
        panels.effect_controls.catalogue = catalogue.into();
        let id = motion_ui::dock::Panel::EffectControls.dock();
        let layout = motion_ui::dock::Node::Dock { id, tabs: vec![id], active: 0 };
        let top = crate::panels::frames(&layout, [400., 1000.])[0].content.min[1] + 8.;
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut draw = |panels: &mut crate::panels::Panels, input: &Input| {
            let state = engine.state().unwrap();
            crate::panels::chrome(&mut PaintList::default(), &mut atlas, [400., 1000.],
                &layout, panels, &state, input)
        };
        assert!(draw(&mut panels, &input(100., top + 177., true, false)).is_empty());
        assert!(draw(
            &mut panels,
            &Input {
                mouse: [120., top + 177.],
                ..Default::default()
            }
        )
        .is_empty());
        run(
            &engine,
            draw(&mut panels, &input(120., top + 177., false, false)),
        );
        assert!(!engine.state().unwrap()["canUndo"].as_bool().unwrap());
        assert!(panels.effect_controls.drag_readout.is_some());
        run(
            &engine,
            draw(&mut panels, &input(130., top + 177., false, false)),
        );
        run(
            &engine,
            draw(&mut panels, &input(130., top + 177., false, true)),
        );
        let after = engine.state().unwrap();
        assert_ne!(after["project"], before["project"]);
        for (key, param) in before["project"]["layers"][0]["effects"][0]["params"]
            .as_object()
            .unwrap()
        {
            if key != "p0001" {
                assert_eq!(
                    &after["project"]["layers"][0]["effects"][0]["params"][key],
                    param
                );
            }
        }
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    }
    #[test]
    fn camera_scrub_preserves_other_axes_and_undo() {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine
            .edit(serde_json::to_string(&Command::CreateCamera).unwrap())
            .unwrap();
        let mut controls = camera_controls::Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut draw = |controls: &mut camera_controls::Controls, input: &Input| {
            let state = engine.state().unwrap();
            let mut actions = vec![];
            let mut y = 0.;
            controls.render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 400., 1000.),
                &mut y,
                &state,
                input,
                Locale::En,
                &mut actions,
            );
            actions
        };
        assert!(draw(&mut controls, &input(100., 65., true, false)).is_empty());
        run(
            &engine,
            draw(&mut controls, &input(120., 65., false, false)),
        );
        assert!(controls.drag_readout.is_some());
        run(&engine, draw(&mut controls, &input(130., 65., false, true)));
        let after = engine.state().unwrap();
        let a = &after["sampledCamera"]["position"];
        let b = &before["sampledCamera"]["position"];
        assert_ne!(a[0], b[0]);
        assert_eq!(a[1], b[1]);
        assert_eq!(a[2], b[2]);
        assert_eq!(
            after["project"]["camera"]["target"],
            before["project"]["camera"]["target"]
        );
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    }
}
