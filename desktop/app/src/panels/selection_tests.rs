use super::*;
use motion_ui::input::{Key, Modifiers, MouseButton};

fn state() -> Value {
    serde_json::json!({"frame":0,"project":{"composition_id":"main","frames":120,"layers":[
        {"id":1,"locked":false,"content":{"kind":"solid"}},
        {"id":2,"locked":true,"content":{"kind":"solid"}},
        {"id":3,"locked":false,"content":{"kind":"solid"}}
    ]}})
}
fn chord(key: Key, control: bool, shift: bool) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control,
                shift,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}

#[test]
fn selection_is_valid_after_deletion_undo_and_composition_switch_with_reused_ids() {
    let mut panels = Panels::default();
    let mut value = state();
    panels.select_only(Some(1));
    panels.add_selection(3);
    panels.normalize_selection(&value);
    value["project"]["layers"].as_array_mut().unwrap().pop();
    panels.normalize_selection(&value);
    assert_eq!(panels.selected, Some(1));
    assert_eq!(panels.selection_ids(), vec![1]);
    panels.normalize_selection(&state());
    assert_eq!(panels.selection_ids(), vec![1]);
    value["project"]["composition_id"] = serde_json::json!("another");
    panels.normalize_selection(&value);
    assert_eq!(panels.selected, None);
    assert!(panels.selection_ids().is_empty());
    panels.selected = Some(1);
    panels.toggle_selection(1);
    assert!(panels.selection_ids().is_empty());
    panels.select_only(Some(0));
    assert!(panels.selection_ids().is_empty());
}

#[test]
fn all_effect_entry_points_share_unlocked_targets_and_preserve_explicit_other_targets() {
    let mut panels = Panels::default();
    panels.select_only(Some(1));
    panels.add_selection(2);
    panels.add_selection(3);
    let request = serde_json::json!({"op":"add","object":3,"effect":"blur"});
    let request = panels.effect_request(request, &state());
    assert_eq!(request["objects"], serde_json::json!([3, 1]));
    assert!(request.get("object").is_none());
    assert_eq!(panels.selected, Some(3));
    let request = serde_json::json!({"op":"add","object":99});
    assert_eq!(panels.effect_request(request.clone(), &state()), request);
    let request = serde_json::json!({"op":"upgrade","object":3});
    assert_eq!(panels.effect_request(request.clone(), &state()), request);
}

#[test]
fn select_all_clear_and_batch_edits_use_same_router_in_main_and_floating_timeline() {
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Timeline)] {
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(3));
            let mut actions = vec![];
            keyboard::route(
                &chord(Key::A, true, false),
                &mut panels,
                &state(),
                false,
                owner,
                &mut actions,
            );
            assert_eq!(panels.selection_ids(), vec![3, 1, 2]);
            assert_eq!(panels.selected, Some(3));
            keyboard::route(
                &chord(Key::D, true, false),
                &mut panels,
                &state(),
                false,
                owner,
                &mut actions,
            );
            assert!(
                matches!(actions.as_slice(),[Action::EditBatch(commands)] if matches!(commands.as_slice(),[Command::Duplicate {object:3},Command::Duplicate {object:1}]))
            );
            keyboard::route(
                &chord(Key::A, true, true),
                &mut panels,
                &state(),
                false,
                owner,
                &mut actions,
            );
            assert!(panels.selection_ids().is_empty());
        }
    }
}

