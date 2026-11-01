//! Exact local identities travel through real main/floating timeline input.
use super::super::local::mapped;
use super::*;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};
use motion_core::{LocalKeyEdit, LocalValueOffset};
#[cfg(target_os = "windows")]
#[path = "local_timing_native_tests.rs"]
mod native;
#[path = "local_typed_tests.rs"]
mod typed;
#[path = "local_clipboard_tests.rs"]
mod clipboard;

fn local(object: u64, axis: Option<Axis>, frame: i32) -> AddressedKey {
    LocalKey {
        target: key(object, axis, 0).target,
        frame,
    }
    .into()
}
fn at_time(paint: &PaintList, panels: &Panels, time: f64) -> [f32; 2] {
    let track = panels.timeline.zoom_track.unwrap();
    let x = track.min[0] + time as f32 * track.width() / 120. * panels.timeline.zoom.max(1.);
    let y = paint
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Procedural {
                area,
                kind: Pattern::Diamond,
                ..
            } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.02 => {
                Some((area.min[1] + area.max[1]) * 0.5)
            }
            _ => None,
        })
        .min_by(f32::total_cmp)
        .expect("actual fractional-time key diamond");
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
                "one release = one transaction"
            ),
            Action::Seek(_) => {}
            other => panic!("unexpected action {other:?}"),
        }
    }
    result.expect("release submits the local collection")
}
fn setup(locale: Locale, graph: bool, expanded: bool) -> Panels {
    let mut panels = Panels {
        locale,
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels.select_only(Some(7));
    panels.timeline.graph_mode = graph;
    panels.timeline.speed_graph = false;
    if expanded {
        panels.timeline.expanded.insert(7);
    }
    panels
        .animation_controls
        .display_track(7, Property::Position, None);
    panels
}

#[test]
fn common_grid_moves_mixed_clocks_together_and_value_offsets_only_the_target() {
    let mut project = mapped(false).snapshot();
    project.layers[1].time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(5, 2).unwrap(),
            TimeDirection::Reverse,
            20.,
            0.,
        )
        .unwrap(),
        remap: None,
    });
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    let mut selected = Set::default();
    selected
        .apply_addressed(
            &state(&engine),
            [local(7, None, 3), local(9, Some(Axis::X), 3)],
            Mode::Replace,
        )
        .unwrap();
    let plan = transform::Plan::capture(&selected, &state(&engine)).unwrap();
    assert_eq!(plan.delta_frame(1), 7.5);
    assert_eq!(plan.exact_steps(1.), None);
    assert_eq!(plan.nearest_steps(7.4), Some(1));
    assert!(plan.allows(1, false));
    let commands = plan
        .commands(
            1,
            Some(motion_core::KeyframeValueOffset {
                target: key(7, None, 0).target,
                component: 0,
                delta: 4.,
            }),
            false,
        )
        .unwrap();
    assert!(matches!(
        &commands[0],
        Command::LocalKeyframes {
            edit: LocalKeyEdit::Translate {
                delta_frame: 5,
                value_offset: Some(LocalValueOffset {
                    component: 0,
                    delta: 4.
                }),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        &commands[1],
        Command::LocalKeyframes {
            edit: LocalKeyEdit::Translate {
                delta_frame: -3,
                value_offset: None,
                ..
            },
            ..
        }
    ));
    engine.apply_batch(commands).unwrap();
    for (index, axis, from, to) in [(0, None, 3, 8), (1, Some(Axis::X), 3, 0)] {
        let a = before.layers[index]
            .property_composition_frame(f64::from(from))
            .unwrap();
        let b = engine.project().layers[index]
            .property_composition_frame(f64::from(to))
            .unwrap();
        assert_eq!(b - a, 7.5);
        let track = &engine.project().layers[index].transform.position;
        assert!(if let Some(axis) = axis {
            {
                let axes = track.axes.as_ref().unwrap();
                let lane = match axis {
                    Axis::X => &axes.x,
                    Axis::Y => &axes.y,
                    Axis::Z => &axes.z,
                };
                lane.keys.iter().any(|k| k.frame == to)
            }
        } else {
            track.keys.iter().any(|k| k.frame == to && k.value[0] == 7.)
        });
    }
    let shifted = plan.translated(1).unwrap();
    assert!(
        shifted.contains_addressed(&local(7, None, 8))
            && shifted.contains_addressed(&local(9, Some(Axis::X), 0))
    );
    assert_eq!(shifted.status(&state(&engine)), Status::Current);
    let saved: Project =
        serde_json::from_slice(&serde_json::to_vec(engine.project()).unwrap()).unwrap();
    assert_eq!(saved, engine.snapshot());
    saved.validate().unwrap();
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn common_grid_with_legacy_keys_collision_and_copy_are_checked_before_commands() {
    let engine = mapped(false);
    let mut selected = Set::default();
    selected
        .apply_addressed(
            &state(&engine),
            [local(7, None, 3), key(9, Some(Axis::X), 43).into()],
            Mode::Replace,
        )
        .unwrap();
    let plan = transform::Plan::capture(&selected, &state(&engine)).unwrap();
    assert_eq!(plan.delta_frame(1), 3.);
    let commands = plan.commands(1, None, true).unwrap();
    assert!(commands.iter().any(|c| matches!(
        c,
        Command::TransformKeyframes {
            delta_frame: 3,
            copy: true,
            ..
        }
    )));
    assert!(commands.iter().any(|c| matches!(
        c,
        Command::LocalKeyframes {
            edit: LocalKeyEdit::Translate {
                delta_frame: 2,
                copy: true,
                ..
            },
            ..
        }
    )));
    selected
        .apply_addressed(&state(&engine), [local(7, None, 3)], Mode::Replace)
        .unwrap();
    let plan = transform::Plan::capture(&selected, &state(&engine)).unwrap();
    assert!(!plan.allows(10, false));
    assert!(plan.commands(10, None, false).is_err());
    assert!(!plan.allows(-6, false));
    assert!(!plan.allows(i32::MAX, false));
    assert!(!plan.allows(0, true));
    assert!(plan.allows(0, false));
    selected
        .apply_addressed(
            &state(&engine),
            [local(7, None, 3), local(7, None, 13)],
            Mode::Replace,
        )
        .unwrap();
    let plan = transform::Plan::capture(&selected, &state(&engine)).unwrap();
    assert!(
        plan.allows(10, false),
        "moving selected source out of the way is atomic"
    );
    assert!(
        !plan.allows(10, true),
        "copy must preserve its selected source"
    );
}

#[test]
fn keyboard_nudge_is_exact_composition_time_and_never_rounds_each_layer() {
    for denominator in [1, 2] {
        let mut project = mapped(false).snapshot();
        project.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
            TimeStretch::new(1, denominator).unwrap(),
            TimeDirection::Forward,
            0.,
            0.,
        )
        .unwrap();
        let mut engine = Engine::new(project).unwrap();
        let mut panels = setup(Locale::Zh, false, false);
        panels
            .selected_keys
            .apply_addressed(&state(&engine), [local(7, None, 3)], Mode::Replace)
            .unwrap();
        let mut actions = Vec::new();
        assert!(shortcut(
            &mut panels,
            &state(&engine),
            crate::shortcuts::Command::NudgeKeyLater,
            &mut actions
        ));
        let commands = atomic(actions);
        assert!(
            matches!(commands.as_slice(),[Command::LocalKeyframes {edit:LocalKeyEdit::Translate {delta_frame,..},..}] if *delta_frame==denominator as i32)
        );
        engine.apply_batch(commands).unwrap();
        assert!(panels
            .selected_keys
            .contains_addressed(&local(7, None, 3 + denominator as i32)));
    }
}

#[test]
fn fractional_pointer_drag_uses_release_position_in_both_owners_languages_and_lanes() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for expanded in [false, true] {
                for reverse in [false, true] {
                    for copy in [false, true] {
                        let mut engine = mapped(reverse);
                        let before = engine.snapshot();
                        let s = view(&engine);
                        let mut panels = setup(locale, false, expanded);
                        let (paint, _) =
                            draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                        let time = engine.project().layers[0]
                            .property_composition_frame(3.)
                            .unwrap();
                        let start = at_time(&paint, &panels, time);
                        let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                        let modifiers = Modifiers {
                            control: copy,
                            alt: true,
                            ..Default::default()
                        };
                        let (_, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(start, true, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        assert!(
                            actions
                                .iter()
                                .any(|a| matches!(a,Action::Seek(f) if *f==time)),
                            "seek must retain fractional projection"
                        );
                        selection_only(&actions);
                        assert!(panels.key_gestures.dragging());
                        assert!(panels.selected_keys.contains_addressed(&local(7, None, 3)));
                        assert!(panels.selected_keys.selections().is_empty());
                        let next = [start[0] + 4.5 * ppf, start[1]];
                        let (preview, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(next, false, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        selection_only(&actions);
                        assert_eq!(engine.project(), &before);
                        assert!(!engine.can_undo());
                        assert!(preview.shapes.iter().any(|shape|matches!(shape,Shape::Procedural {area,kind:Pattern::Diamond,..} if ((area.min[0]+area.max[0])*0.5-next[0]).abs()<0.02)),"ghost follows exact common delta");
                        // Release was not preceded by a move at this position.
                        let release = [start[0] + 6. * ppf, start[1]];
                        let (_, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(release, false, true, modifiers),
                            floating,
                            &mut atlas,
                        );
                        let commands = atomic(actions);
                        let delta = if reverse { -4 } else { 4 };
                        assert!(
                            matches!(commands.as_slice(),[Command::LocalKeyframes {edit:LocalKeyEdit::Translate {frames,delta_frame,copy:copied,..},..}] if frames==&[3] && *delta_frame==delta && *copied==copy)
                        );
                        engine.apply_batch(commands).unwrap();
                        let keys = &engine.project().layers[0].transform.position.keys;
                        assert!(keys.iter().any(|k| k.frame == 3 + delta));
                        assert_eq!(keys.iter().any(|k| k.frame == 3), copy);
                        let (_, actions) = draw(
                            &mut panels,
                            &view(&engine),
                            Input {
                                mouse: [release[0] + 20., release[1]],
                                ..Default::default()
                            },
                            floating,
                            &mut atlas,
                        );
                        assert!(
                            actions
                                .iter()
                                .all(|a| !matches!(a, Action::EditorAction { .. })),
                            "movement after release cannot edit"
                        );
                        assert!(engine.undo().unwrap());
                        assert_eq!(engine.project(), &before);
                        assert!(!engine.can_undo());
                    }
                }
            }
        }
    }
}

#[test]
fn local_drag_cancels_on_escape_focus_loss_revision_and_rejects_occupied_destination() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mode in ["escape", "focus", "revision", "collision"] {
        for floating in [false, true] {
            let engine = mapped(false);
            let before = engine.snapshot();
            let mut s = view(&engine);
            let mut panels = setup(Locale::Zh, false, false);
            let (paint, _) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
            let start = at_time(&paint, &panels, 6.5);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            draw(
                &mut panels,
                &s,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            let next = [start[0] + 15. * ppf, start[1]];
            draw(
                &mut panels,
                &s,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            let mut input = pointer(next, false, true, modifiers);
            match mode {
                "escape" => {
                    input
                        .keys
                        .push((motion_ui::input::Key::Escape, Modifiers::default()));
                }
                "focus" => {
                    input.focus_lost = true;
                }
                "revision" => {
                    s["revision"] = json!(engine.revision() + 1);
                }
                _ => {}
            }
            let (_, actions) = draw(&mut panels, &s, input, floating, &mut atlas);
            assert!(
                actions
                    .iter()
                    .all(|a| !matches!(a, Action::EditorAction { .. })),
                "{mode} cannot commit"
            );
            assert!(!panels.key_gestures.dragging());
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn graph_local_drag_and_select_all_retain_storage_identity() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for reverse in [false, true] {
                let mut engine = mapped(reverse);
                let s = view(&engine);
                let before = engine.snapshot();
                let mut panels = setup(locale, true, false);
                let (paint, _) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                let time = engine.project().layers[0]
                    .property_composition_frame(3.)
                    .unwrap();
                let start = at_time(&paint, &panels, time);
                let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                let modifiers = Modifiers {
                    alt: true,
                    ..Default::default()
                };
                let (_, actions) = draw(
                    &mut panels,
                    &s,
                    pointer(start, true, false, modifiers),
                    floating,
                    &mut atlas,
                );
                selection_only(&actions);
                assert!(panels.key_gestures.dragging());
                assert!(panels.timeline.key_drag.is_none() && panels.timeline.graph_drag.is_none());
                let next = [start[0] + 6. * ppf, start[1] - 30.];
                draw(
                    &mut panels,
                    &s,
                    pointer(next, false, false, modifiers),
                    floating,
                    &mut atlas,
                );
                let (_, actions) = draw(
                    &mut panels,
                    &s,
                    pointer(next, false, true, modifiers),
                    floating,
                    &mut atlas,
                );
                engine.apply_batch(atomic(actions)).unwrap();
                let delta = if reverse { -4 } else { 4 };
                assert!(engine.project().layers[0]
                    .transform
                    .position
                    .keys
                    .iter()
                    .any(|k| k.frame == 3 + delta && k.value == [3., 20., 0.]));
                assert!(engine.undo().unwrap());
                assert_eq!(engine.project(), &before);
                panels.selected_keys.clear(&view(&engine));
                let mut actions = Vec::new();
                assert!(shortcut(
                    &mut panels,
                    &view(&engine),
                    crate::shortcuts::Command::SelectAll,
                    &mut actions
                ));
                draw(
                    &mut panels,
                    &view(&engine),
                    Input::default(),
                    floating,
                    &mut atlas,
                );
                assert!(panels.selected_keys.has_local());
                assert!(panels.selected_keys.selections().is_empty());
                let expected = if reverse {
                    vec![-3, 3, 13]
                } else {
                    vec![3, 13]
                };
                assert_eq!(panels.selected_keys.local_selections()[0].1, expected);
            }
        }
    }
}

#[test]
fn malformed_projection_and_row_clocks_are_bounded_visible_and_stdout_isolated() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::malformed_projection_and_row_clocks_are_bounded_visible_and_stdout_isolated";
    const CHILD: &str = "MOTION_LOCAL_TIMING_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let s = view(&mapped(false));
        let mut panels = setup(Locale::Zh, false, false);
        let target = key(7, None, 0).target;
        let mut paint = PaintList::default();
        let lane = crate::panels::Rect::new(0., 0., 100., 22.);
        panels
            .selected_keys
            .apply_addressed(&s, [local(7, None, 3)], Mode::Replace)
            .unwrap();
        crate::panels::key_selection::ui::begin(
            &mut panels,
            &s,
            &Input::default(),
            lane,
            [1., 0., 120., 0., 1.],
            0,
        );
        let empty = json!({"keys":[]});
        for _ in 0..120 {
            crate::panels::timeline::rows(&s);
            crate::panels::key_selection::ui::local_channel(
                &mut paint,
                &mut panels,
                &s,
                target.clone(),
                &empty,
                lane,
                1.,
                0.,
            );
        }
        crate::logging::flush();
        assert!(!std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .contains("panel degradation"));
        let mut bad = s.clone();
        bad["project"]["layers"][0]["time_mapping"]["clock"]["stretch"]["denominator"] = json!(0);
        let invalid = json!({"keys":[{"frame":6.5,"local_frame":"not-an-integer"}]});
        for _ in 0..120 {
            let rows = crate::panels::timeline::rows(&bad);
            assert!(rows
                .iter()
                .find(|r| r.id == 7)
                .unwrap()
                .property_clock
                .is_none());
            crate::panels::key_selection::ui::local_channel(
                &mut paint,
                &mut panels,
                &s,
                target.clone(),
                &json!({}),
                lane,
                1.,
                0.,
            );
            crate::panels::key_selection::ui::local_channel(
                &mut paint,
                &mut panels,
                &s,
                target.clone(),
                &invalid,
                lane,
                1.,
                0.,
            );
            crate::panels::timeline::rows(&s);
            crate::panels::key_selection::ui::local_channel(
                &mut paint,
                &mut panels,
                &s,
                target.clone(),
                &empty,
                lane,
                1.,
                0.,
            );
        }
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut graph_panels = setup(Locale::Zh, true, false);
        let mut invalid_graph = s.clone();
        invalid_graph["timeline_layers"][0]["properties"]["position"]["keys"][1]["local_frame"] =
            json!("not-an-integer");
        for _ in 0..120 {
            draw(
                &mut graph_panels,
                &invalid_graph,
                Input::default(),
                false,
                &mut atlas,
            );
        }
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in [
            "timeline.property_clock",
            "timeline.local_key_array",
            "timeline.local_key",
            "timeline.graph_local_key",
        ] {
            let records: Vec<_> = file
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(records.len(), 2, "{site}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
            assert!(records
                .iter()
                .all(|l| l.contains("object=7") && l.contains("phase=")));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_TIMING_LOG_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &root)
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for site in [
        "timeline.property_clock",
        "timeline.local_key_array",
        "timeline.local_key",
        "timeline.graph_local_key",
    ] {
        assert!(stderr.contains(&format!("site={site} ")));
    }
    assert!(!stderr.contains("count=1 "));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}

#[test]
fn mapped_graph_value_drag_uses_local_target_and_preserves_opacity_limits() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for reverse in [false, true] {
            let mut project = mapped(reverse).snapshot();
            project.layers[0]
                .transform
                .opacity
                .upsert(3, 0.25, Ease::Linear)
                .unwrap();
            project.layers[0]
                .transform
                .opacity
                .upsert(13, 0.75, Ease::Linear)
                .unwrap();
            let mut engine = Engine::new(project).unwrap();
            let before = engine.snapshot();
            let s = view(&engine);
            let mut panels = setup(Locale::Zh, true, false);
            panels
                .animation_controls
                .display_track(7, Property::Opacity, None);
            let (paint, _) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
            let time = engine.project().layers[0]
                .property_composition_frame(3.)
                .unwrap();
            let start = at_time(&paint, &panels, time);
            let modifiers = Modifiers {
                shift: true,
                ..Default::default()
            };
            draw(
                &mut panels,
                &s,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            let next = [start[0], start[1] - 2000.];
            draw(
                &mut panels,
                &s,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            let (_, actions) = draw(
                &mut panels,
                &s,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            let commands = atomic(actions);
            assert!(matches!(
                commands.as_slice(),
                [Command::LocalKeyframes {
                    target: KeyframeTarget::Property {
                        property: Property::Opacity,
                        ..
                    },
                    edit: LocalKeyEdit::Translate {
                        delta_frame: 0,
                        value_offset: Some(LocalValueOffset {
                            component: 0,
                            delta: 0.75
                        }),
                        ..
                    }
                }]
            ));
            engine.apply_batch(commands).unwrap();
            assert_eq!(
                engine.project().layers[0].transform.opacity.keys[0].value,
                1.
            );
            assert_eq!(
                engine.project().layers[0].transform.opacity.keys[1],
                before.layers[0].transform.opacity.keys[1]
            );
            assert_eq!(
                engine.project().layers[0].transform.position,
                before.layers[0].transform.position
            );
            assert!(engine.undo().unwrap());
            assert_eq!(engine.project(), &before);
        }
    }
}

#[test]
fn clicking_a_local_member_keeps_the_mixed_collection_and_moves_on_its_common_grid() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for expanded in [false, true] {
            let mut engine = mapped(false);
            let before = engine.snapshot();
            let s = view(&engine);
            let mut panels = setup(Locale::Zh, false, expanded);
            panels
                .selected_keys
                .apply_addressed(
                    &s,
                    [local(7, None, 3), key(9, Some(Axis::Y), 43).into()],
                    Mode::Replace,
                )
                .unwrap();
            let (paint, _) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
            let start = at_time(&paint, &panels, 6.5);
            let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
            let modifiers = Modifiers {
                alt: true,
                ..Default::default()
            };
            draw(
                &mut panels,
                &s,
                pointer(start, true, false, modifiers),
                floating,
                &mut atlas,
            );
            assert_eq!(panels.selected_keys.local_selections()[0].1, [3]);
            assert_eq!(panels.selected_keys.selections()[0].frames, [43]);
            let next = [start[0] + 3. * ppf, start[1]];
            draw(
                &mut panels,
                &s,
                pointer(next, false, false, modifiers),
                floating,
                &mut atlas,
            );
            let (_, actions) = draw(
                &mut panels,
                &s,
                pointer(next, false, true, modifiers),
                floating,
                &mut atlas,
            );
            let commands = atomic(actions);
            assert_eq!(commands.len(), 2);
            assert!(commands
                .iter()
                .any(|c| matches!(c, Command::TransformKeyframes { delta_frame: 3, .. })));
            assert!(commands.iter().any(|c| matches!(
                c,
                Command::LocalKeyframes {
                    edit: LocalKeyEdit::Translate { delta_frame: 2, .. },
                    ..
                }
            )));
            engine.apply_batch(commands).unwrap();
            assert!(panels.selected_keys.contains_addressed(&local(7, None, 5)));
            assert!(panels.selected_keys.contains(&key(9, Some(Axis::Y), 46)));
            assert!(engine.undo().unwrap());
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn ordinary_select_all_uses_local_identity_for_integer_and_fractional_times_offscreen() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for reverse in [false, true] {
            let mut project = mapped(reverse).snapshot();
            project.layers[0]
                .transform
                .position
                .upsert(4, [4., 20., 0.], Ease::Linear)
                .unwrap();
            for id in 10..85 {
                let mut layer = project.layers[0].clone();
                layer.id = id;
                project.layers.push(layer);
            }
            let mut engine = Engine::new(project).unwrap();
            let before = engine.snapshot();
            let s = view(&engine);
            let mut panels = setup(Locale::Zh, false, false);
            panels
                .selected_keys
                .apply_addressed(&s, [local(7, None, 3)], Mode::Replace)
                .unwrap();
            let mut actions = Vec::new();
            assert!(shortcut(
                &mut panels,
                &s,
                crate::shortcuts::Command::SelectAll,
                &mut actions
            ));
            let (_, actions) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
            selection_only(&actions);
            assert!(
                panels.message.is_none(),
                "whole select-all must not mix local and composition addresses: {:?}",
                panels.message
            );
            for object in std::iter::once(7).chain(10..85) {
                let selected = panels
                    .selected_keys
                    .local_selections()
                    .into_iter()
                    .find(|(target, _)| target.object() == object)
                    .unwrap()
                    .1;
                assert_eq!(
                    selected,
                    if reverse {
                        vec![-3, 3, 4, 13]
                    } else {
                        vec![3, 4, 13]
                    }
                );
            }
            assert!(
                panels.selected_keys.contains(&key(9, Some(Axis::X), 43)),
                "unmapped channels keep their integer address"
            );
            assert!(panels
                .selected_keys
                .selections()
                .iter()
                .all(|s| s.target.object() == 9));
            let commands = crate::panels::key_selection::delete_commands(&panels, &s)
                .expect("complete mixed deletion");
            engine.apply_batch(commands).unwrap();
            assert!(engine.project().layers.iter().all(|l| l
                .transform
                .position
                .keys
                .iter()
                .all(|k| k.frame < 0 && !reverse)));
            assert!(engine.undo().unwrap());
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
        }
    }
}
