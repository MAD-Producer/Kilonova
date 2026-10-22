use super::*;
use crate::engine::Engine;
use motion_ui::input::{Key, Modifiers};
#[cfg(target_os = "windows")]
mod native;

fn edit(engine: &Engine, command: Command) -> Value {
    engine
        .edit(serde_json::to_string(&command).unwrap())
        .unwrap()
}
fn run(engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => {
                edit(engine, command);
            }
            Action::EditBatch(commands) => {
                engine
                    .edit(serde_json::to_string(&commands).unwrap())
                    .unwrap();
            }
            Action::History(op) => {
                engine.history(op).unwrap();
            }
            Action::Seek(frame) => {
                engine.seek(frame).unwrap();
            }
            other => panic!("unexpected action {other:?}"),
        }
    }
}
fn fixture(root: &std::path::Path, legacy: bool) -> (Engine, u64) {
    let mut project = motion_core::Project::new(1920, 1080, 30, 100).unwrap();
    project.camera = motion_core::Camera::new(1920, 1080);
    motion_core::storage::save(root, &project).unwrap();
    let engine = Engine::start(root.into(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let object = if legacy {
        0
    } else {
        edit(&engine, Command::CreateCamera)["project"]["layers"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    (engine, object)
}
fn draw(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    object: u64,
    input: &Input,
    locale: Locale,
) -> Vec<Action> {
    let mut out = vec![];
    controls.render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 360., 760.),
        &mut 0.,
        state,
        object,
        input,
        locale,
        &mut Default::default(),
        &mut 0,
        &mut out,
    );
    out
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn press(point: [f32; 2], button: MouseButton) -> Input {
    Input {
        mouse: point,
        pressed: Some((button, point)),
        held: Some(button),
        ..Default::default()
    }
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn rect(controls: &Controls, property: Property, axis: usize) -> Rect {
    controls
        .rects
        .iter()
        .find(|(p, a, _)| *p == property && *a == axis)
        .unwrap()
        .2
}
fn open_reset(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    object: u64,
    property: Property,
    axis: usize,
    locale: Locale,
) {
    assert!(draw(controls, atlas, state, object, &Input::default(), locale).is_empty());
    let point = center(rect(controls, property, axis));
    assert!(draw(
        controls,
        atlas,
        state,
        object,
        &press(point, MouseButton::Right),
        locale
    )
    .is_empty());
    assert!(controls.reset.open);
}

#[test]
fn reset_is_one_undoable_value_edit_preserving_animation_and_other_channels_after_reopen() {
    let folder = tempfile::tempdir().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for legacy in [false, true] {
        let root = folder.path().join(format!("legacy-{legacy}"));
        let (engine, object) = fixture(&root, legacy);
        for (property, value) in [
            (Property::Position, [1060., 600., -1400.]),
            (Property::Target, [1100., 700., 30.]),
        ] {
            edit(
                &engine,
                Command::SetVector {
                    object,
                    property,
                    frame: 0,
                    value,
                },
            );
            edit(
                &engine,
                Command::Animate {
                    object,
                    property,
                    axis: None,
                    frame: 0,
                    enabled: true,
                },
            );
            edit(
                &engine,
                Command::SetVector {
                    object,
                    property,
                    frame: 20,
                    value: if property == Property::Position {
                        [1200., 800., -1200.]
                    } else {
                        [1300., 900., 50.]
                    },
                },
            );
        }
        for property in [
            Property::Fov,
            Property::Roll,
            Property::Radius,
            Property::Azimuth,
            Property::Elevation,
        ] {
            edit(
                &engine,
                Command::Animate {
                    object,
                    property,
                    axis: None,
                    frame: 0,
                    enabled: true,
                },
            );
            edit(
                &engine,
                Command::SetScalar {
                    object,
                    property,
                    frame: 20,
                    value: if property == Property::Radius {
                        500.
                    } else {
                        60.
                    },
                },
            );
        }
        for separated in [false, true] {
            if separated {
                edit(
                    &engine,
                    Command::SeparateDimensions {
                        object,
                        property: Property::Target,
                    },
                );
            }
            for (property, axis, orbit, name) in [
                (Property::Position, 0, false, "position"),
                (Property::Target, 1, false, "target"),
                (Property::Fov, 0, false, "fov"),
                (Property::Roll, 0, false, "roll"),
                (Property::Radius, 0, true, "radius"),
                (Property::Azimuth, 0, true, "azimuth"),
                (Property::Elevation, 0, true, "elevation"),
            ] {
                edit(
                    &engine,
                    Command::CameraMode {
                        object,
                        mode: if orbit {
                            CameraMode::Orbit
                        } else {
                            CameraMode::Position
                        },
                    },
                );
                // Mode conversion legitimately bakes position/orbit tracks.
                // Author a known two-key channel AFTER conversion, then test
                // that resetting a value preserves those authored keys.
                edit(
                    &engine,
                    Command::Animate {
                        object,
                        property,
                        axis: None,
                        frame: 0,
                        enabled: false,
                    },
                );
                if property.is_scalar() {
                    edit(
                        &engine,
                        Command::SetScalar {
                            object,
                            property,
                            frame: 0,
                            value: if property == Property::Radius {
                                1000.
                            } else {
                                20.
                            },
                        },
                    );
                    edit(
                        &engine,
                        Command::Animate {
                            object,
                            property,
                            axis: None,
                            frame: 0,
                            enabled: true,
                        },
                    );
                    edit(
                        &engine,
                        Command::SetScalar {
                            object,
                            property,
                            frame: 20,
                            value: if property == Property::Radius {
                                500.
                            } else {
                                60.
                            },
                        },
                    );
                } else {
                    edit(
                        &engine,
                        Command::SetVector {
                            object,
                            property,
                            frame: 0,
                            value: if property == Property::Position {
                                [1060., 600., -1400.]
                            } else {
                                [1100., 700., 30.]
                            },
                        },
                    );
                    edit(
                        &engine,
                        Command::Animate {
                            object,
                            property,
                            axis: None,
                            frame: 0,
                            enabled: true,
                        },
                    );
                    edit(
                        &engine,
                        Command::SetVector {
                            object,
                            property,
                            frame: 20,
                            value: if property == Property::Position {
                                [1200., 800., -1200.]
                            } else {
                                [1300., 900., 50.]
                            },
                        },
                    );
                }
                let before = engine.seek(10.).unwrap();
                let mut controls = Controls::default();
                open_reset(
                    &mut controls,
                    &mut atlas,
                    &before,
                    object,
                    property,
                    axis,
                    Locale::En,
                );
                let actions = draw(
                    &mut controls,
                    &mut atlas,
                    &before,
                    object,
                    &key(Key::Enter),
                    Locale::En,
                );
                assert!(matches!(actions.as_slice(), [Action::Edit(_)]));
                run(&engine, actions);
                let after = engine.state().unwrap();
                let expected = values::default_value(&before, property, axis).unwrap();
                let value =
                    values::sampled_value(sample(&after, object), property, name, axis).unwrap();
                assert!(
                    (value - f64::from(expected)).abs() < 1e-3,
                    "{property:?} {value} {expected}"
                );
                let mut previous = camera(&before, object).clone();
                previous[name] = camera(&after, object)[name].clone();
                assert_eq!(
                    &previous,
                    camera(&after, object),
                    "unrelated camera tracks must survive"
                );
                if property.is_scalar() {
                    let keys = camera(&after, object)[name]["keys"].as_array().unwrap();
                    assert_eq!(keys.len(), 3);
                    assert_eq!(keys[0], camera(&before, object)[name]["keys"][0]);
                    assert_eq!(keys[2], camera(&before, object)[name]["keys"][1]);
                } else {
                    for other in (0..3).filter(|i| *i != axis) {
                        assert_eq!(
                            sample(&after, object)[name][other],
                            sample(&before, object)[name][other]
                        );
                    }
                    if separated && property == Property::Target {
                        for other in ["x", "z"] {
                            assert_eq!(
                                camera(&after, object)[name]["axes"][other],
                                camera(&before, object)[name]["axes"][other]
                            );
                        }
                    } else {
                        assert_eq!(
                            camera(&after, object)[name]["keys"][0],
                            camera(&before, object)[name]["keys"][0]
                        );
                        assert_eq!(
                            camera(&after, object)[name]["keys"][2],
                            camera(&before, object)[name]["keys"][1]
                        );
                    }
                }
                assert_eq!(
                    after["history"]["undoEntries"].as_u64().unwrap(),
                    before["history"]["undoEntries"].as_u64().unwrap() + 1
                );
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
                assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
                engine.save().unwrap();
                let saved = motion_core::storage::load(&root).unwrap();
                assert_eq!(serde_json::to_value(saved).unwrap(), after["project"]);
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            }
        }
        engine.save().unwrap();
        let saved = engine.state().unwrap()["project"].clone();
        drop(engine);
        let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], saved);
    }
}

#[test]
fn reset_popup_cancels_on_escape_focus_loss_context_lock_and_subframe_without_editing() {
    let folder = tempfile::tempdir().unwrap();
    let (engine, object) = fixture(&folder.path().join("project"), false);
    let original = engine.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for case in 0..7 {
        let mut controls = Controls::default();
        open_reset(
            &mut controls,
            &mut atlas,
            &original,
            object,
            Property::Fov,
            0,
            Locale::Zh,
        );
        let mut state = original.clone();
        let input = match case {
            0 => key(Key::Escape),
            1 => Input {
                focus_lost: true,
                ..Default::default()
            },
            _ => key(Key::Enter),
        };
        match case {
            2 => state["composition"] = "another".into(),
            3 => state["root"] = "another".into(),
            4 => state["project"]["layers"][0]["locked"] = true.into(),
            5 => state["observing"] = true.into(),
            6 => state["frame"] = 0.5.into(),
            _ => (),
        }
        assert!(
            draw(
                &mut controls,
                &mut atlas,
                &state,
                object,
                &input,
                Locale::Zh
            )
            .is_empty(),
            "case {case}"
        );
        assert!(!controls.editing(), "case {case}");
        let point = center(rect(&controls, Property::Fov, 0));
        if case >= 4 {
            assert!(draw(
                &mut controls,
                &mut atlas,
                &state,
                object,
                &press(point, MouseButton::Left),
                Locale::Zh
            )
            .is_empty());
            assert!(!controls.editing());
        }
    }
    assert_eq!(engine.state().unwrap()["project"], original["project"]);
    // Opening reset during a live drag rolls the gesture back before the
    // later menu action reads the new snapshot and creates its own undo step.
    let before = edit(
        &engine,
        Command::SetScalar {
            object,
            property: Property::Fov,
            frame: 0,
            value: 60.,
        },
    );
    let mut controls = Controls::default();
    draw(
        &mut controls,
        &mut atlas,
        &before,
        object,
        &Input::default(),
        Locale::En,
    );
    let [x, y] = center(rect(&controls, Property::Fov, 0));
    run(
        &engine,
        draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &press([x, y], MouseButton::Left),
            Locale::En,
        ),
    );
    let drag = Input {
        mouse: [x + 20., y],
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    run(
        &engine,
        draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &drag,
            Locale::En,
        ),
    );
    let changed = engine.state().unwrap();
    assert_ne!(changed["project"], before["project"]);
    let actions = draw(
        &mut controls,
        &mut atlas,
        &changed,
        object,
        &press([x, y], MouseButton::Right),
        Locale::En,
    );
    assert!(matches!(actions.as_slice(), [Action::History(4)]));
    run(&engine, actions);
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    let actions = draw(
        &mut controls,
        &mut atlas,
        &engine.state().unwrap(),
        object,
        &key(Key::Enter),
        Locale::En,
    );
    run(&engine, actions);
    let after = engine.state().unwrap();
    assert_eq!(sample(&after, object)["fov"], 45.);
    assert_eq!(
        after["history"]["undoEntries"].as_u64().unwrap(),
        before["history"]["undoEntries"].as_u64().unwrap() + 1
    );
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    let mut near_integer = engine.state().unwrap();
    near_integer["frame"] = serde_json::json!(9.99999999);
    open_reset(
        &mut controls,
        &mut atlas,
        &near_integer,
        object,
        Property::Fov,
        0,
        Locale::En,
    );
    let actions = draw(
        &mut controls,
        &mut atlas,
        &near_integer,
        object,
        &key(Key::Enter),
        Locale::En,
    );
    assert!(
        matches!(
            actions.as_slice(),
            [Action::Edit(Command::SetScalar { frame: 10, .. })]
        ),
        "{actions:?}"
    );
}

#[test]
fn elevation_rejects_poles_and_non_finite_text_and_clamps_drag_with_localized_feedback() {
    let folder = tempfile::tempdir().unwrap();
    let (engine, object) = fixture(&folder.path().join("project"), false);
    edit(
        &engine,
        Command::CameraMode {
            object,
            mode: CameraMode::Orbit,
        },
    );
    let before = engine.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for invalid in ["90", "-90", "NaN", "1e999"] {
            let mut controls = Controls::default();
            draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &Input::default(),
                locale,
            );
            let point = center(rect(&controls, Property::Elevation, 0));
            run(
                &engine,
                draw(
                    &mut controls,
                    &mut atlas,
                    &before,
                    object,
                    &press(point, MouseButton::Left),
                    locale,
                ),
            );
            let release = Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            };
            run(
                &engine,
                draw(&mut controls, &mut atlas, &before, object, &release, locale),
            );
            let mut select_all = key(Key::A);
            select_all.keys[0].1.control = true;
            draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &select_all,
                locale,
            );
            draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &Input {
                    text: invalid.into(),
                    ..Default::default()
                },
                locale,
            );
            assert!(draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &key(Key::Enter),
                locale
            )
            .is_empty());
            let note = &controls.drag_readout.as_ref().unwrap().1;
            assert!(
                note.contains("89")
                    && note.contains(if locale == Locale::Zh {
                        "有限数值"
                    } else {
                        "finite value"
                    }),
                "{note}"
            );
            run(
                &engine,
                draw(
                    &mut controls,
                    &mut atlas,
                    &before,
                    object,
                    &key(Key::Escape),
                    locale,
                ),
            );
            assert_eq!(engine.state().unwrap()["project"], before["project"]);
        }
        let mut controls = Controls::default();
        draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &Input::default(),
            locale,
        );
        let [x, y] = center(rect(&controls, Property::Elevation, 0));
        run(
            &engine,
            draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &press([x, y], MouseButton::Left),
                locale,
            ),
        );
        let drag = Input {
            mouse: [x + 1000., y],
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        run(
            &engine,
            draw(&mut controls, &mut atlas, &before, object, &drag, locale),
        );
        let changed = engine.state().unwrap();
        assert_eq!(sample(&changed, object)["elevation"], 89.);
        assert!(controls.drag_readout.as_ref().unwrap().1.contains("89"));
        run(
            &engine,
            draw(
                &mut controls,
                &mut atlas,
                &changed,
                object,
                &key(Key::Escape),
                locale,
            ),
        );
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
}

