use super::*;
use crate::{panels, Locale, Shell};

fn capture(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    root: &std::path::Path,
    locale: Locale,
    size: [f32; 2],
    scale: f32,
    floating: bool,
    busy: bool,
) -> PathBuf {
    shell.scale = scale;
    shell.workspace.locale = locale;
    shell.panels.locale = locale;
    shell.atlas.set_pixel_ratio(scale);
    shell.sync_preview_preparation();
    assert_eq!(shell.panels.preview_preparing, busy);
    let window = if floating { 7 } else { 0 };
    receive(
        events,
        &shell
            .engine
            .resize_async(window, (size[0] * scale) as u32, (size[1] * scale) as u32)
            .unwrap(),
    );
    let (paint, actions) = if floating {
        let mut paint = shell.engine.take_paint();
        paint.stylesheet = shell.panels.styles.clone();
        let actions = panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            motion_ui::dock::Panel::Properties,
            size,
            &mut shell.panels,
            &shell.state,
            &Default::default(),
        );
        (paint, actions)
    } else {
        shell.draw_commands(size)
    };
    assert!(actions.is_empty(), "status painting dispatched actions");
    let upload = shell.atlas.config();
    let mut ui = frame(window);
    ui.paint = paint;
    ui.logical_size = size;
    // Capture the real chrome while its composition's worker is held. The
    // screenshot request must not wait for that deferred composition.
    ui.composition = None;
    ui.atlas = Some(AtlasUpload {
        pixels: shell.atlas.pixel_data().to_vec(),
        width: upload.width,
        height: upload.height,
    });
    let name = format!(
        "{}-{}-{}x{}-{}-{}.png",
        if locale == Locale::Zh { "zh" } else { "en" },
        if floating { "floating" } else { "main" },
        size[0],
        size[1],
        scale,
        if busy { "preparing" } else { "ready" }
    );
    let path = root.join(name);
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    path
}

