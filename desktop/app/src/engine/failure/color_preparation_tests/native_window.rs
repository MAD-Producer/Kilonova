//! Actual attached worker errors use the production file/stderr sink at Info.
use super::*;
use crate::engine::failure::color_tests::runtime_platform::RuntimePlatform;
use winit::platform::windows::EventLoopBuilderExtWindows;
#[path = "native_window_roi.rs"]
mod roi;
#[path = "native_window_components.rs"]
mod components;

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
        let motion_core::color_management::ProjectColorManagement::Ocio { settings: declared } =
            managed.color_management.as_ref()
        else {
            unreachable!()
        };
        let config = motion_effects::color_config::VerifiedColorConfig::new(
            declared.manifest.clone(),
            Arc::from(
                fs::read(active.join(&managed.color_management.resources().unwrap()[0].path))
                    .unwrap(),
            ),
            std::collections::BTreeMap::new(),
        )
        .unwrap();
        let native = motion_effects::native_color::NativeColorRuntime::load(std::path::Path::new(
            &std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap(),
        ))
        .unwrap();
        let working = native
            .prepare(
                &config,
                &declared.selection,
                motion_effects::color_processor::ColorProcessorPhase::InputToWorking,
                declared.default_input.alpha,
                declared.working.alpha,
            )
            .unwrap()
            .apply_rgb([128. / 255., 64. / 255., 32. / 255.])
            .unwrap();
        let rgb = native
            .prepare(
                &config,
                &declared.selection,
                motion_effects::color_processor::ColorProcessorPhase::WorkingToDisplay,
                declared.working.alpha,
                declared.working.alpha,
            )
            .unwrap()
            .apply_rgb(working)
            .unwrap();
        let expected = rgb.map(|value| (value.clamp(0., 1.) * 255.).round() as u8);
        fs::write(
            root.join("capture-reference.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "manifest":declared.manifest,"input":[128,64,32,255],"working":working,
                "display":rgb,"expected":expected,
            }))
            .unwrap(),
        )
        .unwrap();
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
                let mut frame = ui();
                frame.window = owner;
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
        let present = || present_in(0);
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
        while !engine
            .present_frozen_async(ui())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap()
        {
            assert!(
                Instant::now() < until,
                "managed frozen Present did not complete"
            );
        }
        assert_eq!(engine.state().unwrap()["frame"], 1.25);
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            attempts,
            "frozen Present must not sample the advanced clock"
        );
        present();
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"]
                .as_u64()
                .unwrap(),
            attempts.as_u64().unwrap() + 1,
            "ordinary Present must render the new clock after a frozen repaint"
        );
        #[allow(deprecated)]
        let floating = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
                )
                .unwrap(),
        );
        engine.float_window(7, floating, 64, 64).unwrap();
        let owner_attempts = engine.preview_info().unwrap()["video"]["renderAttempts"]
            .as_u64()
            .unwrap();
        present_in(7);
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            owner_attempts + 1,
            "another window must not inherit the main window's managed owner"
        );
        present_in(7);
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            owner_attempts + 1,
            "the floating window must reuse its own published group"
        );
        present();
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            owner_attempts + 2,
            "returning to the main window must restore the correct owner"
        );
        engine.close_window(7).unwrap();
        #[allow(deprecated)]
        let capture_window = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(96, 64)),
                )
                .unwrap(),
        );
        engine.float_window(8, capture_window, 96, 64).unwrap();
        let float_ui = || {
            let mut frame = ui();
            frame.window = 8;
            frame.logical_size = [96., 64.];
            frame.composition = Some([0., 0., 96., 64.]);
            frame
        };
        let until = Instant::now() + Duration::from_secs(30);
        while !engine
            .present_async(float_ui())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap()
        {
            assert!(Instant::now() < until, "floating capture did not publish");
        }
        let float_path = root.join("managed-display-floating.png");
        engine.capture_ui(float_ui(), float_path.clone()).unwrap();
        let pixels = image::open(float_path).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (96, 64));
        let pixel = pixels.get_pixel(48, 32).0;
        assert!(
            (0..3).all(|channel| pixel[channel].abs_diff(expected[channel]) <= 1),
            "floating capture used another surface: actual={pixel:?} CPU={expected:?}"
        );
        assert_eq!(pixel[3], 255);
        engine.close_window(8).unwrap();
        present();
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
        let layout = |count| {
            let mut frame = ui();
            if count > 1 {
                frame.views = Some(crate::engine::viewer_frame::Plan {
                    active: 0,
                    cells: (0..count)
                        .map(|index| crate::engine::viewer_frame::Cell {
                            camera: crate::engine::viewer_frame::Camera::Composition,
                            placement: motion_render::multiview::ViewPlacement {
                                viewport: if count == 2 {
                                    [index as f32 * 32., 0., 32., 64.]
                                } else {
                                    [(index % 2) as f32 * 32., (index / 2) as f32 * 32., 32., 32.]
                                },
                                image: None,
                            },
                            inspection: Default::default(),
                        })
                        .collect(),
                });
            }
            frame
        };
        for count in [1, 2, 4] {
            let until = Instant::now() + Duration::from_secs(30);
            while !engine
                .present_async(layout(count))
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap()
            {
                assert!(Instant::now() < until, "capture layout did not publish");
            }
            let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
            let before_capture = engine.state().unwrap();
            let path = root.join(format!("managed-display-{count}.png"));
            engine.capture_ui(layout(count), path.clone()).unwrap();
            let pixels = image::open(path).unwrap().to_rgba8();
            assert_eq!(pixels.dimensions(), (64, 64));
            for index in 0..count {
                let (x, y) = match count {
                    1 => (32, 32),
                    2 => (index * 32 + 16, 32),
                    _ => (index % 2 * 32 + 16, index / 2 * 32 + 16),
                };
                let pixel = pixels.get_pixel(x as u32, y as u32).0;
                assert!(
                    (0..3).all(|channel| pixel[channel].abs_diff(expected[channel]) <= 1),
                    "managed worker capture count={count} cell={index} actual={pixel:?} CPU={expected:?}"
                );
                assert_eq!(pixel[3], 255);
            }
            let after_capture = engine.state().unwrap();
            for key in [
                "project", "revision", "frame", "history", "canUndo", "canRedo",
            ] {
                assert_eq!(
                    after_capture[key], before_capture[key],
                    "capture changed {key}"
                );
            }
            assert_eq!(
                engine.preview_info().unwrap()["video"]["renderAttempts"],
                attempts
            );
            roi::qualify(&engine, || layout(count), count, expected, &root);
            components::qualify(&engine, || layout(count), count, working, expected, &root);
        }
        engine.seek(0.625).unwrap();
        let stale_path = root.join("stale-display.png");
        let stale_before = engine.state().unwrap();
        let stale_error = engine
            .capture_ui(layout(4), stale_path.clone())
            .unwrap_err();
        let stale_cause = crate::engine::error::command_cause(stale_error.host().unwrap());
        for context in [
            "phase=surface_capture",
            "completed group is no longer current",
            "document=",
            "revision=",
            "owner=0",
            "frame=0.625",
            "published=",
        ] {
            assert!(stale_cause.contains(context), "{stale_cause}");
        }
        assert!(!stale_path.exists());
        let stale_after = engine.state().unwrap();
        for key in [
            "project", "revision", "frame", "history", "canUndo", "canRedo",
        ] {
            assert_eq!(
                stale_after[key], stale_before[key],
                "rejected capture changed {key}"
            );
        }
        present();
        let capture_path = root.join("unqualified-capture.png");
        let mut unqualified_capture = ui();
        unqualified_capture.inspection.comparison_split = Some(0.5);
        let capture_error = engine
            .capture_ui(unqualified_capture, capture_path.clone())
            .unwrap_err();
        assert!(
            capture_error.to_string().contains("phase=surface_capture"),
            "{capture_error}"
        );
        assert!(
            !capture_path.exists(),
            "unqualified capture wrote a legacy image"
        );
        let mut unqualified = ui();
        unqualified.inspection.comparison_split = Some(0.5);
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
        for name in [
            "capture-reference.json",
            "managed-display-1.png",
            "managed-display-2.png",
            "managed-display-4.png",
            "managed-display-floating.png",
            "managed-roi-1.png",
            "managed-roi-2.png",
            "managed-roi-4.png",
            "managed-roi-pan-1.png",
            "managed-roi-pan-2.png",
            "managed-roi-pan-4.png",
        ] {
            let source = root.path().join(name);
            if source.exists() {
                fs::copy(source, evidence.join(name)).unwrap();
            }
        }
        for count in [1, 2, 4] {
            for phase in 0..5 {
                let name = format!("managed-components-{count}-{phase}.png");
                fs::copy(root.path().join(&name), evidence.join(name)).unwrap();
            }
        }
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}\n{file}");
    assert!(file.contains("logging initialized level=INFO"), "{file}");
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(" ERROR ") && line.contains("window.png"))
        .collect();
    assert_eq!(
        errors.len(),
        4,
        "source, image-set, surface and worker must each preserve the original cause: {file}"
    );
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
    let capture = file
        .lines()
        .find(|line| line.contains(" ERROR ") && line.contains("phase=surface_capture"))
        .unwrap();
    assert!(stderr.contains(capture.split_once(": ").unwrap().1));
    assert!(!stdout.contains("phase=surface_capture"));
    for reason in [
        "completed group is no longer current",
        "managed capture comparison requires explicit integration",
        "invalid viewer inspection viewport, image or region",
    ] {
        let record = file
            .lines()
            .find(|line| line.contains(" ERROR ") && line.contains(reason))
            .unwrap();
        assert!(stderr.contains(record.split_once(": ").unwrap().1));
    }
}
