use super::*;
#[cfg(target_os = "windows")]
#[path = "drag_native_tests.rs"]
mod native;

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
fn collection(panels: &mut Panels, state: &Value, keys: impl IntoIterator<Item = Key>) {
    panels
        .selected_keys
        .apply(state, keys, Mode::Replace)
        .unwrap();
    crate::panels::key_selection::ui::project_primary(panels, state);
}
fn atomic(actions: Vec<Action>) -> Vec<Command> {
    let mut commands = None;
    for action in actions {
        match action {
            Action::EditorAction { commands: next, .. } => {
                assert!(commands.replace(next).is_none(), "one release commits once");
            }
            Action::Seek(_) => {}
            other => panic!("unexpected drag action {other:?}"),
        }
    }
    commands.expect("release must submit the selected collection")
}
#[test]
fn multi_key_pointer_drag_previews_then_commits_once_and_copy_keeps_sources() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for copy in [false, true] {
                let mut engine = fixture();
                let before = engine.snapshot();
                let state = view(&engine);
                let mut panels = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                collection(
                    &mut panels,
                    &state,
                    [key(7, None, 10), key(9, Some(Axis::Y), 43)],
                );
                let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
                let start = point(&paint, &panels, 10);
                let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                let modifiers = Modifiers {
                    control: copy,
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
                assert!(panels.key_gestures.dragging());
                let next = [start[0] + 5. * ppf, start[1] + 30.];
                let (preview, actions) = draw(
                    &mut panels,
                    &state,
                    pointer(next, false, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                assert_eq!(engine.project(), &before);
                assert!(!engine.can_undo());
                assert_eq!(
                    selected(&panels),
                    2,
                    "clicking a member keeps the collection during a drag"
                );
                assert!(
                    preview.shapes.iter().any(
                        |shape| matches!(shape,Shape::Procedural {area,kind:Pattern::Diamond,..}
                    if ((area.min[0]+area.max[0])*0.5-next[0]).abs()<0.01)
                    ),
                    "preview must follow the pointer"
                );
                let (_, actions) = draw(
                    &mut panels,
                    &state,
                    pointer(next, false, true, modifiers),
                    floating,
                    &mut atlas,
                );
                assert!(!panels.key_gestures.dragging());
                let commands = atomic(actions);
                let [Command::TransformKeyframes {
                    delta_frame,
                    value_offset,
                    copy: copied,
                    ..
                }] = commands.as_slice()
                else {
                    panic!("{commands:?}");
                };
                assert_eq!(*delta_frame, 5);
                assert_eq!(*copied, copy);
                assert!(value_offset.is_none());
                engine.apply_batch(commands).unwrap();
                assert!(present(&engine, &key(7, None, 15).target, 15));
                assert!(present(&engine, &key(9, Some(Axis::Y), 48).target, 48));
                assert_eq!(present(&engine, &key(7, None, 10).target, 10), copy);
                assert_eq!(
                    engine.project().layers[1]
                        .transform
                        .position
                        .axes
                        .as_ref()
                        .unwrap()
                        .x,
                    before.layers[1].transform.position.axes.as_ref().unwrap().x
                );
                let directory = tempfile::tempdir().unwrap();
                motion_core::storage::save(directory.path(), engine.project()).unwrap();
                assert_eq!(
                    motion_core::storage::load(directory.path()).unwrap(),
                    engine.snapshot()
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                assert!(!engine.can_undo());
            }
        }
    }
}

#[test]
fn multi_key_pointer_rejects_collision_and_bounds_without_partial_edits() {
    let engine = fixture();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for delta in [10., -15.] {
        for floating in [false, true] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            collection(
                &mut panels,
                &state,
                [key(7, None, 10), key(9, Some(Axis::Y), 43)],
            );
            let selected = panels.selected_keys.selections();
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let start = point(&paint, &panels, 10);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            draw(
                &mut panels,
                &state,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            let next = [start[0] + delta * ppf, start[1]];
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert_eq!(panels.selected_keys.selections(), selected);
            assert!(panels.message.is_some());
            assert!(!panels.key_gestures.dragging());
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn multi_key_pointer_cancel_stale_and_late_release_never_modify_authored_data() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut project = fixture().snapshot();
    let layer = project.layers[0].clone();
    for id in 100..140 {
        let mut copy = layer.clone();
        copy.id = id;
        project.layers.push(copy);
    }
    let engine = Engine::new(project).unwrap();
    let original = view(&engine);
    for floating in [false, true] {
        for reason in [
            "escape",
            "focus",
            "revision",
            "generation",
            "composition",
            "zoom",
            "scroll",
            "boundary",
        ] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            collection(
                &mut panels,
                &original,
                [key(139, None, 10), key(9, Some(Axis::Y), 43)],
            );
            let before = panels.selected_keys.selections();
            let (paint, _) = draw(
                &mut panels,
                &original,
                Input::default(),
                floating,
                &mut atlas,
            );
            let start = point(&paint, &panels, 10);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            draw(
                &mut panels,
                &original,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            let next = [start[0] + 5. * ppf, start[1]];
            let (_, actions) = draw(
                &mut panels,
                &original,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert!(panels.key_gestures.dragging(), "{reason}");
            let mut changed = original.clone();
            let mut input = pointer(next, false, false, modifiers);
            match reason {
                "escape" => input
                    .keys
                    .push((motion_ui::input::Key::Escape, Modifiers::default())),
                "focus" => input.focus_lost = true,
                "revision" => changed["revision"] = json!(engine.revision() + 1),
                "generation" => changed["document_generation"] = json!(8),
                "composition" => changed["composition"] = json!("another"),
                "zoom" => panels.timeline.zoom = 2.,
                "scroll" => panels.timeline.scroll = 10.,
                "boundary" => {
                    let actions = panels::cancel_interaction(&mut panels);
                    assert!(actions.is_empty());
                }
                _ => unreachable!(),
            }
            let (_, actions) = draw(&mut panels, &changed, input, floating, &mut atlas);
            selection_only(&actions);
            assert!(!panels.key_gestures.dragging(), "{reason} float={floating}");
            let (_, actions) = draw(
                &mut panels,
                &changed,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert_eq!(
                panels.selected_keys.selections(),
                before,
                "{reason} float={floating}"
            );
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn graph_multi_key_body_drag_preserves_spacing_and_constrains_time_or_value() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for mode in ["free", "free_vertical", "time", "value", "shift_time"] {
            let mut engine = fixture();
            let before = engine.snapshot();
            let state = view(&engine);
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            collection(&mut panels, &state, [key(7, None, 10), key(7, None, 20)]);
            panels.timeline.graph_mode = true;
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let start = point(&paint, &panels, 10);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                alt: mode == "time",
                shift: mode == "value" || mode == "shift_time",
                ..Default::default()
            };
            draw(
                &mut panels,
                &state,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            assert!(panels.key_gestures.dragging(), "{mode} float={floating}");
            let next = [
                start[0]
                    + if mode == "free_vertical" {
                        0.
                    } else {
                        5. * ppf
                    },
                start[1] - if mode == "shift_time" { 1. } else { 50. },
            ];
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            let commands = atomic(actions);
            let [Command::TransformKeyframes {
                delta_frame,
                value_offset,
                ..
            }] = commands.as_slice()
            else {
                panic!("{commands:?}");
            };
            assert_eq!(
                *delta_frame,
                if mode == "value" || mode == "free_vertical" {
                    0
                } else {
                    5
                },
                "{mode}"
            );
            assert_eq!(
                value_offset.is_some(),
                mode == "free" || mode == "free_vertical" || mode == "value",
                "{mode}"
            );
            engine.apply_batch(commands).unwrap();
            let keys = &engine.project().layers[0].transform.position.keys;
            assert_eq!(keys[1].frame - keys[0].frame, 10);
            assert!((keys[1].value[0] - keys[0].value[0] - 10.).abs() < 0.00001);
            for (old, new) in before.layers[0].transform.position.keys.iter().zip(keys) {
                assert_eq!(old.value[1..], new.value[1..]);
                assert_eq!(old.ease, new.ease);
            }
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn multi_key_snapping_uses_screen_pixels_at_different_timeline_zooms() {
    let mut project = fixture().snapshot();
    project.work_area.in_frame = 17;
    project.work_area.out_frame = 100;
    let engine = Engine::new(project).unwrap();
    let state = view(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for (zoom, expected, guide) in [(1., 7, Some(17.)), (4., 5, None)] {
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            collection(
                &mut panels,
                &state,
                [key(7, None, 10), key(9, Some(Axis::Y), 43)],
            );
            panels.timeline.zoom = zoom;
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let track = panels.timeline.zoom_track.unwrap();
            let ppf = track.width() / 120. * zoom;
            let x = track.min[0] + 10. * ppf;
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
                .unwrap();
            let start = [x, y];
            draw(
                &mut panels,
                &state,
                pointer(start, true, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            let next = [x + 5. * ppf, y];
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert_eq!(
                panels.timeline.snap_guide, guide,
                "zoom={zoom} float={floating}"
            );
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(next, false, true, Modifiers::default()),
                floating,
                &mut atlas,
            );
            let commands = atomic(actions);
            assert!(
                matches!(commands.as_slice(),[Command::TransformKeyframes {delta_frame,..}] if *delta_frame==expected),
                "{commands:?}"
            );
        }
    }
}

#[test]
fn single_key_snap_keeps_existing_lane_history_and_rejects_stale_input_before_editing() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for stale in [false, true] {
            let mut project = fixture().snapshot();
            project.work_area.in_frame = 17;
            project.work_area.out_frame = 100;
            let mut engine = Engine::new(project).unwrap();
            let before = engine.snapshot();
            let state = view(&engine);
            let mut panels = Panels {
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let (paint, _) = draw(&mut panels, &state, Input::default(), floating, &mut atlas);
            let start = point(&paint, &panels, 10);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let (_, actions) = draw(
                &mut panels,
                &state,
                pointer(start, true, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            selection_only(&actions);
            assert!(panels.key_gestures.snapping());
            if stale {
                engine
                    .apply(Command::SetScalar {
                        object: 7,
                        property: Property::Opacity,
                        frame: 0,
                        value: 0.8,
                    })
                    .unwrap();
            }
            let external = engine.snapshot();
            let next = [start[0] + 5. * ppf, start[1]];
            let (_, actions) = draw(
                &mut panels,
                &view(&engine),
                pointer(next, false, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            if stale {
                selection_only(&actions);
                assert!(!panels.key_gestures.snapping());
            } else {
                assert_eq!(panels.timeline.snap_guide, Some(17.));
                assert!(actions.iter().any(|a|matches!(a,Action::EditBatch(commands) if matches!(commands.as_slice(),[Command::MoveKey {from:10,to:17,..}]))));
                run(&mut engine, actions);
            }
            let (_, actions) = draw(
                &mut panels,
                &view(&engine),
                pointer(next, false, true, Modifiers::default()),
                floating,
                &mut atlas,
            );
            if stale {
                selection_only(&actions);
                assert_eq!(engine.project(), &external);
            } else {
                run(&mut engine, actions);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                assert!(!engine.can_undo());
            }
            assert!(!panels.key_gestures.snapping());
        }
    }
}

#[test]
fn expanded_row_drag_keeps_other_selected_channels_at_the_same_time() {
    let mut project = fixture().snapshot();
    project.layers[0]
        .transform
        .opacity
        .set_animated(3, true)
        .unwrap();
    let mut engine = Engine::new(project).unwrap();
    let state = view(&engine);
    let before = engine.snapshot();
    let opacity = KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels.select_only(Some(7));
    collection(
        &mut panels,
        &state,
        [
            key(7, None, 10),
            Key {
                target: opacity.clone(),
                frame: 10,
            },
        ],
    );
    panels.timeline.expanded.insert(7);
    panels
        .animation_controls
        .begin_row_probe(7, Property::Position, None);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(&mut panels, &state, Input::default(), false, &mut atlas);
    let track = panels.timeline.zoom_track.unwrap();
    let ppf = track.width() / 120.;
    let row = panels.animation_controls.probed_row(None).unwrap();
    let start = [track.min[0] + 10. * ppf, (row.min[1] + row.max[1]) * 0.5];
    let modifiers = Modifiers {
        alt: true,
        ..Default::default()
    };
    draw(
        &mut panels,
        &state,
        pointer(start, true, false, modifiers),
        false,
        &mut atlas,
    );
    assert!(
        panels.key_gestures.dragging(),
        "an expanded row must not reduce an explicit multi-channel selection to its own key"
    );
    let next = [start[0] + 5. * ppf, start[1]];
    let (_, actions) = draw(
        &mut panels,
        &state,
        pointer(next, false, false, modifiers),
        false,
        &mut atlas,
    );
    selection_only(&actions);
    let (_, actions) = draw(
        &mut panels,
        &state,
        pointer(next, false, true, modifiers),
        false,
        &mut atlas,
    );
    engine.apply_batch(atomic(actions)).unwrap();
    assert!(present(&engine, &key(7, None, 15).target, 15) && present(&engine, &opacity, 15));
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}