#[test]
fn missing_sample_streak_reports_once_at_sixty_and_recovers_at_default_log_level() {
    const CHILD: &str = "MOTION_CAMERA_FIELDS_FAILURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (engine, object) = fixture(&root.join("project"), false);
        let healthy = engine.state().unwrap();
        let mut broken = healthy.clone();
        broken["sampledLayers"][0]["position"][1] = Value::Null;
        let mut controls = Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for count in 1..=120 {
            broken["frame"] = serde_json::json!(count % 30);
            assert!(draw(
                &mut controls,
                &mut atlas,
                &broken,
                object,
                &Input::default(),
                Locale::En
            )
            .is_empty());
            assert_eq!(
                controls.sample_failures,
                if count < 60 { count } else { 61 }
            );
        }
        assert!(!controls.editing());
        draw(
            &mut controls,
            &mut atlas,
            &healthy,
            object,
            &Input::default(),
            Locale::En,
        );
        assert_eq!(controls.sample_failures, 0);
        for _ in 0..60 {
            draw(
                &mut controls,
                &mut atlas,
                &broken,
                object,
                &Input::default(),
                Locale::En,
            );
        }
        log::logger().flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CAMERA_FIELDS_FAILURE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "panels::camera_controls::tests::missing_sample_streak_reports_once_at_sixty_and_recovers_at_default_log_level"])
        .env(CHILD, &root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    for output in [&*file, &*stderr] {
        assert_eq!(
            output
                .lines()
                .filter(|line| line.contains("camera.fields sample unavailable"))
                .count(),
            2,
            "{output}"
        );
        assert!(
            output.contains("occurrence=60")
                && output.contains("property=Position")
                && output.contains("frame=0")
        );
        assert!(!output
            .lines()
            .any(|line| line.contains("camera.fields") && line.contains("occurrence=1")));
    }
    assert!(!stdout.contains("camera.fields") && !stdout.contains("sample unavailable"));
}