#[test]
fn text_ime_node_curve_and_key_focus_do_not_duplicate_layers() {
    for mode in ["text", "ime", "nodes", "curves", "key", "path"] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.select_only(Some(1));
        panels.add_selection(3);
        let mut input = chord(Key::D, true, false);
        match mode {
            "text" => panels.timeline.timeline_search = true,
            "ime" => input.composing = true,
            "curves" => panels.focused = Some(Panel::Curves),
            "key" => panels
                .animation_controls
                .select_key(3, Property::Position, None, 0),
            "path" => {
                panels.path_controls.tool = path_controls::Tool::Pen;
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                panels.path_controls.draw(
                    &mut PaintList::default(),
                    &mut atlas,
                    Rect::new(0., 0., 100., 100.),
                    &state(),
                    Some(3),
                    &Input {
                        pressed: Some((MouseButton::Middle, [10., 10.])),
                        ..Default::default()
                    },
                    Locale::Zh,
                    &mut vec![],
                );
            }
            _ => {}
        }
        let mut actions = vec![];
        keyboard::route(
            &input,
            &mut panels,
            &state(),
            mode == "nodes",
            None,
            &mut actions,
        );
        assert!(actions.is_empty(), "{mode}");
        if mode == "key" {
            keyboard::route(
                &chord(Key::Delete, false, false),
                &mut panels,
                &state(),
                false,
                None,
                &mut actions,
            );
            assert!(
                actions.is_empty(),
                "stale key must not fall through to layer deletion"
            );
        }
    }
}

#[test]
fn group_drag_has_shared_bounds_and_cancel_releases_the_whole_gesture() {
    let mut panels = Panels::default();
    panels.select_only(Some(1));
    panels.add_selection(3);
    let moving = types::MovingLayers {
        start_mouse: 100.,
        initial: 10,
        last: 10,
        layers: vec![(1, 10, 30), (3, 40, 60)],
    };
    assert_eq!(
        moving.clamp_delta(i64::MAX),
        motion_core::MAX_FRAMES as i32 - 100
    );
    assert_eq!(
        moving.clamp_delta(i64::MIN),
        -(motion_core::MAX_FRAMES as i32) - 10
    );
    panels.timeline.moving = Some(moving);
    assert!(matches!(
        cancel_interaction(&mut panels).as_slice(),
        [Action::History(4)]
    ));
    assert!(panels.timeline.moving.is_none());
    assert_eq!(panels.selection_ids(), vec![3, 1]);
    assert!(cancel_interaction(&mut panels).is_empty());
}

#[test]
fn timeline_modifier_clicks_and_blank_clear_selection_without_editing() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut panels = Panels::default();
    let area = Rect::new(0., 0., 1200., 400.);
    let mut actions = vec![];
    let styles = PaintList::default().styles();
    let body_top = timeline::header_height(&styles);
    let row_height = styles.metric("row.height", 22.);
    for (row, control, shift, count) in [
        (0., false, false, 1),
        (2., false, true, 3),
        (1., true, false, 2),
        (1., true, false, 3),
        (1., true, false, 2),
        (12., false, false, 0),
    ] {
        let y = body_top + (row + 0.5) * row_height;
        let point = [150., y];
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            modifiers: Modifiers {
                control,
                shift,
                ..Default::default()
            },
            ..Default::default()
        };
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &state(),
            &input,
            &mut actions,
        );
        assert_eq!(panels.selection_ids().len(), count, "y={y}");
        assert!(!panels.layer_actions.editing());
    }
    assert!(actions.is_empty());
}

fn editing_engine() -> motion_core::Engine {
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    for id in 1..=3 {
        let mut layer = motion_core::Layer::solid(id, "layer", [64.; 2], [32., 32., 0.], [0.; 4]);
        layer.locked = id == 2;
        project.layers.push(layer);
    }
    motion_core::Engine::new(project).unwrap()
}

