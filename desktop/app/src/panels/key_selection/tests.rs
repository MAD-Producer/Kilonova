use super::*;
use motion_core::{
    Axis, Command, Ease, Engine, Keyframe, KeyframeClipboard, Layer, LayerTimeline, Project,
    Property,
};
use serde_json::json;
#[path = "source_tests.rs"]
mod sources;
#[path = "ui_tests.rs"]
mod ui;

fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    for (id, offset) in [(7, 7), (9, 40)] {
        let mut layer = Layer::solid(id, "Keys", [64.; 2], [0.; 3], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: offset,
            out_frame: 110,
            offset_frame: offset,
        });
        layer.transform.position.keys = [3, 13]
            .into_iter()
            .map(|frame| Keyframe {
                frame,
                value: [frame as f32, 20., 0.],
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            })
            .collect();
        project.layers.push(layer);
    }
    project.layers[1].transform.position.separate().unwrap();
    Engine::new(project).unwrap()
}
fn state(engine: &Engine) -> Value {
    json!({"root":"key-selection-fixture", "composition":motion_core::MAIN_COMPOSITION,
        "document_generation":7, "revision":engine.revision(), "project":engine.project()})
}
fn key(object: u64, axis: Option<Axis>, frame: u32) -> Key {
    Key {
        target: KeyframeTarget::Property {
            object,
            property: Property::Position,
            axis,
        },
        frame,
    }
}
fn present(engine: &Engine, target: &KeyframeTarget, frame: u32) -> bool {
    KeyframeClipboard::capture(
        engine.project(),
        &[KeyframeSelection {
            target: target.clone(),
            frames: vec![frame],
        }],
    )
    .is_ok()
}
fn validate(set: &mut Set, engine: &Engine) -> Status {
    set.validate(&state(engine), |target, frame| {
        present(engine, target, frame)
    })
}

#[test]
fn selection_addresses_cross_layers_separated_axes_and_signed_local_offsets() {
    let engine = fixture();
    let keys = [
        key(7, None, 20),
        key(9, Some(Axis::X), 43),
        key(7, None, 10),
        key(9, Some(Axis::Y), 43),
    ];
    let mut set = Set::default();
    set.apply(&state(&engine), keys.clone(), Mode::Replace)
        .unwrap();
    assert_eq!(set.status(&state(&engine)), Status::Current);
    assert_eq!(set.primary(), keys.last());
    assert_eq!(validate(&mut set, &engine), Status::Current);
    assert_eq!(
        set.selections()
            .iter()
            .map(|s| s.frames.len())
            .sum::<usize>(),
        4
    );
    for key in &keys {
        assert!(set.contains(key));
    }
    let frozen = KeyframeClipboard::capture(engine.project(), &set.selections()).unwrap();
    assert_eq!(frozen.tracks.len(), 3);
    assert!(set.selections().iter().any(|s| s.frames == [10, 20]));
    // The parent of a separated track is deliberately not an alias for its axes.
    set.apply(&state(&engine), [key(9, None, 43)], Mode::Add)
        .unwrap();
    assert_eq!(validate(&mut set, &engine), Status::Stale);
    assert!(set.owns_keys());
}

#[test]
fn add_toggle_and_empty_replace_preserve_explicit_key_ownership() {
    let engine = fixture();
    let state = state(&engine);
    let a = key(7, None, 10);
    let b = key(7, None, 20);
    let c = key(9, Some(Axis::X), 43);
    let mut set = Set::default();
    assert_eq!(validate(&mut set, &engine), Status::Unowned);
    set.apply(&state, [a.clone(), a.clone()], Mode::Replace)
        .unwrap();
    set.apply(&state, [b.clone(), c.clone()], Mode::Add)
        .unwrap();
    assert_eq!(set.primary(), Some(&c));
    set.apply(&state, [b.clone(), b.clone()], Mode::Toggle)
        .unwrap();
    assert!(
        !set.contains(&b),
        "duplicate hits in one gesture toggle once"
    );
    assert!(set.contains(&a) && set.contains(&c));
    set.apply(&state, [c.clone()], Mode::Toggle).unwrap();
    assert_eq!(set.primary(), Some(&a));
    set.apply(&state, [], Mode::Replace).unwrap();
    assert!(set.primary().is_none());
    assert!(
        set.owns_keys(),
        "empty marquee must not route Delete to layers"
    );
    assert_eq!(validate(&mut set, &engine), Status::Empty);
    set.apply(&state, [a], Mode::Add).unwrap();
    set.clear(&state);
    assert!(set.owns_keys());
    assert_eq!(validate(&mut set, &engine), Status::Empty);
    set.release();
    assert!(!set.owns_keys());
    assert_eq!(validate(&mut set, &engine), Status::Unowned);
}

