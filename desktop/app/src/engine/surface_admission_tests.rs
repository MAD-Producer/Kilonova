//! Real worker replacement refusal must preserve both graphics and its factory.
use super::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn native_device_identity(device: &wgpu::Device) -> (wgpu::Backend, usize) {
    // Core Device IDs are local to an Instance and may compare equal across
    // different Instances. Keep the original Device alive and compare native
    // object addresses instead. No native API is called or object modified;
    // each HAL guard is dropped before the next wgpu call.
    // SAFETY: only borrow the live HAL object and observe its stable address.
    unsafe {
        if let Some(native) = device.as_hal::<wgpu::hal::api::Vulkan>() {
            return (wgpu::Backend::Vulkan, std::ptr::from_ref(&*native).addr());
        }
        if let Some(native) = device.as_hal::<wgpu::hal::api::Dx12>() {
            return (wgpu::Backend::Dx12, std::ptr::from_ref(&*native).addr());
        }
    }
    panic!("native Windows device has neither a Vulkan nor a D3D12 identity");
}

fn frame(size: u32) -> UiFrame {
    UiFrame { views: None,
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [size as f32; 2],
        composition: Some([0., 0., size as f32, size as f32]),
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}
fn present(events: &mut EventLoop<()>, engine: &Engine, size: u32) {
    let began = Instant::now();
    loop {
        let pending = engine
            .request(|reply| Command::Present {
                ui: frame(size),
                reply,
            })
            .unwrap();
        if crate::native_test_support::receive(events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap()
        {
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "native front never completed"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_failed_reattach_keeps_original_factory_front_and_default_error_channels() {
    const NAME: &str = "engine::surface_admission_tests::native_failed_reattach_keeps_original_factory_front_and_default_error_channels";
    const CHILD: &str = "MOTION_PRESENTER_WORKER_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, directory.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRESENTER_WORKER_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            let saved = (|| -> std::io::Result<()> {
                std::fs::create_dir_all(&destination)?;
                for name in [
                    "report.json",
                    "motion-studio.log",
                    "before.png",
                    "after.png",
                ] {
                    let source = directory.path().join(name);
                    if source.exists() {
                        std::fs::copy(source, destination.join(name))?;
                    }
                }
                std::fs::write(destination.join("stdout.log"), &output.stdout)?;
                std::fs::write(destination.join("stderr.log"), &output.stderr)
            })();
            assert!(
                output.status.success(),
                "child failed: {}\n{}; artifact copy={saved:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            saved.unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(directory.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        let errors: Vec<_> = log
            .lines()
            .filter(|line| {
                line.contains("error #")
                    && line.contains(" attach ")
                    && line.contains("canvas navigation")
            })
            .collect();
        assert_eq!(errors.len(), 1);
        let error = &errors[0][errors[0].find("error #").unwrap()..];
        assert!(String::from_utf8_lossy(&output.stderr).contains(error));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(error));
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let root = PathBuf::from(directory);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let project_root = root.join("project");
    let mut project = motion_core::Project::new(8, 8, 30, 6).unwrap();
    project.background = [0., 1., 0., 1.];
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = |size| {
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(size, size)),
                )
                .unwrap(),
        )
    };
    let original_window = window(96);
    let replacement_window = window(128);
    let engine = Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
    let pending = engine.attach_async(original_window, 96, 96).unwrap();
    crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    present(&mut events, &engine, 96);
    crate::native_test_support::capture_ui(
        &mut events,
        &engine,
        frame(96),
        root.join("before.png"),
    )
    .unwrap();
    let before = engine.state().unwrap();
    let original_device = engine
        .factory
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .device
        .clone();
    let info = engine.allocation_budget.info().unwrap();
    let limit = info.admitted_bytes + motion_render::FIXED_RENDERER_ALLOCATION_BYTES + 40 + 4 + 63;
    engine.allocation_budget.set_limit(limit).unwrap();
    let pending = engine
        .attach_async(replacement_window.clone(), 128, 128)
        .unwrap();
    let error = crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(error.contains("canvas navigation"), "{error}");
    assert!(
        error.contains(&limit.to_string()) && error.contains(&(limit + 1).to_string()),
        "{error}"
    );
    {
        let factory = engine.factory.lock().unwrap();
        let factory = factory
            .as_ref()
            .expect("failed replacement discarded the original factory");
        assert_eq!([factory.config.width, factory.config.height], [96, 96]);
        assert_eq!(
            native_device_identity(&factory.device),
            native_device_identity(&original_device)
        );
    }
    let failed = engine.state().unwrap();
    for key in [
        "project",
        "revision",
        "frame",
        "lastPresentedFrame",
        "canUndo",
    ] {
        assert_eq!(failed[key], before[key], "failed attachment changed {key}");
    }
    // Exercise the retained old device/surface before releasing the pressure.
    present(&mut events, &engine, 96);
    engine
        .allocation_budget
        .set_limit(info.budget_bytes)
        .unwrap();
    crate::native_test_support::capture_ui(&mut events, &engine, frame(96), root.join("after.png"))
        .unwrap();
    assert_eq!(
        std::fs::read(root.join("before.png")).unwrap(),
        std::fs::read(root.join("after.png")).unwrap()
    );
    let pending = engine.attach_async(replacement_window, 128, 128).unwrap();
    crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    {
        let factory = engine.factory.lock().unwrap();
        let factory = factory.as_ref().unwrap();
        assert_eq!([factory.config.width, factory.config.height], [128, 128]);
        assert_ne!(
            native_device_identity(&factory.device),
            native_device_identity(&original_device)
        );
    }
    present(&mut events, &engine, 128);
    let recovered = engine.state().unwrap();
    for key in ["project", "revision", "frame", "canUndo"] {
        assert_eq!(recovered[key], before[key]);
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "refusal": error, "limit": limit, "required": limit + 1,
            "oldFactoryRetained": true, "oldFrontPresentedUnderPressure": true,
            "pixelsEqual": true, "authoredStateUnchanged": true, "replacementRecovered": true,
        }))
        .unwrap(),
    )
    .unwrap();
    drop(engine);
    crate::logging::flush();
}