#[test]
fn duplicate_delete_batches_are_atomic_single_undo_and_preserve_locked_layers() {
    let mut engine = editing_engine();
    let initial = engine.snapshot();
    for key in [Key::D, Key::Delete] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.select_only(Some(1));
        panels.add_selection(2);
        panels.add_selection(3);
        let mut actions = vec![];
        let value = serde_json::json!({"project":engine.project(),"frame":0});
        keyboard::route(
            &chord(key, key == Key::D, false),
            &mut panels,
            &value,
            false,
            None,
            &mut actions,
        );
        match actions.pop().unwrap() {
            Action::EditBatch(commands) if key == Key::D => {
                assert_eq!(commands.len(), 2);
                engine.apply_batch(commands).unwrap();
            }
            Action::Edit(Command::RemoveLayers { objects, frame }) if key == Key::Delete => {
                let mut selected = objects.clone();
                selected.sort_unstable();
                assert_eq!(selected, vec![1, 3]);
                assert_eq!(frame, 0);
                engine.apply(Command::RemoveLayers { objects, frame }).unwrap();
            }
            action => panic!("unexpected action: {action:?}"),
        }
        assert_eq!(
            engine.project().layers.len(),
            if key == Key::D { 5 } else { 1 }
        );
        assert!(engine
            .project()
            .layers
            .iter()
            .any(|l| l.id == 2 && l.locked));
        let directory = tempfile::tempdir().unwrap();
        motion_core::storage::save(directory.path(), engine.project()).unwrap();
        assert_eq!(
            motion_core::storage::load(directory.path()).unwrap().layers,
            engine.project().layers
        );
        engine.undo().unwrap();
        assert_eq!(engine.snapshot(), initial);
    }
    let before = engine.snapshot();
    assert!(engine
        .apply_batch(vec![
            Command::Delete { object: 1 },
            Command::Delete { object: 2 }
        ])
        .is_err());
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn timeline_group_drag_keeps_offsets_and_whole_gesture_commits_or_cancels() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for commit in [false, true] {
        let mut engine = editing_engine();
        engine
            .apply_batch(vec![
                Command::MoveLayerClip {
                    object: 1,
                    in_frame: 10,
                },
                Command::MoveLayerClip {
                    object: 3,
                    in_frame: 40,
                },
            ])
            .unwrap();
        let before = engine.snapshot();
        let value = serde_json::json!({"project":engine.project(),"frame":0,"timeline_layers":[
            {"object":1,"in_frame":10,"out_frame":130}, {"object":3,"in_frame":40,"out_frame":160}
        ]});
        let mut panels = Panels::default();
        panels.select_only(Some(1));
        panels.add_selection(3);
        panels.timeline.moving = Some(types::MovingLayers {
            start_mouse: 100.,
            initial: 10,
            last: 10,
            layers: vec![(1, 10, 120), (3, 40, 120)],
        });
        engine.begin_gesture().unwrap();
        let area = Rect::new(0., 0., 1200., 400.);
        let ppf = (area.width() - GUTTER.min(area.width() * 0.52) - 8.) / 120.;
        let mut input = Input {
            mouse: [100. + ppf * 10., timeline::header_height(&PaintList::default().styles()) + 11.],
            held: Some(MouseButton::Left),
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut actions = vec![];
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &value,
            &input,
            &mut actions,
        );
        let Action::EditBatch(commands) = actions.remove(0) else {
            panic!("expected moves")
        };
        assert!(matches!(
            commands.as_slice(),
            [
                Command::MoveLayerClip {
                    object: 1,
                    in_frame: 20
                },
                Command::MoveLayerClip {
                    object: 3,
                    in_frame: 50
                }
            ]
        ));
        engine.apply_batch(commands).unwrap();
        if commit {
            input.released = Some((MouseButton::Left, input.mouse));
            timeline::render(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &mut panels,
                &value,
                &input,
                &mut actions,
            );
            assert!(matches!(actions.as_slice(), [Action::History(3)]));
        } else {
            actions.clear();
            timeline::render(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &mut panels,
                &value,
                &chord(Key::Escape, false, false),
                &mut actions,
            );
            assert!(matches!(actions.as_slice(), [Action::History(4)]));
        }
        engine.end_gesture(commit).unwrap();
        if commit {
            engine.undo().unwrap();
        }
        assert_eq!(engine.snapshot(), before);
    }
}