#[test]
fn authored_delete_and_undo_invalidate_and_restore_keys_without_layer_fallback() {
    let mut engine = fixture();
    let before = engine.snapshot();
    let mut set = Set::default();
    set.apply(&state(&engine), [key(7, None, 10)], Mode::Replace)
        .unwrap();
    assert_eq!(validate(&mut set, &engine), Status::Current);
    engine
        .apply(Command::DeleteKey {
            object: 7,
            property: Property::Position,
            axis: None,
            frame: 10,
        })
        .unwrap();
    assert_eq!(validate(&mut set, &engine), Status::Stale);
    assert!(set.owns_keys());
    assert_eq!(set.primary(), Some(&key(7, None, 10)));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert_eq!(validate(&mut set, &engine), Status::Current);
    engine
        .apply(Command::MoveLayerClip {
            object: 7,
            in_frame: 8,
        })
        .unwrap();
    assert_eq!(
        validate(&mut set, &engine),
        Status::Stale,
        "composition key identities include the offset"
    );
    engine.undo().unwrap();
    assert_eq!(validate(&mut set, &engine), Status::Current);
}

#[test]
fn revision_cache_is_document_scoped_and_missing_revision_never_caches() {
    let engine = fixture();
    let mut state = state(&engine);
    let mut set = Set::default();
    set.apply(&state, [key(7, None, 10)], Mode::Replace)
        .unwrap();
    let mut calls = 0;
    assert_eq!(
        set.validate(&state, |t, f| {
            calls += 1;
            present(&engine, t, f)
        }),
        Status::Current
    );
    state["frame"] = json!(50);
    assert_eq!(
        set.validate(&state, |_, _| panic!(
            "view changes must reuse the authoritative revision"
        )),
        Status::Current
    );
    assert_eq!(calls, 1);
    state["document_generation"] = json!(8);
    assert_eq!(
        set.validate(&state, |_, _| panic!(
            "old document must be rejected before lookup"
        )),
        Status::Stale
    );
    state["document_generation"] = json!(7);
    state["composition"] = json!("other");
    assert_eq!(set.validate(&state, |_, _| true), Status::Stale);
    state["composition"] = json!(motion_core::MAIN_COMPOSITION);
    state["root"] = json!("other");
    assert_eq!(set.validate(&state, |_, _| true), Status::Stale);
    state["root"] = json!("key-selection-fixture");
    state.as_object_mut().unwrap().remove("revision");
    for _ in 0..2 {
        assert_eq!(
            set.validate(&state, |t, f| {
                calls += 1;
                present(&engine, t, f)
            }),
            Status::Current
        );
    }
    assert_eq!(calls, 3);
}

#[test]
fn oversized_gesture_is_atomic_and_new_document_add_cannot_resurrect_old_keys() {
    let engine = fixture();
    let mut state = state(&engine);
    let mut set = Set::default();
    let a = key(7, None, 10);
    set.apply(&state, [a.clone()], Mode::Replace).unwrap();
    let before = set.selections();
    let huge = Key {
        target: KeyframeTarget::Effect {
            object: 7,
            effect: 1,
            param: "x".repeat(MAX_KEYFRAME_CLIPBOARD_BYTES),
        },
        frame: 20,
    };
    assert!(set
        .apply(&state, [key(7, None, 20), huge], Mode::Add)
        .unwrap_err()
        .to_string()
        .contains("metadata limit"));
    assert_eq!(set.selections(), before);
    assert_eq!(set.primary(), Some(&a));
    state["document_generation"] = json!(8);
    let b = key(9, Some(Axis::Y), 43);
    set.apply(&state, [b.clone()], Mode::Add).unwrap();
    assert!(!set.contains(&a));
    assert_eq!(set.primary(), Some(&b));
    assert_eq!(set.selections().len(), 1);
}

fn panels(engine: &Engine) -> super::super::Panels {
    let mut panels = super::super::Panels {
        focused: Some(motion_ui::dock::Panel::Timeline),
        keyframe_clipboard_available: true,
        clipboard_available: true,
        ..Default::default()
    };
    panels.select_only(Some(7));
    panels
        .selected_keys
        .apply(
            &state(engine),
            [key(7, None, 10), key(9, Some(Axis::Y), 43)],
            Mode::Replace,
        )
        .unwrap();
    panels
}
fn route(
    panels: &mut super::super::Panels,
    state: &Value,
    key: motion_ui::input::Key,
    modifiers: motion_ui::input::Modifiers,
    owner: Option<motion_ui::dock::Panel>,
) -> Vec<super::super::Action> {
    let mut out = vec![];
    let input = motion_ui::input::Input {
        keys: vec![(key, modifiers)],
        ..Default::default()
    };
    super::super::keyboard::route(&input, panels, state, false, owner, &mut out);
    out
}

