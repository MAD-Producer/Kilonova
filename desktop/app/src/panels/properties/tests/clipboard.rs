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
fn numeric_clipboard_drafts_commit_one_typed_edit_and_undo_once() {
    let mut panels = panels();
    let state = state();
    let mut actions = vec![];
    let cache = Arc::new(TextClipboard::default());
    process_field(&mut panels, &state, &release(100.), &mut actions);
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::X, true),
        &mut actions,
    );
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "");
    assert_eq!(&*cache.read().unwrap().unwrap(), "10.00");
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::Z, true),
        &mut actions,
    );
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "10.00");
    cache.store("64").unwrap();
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::V, true),
        &mut actions,
    );
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "64");
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::Z, true),
        &mut actions,
    );
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "10.00");
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::Y, true),
        &mut actions,
    );
    assert!(actions.is_empty());
    process_field(
        &mut panels,
        &state,
        &key(&cache, Key::Enter, false),
        &mut actions,
    );
    assert!(
        matches!(actions.as_slice(), [Action::Edit(Command::SetComponent {
        object:7, property:Property::Position, axis:motion_core::Axis::X, frame:21, value
    })] if *value == 64.)
    );
    let mut project = motion_core::Project::new(200, 100, 30, 90).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7,
        "Layer",
        [100., 100.],
        [10., 20., 0.],
        [1.; 4],
    ));
    let mut engine = motion_core::Engine::new(project).unwrap();
    engine
        .apply(Command::SeparateDimensions {
            object: 7,
            property: Property::Position,
        })
        .unwrap();
    let before = engine.project().clone();
    let Action::Edit(command) = actions.pop().unwrap() else {
        unreachable!()
    };
    engine.apply(command).unwrap();
    assert_ne!(engine.project(), &before);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(
        &motion_core::storage::load(directory.path()).unwrap(),
        engine.project()
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn numeric_clipboard_failure_and_ime_confirmation_do_not_commit_or_erase_the_field() {
    let mut panels = panels();
    let state = state();
    let mut actions = vec![];
    let cache = Arc::new(TextClipboard::default());
    process_field(&mut panels, &state, &release(100.), &mut actions);
    let mut missing = key(&cache, Key::X, true);
    missing.text_clipboard = None;
    process_field(&mut panels, &state, &missing, &mut actions);
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "10.00");
    assert!(panels
        .properties
        .field
        .as_ref()
        .unwrap()
        .editor
        .error
        .is_some());
    let mut ime = key(&cache, Key::Enter, false);
    ime.ime_consumed = true;
    ime.text = "32".into();
    process_field(&mut panels, &state, &ime, &mut actions);
    assert_eq!(panels.properties.field.as_ref().unwrap().text, "32");
    let mut escape = key(&cache, Key::Escape, false);
    escape.composing = true;
    process_field(&mut panels, &state, &escape, &mut actions);
    assert!(panels.properties.field.is_some() && actions.is_empty());
    process_field(
        &mut panels,
        &state,
        &Input {
            focus_lost: true,
            ..Default::default()
        },
        &mut actions,
    );
    assert!(panels.properties.field.is_none() && actions.is_empty());
}
