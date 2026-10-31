use super::*;
use crate::panels::key_selection::{AddressedKey, SpatialKey};
use motion_core::SpatialKeyEdit;
#[cfg(target_os = "windows")]
#[path = "spatial_native_tests.rs"]
mod native;

fn spatial_fixture(coincident: bool) -> Engine {
    let mut project = fixture().snapshot();
    let track = &mut project.layers[0].transform.position;
    track.keys.clear();
    for (frame, x) in [
        (0, 0.),
        (4, 1.),
        (10, if coincident { 1. } else { 4. }),
        (20, 8.),
    ] {
        track.upsert(frame, [x, 20., 0.], Ease::Linear).unwrap();
    }
    track
        .enable_roving(if coincident { &[4, 10] } else { &[4] })
        .unwrap();
    Engine::new(project).unwrap()
}
fn identity(id: u64) -> AddressedKey {
    SpatialKey {
        target: key(7, None, 0).target,
        id,
    }
    .into()
}
fn identities(panels: &Panels) -> Vec<u64> {
    panels
        .selected_keys
        .spatial_selections()
        .into_iter()
        .flat_map(|(_, ids)| ids)
        .collect()
}
fn at_time(paint: &PaintList, panels: &Panels, time: f64) -> [f32; 2] {
    let track = panels.timeline.zoom_track.unwrap();
    let x = track.min[0] + time as f32 * track.width() / 120.;
    let y = paint
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Procedural {
                area,
                kind: Pattern::Diamond,
                ..
            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.01 => {
                Some((area.min[1] + area.max[1]) * 0.5)
            }
            _ => None,
        })
        .min_by(f32::total_cmp)
        .expect("real continuous-time spatial diamond");
    [x, y]
}

fn pointer(at: [f32; 2], press: bool, release: bool, modifiers: Modifiers) -> Input {
    Input {
        mouse: at,
        pressed: press.then_some((MouseButton::Left, at)),
        released: release.then_some((MouseButton::Left, at)),
        held: (!release).then_some(MouseButton::Left),
        modifiers,
        ..Default::default()
    }
}
fn atomic(actions: Vec<Action>) -> Vec<Command> {
    let mut result = None;
    for action in actions {
        match action {
            Action::EditorAction { commands, .. } => assert!(
                result.replace(commands).is_none(),
                "one release commits once"
            ),
            Action::Seek(_) => {}
            other => panic!("unexpected spatial drag action {other:?}"),
        }
    }
    result.expect("spatial release submits one atomic transaction")
}

#[test]
fn stable_selection_keeps_coincident_identities_and_invalidates_only_deleted_ids_or_scope() {
    let mut engine = spatial_fixture(true);
    let original = view(&engine);
    let source = &original["timeline_layers"][0]["properties"]["position"]["spatial_keys"];
    assert_eq!(
        source[1]["frame"], source[2]["frame"],
        "fixture has distinct keys at one solved time"
    );
    let mut set = Set::default();
    set.apply_addressed(&original, [identity(2), identity(3)], Mode::Replace)
        .unwrap();
    assert_eq!(set.status(&original), Status::Current);
    assert_eq!(set.spatial_selections()[0].1, [2, 3]);
    assert!(
        set.primary().is_none(),
        "stable IDs must never project a fake integer frame"
    );
    let before = set.clone();
    set.apply_addressed(&original, [identity(2), identity(2)], Mode::Toggle)
        .unwrap();
    assert_eq!(set.spatial_selections()[0].1, [3]);
    assert!(
        !set.same_selection(&before),
        "popup identity includes selected IDs"
    );
    engine
        .apply(Command::SpatialKeyframes {
            target: key(7, None, 0).target,
            edit: SpatialKeyEdit::Time {
                key: 3,
                time: 11.25,
            },
        })
        .unwrap();
    assert_eq!(
        set.status(&view(&engine)),
        Status::Current,
        "time changes do not change identity"
    );
    engine
        .apply(Command::SpatialKeyframes {
            target: key(7, None, 0).target,
            edit: SpatialKeyEdit::Delete { key: 3 },
        })
        .unwrap();
    assert_eq!(set.status(&view(&engine)), Status::Stale);
    engine.undo().unwrap();
    let restored = view(&engine);
    assert_eq!(set.status(&restored), Status::Current);
    for field in ["root", "composition", "document_generation"] {
        let mut changed = restored.clone();
        changed[field] = if field == "document_generation" {
            json!(999)
        } else {
            json!("other")
        };
        assert_eq!(set.status(&changed), Status::Stale);
    }
    let mut locked = restored.clone();
    locked["project"]["layers"][0]["locked"] = json!(true);
    assert!(!set.writable(&locked));
    let mut observing = restored.clone();
    observing["observing"] = json!(true);
    assert!(!set.writable(&observing));
    assert!(
        !source::contains(&restored, &key(7, None, 0).target, 11),
        "frame addresses cannot alias spatial storage"
    );
}