#[test]
fn key_delete_and_ease_dispatch_the_entire_collection_with_guarded_document_identity() {
    use super::super::Action;
    use motion_ui::{
        dock::Panel,
        input::{Key, Modifiers},
    };
    for locale in [crate::i18n::Locale::Zh, crate::i18n::Locale::En] {
        for owner in [None, Some(Panel::Timeline)] {
            let engine = fixture();
            let state = state(&engine);
            let mut panels = panels(&engine);
            panels.locale = locale;
            for key in [Key::Delete, Key::F9] {
                let actions = route(&mut panels, &state, key, Modifiers::default(), owner);
                let [Action::EditorAction { source, commands }] = actions.as_slice() else {
                    panic!("{actions:?}");
                };
                assert_eq!(source, &crate::layer_clipboard::KeyframeSource::of(&state));
                assert_eq!(commands.len(), 1);
                let selected = match &commands[0] {
                    Command::DeleteKeyframes { selections } => {
                        assert_eq!(key, Key::Delete);
                        selections
                    }
                    Command::EaseKeyframes {
                        selections,
                        incoming,
                        outgoing,
                    } => {
                        assert_eq!(key, Key::F9);
                        assert!(*incoming && *outgoing);
                        selections
                    }
                    other => panic!("unexpected command: {other:?}"),
                };
                assert_eq!(selected.len(), 2);
                let mut edited = Engine::new(engine.snapshot()).unwrap();
                edited.apply_batch(commands.clone()).unwrap();
                assert_eq!(edited.project().layers.len(), engine.project().layers.len());
                edited.undo().unwrap();
                assert_eq!(edited.project(), engine.project());
                assert!(!edited.can_undo());
            }
        }
    }
}

#[test]
fn empty_stale_and_read_only_collections_consume_delete_and_never_copy_or_cut_layers() {
    use super::super::Action;
    use motion_ui::{
        dock::Panel,
        input::{Key, Modifiers},
    };
    for scenario in [
        "empty",
        "stale",
        "locked",
        "generation",
        "composition",
        "missing-layer",
    ] {
        let mut engine = fixture();
        let mut panels = panels(&engine);
        let mut state = state(&engine);
        match scenario {
            "empty" => panels.selected_keys.clear(&state),
            "stale" => {
                engine
                    .apply(Command::DeleteKey {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                        frame: 10,
                    })
                    .unwrap();
                state = self::state(&engine);
            }
            "locked" => {
                state["project"]["layers"][1]["locked"] = json!(true);
                state["revision"] = json!(123);
            }
            "generation" => state["document_generation"] = json!(8),
            "composition" => state["composition"] = json!("comp-other"),
            "missing-layer" => {
                state["project"]["layers"].as_array_mut().unwrap().remove(1);
                state["revision"] = json!(123);
            }
            _ => unreachable!(),
        }
        for key in [Key::Delete, Key::F9, Key::Delete] {
            assert!(
                route(
                    &mut panels,
                    &state,
                    key,
                    Modifiers::default(),
                    Some(Panel::Timeline)
                )
                .is_empty(),
                "{scenario}"
            );
            assert!(
                panels.message.is_some(),
                "invalid key selection needs visible feedback: {scenario}"
            );
            super::super::timeline_shortcuts::refresh_key_notice(&mut panels, &state);
            assert!(panels.message.is_some(), "notice disappeared before recovery: {scenario}");
            assert!(panels.selected_keys.owns_keys());
        }
        for key in [Key::C, Key::X] {
            let actions = route(
                &mut panels,
                &state,
                key,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                Some(Panel::Timeline),
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::LayerClipboard(_))),
                "{scenario}: {actions:?}"
            );
        }
    }
}

#[test]
fn primary_property_focus_arbitration_preserves_same_gesture_and_releases_explicit_navigation() {
    let engine = fixture();
    let state = state(&engine);
    let mut panels = panels(&engine);
    panels
        .animation_controls
        .select_key_in(&state, 7, Property::Position, None, 10);
    super::super::property_focus::settle(&mut panels);
    assert_eq!(
        panels.selected_keys.selections().len(),
        2,
        "primary controller projection cannot erase the collection in the same gesture"
    );
    panels
        .animation_controls
        .choose_track_in(&state, 7, Property::Opacity, None);
    super::super::property_focus::settle(&mut panels);
    assert!(
        !panels.selected_keys.owns_keys(),
        "explicit later property navigation gives ownership back to that property"
    );
    panels
        .selected_keys
        .apply(&state, [key(7, None, 10)], Mode::Replace)
        .unwrap();
    panels.select_only(Some(9));
    assert!(!panels.selected_keys.owns_keys());
}
