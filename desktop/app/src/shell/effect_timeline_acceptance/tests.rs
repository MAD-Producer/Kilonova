//! A real file-write failure must restore the GPU target and preserve its cause.
use crate::*;
use winit::platform::windows::EventLoopBuilderExtWindows;

#[test]
fn screenshot_write_failure_restores_minimum_surface_and_preserves_project() {
    const CHILD: &str = "MOTION_EFFECT_CAPTURE_FAILURE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "shell::effect_timeline_acceptance::tests::screenshot_write_failure_restores_minimum_surface_and_preserves_project", "--nocapture"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        if let Some(destination) = std::env::var_os("MOTION_EFFECT_CAPTURE_FAILURE_ARTIFACTS") {
            std::fs::create_dir_all(&destination).unwrap();
            for file in std::fs::read_dir(root.path()).unwrap() {
                let file = file.unwrap();
                if file.file_type().unwrap().is_file() {
                    std::fs::copy(file.path(), PathBuf::from(&destination).join(file.file_name())).unwrap();
                }
            }
            std::fs::write(PathBuf::from(&destination).join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(PathBuf::from(&destination).join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        assert!(file.lines().any(|line| line.contains("ERROR") && line.contains("capture_ui")));
        assert!(String::from_utf8_lossy(&output.stderr).lines()
            .any(|line| line.contains("ERROR") && line.contains("capture_ui")));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("logging initialized"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("ERROR"));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = logging::init(root.clone()).unwrap();
    let project = motion_core::Project::new(64, 64, 30, 60).unwrap();
    let directory = root.join("project");
    motion_core::storage::save(&directory, &project).unwrap();
    let mut shell = Shell::new(directory, None, Some(Locale::En)).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(Window::default_attributes()
        .with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(900, 600))).unwrap());
    let reply = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60)).unwrap().unwrap();
    shell.window = Some(window);
    shell.scale = 1.;
    shell.atlas.set_pixel_ratio(1.);
    let before = shell.engine.state().unwrap();
    let report = root.join("report.json");
    let blocked = report.with_extension("effect-timeline-main-parameters.png");
    std::fs::create_dir(&blocked).unwrap();
    let expected = image::save_buffer(&blocked, &[0; 4], 1, 1, image::ColorType::Rgba8).unwrap_err().to_string();
    let mut screens = serde_json::Map::new();
    // Keep the native owner dispatching while the helper uses the serial worker.
    let cause = std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::channel();
        let subject = &mut shell;
        let captures = &mut screens;
        let report = &report;
        let job = scope.spawn(move || {
            let result = subject.capture_effect_timeline_screens([900., 600.], report, captures);
            send.send(result).unwrap();
        });
        let result = crate::native_test_support::receive(&mut events, &receive, Duration::from_secs(60)).unwrap();
        job.join().unwrap();
        result.unwrap_err()
    });
    assert_eq!(cause, expected, "preserve the original file-write failure");
    assert!(screens.is_empty());
    let paint = shell.draw_commands([900., 600.]).0;
    let mut ui = shell.ui_frame(paint, [900., 600.], true);
    ui.node_preview = None;
    let restored = root.join("restored.png");
    let capture = crate::native_test_support::capture_ui(&mut events, &shell.engine, ui, restored.clone()).unwrap();
    assert_eq!((capture["width"].as_u64(), capture["height"].as_u64()), (Some(900), Some(600)));
    assert_eq!(image::open(restored).unwrap().into_rgba8().dimensions(), (900, 600));
    let after = shell.engine.state().unwrap();
    for field in ["project", "revision", "canUndo", "canRedo"] {
        assert_eq!(before[field], after[field], "capture changed {field}");
    }
    std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "originalWriteCausePreserved":true,"restoredCaptureDimensions":[900,600],
        "projectRevisionAndHistoryUntouched":true,"cause":cause
    })).unwrap()).unwrap();
}
