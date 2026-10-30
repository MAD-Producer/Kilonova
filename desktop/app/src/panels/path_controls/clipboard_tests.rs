use super::*;
use motion_ui::text_clipboard::TextClipboard;
use std::sync::Arc;

fn key(cache: &Arc<TextClipboard>, key: Key, control: bool) -> Input {
    Input {
        text_clipboard: Some(cache.clone()),
        keys: vec![(
            key,
            motion_ui::input::Modifiers {
                control,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
#[test]
fn mask_numeric_clipboard_preserves_draft_history_range_validation_save_and_single_undo() {
    let mut project = motion_core::Project::new(200, 100, 30, 90).unwrap();
    let mut layer = Layer::solid(7, "Mask", [100., 100.], [0.; 3], [1.; 4]);
    let path = VectorPath {
        id: 1,
        closed: true,
        nodes: [[0., 0.], [100., 0.], [0., 100.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut mask = LayerMask::new(1, path);
    mask.opacity = Track::constant(50.);
    layer.masks.push(mask);
    project.layers.push(layer);
    let mut engine = motion_core::Engine::new(project).unwrap();
    let before = engine.project().clone();
    let mut controls = Controls::default();
    controls.parameter = Some(Parameter {
        host: motion_ui::dock::Panel::Masks,
        object: 7,
        mask: 1,
        property: MaskProperty::Opacity,
        axis: 0,
        values: vec![50.],
        text: "50".into(),
        replace: true,
        editor: Default::default(),
        rect: Rect::new(90., 0., 120., 30.),
        start: None,
        begun: false,
        last_value: 50.,
        linked: false,
        range: [0., 100.],
    });
    let state = serde_json::json!({"frame":0});
    let cache = Arc::new(TextClipboard::default());
    let mut actions = vec![];
    controls.parameter_input(&state, &key(&cache, Key::X, true), &mut actions, 3.);
    assert_eq!(controls.parameter.as_ref().unwrap().text, "");
    assert_eq!(&*cache.read().unwrap().unwrap(), "50");
    controls.parameter_input(&state, &key(&cache, Key::Z, true), &mut actions, 3.);
    assert_eq!(controls.parameter.as_ref().unwrap().text, "50");
    cache.store("150").unwrap();
    controls.parameter_input(&state, &key(&cache, Key::V, true), &mut actions, 3.);
    assert_eq!(controls.parameter.as_ref().unwrap().text, "150");
    controls.parameter_input(&state, &key(&cache, Key::Z, true), &mut actions, 3.);
    assert_eq!(controls.parameter.as_ref().unwrap().text, "50");
    controls.parameter_input(&state, &key(&cache, Key::Y, true), &mut actions, 3.);
    assert!(actions.is_empty());
    controls.parameter_input(&state, &key(&cache, Key::Enter, false), &mut actions, 3.);
    assert!(controls.parameter.is_none());
    assert!(
        matches!(actions.as_slice(), [Action::Edit(Command::Mask {object:7, action:MaskAction::Set {mask:1,value,..}})] if value == &[100.])
    );
    let Action::Edit(command) = actions.pop().unwrap() else {
        unreachable!()
    };
    engine.apply(command).unwrap();
    assert_eq!(
        engine.project().layers[0].masks[0]
            .opacity
            .sample(0.)
            .unwrap(),
        100.
    );
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(
        &motion_core::storage::load(directory.path()).unwrap(),
        engine.project()
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
