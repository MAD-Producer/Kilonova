//! Actual Surface presentation and retained-front invalidation for plate replacement.
use super::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

#[path = "tracking_surface_binding_tests.rs"]
mod bindings;

fn ui(extent: [u32; 2]) -> UiFrame {
    UiFrame {
        views: None,
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: extent.map(|v| v as f32),
        composition: Some([0., 0., extent[0] as f32, extent[1] as f32]),
        inspection: Default::default(),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}

fn presented(events: &mut EventLoop<()>, engine: &Engine, extent: [u32; 2]) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let reply = engine.present_async(ui(extent)).unwrap();
        let rendered = crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        if rendered {
            return engine.state().unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "tracked Surface never completed: {}",
            engine.state().unwrap()
        );
    }
}

fn front(
    events: &mut EventLoop<()>,
    engine: &Engine,
    extent: [u32; 2],
    path: &Path,
) -> image::RgbaImage {
    // CaptureUi reads the already committed scratch front. It does not sample
    // or render the composition, unlike Engine::render_preview.
    crate::native_test_support::capture_ui(events, engine, ui(extent), path.into()).unwrap();
    image::open(path).unwrap().to_rgba8()
}

fn retained(events: &mut EventLoop<()>, engine: &Engine, extent: [u32; 2]) {
    let before = engine.state().unwrap();
    let after = presented(events, engine, extent);
    assert_eq!(
        after["desktop"]["uiPerformance"]["compositionRenders"],
        before["desktop"]["uiPerformance"]["compositionRenders"]
    );
    assert!(
        after["desktop"]["uiPerformance"]["compositionReuses"]
            .as_u64()
            .unwrap()
            > before["desktop"]["uiPerformance"]["compositionReuses"]
                .as_u64()
                .unwrap()
    );
    assert!(after["presented"].as_u64().unwrap() > before["presented"].as_u64().unwrap());
    assert_eq!(after["project"], before["project"]);
    assert_eq!(after["revision"], before["revision"]);
}