#[test]
fn mixed_nudge_is_one_undo_and_unsupported_clipboard_and_easing_do_not_edit_a_subset() {
    let mut engine = spatial_fixture(false);
    let before = engine.snapshot();
    let state = view(&engine);
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .selected_keys
        .apply_addressed(
            &state,
            [identity(2), key(9, Some(Axis::Y), 43).into()],
            Mode::Replace,
        )
        .unwrap();
    let clipboard = crate::panels::keyframe_clipboard::selection(&panels, &state).unwrap();
    assert!(clipboard.selections.is_empty() && clipboard.targets.is_empty() && !clipboard.writable);
    assert!(crate::panels::key_selection::ease_command(
        &panels,
        &state,
        crate::shortcuts::Command::EaseBoth
    )
    .is_none());
    let mut actions = vec![];
    assert!(crate::panels::key_selection::shortcut(
        &mut panels,
        &state,
        crate::shortcuts::Command::NudgeKeyLater,
        &mut actions
    ));
    let commands = atomic(actions);
    assert_eq!(commands.len(), 2);
    engine.apply_batch(commands).unwrap();
    assert_eq!(identities(&panels), [2]);
    assert!(panels.selected_keys.contains(&key(9, Some(Axis::Y), 44)));
    let track = &engine.project().layers[0].transform.position;
    assert_eq!(
        track
            .spatial_animation
            .as_ref()
            .unwrap()
            .prepared()
            .keys()
            .nth(1)
            .unwrap(),
        (2, 3.5)
    );
    assert_eq!(panels.selected_keys.status(&view(&engine)), Status::Current);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
    engine.redo().unwrap();
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(directory.path()).unwrap(),
        engine.snapshot()
    );
}

#[test]
fn fractional_spatial_pointer_drag_uses_stable_ids_in_summary_and_expanded_rows() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            for expanded in [false, true] {
                let mut engine = spatial_fixture(false);
                let original = engine.snapshot();
                let state = view(&engine);
                let mut panels = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                if expanded {
                    panels.timeline.expanded.insert(7);
                }
                let (paint, actions) =
                    draw(&mut panels, &state, Input::default(), floating, &mut atlas);
                selection_only(&actions);
                let start = at_time(&paint, &panels, 9.5);
                let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                let modifiers = Modifiers {
                    alt: true,
                    ..Default::default()
                };
                let (_, actions) = draw(
                    &mut panels,
                    &state,
                    pointer(start, true, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                assert_eq!(
                    identities(&panels),
                    [2],
                    "{locale:?} floating={floating} expanded={expanded}"
                );
                assert!(panels.key_gestures.dragging());
                assert!(panels
                    .animation_controls
                    .user_key_in(&state, Some(7))
                    .is_none());
                let next = [start[0] + 5. * ppf, start[1]];
                let (paint, actions) = draw(
                    &mut panels,
                    &state,
                    pointer(next, false, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                at_time(&paint, &panels, 14.5);
                assert_eq!(engine.project(), &original);
                assert!(!engine.can_undo());
                let (_, actions) = draw(
                    &mut panels,
                    &state,
                    pointer(next, false, true, modifiers),
                    floating,
                    &mut atlas,
                );
                let commands = atomic(actions);
                assert!(
                    matches!(commands.as_slice(), [Command::SpatialKeyframes { edit: SpatialKeyEdit::Move { keys, delta }, .. }] if keys == &[2] && *delta == 5.)
                );
                engine.apply_batch(commands).unwrap();
                assert_eq!(identities(&panels), [2]);
                assert_eq!(
                    engine.project().layers[0]
                        .transform
                        .position
                        .spatial_animation
                        .as_ref()
                        .unwrap()
                        .prepared()
                        .keys()
                        .nth(1)
                        .unwrap(),
                    (2, 7.5)
                );
                assert_eq!(
                    engine.project().layers[0].timeline,
                    original.layers[0].timeline
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &original);
                assert!(!engine.can_undo());
            }
        }
    }
}

#[test]
fn coincident_spatial_pointer_hits_keep_both_ids_and_select_all_keeps_integer_channels() {
    let engine = spatial_fixture(true);
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
        let time = state["timeline_layers"][0]["properties"]["position"]["spatial_keys"][1]
            ["frame"]
            .as_f64()
            .unwrap();
        let point = at_time(&paint, &panels, time);
        click(
            &mut panels,
            &state,
            point,
            Modifiers::default(),
            floating,
            &mut atlas,
        );
        assert_eq!(identities(&panels), [2, 3]);
        assert_eq!(selected(&panels), 0);
        assert!(crate::panels::key_selection::shortcut(
            &mut panels,
            &state,
            crate::shortcuts::Command::SelectAll,
            &mut vec![]
        ));
        let (_, actions) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
        selection_only(&actions);
        assert_eq!(identities(&panels), [1, 2, 3, 4]);
        assert_eq!(
            selected(&panels),
            6,
            "separated ordinary axes must not disappear"
        );
        assert!(
            crate::panels::key_selection::delete_commands(&panels, &state).is_none(),
            "deleting all spatial keys is an explicit unsupported capability"
        );
    }
}

