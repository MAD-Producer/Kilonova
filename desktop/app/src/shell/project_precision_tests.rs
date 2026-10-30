//! Actual menu/dialog routing, history, persistence and native GPU screenshots.
//! Pixel ratios are controlled rendering inputs, not changes to OS DPI settings.
use crate::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_ui::input::Modifiers;
use motion_ui::theme::metrics;
use winit::platform::windows::EventLoopBuilderExtWindows;

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
fn draw(shell: &mut Shell, size: [f32; 2], input: Input) -> (PaintList, Vec<panels::Action>) {
    shell.input = input;
    let output = shell.draw_commands(size);
    shell.input = Input::default();
    output
}
fn neutral(shell: &mut Shell, size: [f32; 2], input: Input) -> PaintList {
    let (paint, actions) = draw(shell, size, input);
    assert!(actions.is_empty(), "dialog input escaped to a panel action");
    paint
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    path: PathBuf,
) {
    let before = shell.engine.state().unwrap();
    let mut ui = shell.ui_frame(paint, size, true);
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(&path).unwrap().into_rgba8();
    assert_eq!(
        image.dimensions(),
        (
            (size[0] * shell.scale) as u32,
            (size[1] * shell.scale) as u32
        )
    );
    let first = image.get_pixel(0, 0);
    assert!(image.pixels().any(|p| p != first), "blank UI capture");
    let after = shell.engine.state().unwrap();
    for field in ["revision", "project", "canUndo", "canRedo"] {
        assert_eq!(before[field], after[field], "capture changed {field}");
    }
}
fn present(shell: &Shell, events: &mut EventLoop<()>, size: [f32; 2]) {
    let began = Instant::now();
    loop {
        let ui = UiFrame {
            paint: PaintList::default(),
            atlas: Some(AtlasUpload {
                pixels: vec![255; 4],
                width: 1,
                height: 1,
            }),
            logical_size: size,
            composition: Some([0., 0., 64., 64.]),
            inspection: Default::default(),
            composition_image: None,
            node_preview: None,
            window: 0,
        };
        let pending = shell.engine.present_async(ui).unwrap();
        if crate::native_test_support::receive(events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap()
        {
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "requested precision never rendered"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn project_precision_menu_apply_cancel_history_and_gpu_screens_cover_locales_sizes_and_ratios() {
    const NAME: &str = "shell::project_precision_tests::project_precision_menu_apply_cancel_history_and_gpu_screens_cover_locales_sizes_and_ratios";
    const CHILD: &str = "MOTION_PROJECT_PRECISION_UI_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
            std::fs::create_dir_all(&destination).unwrap();
            for file in std::fs::read_dir(root.path()).unwrap() {
                let file = file.unwrap();
                if file.file_type().unwrap().is_file() {
                    std::fs::copy(
                        file.path(),
                        PathBuf::from(&destination).join(file.file_name()),
                    )
                    .unwrap();
                }
            }
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("logging initialized"));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut records = Vec::new();
    for (locale, language) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
        let directory = root.join(language);
        let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        project.name = "Precision UI probe".into();
        project.background = [0.04, 0.06, 0.08, 1.];
        project.layers.push(motion_core::Layer::solid(
            1,
            "Independent color probe",
            [32.; 2],
            [32., 32., 0.],
            [0.1, 0.5, 0.8, 1.],
        ));
        motion_core::storage::save(&directory, &project).unwrap();
        let mut shell = Shell::new(directory.clone(), None, Some(locale)).unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
                )
                .unwrap(),
        );
        let pending = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
        shell.state =
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
                .unwrap()
                .unwrap();
        shell.window = Some(window.clone());
        shell.attached = true;
        for size in [[900., 600.], [1600., 1000.]] {
            for ratio in [1., 1.5, 2.] {
                shell.scale = ratio;
                let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
                let _ = window
                    .request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
                let pending = shell
                    .engine
                    .resize_async(0, physical[0], physical[1])
                    .unwrap();
                shell.state = crate::native_test_support::receive(
                    &mut events,
                    &pending,
                    Duration::from_secs(60),
                )
                .unwrap()
                .unwrap();
                present(&shell, &mut events, size);
                let prefix = format!(
                    "precision-{language}-{}x{}-ratio{ratio}",
                    size[0] as u32, size[1] as u32
                );
                let before = shell.engine.state().unwrap();
                assert_eq!(before["project"]["working_precision"], "unorm8");
                let file = [20., metrics::TITLE_BAR + metrics::MENU_BAR * 0.5];
                let paint = neutral(&mut shell, size, click(file));
                assert_eq!(shell.panels.menu, Some(0));
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    root.join(format!("{prefix}-file.png")),
                );
                // Existing flat File menu: the new entry follows Export Package.
                let entry = [64., metrics::TITLE_BAR + metrics::MENU_BAR + 4. + 4.5 * 32.];
                let (_, mut actions) = draw(&mut shell, size, click(entry));
                assert_eq!(actions.len(), 1);
                assert!(matches!(
                    actions[0],
                    panels::Action::ProjectPrecisionSettings
                ));
                shell.action(actions.remove(0), None).unwrap();
                assert!(shell.dialog.is_some() && shell.panels.menu.is_none());
                let paint = neutral(&mut shell, size, Input::default());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    root.join(format!("{prefix}-dialog.png")),
                );
                let paint = neutral(&mut shell, size, key(Key::Space));
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    root.join(format!("{prefix}-options.png")),
                );
                for _ in 0..3 {
                    neutral(&mut shell, size, key(Key::Down));
                }
                let paint = neutral(&mut shell, size, key(Key::Enter));
                assert!(shell.dialog.is_some(), "selection also applied");
                assert_eq!(
                    shell.engine.state().unwrap()["revision"],
                    before["revision"]
                );
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    root.join(format!("{prefix}-float32-draft.png")),
                );
                neutral(&mut shell, size, key(Key::Escape));
                assert!(shell.dialog.is_none());
                assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
                shell
                    .action(panels::Action::ProjectPrecisionSettings, None)
                    .unwrap();
                neutral(&mut shell, size, key(Key::Space));
                for _ in 0..3 {
                    neutral(&mut shell, size, key(Key::Down));
                }
                neutral(&mut shell, size, key(Key::Enter));
                neutral(&mut shell, size, key(Key::Enter));
                assert!(shell.dialog.is_none());
                let applied = shell.engine.state().unwrap();
                assert_eq!(
                    applied["revision"].as_u64().unwrap(),
                    before["revision"].as_u64().unwrap() + 1
                );
                let mut expected = before["project"].clone();
                expected["working_precision"] = serde_json::json!(ChannelPrecision::Float32);
                assert_eq!(applied["project"], expected);
                shell.engine.save().unwrap();
                shell.state = shell.engine.state().unwrap();
                assert_eq!(
                    motion_core::storage::load(&directory)
                        .unwrap()
                        .working_precision,
                    ChannelPrecision::Float32
                );
                present(&shell, &mut events, size);
                let ledger = shell
                    .engine
                    .plugin(r#"{"op":"resource_ledger"}"#.into())
                    .unwrap();
                assert!(
                    ledger["resources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["format"] == "Rgba32Float")
                );
                shell
                    .action(panels::Action::ProjectPrecisionSettings, None)
                    .unwrap();
                let paint = neutral(&mut shell, size, Input::default());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    root.join(format!("{prefix}-float32-saved.png")),
                );
                neutral(&mut shell, size, key(Key::Escape));
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before["project"]);
                assert_eq!(shell.state["canUndo"], false);
                shell.state = shell.engine.history(1).unwrap();
                assert_eq!(shell.state["project"], expected);
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before["project"]);
                records.push(
                    serde_json::json!({"locale":language,"logicalSize":size,"pixelRatio":ratio,
                    "physicalSize":physical,"requestedAndRendered":"float32","historySteps":1,
                    "cancelKeptProject":true,"savedPrecision":"float32","screenshotPrefix":prefix}),
                );
            }
        }
        drop(shell);
    }
    std::fs::write(
        root.join("precision-ui-report.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
    logging::flush();
}
