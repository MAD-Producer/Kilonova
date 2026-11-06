//! Actual modal input and attached-surface pixels, without OS input injection.
use crate::*;
use motion_ui::input::Modifiers;
use winit::platform::windows::EventLoopBuilderExtWindows;

const SIZE: [f32; 2] = [1280., 900.];

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((motion_ui::MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(shell: &mut Shell, input: Input) -> PaintList {
    shell.input = input;
    let (paint, actions) = shell.draw_commands(SIZE);
    shell.input = Input::default();
    assert!(actions.is_empty(), "modal input escaped to the workspace");
    paint
}
fn capture(shell: &mut Shell, events: &mut EventLoop<()>, paint: PaintList, path: PathBuf) {
    let before = shell.engine.state().unwrap();
    let mut ui = shell.ui_frame(paint, SIZE, true);
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap().into_rgba8();
    assert_eq!(image.dimensions(), (1280, 900));
    assert!(
        image
            .pixels()
            .map(|p| p.0)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 16
    );
    let after = shell.engine.state().unwrap();
    for field in ["frame", "revision", "project", "canUndo", "canRedo"] {
        assert_eq!(before[field], after[field], "capture changed {field}");
    }
}
fn draft_rate(shell: &mut Shell, preserve_frames: bool) {
    // Real field focus/text and real dropdown navigation. These fixture points
    // target the existing 640 x 510 composition modal at this fixed viewport.
    draw(shell, click([500., 499.]));
    draw(
        shell,
        Input {
            text: "30000/1001".into(),
            ..Default::default()
        },
    );
    draw(shell, click([780., 499.]));
    if preserve_frames {
        draw(shell, key(Key::Down));
    }
    draw(shell, key(Key::Enter));
    assert!(shell.dialog.is_some(), "selecting a policy also applied it");
}

#[test]
fn rational_rate_settings_native_dropdown_cancel_apply_and_history() {
    const CHILD: &str = "MOTION_RATIONAL_SETTINGS_UI_CHILD";
    const NAME: &str = "shell::rational_clock_native_tests::settings::rational_rate_settings_native_dropdown_cancel_apply_and_history";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child={} code={:?}\n{}\n{}",
            output.status,
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_RATIONAL_SETTINGS_UI_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_path_buf());
    std::fs::create_dir_all(&root).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut records = Vec::new();
    for (locale, language) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
        let old_rate = motion_core::FrameRate::new(24_000, 1001).unwrap();
        let new_rate = motion_core::FrameRate::new(30_000, 1001).unwrap();
        let mut project =
            motion_core::Project::new_with_frame_rate(640, 360, old_rate, 240).unwrap();
        let mut layer = motion_core::Layer::solid(
            7,
            "Clock settings study",
            [64.; 2],
            [160., 90., 0.],
            [0.2, 0.5, 0.8, 1.],
        );
        for frame in [0, 24, 96] {
            layer
                .transform
                .position
                .upsert(
                    frame,
                    [160. + frame as f32, 90., 0.],
                    motion_core::Ease::Linear,
                )
                .unwrap();
        }
        project.layers.push(layer);
        let directory = root.join(language);
        motion_core::storage::save(&directory, &project).unwrap();
        let mut shell = Shell::new(directory.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 900)),
                )
                .unwrap(),
        );
        let pending = shell
            .engine
            .attach_async(window.clone(), 1280, 900)
            .unwrap();
        shell.state =
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
                .unwrap()
                .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        for preserve_frames in [true, false] {
            let before = shell.engine.state().unwrap();
            let saved = std::fs::read(directory.join("project.json")).unwrap();
            let policy = if preserve_frames { "frames" } else { "seconds" };
            shell
                .action(panels::Action::CompositionSettings, None)
                .unwrap();
            let paint = draw(&mut shell, click([780., 499.]));
            capture(
                &mut shell,
                &mut events,
                paint,
                root.join(format!("{language}-{policy}-options.png")),
            );
            draw(&mut shell, key(Key::Escape)); // close the dropdown, retain the modal
            assert!(shell.dialog.is_some());
            draft_rate(&mut shell, preserve_frames);
            let paint = draw(&mut shell, Input::default());
            capture(
                &mut shell,
                &mut events,
                paint,
                root.join(format!("{language}-{policy}-draft.png")),
            );
            draw(&mut shell, click([790., 666.])); // cancel the actual modal
            assert!(shell.dialog.is_none());
            let cancelled = shell.engine.state().unwrap();
            for field in ["project", "revision", "canUndo", "canRedo"] {
                assert_eq!(cancelled[field], before[field]);
            }
            assert_eq!(
                std::fs::read(directory.join("project.json")).unwrap(),
                saved
            );
            shell
                .action(panels::Action::CompositionSettings, None)
                .unwrap();
            draft_rate(&mut shell, preserve_frames);
            draw(&mut shell, click([885., 666.])); // apply through the real event router
            if !preserve_frames {
                // Keeping 240 frames while retiming the implicit full-length
                // clip to 300 frames is an explicit shortening request. The
                // existing reject policy must preserve both history and data.
                assert!(shell
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.error.is_some()));
                let rejected = shell.engine.state().unwrap();
                for field in ["project", "revision", "canUndo", "canRedo"] {
                    assert_eq!(rejected[field], cancelled[field]);
                }
                let paint = draw(&mut shell, Input::default());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    root.join(format!("{language}-shortening-rejected.png")),
                );
                // Request the matching total duration through the actual field,
                // retaining the rejection and all success assertions below.
                draw(&mut shell, click([500., 571.]));
                draw(
                    &mut shell,
                    Input {
                        text: "300".into(),
                        ..Default::default()
                    },
                );
                let paint = draw(&mut shell, Input::default());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    root.join(format!("{language}-seconds-explicit-duration.png")),
                );
                draw(&mut shell, click([885., 666.]));
            }
            assert!(shell.dialog.is_none());
            let applied = shell.engine.state().unwrap();
            assert_eq!(
                applied["revision"].as_u64().unwrap(),
                cancelled["revision"].as_u64().unwrap() + 1
            );
            let actual: motion_core::Project =
                serde_json::from_value(applied["project"].clone()).unwrap();
            assert_eq!(actual.fps, new_rate);
            assert_eq!(actual.frames, if preserve_frames { 240 } else { 300 });
            let expected_frames = if preserve_frames {
                [0, 24, 96]
            } else {
                [0, 30, 120]
            };
            assert_eq!(
                actual.layers[0]
                    .transform
                    .position
                    .keys
                    .iter()
                    .map(|key| key.frame)
                    .collect::<Vec<_>>(),
                expected_frames
            );
            for (actual, original) in actual.layers[0]
                .transform
                .position
                .keys
                .iter()
                .zip(&project.layers[0].transform.position.keys)
            {
                assert_eq!(actual.value, original.value);
                assert_eq!(actual.ease, original.ease);
            }
            shell.engine.save().unwrap();
            assert_eq!(motion_core::storage::load(&directory).unwrap(), actual);
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"], before["project"]);
            assert_eq!(shell.state["canUndo"], false);
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(shell.state["project"], applied["project"]);
            shell.state = shell.engine.history(0).unwrap();
            shell.engine.save().unwrap();
            // Undo advances the revision while restoring the authored content.
            records.push(serde_json::json!({"locale":language,"policy":policy,"rate":new_rate,
                "keyFrames":expected_frames,"cancelUnchanged":true,"atomicHistory":true,"savedProjectMatches":true,
                "explicitMatchingDuration":!preserve_frames,"shorteningRejectedUnchanged":!preserve_frames}));
        }
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
}