fn capture_clean(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    root: &std::path::Path,
    busy: bool,
) {
    shell.state["renderError"] = serde_json::Value::Null;
    shell.panels.message = None;
    shell.atlas = motion_ui::TextAtlas::from_family(
        motion_ui::style::default_stylesheet().font_family(),
        motion_ui::style::default_stylesheet().base_font_size(),
        true,
    )
    .unwrap();
    let directory = root.join("clean");
    fs::create_dir_all(&directory).unwrap();
    for floating in [false, true] {
        let image = capture(
            events,
            shell,
            &directory,
            Locale::Zh,
            [1600., 1000.],
            1.,
            floating,
            busy,
        );
        let destination = root.join(format!(
            "clean-{}",
            image.file_name().unwrap().to_str().unwrap()
        ));
        fs::rename(image, destination).unwrap();
    }
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surfaces and GPU"
)]
fn native_preparation_status_preserves_errors_and_clears_without_input() {
    const CASE: &str = "engine::frame_completion::tests::native::preparation_status::native_preparation_status_preserves_errors_and_clears_without_input";
    const CHILD: &str = "MOTION_PREPARATION_STATUS_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .stdout(fs::File::create(root.path().join("stdout")).unwrap())
            .stderr(fs::File::create(root.path().join("stderr")).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "preparation status child timed out: {}",
                    fs::read_to_string(root.path().join("stderr")).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if let Some(destination) = std::env::var_os("MOTION_PREPARATION_STATUS_EVIDENCE") {
            fs::create_dir_all(&destination).unwrap();
            for item in fs::read_dir(root.path()).unwrap() {
                let item = item.unwrap();
                if item.file_type().unwrap().is_file() {
                    fs::copy(
                        item.path(),
                        PathBuf::from(&destination).join(item.file_name()),
                    )
                    .unwrap();
                }
            }
        }
        assert!(
            exit.success(),
            "{}\n{}",
            fs::read_to_string(root.path().join("stdout")).unwrap(),
            fs::read_to_string(root.path().join("stderr")).unwrap()
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut project = motion_core::Project::new(32, 32, 30, 60).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Preparation",
        [24.; 2],
        [16., 16., 0.],
        [0.8, 0.2, 0.1, 1.],
    ));
    let package = motion_effects::builtin::package().unwrap();
    let effect = package
        .manifest
        .effects
        .iter()
        .find(|effect| effect.id == "invert")
        .unwrap();
    project.layers[0]
        .effects
        .push(motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            effect,
            [24.; 2],
        ));
    project.rebuild_plugin_dependencies();
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let (entered, entry) = channel();
    let (release, gate) = channel();
    let gate = Mutex::new(gate);
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    // SAFETY: the native test owns this private leaf; production establishes
    // the account-protected capability before using the same cache protocol.
    let storage = unsafe {
        motion_host::session::DriverPipelineCacheStorage::new(
            root.join("private/cache"),
            Arc::new(move |path| {
                if callback_calls.fetch_add(1, Ordering::AcqRel) == 0 {
                    entered.send(()).map_err(std::io::Error::other)?;
                    gate.lock()
                        .map_err(|_| std::io::Error::other("fixture gate poisoned"))?
                        .recv()
                        .map_err(std::io::Error::other)?;
                }
                crate::pipeline_cache::prepare_test_storage(path)
            }),
        )
    };
    let engine = Arc::new(
        Engine::start_with_pipeline_cache(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
            Some(storage),
        )
        .unwrap(),
    );
    // Replace only the test fixture's idle engine; all drawing and status
    // synchronization below use Shell's production methods.
    let mut shell = Shell::new(root.join("idle-fixture"), None, Some(Locale::En)).unwrap();
    shell.engine = engine.clone();
    shell.state = engine.state().unwrap();
    let before = shell.state["project"].clone();
    let revision = shell.state["revision"].clone();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    receive(&mut events, &engine.attach_async(main, 1600, 1000).unwrap());
    engine.float_window(7, floating, 1600, 1000).unwrap();
    let supported = engine.call(|reply|Command::TestStudy { operation:|id|
        motion_host::session::with_session(id,|s|Ok(serde_json::json!({"supported":s.graphics.as_ref().unwrap().renderer.driver_pipeline_cache_info().supported}))),reply }).unwrap()["supported"]==true;
    let first = engine.present_async(frame(0)).unwrap();
    if !supported {
        assert!(receive(&mut events, &first));
        assert!(!engine.preview_preparing());
        fs::write(
            root.join("report.json"),
            b"{\"cacheSupported\":false,\"blockedRestoreQualified\":false}",
        )
        .unwrap();
        return;
    }
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(engine.preview_preparing());
    assert!(matches!(
        first.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    shell.dirty = false;
    shell.sync_preview_preparation();
    assert!(
        shell.dirty,
        "starting preparation did not invalidate chrome"
    );
    shell.dirty = false;
    shell.sync_preview_preparation();
    assert!(
        !shell.dirty,
        "healthy polling continuously invalidated chrome"
    );
    let mut pending = Vec::new();
    for locale in [Locale::En, Locale::Zh] {
        shell.atlas = motion_ui::TextAtlas::from_family(
            motion_ui::style::default_stylesheet().font_family(),
            motion_ui::style::default_stylesheet().base_font_size(),
            locale == Locale::Zh,
        )
        .unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            for scale in [1., 1.5, 2.] {
                for floating in [false, true] {
                    // A real existing render error keeps precedence over the
                    // ordinary message, while preparation remains separate.
                    let error = if size[0] == 900. {
                        "Existing render failure: source missing; ".repeat(20)
                    } else {
                        "Existing render failure: source missing".into()
                    };
                    shell.state["renderError"] = serde_json::json!(error);
                    shell.panels.message =
                        Some("ordinary status must not replace the error".into());
                    let path = capture(
                        &mut events,
                        &mut shell,
                        &root,
                        locale,
                        size,
                        scale,
                        floating,
                        true,
                    );
                    pending.push((locale, size, scale, floating, path, error));
                }
            }
        }
    }
    capture_clean(&mut events, &mut shell, &root, true);
    assert_eq!(
        calls.load(Ordering::Acquire),
        1,
        "chrome capture restarted blocked preparation"
    );
    // Every resize invalidates the queued frame's extent/view stamp. Preserve
    // that production cancellation, then finish a new request at final size.
    assert!(!receive(&mut events, &first));
    let latest = engine.present_async(frame(0)).unwrap();
    release.send(()).unwrap();
    assert!(receive(&mut events, &latest));
    shell.dirty = false;
    shell.sync_preview_preparation();
    assert!(
        shell.dirty,
        "completion without input did not invalidate chrome"
    );
    assert!(!shell.panels.preview_preparing);
    shell.dirty = false;
    shell.sync_preview_preparation();
    assert!(!shell.dirty);
    let mut report = Vec::new();
    for (locale, size, scale, floating, busy, error) in pending {
        shell.state["renderError"] = serde_json::json!(error);
        shell.atlas = motion_ui::TextAtlas::from_family(
            motion_ui::style::default_stylesheet().font_family(),
            motion_ui::style::default_stylesheet().base_font_size(),
            locale == Locale::Zh,
        )
        .unwrap();
        let ready = capture(
            &mut events,
            &mut shell,
            &root,
            locale,
            size,
            scale,
            floating,
            false,
        );
        let a = image::open(&busy).unwrap().to_rgba8();
        let b = image::open(&ready).unwrap().to_rgba8();
        assert_eq!(a.dimensions(), b.dimensions());
        assert_eq!(
            a.dimensions(),
            ((size[0] * scale) as u32, (size[1] * scale) as u32)
        );
        let height = (20. * scale) as u32;
        let y = a.height() - height;
        let left = (200. * scale) as u32;
        let right = (200. * scale) as u32;
        assert_eq!(
            image::imageops::crop_imm(&a, 0, y, left, height).to_image(),
            image::imageops::crop_imm(&b, 0, y, left, height).to_image(),
            "existing error pixels changed"
        );
        assert_ne!(
            image::imageops::crop_imm(&a, a.width() - right, y, right, height).to_image(),
            image::imageops::crop_imm(&b, b.width() - right, y, right, height).to_image(),
            "status was not visible or did not clear"
        );
        report.push(serde_json::json!({"busy":busy.file_name().unwrap(),"ready":ready.file_name().unwrap(),
            "size":size,"scale":scale,"floating":floating,"errorPixelsPreserved":true,"statusPixelsChanged":true}));
    }
    capture_clean(&mut events, &mut shell, &root, false);
    let after = engine.state().unwrap();
    assert_eq!(after["project"], before);
    assert_eq!(after["revision"], revision);
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
        "cacheSupported":true,"sourceUnchanged":true,"revisionUnchanged":true,"cases":report,
        "completedWithoutInput":true,"stablePollDoesNotInvalidate":true}))
        .unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