#[test]
fn spatial_drag_cancel_stale_focus_and_copy_never_dispatch_partial_edits_or_late_release() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for reason in [
            "escape",
            "focus",
            "revision",
            "generation",
            "observing",
            "locked",
            "copy",
        ] {
            let engine = spatial_fixture(false);
            let state = view(&engine);
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let start = at_time(&paint, &panels, 9.5);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                control: reason == "copy",
                alt: true,
                ..Default::default()
            };
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let next = [start[0] + 5. * ppf, start[1]];
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let mut changed = state.clone();
            let mut input = pointer(next, false, false, modifiers);
            match reason {
                "escape" => input
                    .keys
                    .push((motion_ui::input::Key::Escape, Modifiers::default())),
                "focus" => input.focus_lost = true,
                "revision" => changed["revision"] = json!(999),
                "generation" => changed["document_generation"] = json!(999),
                "observing" => changed["observing"] = json!(true),
                "locked" => changed["project"]["layers"][0]["locked"] = json!(true),
                "copy" => {}
                _ => unreachable!(),
            }
            let (_, actions) = draw(&mut panels, &changed, input, floating, &mut atlas);
            selection_only(&actions);
            assert!(!panels.key_gestures.dragging(), "{reason}");
            let (_, actions) = draw(
                &mut panels,
                &changed,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let (_, actions) = draw(
                &mut panels,
                &changed,
                Input {
                    mouse: [next[0] + 100., next[1]],
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn spatial_bounds_and_order_rejections_keep_the_entire_mixed_selection_unchanged() {
    let engine = spatial_fixture(false);
    let state = view(&engine);
    for command in [
        crate::shortcuts::Command::NudgeKeysEarlierTen,
        crate::shortcuts::Command::NudgeKeysLaterTen,
    ] {
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels
            .selected_keys
            .apply_addressed(
                &state,
                [identity(2), key(9, Some(Axis::Y), 43).into()],
                Mode::Replace,
            )
            .unwrap();
        let before = panels.selected_keys.clone();
        let mut actions = vec![];
        assert!(crate::panels::key_selection::shortcut(
            &mut panels,
            &state,
            command,
            &mut actions
        ));
        assert!(actions.is_empty());
        assert!(panels.message.is_some());
        assert!(panels.selected_keys.same_selection(&before));
        assert!(!engine.can_undo());
    }
}

#[test]
fn spatial_delete_addresses_the_id_and_keeps_layer_and_survivor_identity_through_undo() {
    let mut engine = spatial_fixture(false);
    let before = engine.snapshot();
    let state = view(&engine);
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .selected_keys
        .apply_addressed(&state, [identity(2)], Mode::Replace)
        .unwrap();
    let commands = crate::panels::key_selection::delete_commands(&panels, &state).unwrap();
    assert!(matches!(
        commands.as_slice(),
        [Command::SpatialKeyframes {
            edit: SpatialKeyEdit::Delete { key: 2 },
            ..
        }]
    ));
    engine.apply_batch(commands).unwrap();
    assert_eq!(engine.project().layers.len(), 2);
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .keys()
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        [1, 3, 4]
    );
    assert_eq!(panels.selected_keys.status(&view(&engine)), Status::Stale);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert_eq!(panels.selected_keys.status(&view(&engine)), Status::Current);
    assert!(!engine.can_undo());
}

#[test]
fn zero_duration_preview_rejects_moving_spans_but_accepts_stationary_coincident_keys() {
    let engine = spatial_fixture(false);
    let state = view(&engine);
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .selected_keys
        .apply_addressed(&state, [identity(3)], Mode::Replace)
        .unwrap();
    let mut actions = vec![];
    assert!(crate::panels::key_selection::shortcut(
        &mut panels,
        &state,
        crate::shortcuts::Command::NudgeKeysLaterTen,
        &mut actions
    ));
    assert!(
        actions.is_empty(),
        "a positive-length span cannot collapse to zero duration"
    );
    assert!(panels.message.is_some());
    assert!(!engine.can_undo());

    let mut project = spatial_fixture(true).snapshot();
    let animation = project.layers[0]
        .transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap();
    animation.set_fixed_time(2, 2.5).unwrap();
    animation.set_fixed_time(3, 3.5).unwrap();
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let state = view(&engine);
    let keys = &state["timeline_layers"][0]["properties"]["position"]["spatial_keys"];
    assert_eq!(keys[2]["incoming_length"], 0.);
    assert!(keys[1]["incoming_length"].as_f64().unwrap() > 0.);
    panels
        .selected_keys
        .apply_addressed(&state, [identity(3)], Mode::Replace)
        .unwrap();
    assert!(crate::panels::key_selection::shortcut(
        &mut panels,
        &state,
        crate::shortcuts::Command::NudgeKeyEarlier,
        &mut actions
    ));
    engine.apply_batch(atomic(actions)).unwrap();
    let times: Vec<_> = engine.project().layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .prepared()
        .keys()
        .collect();
    assert_eq!(times[1].1, times[2].1);
    assert_ne!(times[1].0, times[2].0);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
