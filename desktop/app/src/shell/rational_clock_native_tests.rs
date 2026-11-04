//! Native readback verifies actual chrome independently of desktop screen capture.
use crate::*;
use winit::platform::windows::EventLoopBuilderExtWindows;

#[test]
fn fractional_project_clocks_render_native_readouts_in_both_locales() {
    const CHILD: &str = "MOTION_RATIONAL_CLOCK_UI_CHILD";
    const NAME: &str = "shell::rational_clock_native_tests::fractional_project_clocks_render_native_readouts_in_both_locales";
    // winit permits one event loop per process, even after it is dropped.
    // Other native tests may already have consumed the parent process's loop.
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_RATIONAL_CLOCK_UI_ARTIFACTS")
        .map(PathBuf::from).unwrap_or_else(|| temporary.path().to_path_buf());
    std::fs::create_dir_all(&root).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut records = Vec::new();
    for (numerator, frame) in [(30_000, 1800.75), (24_000, 1440.75)] {
        for (locale, language) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
            let rate = motion_core::FrameRate::new(numerator, 1001).unwrap();
            let name = format!("{numerator}-1001-{language}");
            let directory = root.join(&name);
            let project = motion_core::Project::new_with_frame_rate(640, 360, rate, 3600).unwrap();
            motion_core::storage::save(&directory, &project).unwrap();
            let saved = std::fs::read(directory.join("project.json")).unwrap();
            let mut shell = Shell::new(directory.clone(), None, Some(locale)).unwrap();
            #[allow(deprecated)]
            let window = Arc::new(events.create_window(Window::default_attributes()
                .with_visible(false)
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 900))).unwrap());
            let pending = shell.engine.attach_async(window.clone(), 1280, 900).unwrap();
            shell.state = crate::native_test_support::receive(
                &mut events, &pending, Duration::from_secs(60)).unwrap().unwrap();
            shell.window = Some(window);
            shell.attached = true;
            shell.state = shell.engine.seek(frame).unwrap();
            assert_eq!(shell.state["frame"], frame);
            assert_eq!(shell.state["project"]["fps"], serde_json::to_value(rate).unwrap());
            let before = shell.engine.state().unwrap();
            let (paint, actions) = shell.draw_commands([1280., 900.]);
            assert!(actions.is_empty(), "a readout capture emitted editing actions");
            let mut ui = shell.ui_frame(paint, [1280., 900.], true);
            ui.node_preview = None;
            let path = root.join(format!("{name}.png"));
            crate::native_test_support::capture_ui(
                &mut events, &shell.engine, ui, path.clone()).unwrap();
            let image = image::open(&path).unwrap().into_rgba8();
            assert_eq!(image.dimensions(), (1280, 900));
            let colors = image.pixels().map(|pixel| pixel.0)
                .collect::<std::collections::HashSet<_>>();
            assert!(colors.len() > 16, "native readback contains no readable chrome");
            let after = shell.engine.state().unwrap();
            for field in ["frame", "revision", "project", "canUndo", "canRedo"] {
                assert_eq!(before[field], after[field], "capture changed {field}");
            }
            assert_eq!(std::fs::read(directory.join("project.json")).unwrap(), saved);
            records.push(serde_json::json!({"name":name,"frame":frame,"fps":rate,
                "expectedNominalTimecode":"00:01:00:00","screenshot":path,
                "uniqueColors":colors.len(),"documentUnchanged":true,
                "visualInspection":"pending"}));
        }
    }
    assert_eq!(records.len(), 4);
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&records).unwrap()).unwrap();
}
