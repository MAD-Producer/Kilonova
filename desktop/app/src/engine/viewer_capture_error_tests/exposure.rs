//! Actual exposure Present failure through the production worker and logger.
use super::*;

const TEST: &str = "engine::viewer_capture_error_tests::exposure::native_exposure_present_preserves_png_cause_default_sinks_and_edit_recovery";
const CHILD: &str = "MOTION_EXPOSURE_ERROR_CHILD";

fn original(error: &Error) -> &Arc<motion_render::RenderError> {
    let motion_host::Error::Render(motion_render::RenderError::Shared(cause)) = native(error) else {
        panic!("exposure Present reply lost typed shared cause: {error:?}");
    };
    cause
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires a native window and GPU")]
fn native_exposure_present_preserves_png_cause_default_sinks_and_edit_recovery() {
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(root.path().join("child.stdout")).unwrap())
            .stderr(std::fs::File::create(root.path().join("child.stderr")).unwrap())
            .spawn().unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() { break status; }
            if began.elapsed() > Duration::from_secs(120) {
                child.kill().unwrap();
                panic!("exposure error child exceeded watchdog: {:?}", child.wait());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        if let Some(destination) = std::env::var_os("MOTION_EXPOSURE_ERROR_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["child.stdout", "child.stderr", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.is_file() { std::fs::copy(source, destination.join(name)).unwrap(); }
            }
        }
        let stdout = std::fs::read_to_string(root.path().join("child.stdout")).unwrap();
        let stderr = std::fs::read_to_string(root.path().join("child.stderr")).unwrap();
        assert!(status.success(), "{status}\n{stdout}\n{stderr}");
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let failures: Vec<_> = file.lines().filter(|line| line.contains("motion_studio::engine: error #")
            && line.contains(" present ") && line.contains("preview exposure composition=")).collect();
        assert_eq!(failures.len(), 2, "{file}");
        for line in failures {
            let record = line.split_once("motion_studio::engine: ").unwrap().1;
            assert!(record.contains("phase=request") && record.contains("view=1 surface=1"));
            assert!(stderr.contains(record), "Error missing from stderr: {record}");
            assert!(!stdout.contains(record), "Error leaked to stdout: {record}");
        }
        assert!(file.contains(" start #") && file.contains(" edit"));
        assert!(!stdout.contains("preview exposure composition="));
        return;
    };
    let root = PathBuf::from(root);
    assert!(std::env::var_os("MOTION_LOG").is_none());
    let _logging = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    let path = directory.join("assets/image.png");
    image::save_buffer(&path, &[20, 40, 60, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    let mut corrupt = std::fs::read(&path).unwrap();
    let idat = corrupt.windows(4).position(|bytes| bytes == b"IDAT").unwrap();
    let len = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + len] ^= 1;
    std::fs::write(&path, &corrupt).unwrap();
    let mut project = motion_core::Project::new(16, 16, 30, 60).unwrap();
    project.background = [0., 1., 0., 1.];
    project.assets.push(motion_core::Asset { id: 17, path: "assets/image.png".into(), width: 1, height: 1 });
    let mut layer = motion_core::Layer::solid(1, "native PNG failure", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 17 };
    layer.sampling.motion_blur = Some(motion_core::render_sampling::Shutter { frames: 0.5, samples: 4 });
    project.layers.push(layer);
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(winit::window::Window::default_attributes()
        .with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(64, 64))).unwrap());
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap()).unwrap();
    let before = engine.state().unwrap();
    let began = Instant::now();
    let first = loop {
        match receive(&mut events, &engine.present_async(frame()).unwrap()) {
            Err(error) => break error,
            Ok(false) => assert!(began.elapsed() < Duration::from_secs(30)),
            Ok(true) => panic!("corrupt PNG exposure was presented as complete"),
        }
    };
    let cause = original(&first).clone();
    let mut current = cause.as_ref();
    let image = loop {
        match current {
            motion_render::RenderError::Diagnostic { cause, .. } | motion_render::RenderError::Context { cause, .. } => current = cause,
            motion_render::RenderError::Shared(cause) => current = cause,
            motion_render::RenderError::ImageResource(image) => break image,
            other => panic!("lost native image cause: {other:?}"),
        }
    };
    assert!(matches!(image, motion_render::image_resources::Error::Png(_)));
    assert!(std::error::Error::source(image).is_some());
    let again = receive(&mut events, &engine.present_async(frame()).unwrap()).unwrap_err();
    assert!(Arc::ptr_eq(&cause, original(&again)));
    assert_eq!(first.to_string(), again.to_string());
    let after = engine.state().unwrap();
    assert_eq!(after["project"], before["project"]);
    assert_eq!(after["revision"], before["revision"]);
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    engine.edit(serde_json::to_string(&motion_core::Command::Remove { object: 1, frame: 0 }).unwrap()).unwrap();
    let began = Instant::now();
    loop {
        if receive(&mut events, &engine.present_async(frame()).unwrap()).unwrap() { break; }
        assert!(began.elapsed() < Duration::from_secs(30));
    }
    let snapshot = receive(&mut events, &engine.capture_viewer_snapshot_async(0).unwrap()).unwrap();
    assert_eq!(snapshot["generation"], 1);
    let output = root.join("recovered.png");
    receive(&mut events, &engine.save_viewer_snapshot_async(0, output.clone(), motion_render::ViewChannel::Color).unwrap()).unwrap();
    let pixels = image::open(output).unwrap().into_rgba8();
    assert_eq!(pixels.dimensions(), (16, 16));
    assert!(pixels.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));
    drop(engine);
    crate::logging::flush();
}
