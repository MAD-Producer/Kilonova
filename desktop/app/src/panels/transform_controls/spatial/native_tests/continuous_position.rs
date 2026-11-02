//! Real window routes, GPU pixels and a second process reopening saved keys.
use super::super::continuous_position_tests::{base, mapped};
use super::*;
use serde_json::json;
const NAME:&str="panels::transform_controls::spatial::native_tests::continuous_position::native_continuous_position_handles_save_reopen_and_undo_in_both_window_owners";
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
#[test]
#[ignore = "explicit Windows GPU, continuous spatial keys, native main/floating owners and process reopen"]
fn native_continuous_position_handles_save_reopen_and_undo_in_both_window_owners() {
    run(
        false,
        NAME,
        "MOTION_CONTINUOUS_POSITION_NATIVE_CHILD",
        "MOTION_CONTINUOUS_POSITION_NATIVE_ARTIFACTS",
    );
}
#[test]
#[ignore = "explicit Windows GPU, free spatial movement, native owners and process reopen"]
fn native_free_position_drag_keeps_stable_keys_in_main_and_floating_owners() {
    run(true, "panels::transform_controls::spatial::native_tests::continuous_position::native_free_position_drag_keeps_stable_keys_in_main_and_floating_owners", "MOTION_FREE_POSITION_NATIVE_CHILD", "MOTION_FREE_POSITION_NATIVE_ARTIFACTS");
}
fn run(free: bool, name: &str, child: &str, artifacts: &str) {
    if let Some(root) = std::env::var_os(child) {
        let root = PathBuf::from(root);
        let reopen = std::env::var_os("MOTION_CONTINUOUS_POSITION_NATIVE_REOPEN").is_some();
        let phase = if reopen { "reopen" } else { "capture" };
        let _logger = crate::logging::init(root.join(format!("{phase}-logs"))).unwrap();
        record(
            root.join(format!("{phase}-process.json")),
            &json!({"pid":std::process::id()}),
        );
        let (base, _) = base();
        let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
        #[allow(deprecated)]
        let main = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(SIZE[0] as u32, SIZE[1] as u32)),
                )
                .unwrap(),
        );
        #[allow(deprecated)]
        let floating = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(SIZE[0] as u32, SIZE[1] as u32)),
                )
                .unwrap(),
        );
        for (window, locale, mode, tag) in [
            (0, Locale::Zh, "forward", "zh-main-forward"),
            (1, Locale::En, "reverse", "en-floating-reverse"),
        ] {
            let folder = root.join(tag);
            std::fs::create_dir_all(&folder).unwrap();
            let (mut before, at) = mapped(base.clone(), mode, true);
            if free && window == 0 {
                before.layers[0].three_d = false;
            }
            if !reopen {
                motion_core::storage::save(&folder.join("project"), &before).unwrap();
            }
            let engine = Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let reply = engine
                .attach_async(main.clone(), SIZE[0] as u32, SIZE[1] as u32)
                .unwrap();
            crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            engine
                .float_window(1, floating.clone(), SIZE[0] as u32, SIZE[1] as u32)
                .unwrap();
            engine.seek(at).unwrap();
            let mut panels = Panels {
                locale,
                focused: Some(panels::Panel::Composition),
                ..Default::default()
            };
            panels.select_only(Some(7));
            let mut atlas =
                TextAtlas::from_family(PaintList::default().styles().font_family(), 12., true)
                    .unwrap();
            let view = capture(
                &mut events,
                &engine,
                &mut panels,
                &mut atlas,
                window,
                &folder.join(if reopen { "reopen.png" } else { "before.png" }),
            );
            if reopen {
                let expected: Value =
                    serde_json::from_slice(&std::fs::read(folder.join("after.json")).unwrap())
                        .unwrap();
                assert_eq!(engine.state().unwrap()["project"], expected);
            } else {
                let initial = engine.state().unwrap();
                record(folder.join("before.json"), &initial["project"]);
                let pose = Pose::new(&initial, 7, view).unwrap();
                let styles = PaintList::default().styles();
                let mut handle_pose = pose.clone();
                handle_pose.three_d = true;
                let frame = Frame::new(&handle_pose, Tool::Move, &styles).unwrap();
                let start = axis(0) * frame.lengths[0].unwrap();
                let from = if free {
                    pose.pivot
                } else {
                    frame.projection.screen3(start).unwrap()
                };
                if !free {
                    assert_eq!(
                        pose.handle(from, Tool::Move, &styles),
                        Some(Handle::Spatial(Constraint::Axis(0), false))
                    );
                } else {
                    assert_eq!(pose.handle(from, Tool::Move, &styles), None);
                }
                let (_, actions, _) = draw(
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &Input {
                        pressed: Some((MouseButton::Left, from)),
                        ..pointer(from)
                    },
                );
                if free {
                    assert!(
                        matches!(actions.as_slice(), [Action::ViewerPick { .. }]),
                        "{actions:?}"
                    );
                    dispatch(&mut events, &engine, actions);
                } else {
                    assert!(actions.is_empty());
                }
                let mut end = from;
                for delta in [40., 80., 120.] {
                    end = if free {
                        [from[0] + delta as f32, from[1] - delta as f32 * 0.5]
                    } else {
                        frame.projection.screen3(start + DVec3::X * delta).unwrap()
                    };
                    assert!(
                        distance(from, end) > styles.metric("motion.drag_threshold", 3.),
                        "native fixture must exceed the production threshold: {from:?} {end:?}"
                    );
                    let mut movement = pointer(end);
                    // Alt is the existing user control for unsnapped translation.
                    movement.modifiers.alt = free;
                    let (_, actions, _) = draw(&engine, &mut panels, &mut atlas, window, &movement);
                    if free {
                        assert!(
                            actions.iter().any(|a| matches!(
                                a,
                                Action::ViewerDrag {
                                    object: 7,
                                    spatial_key: Some(2),
                                    ..
                                }
                            )),
                            "{actions:?}"
                        );
                    } else {
                        assert!(actions.iter().any(|a|matches!(a,Action::EditBatch(commands) if matches!(commands.as_slice(),[Command::TrackAt {edit:motion_core::TrackEdit::SetSpatialKey {key:2,..},..}]))),"{actions:?}");
                    }
                    dispatch(&mut events, &engine, actions);
                }
                let (_, actions, _) = draw(
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &Input {
                        mouse: end,
                        released: Some((MouseButton::Left, end)),
                        modifiers: motion_ui::input::Modifiers {
                            alt: free,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                assert!(matches!(actions.as_slice(), [Action::History(3)]));
                dispatch(&mut events, &engine, actions);
                let after = engine.state().unwrap()["project"].clone();
                let actual: motion_core::Project = serde_json::from_value(after.clone()).unwrap();
                let animation = actual.layers[0]
                    .transform
                    .position
                    .spatial_animation
                    .as_ref()
                    .unwrap();
                assert_eq!(animation.keys().len(), 4);
                assert_eq!(animation.keys()[1].id, 2);
                if free {
                    let origin = super::super::free_position_tests::key_screen(&before, at);
                    let now = super::super::free_position_tests::key_screen(&actual, at);
                    let scale = (view.width() / 1920.).min(view.height() / 1080.);
                    for i in 0..2 {
                        assert!(
                            (now[i] - origin[i] - [120., -60.][i] / scale).abs() < 0.1,
                            "{now:?} {origin:?} scale={scale}"
                        );
                    }
                } else {
                    assert!((animation.keys()[1].value[0] - pose.position[0] - 120.).abs() < 0.03);
                }
                assert_ne!(
                    animation.prepared().keys().nth(1).unwrap().1,
                    before.layers[0]
                        .transform
                        .position
                        .spatial_animation
                        .as_ref()
                        .unwrap()
                        .prepared()
                        .keys()
                        .nth(1)
                        .unwrap()
                        .1
                );
                let mut expected = before.clone();
                expected.layers[0].transform.position = actual.layers[0].transform.position.clone();
                assert_eq!(actual, expected);
                engine.save().unwrap();
                record(folder.join("after.json"), &after);
                capture(
                    &mut events,
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &folder.join("after.png"),
                );
                let reply = engine.history_async(0).unwrap();
                let undo = crate::native_test_support::receive(
                    &mut events,
                    &reply,
                    Duration::from_secs(30),
                )
                .unwrap()
                .unwrap();
                assert_eq!(undo["project"], initial["project"]);
                let reply = engine.history_async(1).unwrap();
                let redo = crate::native_test_support::receive(
                    &mut events,
                    &reply,
                    Duration::from_secs(30),
                )
                .unwrap()
                .unwrap();
                assert_eq!(redo["project"], after);
            }
            engine.close_window(1).unwrap();
            engine.request_shutdown().unwrap();
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os(artifacts)
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    for phase in ["capture", "reopen"] {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            name,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(child, &root)
        .env_remove("MOTION_LOG");
        if phase == "reopen" {
            cmd.env("MOTION_CONTINUOUS_POSITION_NATIVE_REOPEN", "1");
        }
        cmd.stdout(std::fs::File::create(root.join(format!("{phase}-stdout.log"))).unwrap());
        cmd.stderr(std::fs::File::create(root.join(format!("{phase}-stderr.log"))).unwrap());
        let mut child = cmd.spawn().unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(120) {
                let killed = child.kill();
                let waited = child.wait();
                panic!(
                    "native {phase} timeout pid={} kill={killed:?} wait={waited:?}",
                    child.id()
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let stdout = std::fs::read_to_string(root.join(format!("{phase}-stdout.log"))).unwrap();
        let stderr = std::fs::read_to_string(root.join(format!("{phase}-stderr.log"))).unwrap();
        assert!(status.success(), "{stdout}\n{stderr}");
        assert!(!stdout.contains("motion_studio::"));
    }
    let pid = |phase: &str| {
        serde_json::from_slice::<Value>(
            &std::fs::read(root.join(format!("{phase}-process.json"))).unwrap(),
        )
        .unwrap()["pid"]
            .clone()
    };
    assert_ne!(pid("capture"), pid("reopen"));
}
