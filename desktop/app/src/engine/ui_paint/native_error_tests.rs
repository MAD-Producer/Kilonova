//! Real worker capture/present failures retain Painter and its native causes.
use crate::engine::*;
use std::error::Error as _;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn frame() -> UiFrame {
    let mut paint = motion_ui::paint::PaintList::default();
    motion_ui::paint::rect(
        &mut paint,
        motion_ui::Rect::new(0., 0., 64., 64.),
        motion_ui::Color([0., 1., 0., 1.]),
    );
    UiFrame {
        views: None,
        paint,
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [64.; 2],
        composition: None,
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}

fn receive<T>(
    events: &mut EventLoop<()>,
    reply: &std::sync::mpsc::Receiver<Result<T, Error>>,
) -> Result<T, Error> {
    crate::native_test_support::receive(events, reply, Duration::from_secs(60)).unwrap()
}

fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    frame: UiFrame,
    path: PathBuf,
) -> Result<Value, Error> {
    let reply = engine
        .request(|reply| Command::CaptureUi {
            ui: frame,
            path,
            reply,
        })
        .unwrap();
    receive(events, &reply)
}

fn paint_error(error: &Error) -> &motion_ui::paint::PaintError {
    let mut host = error.host().expect("native worker Host error");
    while let motion_host::Error::Context { cause, .. } = host {
        host = cause;
    }
    let motion_host::Error::NativeSource { cause } = host else {
        panic!("Painter error flattened: {host:?}")
    };
    cause.downcast_ref().expect("native PaintError")
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native window and GPU")]
fn actual_painter_errors_preserve_sources_default_channels_and_recovery() {
    const TEST: &str = "engine::ui_paint::native_error_tests::actual_painter_errors_preserve_sources_default_channels_and_recovery";
    const CHILD: &str = "MOTION_PAINT_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, root)
            .env_remove("MOTION_LOG")
            .stdout(std::fs::File::create(root.join("stdout.log")).unwrap())
            .stderr(std::fs::File::create(root.join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(180) {
                child.kill().unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let stdout = std::fs::read_to_string(root.join("stdout.log")).unwrap();
        let stderr = std::fs::read_to_string(root.join("stderr.log")).unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PAINT_NATIVE_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for path in std::fs::read_dir(root).unwrap() {
                let path = path.unwrap();
                if path.file_type().unwrap().is_file() {
                    std::fs::copy(path.path(), destination.join(path.file_name())).unwrap();
                }
            }
        }
        assert!(status.success(), "{status}\n{stdout}\n{stderr}");
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let errors: Vec<_> = file
            .lines()
            .filter(|line| line.contains("motion_studio::engine: error #"))
            .collect();
        assert_eq!(errors.len(), 4, "{file}");
        for line in errors {
            let error = line.split("motion_studio::engine: ").nth(1).unwrap();
            assert_eq!(stderr.matches(error).count(), 1, "{stderr}");
            assert!(!stdout.contains(error));
            if line.contains(" capture_ui ") {
                let request = line
                    .split("error #")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap();
                let start = format!("start #{request} capture_ui");
                assert!(file.find(&start).unwrap() < file.find(error).unwrap());
            }
        }
        assert!(!file
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        assert!(
            !stdout.contains("invalid glyph atlas dimensions")
                && !stdout.contains("render allocations")
        );
        let report: Value =
            serde_json::from_slice(&std::fs::read(root.join("report.json")).unwrap()).unwrap();
        assert_eq!(report["project_unchanged"], true);
        assert_eq!(report["pixels_unchanged"], true);
        assert_eq!(report["painter_budget_native"], true);
        assert_eq!(report["atlas_io_native"], true);
        assert_eq!(report["related_primary_native"], true);
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let project_dir = root.join("project");
    motion_core::storage::save(
        &project_dir,
        &motion_core::Project::new(16, 16, 30, 12).unwrap(),
    )
    .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let engine = Engine::start(project_dir, motion_desktop_media::platform(), 8 << 30).unwrap();
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
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap()).unwrap();
    capture(&mut events, &engine, frame(), root.join("before.png")).unwrap();
    let before = engine.state().unwrap();

    let mut bad = frame();
    bad.atlas = Some(AtlasUpload {
        pixels: vec![],
        width: 0,
        height: 1,
    });
    let capture_failure = capture(&mut events, &engine, bad, root.join("refused.png")).unwrap_err();
    assert!(capture_failure
        .to_string()
        .starts_with("phase=capture.atlas: "));
    let native = paint_error(&capture_failure);
    let io = native
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput);
    assert!(native.capacity_refusal().is_none());
    let clone = capture_failure.clone();
    assert!(std::ptr::eq(native, paint_error(&clone)));
    assert!(std::ptr::eq(
        io,
        paint_error(&clone)
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
    ));
    let text = Error::from(capture_failure.to_string());
    assert_eq!(text.to_string(), capture_failure.to_string());
    assert!(text.host().is_none());
    assert!(!root.join("refused.png").exists());
    let mut bad = frame();
    bad.atlas = Some(AtlasUpload {
        pixels: vec![],
        width: 0,
        height: 1,
    });
    let present_failure = receive(&mut events, &engine.present_async(bad).unwrap()).unwrap_err();
    assert_eq!(
        paint_error(&present_failure)
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );

    let budget = engine.allocation_budget.info().unwrap();
    // A 64x64 RGBA texture and row-aligned staging buffer each take 16384
    // bytes. Refuse the subsequent real Painter buffer, not target creation.
    engine
        .allocation_budget
        .set_limit(budget.admitted_bytes + 2 * 64 * 64 * 4 + 39)
        .unwrap();
    let failure = capture(
        &mut events,
        &engine,
        frame(),
        root.join("budget-refused.png"),
    )
    .unwrap_err();
    assert!(
        failure.to_string().starts_with("phase=capture.painter: "),
        "{failure}"
    );
    let native = paint_error(&failure);
    assert!(native.capacity_refusal().is_some());
    let render = native
        .source()
        .unwrap()
        .downcast_ref::<motion_render::RenderError>()
        .unwrap();
    assert!(matches!(
        render.resource_rejection(),
        Some(motion_render::ResourceRejection::Budget { .. })
    ));
    assert!(!root.join("budget-refused.png").exists());
    engine
        .allocation_budget
        .set_limit(budget.budget_bytes)
        .unwrap();

    // Separate reply-boundary control: both actual causes survive when a host
    // fault and a Painter fault coincide. This is not another worker operation.
    let io = std::fs::File::open(root.join("missing-primary")).unwrap_err();
    let code = io.raw_os_error();
    let combined = super::finish(
        Err(io.into()),
        Some(paint_error(&capture_failure).clone()),
        7,
    )
    .unwrap_err();
    let (reply, rx) = std::sync::mpsc::channel();
    CURRENT_COMMAND.set(Some(Flight {
        request: 9001,
        operation: "present",
        started: Instant::now(),
    }));
    respond(reply, Err::<(), _>(combined));
    CURRENT_COMMAND.set(None);
    let combined = rx.recv().unwrap().unwrap_err();
    let motion_host::Error::Related {
        primary, secondary, ..
    } = combined.host().unwrap()
    else {
        panic!("related error missing")
    };
    let motion_host::Error::Io(io) = primary.as_ref() else {
        panic!("primary IO missing")
    };
    assert_eq!(io.raw_os_error(), code);
    let motion_host::Error::NativeSource { cause } = secondary.as_ref() else {
        panic!("Painter secondary missing")
    };
    let secondary = cause
        .downcast_ref::<motion_ui::paint::PaintError>()
        .unwrap();
    assert!(std::ptr::eq(
        secondary
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap(),
        paint_error(&capture_failure)
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
    ));

    capture(&mut events, &engine, frame(), root.join("after.png")).unwrap();
    receive(&mut events, &engine.present_async(frame()).unwrap()).unwrap();
    let after = engine.state().unwrap();
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["revision"], after["revision"]);
    let before = image::open(root.join("before.png")).unwrap().into_rgba8();
    let after = image::open(root.join("after.png")).unwrap().into_rgba8();
    assert_eq!(before.dimensions(), (64, 64));
    assert_eq!(after, before);
    assert!(after.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "project_unchanged":true,"pixels_unchanged":true,"painter_budget_native":true,
            "atlas_io_native":true,"related_primary_native":true,"primary_os_code":code
        }))
        .unwrap(),
    )
    .unwrap();
    drop(engine);
    crate::logging::flush();
}
