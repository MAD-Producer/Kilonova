//! Source delivery must not depend on the blocked foreground command queue.
use super::*;

fn uniform_source(path: &std::path::Path, green: bool) {
    image::RgbaImage::from_pixel(
        16,
        16,
        image::Rgba(if green {
            [0, 255, 0, 255]
        } else {
            [255, 0, 0, 255]
        }),
    )
    .save_with_format(path, image::ImageFormat::Png)
    .unwrap();
}
fn pixels(value: &Value, expected: [u64; 4]) {
    assert!(
        value["pixels"]
            .as_array()
            .unwrap()
            .chunks_exact(4)
            .all(|p| p == expected),
        "{value}"
    );
}
fn warm(engine: &Engine) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let p = &state["desktop"]["prerender"];
        assert!(p["fault"].is_null(), "{state}");
        if p["cache"]["resident_frames"] == 2 && p["pending"] == 0 {
            return state;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "source warmup deadline: {state}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "explicit native Engine source event, blocked queue and retained-front pixel acceptance"]
fn native_source_change_bypasses_blocked_engine_and_replaces_cached_pixels_without_edit() {
    const NAME: &str = "engine::prerender::tests::source_freshness::native_source_change_bypasses_blocked_engine_and_replaces_cached_pixels_without_edit";
    const CHILD: &str = "MOTION_PRERENDER_SOURCE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_SOURCE_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "child status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    let source = directory.join("assets/source.png");
    uniform_source(&source, false);
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.assets.push(motion_core::Asset {
        id: 17,
        path: "assets/source.png".into(),
        width: 16,
        height: 16,
    });
    let mut layer = motion_core::Layer::solid(1, "Source event", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 17 };
    project.layers.push(layer);
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
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
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
    receive(
        &mut events,
        &engine
            .request(|reply| Command::TestStudy {
                operation: |id| {
                    motion_host::session::with_session(id, |s| {
                        s.preview.set_mode(motion_render::PreviewMode::High);
                        Ok(serde_json::json!({}))
                    })
                },
                reply,
            })
            .unwrap(),
    );
    present(&mut events, &engine);
    let cold = read_front(&mut events, &engine);
    pixels(&cold, [255, 0, 0, 255]);
    engine
        .configure_prerender(motion_host::prerender::Policy {
            enabled: true,
            window: 2,
            idle_delay: Duration::ZERO,
            ..Default::default()
        })
        .unwrap();
    let old_warm = warm(&engine);
    engine.seek(1.).unwrap();
    present(&mut events, &engine);
    let red = read_front(&mut events, &engine);
    pixels(&red, [255, 0, 0, 255]);
    assert_eq!(red["cachedPresented"], 1);
    let stable_warm = warm(&engine);
    let revision = stable_warm["revision"].clone();
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let pending = engine
        .request(|reply| Command::TestWait {
            entered,
            release: blocked,
            reply,
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        engine
            .prerender_observation()
            .unwrap()
            .unwrap()
            .source
            .unwrap()
            .valid
    );
    uniform_source(&source, true);
    let began = Instant::now();
    let invalidated = loop {
        let snapshot = engine.prerender_observation().unwrap().unwrap();
        if snapshot.source.as_ref().is_some_and(|s| !s.valid) {
            break snapshot;
        }
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "source callback needs blocked Engine queue"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(engine.in_flight().unwrap().0, "test_wait");
    assert_eq!(engine.queue_depth(), 0);
    let seek = engine.seek_async(2.).unwrap();
    let shown = engine.present_async(frame()).unwrap();
    release.send(()).unwrap();
    receive(&mut events, &pending);
    receive(&mut events, &seek);
    let rendered = receive(&mut events, &shown);
    if !rendered {
        present(&mut events, &engine);
    }
    let miss = read_front(&mut events, &engine);
    assert_eq!(
        miss["cachedPresented"], red["cachedPresented"],
        "stale cached pixels must not reach Present"
    );
    let new_warm = warm(&engine);
    assert_eq!(
        new_warm["revision"], revision,
        "external source replacement must not author an edit"
    );
    engine.seek(3.).unwrap();
    present(&mut events, &engine);
    let green = read_front(&mut events, &engine);
    pixels(&green, [0, 255, 0, 255]);
    assert_eq!(green["cachedPresented"], 2);
    engine.configure_prerender(Default::default()).unwrap();
    std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "cold":cold,"oldWarm":old_warm,"red":red,"blockedSource":invalidated.source.map(|s| serde_json::json!({"request":s.request,"valid":s.valid,"retired":s.retired,"fault":s.fault,"cause":s.cause})),
        "miss":miss,"newWarm":new_warm,"green":green,
        "limits":"Source delivery/lookup/native pixels only. Existing foreground image reload policy is independent; first fallback after invalidation is not required to reload its image. No OS input or P95 claim."
    })).unwrap()).unwrap();
}
