use super::*;
use motion_core::{Layer, Project};

fn pointer(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        held: (!released).then_some(MouseButton::Left),
        ..Default::default()
    }
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            _ => panic!("unexpected anchor action"),
        }
    }
}
#[test]
fn anchor_rows_keep_one_gesture_when_other_layers_render_and_cancel_when_hidden() {
    for host in [Panel::Properties, Panel::Timeline] {
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        project
            .layers
            .push(Layer::solid(7, "A", [100.; 2], [0.; 3], [1.; 4]));
        project
            .layers
            .push(Layer::solid(8, "B", [100.; 2], [0.; 3], [1.; 4]));
        let mut engine = motion_core::Engine::new(project).unwrap();
        let before = engine.project().clone();
        let mut controls = AnchorControls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 400., 600.);
        let mut paint = PaintList::default();
        let styles = paint.styles();
        let rect = if host == Panel::Timeline {
            super::super::timeline::property_layout(&styles, area, 100., 22., 0, 2).number
        } else {
            inspector_layout(&styles, area, 100., 0, 2).number
        };
        let point = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        let mut actions = vec![];
        let layer = serde_json::to_value(&engine.project().layers[0]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            100.,
            host,
            &layer,
            &pointer(point, true, false),
            false,
            &mut actions,
        );
        assert!(actions.is_empty());
        let other = serde_json::to_value(&engine.project().layers[1]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            150.,
            host,
            &other,
            &Input::default(),
            false,
            &mut actions,
        );
        assert!(controls.editing() && actions.is_empty());
        for (dx, released) in [(20., false), (30., true)] {
            let state = serde_json::json!({"project":engine.project()});
            controls.process(
                &styles,
                &state,
                &pointer([point[0] + dx, point[1]], false, released),
                &mut actions,
            );
            apply(&mut engine, std::mem::take(&mut actions));
        }
        assert!(!controls.editing());
        assert!((engine.project().layers[0].transform.anchor[0] - 0.65).abs() < 0.001);
        assert_eq!(
            engine.project().layers[0].transform.anchor[1],
            before.layers[0].transform.anchor[1]
        );
        assert_eq!(engine.project().layers[1], before.layers[1]);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        let layer = serde_json::to_value(&engine.project().layers[0]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            100.,
            host,
            &layer,
            &pointer(point, true, false),
            false,
            &mut actions,
        );
        controls.process(
            &styles,
            &serde_json::json!({"project":engine.project()}),
            &pointer([point[0] + 20., point[1]], false, false),
            &mut actions,
        );
        apply(&mut engine, std::mem::take(&mut actions));
        // No draw call: a folded or clipped row must still receive cancellation.
        controls.process(
            &styles,
            &serde_json::json!({"project":engine.project()}),
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            &mut actions,
        );
        apply(&mut engine, actions);
        assert!(!controls.editing());
        assert_eq!(engine.project(), &before);
        engine.begin_gesture().unwrap();
        engine.end_gesture(false).unwrap();
    }
}

#[test]
fn embedded_effect_dropdown_blocks_group_folding_in_main_and_floating_properties() {
    let folder = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine
        .edit(
            serde_json::to_string(&Command::Add {
                layer: Layer::solid(7, "Tree", [100.; 2], [0.; 3], [1.; 4]),
            })
            .unwrap(),
        )
        .unwrap();
    let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| {
            package["manifest"]["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|effect| effect["id"] == "drop_shadow")
        })
        .unwrap();
    engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"drop_shadow"}).to_string()).unwrap();
    let before = engine.state().unwrap();
    let effect = before["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    for floating in [false, true] {
        let mut panels = Panels::default();
        panels.selected = Some(7);
        panels.properties.transform_collapsed = true;
        panels.properties.effects_collapsed = false;
        panels.effect_controls.catalogue = catalogue.clone().into();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let layout = Node::dock(Panel::Properties.dock());
        let mut draw = |panels: &mut Panels, input: &Input| {
            let state = engine.state().unwrap();
            if floating {
                super::super::floating_chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    Panel::Properties,
                    [360., 560.],
                    panels,
                    &state,
                    input,
                )
            } else {
                super::super::chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    [360., 660.],
                    &layout,
                    panels,
                    &state,
                    input,
                )
            }
        };
        assert!(draw(&mut panels, &Input::default()).is_empty());
        let menu = panels.properties.effects.test_rects[&(7, effect, "mode".into(), -4)];
        assert!(draw(
            &mut panels,
            &pointer(
                [menu.min[0] + 2., menu.min[1] + menu.height() * 0.5],
                true,
                false
            )
        )
        .is_empty());
        assert!(panels.properties.effects.editing());
        let top = if floating {
            30.
        } else {
            frames(&layout, [360., 660.])[0].content.min[1]
        };
        assert!(draw(
            &mut panels,
            &pointer([60., top + 8. + 22. + 11.], true, false)
        )
        .is_empty());
        assert!(!panels.properties.effects.editing());
        assert!(panels.properties.transform_collapsed);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
}