fn invalidated(events: &mut EventLoop<()>, engine: &Engine, extent: [u32; 2]) -> Value {
    let before = engine.state().unwrap();
    let after = presented(events, engine, extent);
    assert!(
        after["desktop"]["uiPerformance"]["compositionRenders"]
            .as_u64()
            .unwrap()
            > before["desktop"]["uiPerformance"]["compositionRenders"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(after["lastPresentedFrame"], 24.);
    after
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn tracked_plate_replacement_invalidates_actual_surface_front_and_history_retains_reuse() {
    const NAME: &str = "engine::tracking_video_replacement_tests::paired::surface::tracked_plate_replacement_invalidates_actual_surface_front_and_history_retains_reuse";
    const CHILD: &str = "MOTION_TRACKED_SURFACE_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let directory = std::env::var_os("MOTION_TRACKED_SURFACE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().to_path_buf());
        fs::create_dir_all(&directory).unwrap();
        let (output, process) = crate::test_process::capture(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env(CHILD, &directory)
                .env_remove("MOTION_LOG"),
            &directory.join("child-process.json"),
        )
        .unwrap();
        fs::write(directory.join("stdout.log"), &output.stdout).unwrap();
        fs::write(directory.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "child={process}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = fs::read_to_string(directory.join("logs/motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        assert!(log.contains("tracked video replacement start request=surface-pair"));
        let cause = "tracking source content differs";
        assert!(log.contains(cause));
        assert!(String::from_utf8_lossy(&output.stderr).contains(cause));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(cause));
        assert!(
            !log.lines()
                .any(|line| line.contains("start #") && line.ends_with(" present"))
        );
        return;
    };
    let output = PathBuf::from(directory);
    let _logger = crate::logging::init(output.join("logs")).unwrap();
    let root = output.join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let original_source = root.join("assets/original.mp4");
    fs::copy(fixtures.join("video-ctts.mp4"), &original_source).unwrap();
    let old_solve = solve(&original_source, "video-ctts.mp4", 0.);
    let old_document = output.join("old.mstrack");
    fs::write(
        &old_document,
        motion_host::tracking::encode::to_vec(&old_solve).unwrap(),
    )
    .unwrap();
    let platform = motion_desktop_media::platform();
    let mut probe = platform
        .probe_video(&original_source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/original.mp4".into();
    let mut project = authored::camera_fixture();
    project.background = [0., 0., 0., 1.];
    let extent = [project.width, project.height];
    let mut binding = project.camera.tracking.take().unwrap();
    binding.source_anchor_pts = old_solve.source_pts[0];
    binding.crop = [0., 0., probe.asset.width as f64, probe.asset.height as f64];
    project.tracking_assets.clear();
    project.video_assets = vec![probe.asset.clone()];
    let mut layer = motion_core::Layer::solid(
        50,
        "Tracked Surface plate",
        [256., 144.],
        [320., 240., 0.],
        [1.; 4],
    );
    layer.three_d = true;
    layer.content = motion_core::Content::Video {
        video: motion_core::VideoClip::for_source(&probe.asset, false),
    };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 48,
        offset_frame: 0,
    });
    project.layers.push(layer);
    motion_core::storage::save(&root, &project).unwrap();
    let index = motion_media::video_cache_path(&root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let engine = Engine::start(root.clone(), platform, 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let state = engine
        .import_tracking(
            old_document.clone(),
            AssetRef {
                kind: AssetKind::Video,
                id: 1,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    engine
        .edit_at_revision(
            json!({"op":"set_tracking_camera","object":0,"binding":binding}).to_string(),
            state["revision"].as_u64().unwrap(),
        )
        .unwrap();
    engine.preview_mode(1, 0).unwrap();
    engine.seek(24.).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(extent[0], extent[1])),
            )
            .unwrap(),
    );
    let attach = engine
        .attach_async(window.clone(), extent[0], extent[1])
        .unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let before = invalidated(&mut events, &engine, extent);
    let warm = front(&mut events, &engine, extent, &output.join("before.png"));
    assert!(
        warm.pixels()
            .filter(|p| p[0] > 20 || p[1] > 20 || p[2] > 20)
            .count()
            > 100
    );
    retained(&mut events, &engine, extent);
    assert_eq!(
        front(&mut events, &engine, extent, &output.join("retained.png")),
        warm
    );

    let fresh = fixtures.join("video-delayed.mp4");
    let new_solve = solve(&fresh, "video-delayed.mp4", 0.5);
    let new_document = output.join("new.mstrack");
    let new_bytes = motion_host::tracking::encode::to_vec(&new_solve).unwrap();
    fs::write(&new_document, &new_bytes).unwrap();
    let asset = before["project"]["tracking_assets"][0]["id"]
        .as_u64()
        .unwrap();
    let mut next_binding = binding.clone();
    next_binding.source_anchor_pts = new_solve.source_pts[0];
    prepare(&engine, "surface-pair", &fresh);
    let request = |document: &Path| {
        json!({"op":"finish_tracked_video_replacement","request_id":"surface-pair",
        "target":{"kind":"video","id":1},"revision":before["revision"],"document_generation":before["document_generation"],
        "solves":[{"asset":asset,"path":document}],
        "bindings":[{"kind":"camera","composition":project.composition_id,"object":0,"binding":next_binding}]})
    };
    let failure = media(&engine, request(&old_document)).unwrap_err();
    assert!(
        failure.contains("tracking source content differs"),
        "{failure}"
    );
    // Media commands intentionally invalidate the desktop reuse stamp before
    // dispatch, including rejected requests. The committed front itself must
    // stay intact, the next Present must redraw the unchanged project, and
    // ordinary repeated presentation must then return to reuse.
    assert_eq!(
        front(
            &mut events,
            &engine,
            extent,
            &output.join("rejected-front.png")
        ),
        warm
    );
    invalidated(&mut events, &engine, extent);
    assert_eq!(
        front(&mut events, &engine, extent, &output.join("rejected.png")),
        warm
    );
    retained(&mut events, &engine, extent);
    media(&engine, request(&new_document)).unwrap();
    let after = invalidated(&mut events, &engine, extent);
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    let changed = front(&mut events, &engine, extent, &output.join("after.png"));
    assert_ne!(changed, warm);
    retained(&mut events, &engine, extent);
    assert_eq!(
        front(
            &mut events,
            &engine,
            extent,
            &output.join("after-retained.png")
        ),
        changed
    );

    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    invalidated(&mut events, &engine, extent);
    assert_eq!(
        front(&mut events, &engine, extent, &output.join("undo.png")),
        warm
    );
    retained(&mut events, &engine, extent);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    invalidated(&mut events, &engine, extent);
    assert_eq!(
        front(&mut events, &engine, extent, &output.join("redo.png")),
        changed
    );
    retained(&mut events, &engine, extent);
    engine.save().unwrap();
    drop(engine);

    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
    reopened.preview_mode(1, 0).unwrap();
    reopened.seek(24.).unwrap();
    let attach = reopened.attach_async(window, extent[0], extent[1]).unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    invalidated(&mut events, &reopened, extent);
    assert_eq!(
        front(&mut events, &reopened, extent, &output.join("reopened.png")),
        changed
    );
    retained(&mut events, &reopened, extent);
    fs::write(output.join("report.json"), serde_json::to_vec_pretty(&json!({
        "actualSurface":true,"nativeFfmpeg":true,"originalFrame":24,
        "unchangedPresentReusesFront":true,"wrongSolvePreservesFront":true,
        "pairedCommitInvalidatesFront":true,"oneRevision":true,
        "undoRestoresOldPixels":true,"redoAndReopenRestoreNewPixels":true,
        "scope":"committed preview front after actual Surface present; not physical scanout readback or photographed tracking quality",
        "before":before["desktop"]["uiPerformance"],"after":after["desktop"]["uiPerformance"],
        "reopened":reopened.state().unwrap()["desktop"]["uiPerformance"],
    })).unwrap()).unwrap();
    drop(reopened);
    crate::logging::flush();
}
