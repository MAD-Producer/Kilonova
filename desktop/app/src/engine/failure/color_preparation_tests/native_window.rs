//! Actual attached worker errors use the production file/stderr sink at Info.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;
use winit::platform::windows::EventLoopBuilderExtWindows;

#[test]
#[ignore = "requires an actual Windows surface, GPU and pinned native OCIO runtime"]
fn managed_window_source_failure_preserves_project_and_default_error_channels() {
    const CHILD: &str = "MOTION_OCIO_WINDOW_LOG_CHILD";
    const TEST: &str = "engine::failure::color_preparation_tests::native_authored::window::managed_window_source_failure_preserves_project_and_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let active = root.join("active");
        fs::create_dir_all(active.join("assets")).unwrap();
        let mut managed = project(&active);
        managed.layers[0].three_d = false;
        managed.layers[0].content = motion_core::Content::Image { asset: 1 };
        managed.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/window.png".into(),
            width: 2,
            height: 2,
        });
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut managed.color_management)
        else {
            unreachable!()
        };
        settings.display.transfer = motion_core::color_management::ColorTransfer::Srgb;
        let image = active.join("assets/window.png");
        let write_image = || {
            image::RgbaImage::from_pixel(2, 2, image::Rgba([128, 64, 32, 255]))
                .save(&image)
                .unwrap()
        };
        write_image();
        managed.validate().unwrap();
        motion_core::storage::save(&active, &managed).unwrap();
        let platform = Arc::new(RuntimePlatform {
            manifest: PathBuf::from(std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap()),
            before_manifest: None,
            desktop: motion_desktop_media::platform(),
        });
        let engine = Engine::start(active.clone(), platform, 8 << 30).unwrap();
        let events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
                )
                .unwrap(),
        );
        engine
            .attach_async(window, 64, 64)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let render = |frame| {
            let until = Instant::now() + Duration::from_secs(30);
            while !engine.render(frame).unwrap() {
                assert!(Instant::now() < until, "managed worker did not render");
            }
        };
        render(0.25);
        let ui = || crate::engine::UiFrame {
            views: None,
            paint: Default::default(),
            atlas: Some(crate::engine::AtlasUpload {
                pixels: vec![255; 4],
                width: 1,
                height: 1,
            }),
            logical_size: [64.; 2],
            composition: Some([0., 0., 64., 64.]),
            composition_image: None,
            inspection: Default::default(),
            window: 0,
            node_preview: None,
        };
        let present_in = |owner| {
            let until = Instant::now() + Duration::from_secs(30);
            loop {
                let mut frame=ui();
                frame.window=owner;
                if engine
                    .present_async(frame)
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap()
                    .unwrap()
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "managed desktop Present did not complete"
                );
            }
        };
        let present=||present_in(0);
        present();
        let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
        present();
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            attempts,
            "ordinary retained Present must preserve the managed owner"
        );
        engine.seek(1.25).unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        while !engine.present_frozen_async(ui()).unwrap().recv_timeout(Duration::from_secs(30)).unwrap().unwrap() {
            assert!(Instant::now() < until, "managed frozen Present did not complete");
        }
        assert_eq!(engine.state().unwrap()["frame"], 1.25);
        assert_eq!(engine.preview_info().unwrap()["video"]["renderAttempts"], attempts,
            "frozen Present must not sample the advanced clock");
        present();
        assert_eq!(engine.preview_info().unwrap()["video"]["renderAttempts"].as_u64().unwrap(),
            attempts.as_u64().unwrap()+1,
            "ordinary Present must render the new clock after a frozen repaint");
        #[allow(deprecated)]
        let floating=Arc::new(events.create_window(
            winit::window::Window::default_attributes().with_visible(false)
                .with_inner_size(winit::dpi::PhysicalSize::new(64,64)),
        ).unwrap());
        engine.float_window(7,floating,64,64).unwrap();
        let owner_attempts=engine.preview_info().unwrap()["video"]["renderAttempts"].as_u64().unwrap();
        present_in(7);
        assert_eq!(engine.preview_info().unwrap()["video"]["renderAttempts"],owner_attempts+1,
            "another window must not inherit the main window's managed owner");
        present_in(7);
        assert_eq!(engine.preview_info().unwrap()["video"]["renderAttempts"],owner_attempts+1,
            "the floating window must reuse its own published group");
        present();
        assert_eq!(engine.preview_info().unwrap()["video"]["renderAttempts"],owner_attempts+2,
            "returning to the main window must restore the correct owner");
        engine.close_window(7).unwrap();
        let before = engine.state().unwrap();
        fs::remove_file(&image).unwrap();
        let mut changed = managed.color_management.clone();
        let motion_core::color_management::ProjectColorManagement::Ocio { settings } =
            Arc::make_mut(&mut changed)
        else {
            unreachable!()
        };
        settings.output.alpha = if settings.output.alpha == AlphaMode::Straight {
            AlphaMode::Premultiplied
        } else {
            AlphaMode::Straight
        };
        let command = serde_json::to_string(&motion_core::Command::SetColorManagement {
            settings: changed.clone(),
        })
        .unwrap();
        let error = engine.edit(command.clone()).unwrap_err();
        assert!(
            matches!(
                error.host(),
                Some(motion_host::Error::ColorPreparation(
                    prep::Error::SurfaceDisplay(_)
                ))
            ),
            "{error:?}"
        );
        let cause = crate::engine::error::command_cause(error.host().unwrap());
        assert!(cause.contains("window.png"), "{cause}");
        let after = engine.state().unwrap();
        for key in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(
                after[key], before[key],
                "failed source preparation changed {key}"
            );
        }
        assert!(!engine.failed());
        // The old typed image remains usable after the underlying file disappears.
        render(0.5);
        present();
        present();
        let capture_path=root.join("unqualified-capture.png");
        let capture_error=engine.capture_ui(ui(),capture_path.clone()).unwrap_err();
        assert!(capture_error.to_string().contains("phase=surface_capture"),"{capture_error}");
        assert!(!capture_path.exists(),"unqualified capture wrote a legacy image");
        let mut unqualified = ui();
        unqualified.inspection.channel = motion_render::ViewChannel::Alpha;
        let error = engine
            .present_async(unqualified)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        assert!(
            error.to_string().contains("phase=surface_inspection"),
            "{error}"
        );
        present();
        write_image();
        engine.edit(command).unwrap();
        render(0.75);
        assert_eq!(
            engine.state().unwrap()["project"]["color_management"],
            serde_json::to_value(changed).unwrap()
        );
        assert!(!engine.failed());
        drop(engine);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .stdout(fs::File::create(&stdout_path).unwrap())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(120) {
            let killed = child.kill();
            let joined = child.wait();
            panic!("managed window child exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let evidence = PathBuf::from(evidence).join("managed-window-worker");
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("motion-studio.log"), &file).unwrap();
        fs::write(evidence.join("child.stdout"), &stdout).unwrap();
        fs::write(evidence.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}\n{file}");
    assert!(file.contains("logging initialized level=INFO"), "{file}");
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(" ERROR ") && line.contains("window.png"))
        .collect();
    assert_eq!(errors.len(), 4, "source, image-set, surface and worker must each preserve the original cause: {file}");
    let preparation = errors
        .iter()
        .find(|line| line.contains("operation=prepare_surface_display"))
        .unwrap();
    for context in [
        "request=",
        "phase=surface_display",
        "elapsed_ms=",
        "window.png",
    ] {
        assert!(preparation.contains(context), "{preparation}");
    }
    assert!(
        errors.iter().any(|line| line.contains(" edit ")),
        "worker request context missing: {errors:?}"
    );
    for line in &errors {
        let message = line.split_once(": ").unwrap().1;
        assert!(
            stderr.contains(message),
            "original Error absent from stderr: {line}\n{stderr}"
        );
        assert!(
            !stdout.contains(message),
            "Error leaked to stdout: {stdout}"
        );
    }
    assert!(
        file.contains("operation=prepare_surface_display phase=start"),
        "{file}"
    );
    assert!(!stderr.contains("operation=prepare_surface_display phase=start"));
    assert!(!stdout.contains("operation=prepare_surface_display"));
    assert!(!file.contains("phase=abandoned"));
    assert!(
        !file.lines().any(|line| line.contains(" INFO ")
            && (line.contains(" present ")
                || line.contains(" present_poll ")
                || line.contains(" present_frozen "))),
        "healthy Present must remain quiet: {file}"
    );
    let inspection = file
        .lines()
        .find(|line| line.contains(" ERROR ") && line.contains("phase=surface_inspection"))
        .unwrap();
    assert!(stderr.contains(inspection.split_once(": ").unwrap().1));
    assert!(!stdout.contains("phase=surface_inspection"));
    let capture=file.lines().find(|line|line.contains(" ERROR ")&&line.contains("phase=surface_capture")).unwrap();
    assert!(stderr.contains(capture.split_once(": ").unwrap().1));
    assert!(!stdout.contains("phase=surface_capture"));
}
