//! Actual GPU feedback and bounded default-level failure-channel acceptance.
use super::*;

#[test]
#[ignore = "explicit native transform snapping screenshots and failure channels"]
fn native_transform_snapping_preserves_history_and_exposes_reached_targets() {
    const CHILD: &str = "MOTION_TRANSFORM_SNAP_NATIVE_CHILD";
    const NAME: &str = "panels::transform_controls::spatial::native_tests::snapping::native_transform_snapping_preserves_history_and_exposes_reached_targets";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TRANSFORM_SNAP_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("MOTION_LOG")
            .env(CHILD, &root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                std::fs::write(root.join("stdout.txt"), output.stdout).unwrap();
                std::fs::write(root.join("stderr.txt"), output.stderr).unwrap();
                panic!("native snapping acceptance exceeded 240 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let logs = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        for text in [
            "site=transform.snap_targets count=60",
            "selected=Some(7)",
            "layer=8 projected point is not a finite coordinate pair",
        ] {
            assert!(logs.contains(text), "file missing {text}");
            assert!(stderr.contains(text), "stderr missing {text}");
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains(text),
                "stdout leaked {text}"
            );
        }
        assert_eq!(
            logs.matches("site=transform.snap_targets count=60").count(),
            1
        );
        assert_eq!(
            stderr
                .matches("site=transform.snap_targets count=60")
                .count(),
            1
        );
        assert!(!stderr.contains("site=transform.snap_targets count=1 "));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let (_folder, engine, _, _) = super::super::super::tests::fixture(false);
    let reference = motion_core::Layer::solid(
        8,
        "Reference",
        [100.; 2],
        [1125.25, 850., 0.],
        [0.4, 0.5, 0.6, 1.],
    );
    let before = engine
        .edit(serde_json::to_string(&Command::Add { layer: reference }).unwrap())
        .unwrap();
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
    let attached = engine
        .attach_async(main, SIZE[0] as u32, SIZE[1] as u32)
        .unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    engine
        .float_window(1, floating, SIZE[0] as u32, SIZE[1] as u32)
        .unwrap();
    let mut report = vec![];
    for window in [0, 1] {
        for tool in [Tool::Move, Tool::Scale] {
            let mut expected = None;
            for locale in [Locale::Zh, Locale::En] {
                let mut panels = Panels {
                    locale,
                    selected: Some(7),
                    ..Default::default()
                };
                panels.transform_controls.tool = tool;
                let mut atlas =
                    TextAtlas::from_family(PaintList::default().styles().font_family(), 12., true)
                        .unwrap();
                let name = format!("{window}-{tool:?}-{locale:?}");
                let image = capture(
                    &mut events,
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &root.join(format!("{name}-before.png")),
                );
                let pose = Pose::new(&before, 7, image).unwrap();
                let target_local = if tool == Tool::Move {
                    [1125.25, 850.]
                } else {
                    [1175.25, 647.625]
                };
                let target = [
                    image.min[0] + target_local[0] as f32 / 1920. * image.width(),
                    image.min[1] + target_local[1] as f32 / 1080. * image.height(),
                ];
                let from = if tool == Tool::Move {
                    pose.pivot
                } else {
                    pose.corners[2]
                };
                let to = if tool == Tool::Move {
                    [target[0] - 2., target[1] + 3.]
                } else {
                    [target[0] - 2., target[1] - 1.]
                };
                for input in [
                    Input {
                        pressed: Some((MouseButton::Left, from)),
                        ..pointer(from)
                    },
                    pointer(to),
                ] {
                    let (_, actions, _) = draw(&engine, &mut panels, &mut atlas, window, &input);
                    dispatch(&mut events, &engine, actions);
                }
                let after = engine.state().unwrap();
                let actual = Pose::new(&after, 7, image).unwrap();
                let reached = if tool == Tool::Move {
                    actual.pivot
                } else {
                    actual.corners[2]
                };
                assert!(
                    distance(reached, target) < 0.08,
                    "{name} reached={reached:?} target={target:?}"
                );
                assert!(panels
                    .transform_controls
                    .drag
                    .as_ref()
                    .unwrap()
                    .snap_feedback
                    .is_some());
                capture(
                    &mut events,
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &root.join(format!("{name}-snapped.png")),
                );
                let (_, actions, _) = draw(
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &Input {
                        mouse: to,
                        released: Some((MouseButton::Left, to)),
                        ..Default::default()
                    },
                );
                dispatch(&mut events, &engine, actions);
                assert!(!panels.transform_controls.gesture_active());
                if let Some(expected) = &expected {
                    assert_eq!(&after["project"], expected);
                } else {
                    expected = Some(after["project"].clone());
                }
                engine.history(0).unwrap();
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
                for alt in [false, true] {
                    for mut input in [
                        Input {
                            pressed: Some((MouseButton::Left, from)),
                            ..pointer(from)
                        },
                        pointer(to),
                    ] {
                        input.modifiers.alt = alt;
                        let (_, actions, _) =
                            draw(&engine, &mut panels, &mut atlas, window, &input);
                        dispatch(&mut events, &engine, actions);
                    }
                    assert_eq!(
                        panels
                            .transform_controls
                            .drag
                            .as_ref()
                            .unwrap()
                            .snap_feedback
                            .is_none(),
                        alt
                    );
                    if alt {
                        capture(
                            &mut events,
                            &engine,
                            &mut panels,
                            &mut atlas,
                            window,
                            &root.join(format!("{name}-alt.png")),
                        );
                    }
                    let (_, actions, _) = draw(
                        &engine,
                        &mut panels,
                        &mut atlas,
                        window,
                        &Input {
                            focus_lost: true,
                            ..Default::default()
                        },
                    );
                    dispatch(&mut events, &engine, actions);
                    assert!(!panels.transform_controls.gesture_active());
                    assert_eq!(engine.state().unwrap()["project"], before["project"]);
                }
                report.push(serde_json::json!({"owner":window,"tool":format!("{tool:?}"),"locale":format!("{locale:?}"),"actual_gpu_capture":true,"reached_target":reached,"release_and_single_undo":true,"alt_bypass":true,"focus_cancel":true}));
            }
        }
    }
    crate::logging::flush();
    assert!(!std::fs::read_to_string(root.join("motion-studio.log"))
        .unwrap()
        .contains("site=transform.snap_targets"));
    let mut malformed = before.clone();
    malformed["projectedLayers"][1]["corners"] = serde_json::json!([[1., null]]);
    let mapping = crate::viewer_navigation::Navigation::default()
        .mapping(Rect::new(0., 0., 1920., 1080.), [1920., 1080.])
        .unwrap();
    for _ in 0..61 {
        assert!(panels::viewer_guides::Controls::default()
            .transform_snapping(
                &malformed,
                mapping,
                [1920., 1080.],
                Some(7),
                &PaintList::default().styles()
            )
            .is_none());
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    crate::logging::flush();
    engine.close_window(1).unwrap();
}