#[test]
fn legacy_camera_rails_toggle_and_seek_real_coupled_and_separated_keys() {
    let folder = tempfile::tempdir().unwrap();
    let (engine, object) = fixture(&folder.path().join("project"), true);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for separated in [false, true] {
        if separated {
            edit(
                &engine,
                Command::SeparateDimensions {
                    object,
                    property: Property::Target,
                },
            );
        }
        let axis = separated.then_some(Axis::Y);
        edit(
            &engine,
            Command::Animate {
                object,
                property: Property::Target,
                axis,
                frame: 0,
                enabled: true,
            },
        );
        edit(
            &engine,
            if separated {
                Command::SetComponent {
                    object,
                    property: Property::Target,
                    axis: Axis::Y,
                    frame: 20,
                    value: 600.,
                }
            } else {
                Command::SetVector {
                    object,
                    property: Property::Target,
                    frame: 20,
                    value: [1000., 600., 20.],
                }
            },
        );
        let before = engine.seek(5.).unwrap();
        let mut controls = Controls::default();
        draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &Input::default(),
            Locale::En,
        );
        let y = center(rect(&controls, Property::Target, 1))[1];
        let styles = PaintList::default().styles();
        let rail = |index: usize| {
            press(
                [
                    styles.metric("padding.panel", 8.)
                        + styles.metric("icon.slot", 20.) * (index as f32 + 0.5),
                    y,
                ],
                MouseButton::Left,
            )
        };
        let actions = draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &rail(1),
            Locale::En,
        );
        assert!(
            matches!(actions.as_slice(), [Action::EditBatch(_)]),
            "{actions:?}"
        );
        run(&engine, actions);
        let keyed = engine.state().unwrap();
        let source = if separated {
            &camera(&keyed, object)["target"]["axes"]["y"]
        } else {
            &camera(&keyed, object)["target"]
        };
        assert!(source["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["frame"] == 5));
        run(
            &engine,
            draw(
                &mut controls,
                &mut atlas,
                &keyed,
                object,
                &rail(0),
                Locale::En,
            ),
        );
        assert_eq!(engine.state().unwrap()["frame"], 0.);
        run(
            &engine,
            draw(
                &mut controls,
                &mut atlas,
                &engine.state().unwrap(),
                object,
                &rail(2),
                Locale::En,
            ),
        );
        assert_eq!(engine.state().unwrap()["frame"], 5.);
        run(
            &engine,
            draw(
                &mut controls,
                &mut atlas,
                &engine.state().unwrap(),
                object,
                &rail(1),
                Locale::En,
            ),
        );
        let deleted = engine.state().unwrap();
        let source = if separated {
            &camera(&deleted, object)["target"]["axes"]["y"]
        } else {
            &camera(&deleted, object)["target"]
        };
        assert_eq!(source["keys"].as_array().unwrap().len(), 2);
        assert_eq!(engine.history(0).unwrap()["project"], keyed["project"]);
        engine.history(1).unwrap();
    }
}
