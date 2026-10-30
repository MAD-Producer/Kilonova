//! Native work reduction and exact frame ownership around a real layer cut.
use super::*;

fn warm(engine: &Engine, window: u64, completed: u64) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if status["effectiveWindow"] == window
            && status["completed"].as_u64().unwrap_or(0) >= completed
            && status["pending"] == 0
            && status["cache"]["pending_bytes"] == 0
        {
            return state;
        }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pixels(value: &Value, expected: [u64; 4]) {
    assert!(
        value["pixels"]
            .as_array()
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel == expected),
        "{value}"
    );
}

#[test]
#[ignore = "explicit native static-interval scheduling and exact clip-cut pixel control"]
fn native_static_window_reduces_work_and_warms_the_changed_frame_at_a_clip_cut() {
    const NAME: &str = "engine::prerender::tests::static_window::native_static_window_reduces_work_and_warms_the_changed_frame_at_a_clip_cut";
    const CHILD: &str = "MOTION_PRERENDER_STATIC_CHILD";
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
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_STATIC_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let path = root.path().join(name);
                if path.exists() {
                    std::fs::copy(path, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Red interval",
        [16.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    ));
    let mut blue =
        motion_core::Layer::solid(2, "Blue interval", [16.; 2], [8., 8., 0.], [0., 0., 1., 1.]);
    blue.timeline = Some(motion_core::LayerTimeline {
        in_frame: 40,
        out_frame: 80,
        offset_frame: 0,
    });
    project.layers.push(blue);
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
                    motion_host::session::with_session(id, |session| {
                        session.preview.set_mode(motion_render::PreviewMode::High);
                        Ok(serde_json::json!({}))
                    })
                },
                reply,
            })
            .unwrap(),
    );
    engine.seek(10.).unwrap();
    present(&mut events, &engine);
    let cold = read_front(&mut events, &engine);
    pixels(&cold, [255, 0, 0, 255]);
    assert_eq!(cold["cachedPresented"], 0);
    engine.set_prerender_playing(true).unwrap();
    engine
        .configure_prerender(motion_host::prerender::Policy {
            enabled: true,
            window: 3,
            idle_delay: Duration::ZERO,
            ..Default::default()
        })
        .unwrap();
    let reduced = warm(&engine, 2, 2);
    let status = &reduced["desktop"]["prerender"];
    assert_eq!(status["policy"]["window"], 3);
    assert_eq!(status["staticInterval"], serde_json::json!([0., 40.]));
    assert_eq!(status["cache"]["resident_frames"], 2);
    let started = status["started"].as_u64().unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["started"],
        started
    );
    engine.seek(11.).unwrap();
    present(&mut events, &engine);
    let red = read_front(&mut events, &engine);
    pixels(&red, [255, 0, 0, 255]);
    assert_eq!(red["cachedPresented"], 1);

    // The configured three-frame window reaches across the cut. Frame 40
    // must have its own blue native result rather than a red static alias.
    let prior = engine.state().unwrap()["desktop"]["prerender"]["completed"]
        .as_u64()
        .unwrap();
    engine.seek(38.).unwrap();
    present(&mut events, &engine);
    let boundary = warm(&engine, 3, prior + 3);
    assert_eq!(
        boundary["desktop"]["prerender"]["cache"]["resident_frames"],
        3
    );
    engine.seek(40.).unwrap();
    present(&mut events, &engine);
    let changed = read_front(&mut events, &engine);
    pixels(&changed, [0, 0, 255, 255]);
    assert_eq!(
        changed["cachedPresented"],
        red["cachedPresented"].as_u64().unwrap() + 1
    );
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(&project).unwrap()
    );
    engine.set_prerender_playing(false).unwrap();
    engine.configure_prerender(Default::default()).unwrap();
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "reduced": reduced, "boundary": boundary, "cold": cold, "red": red, "changed": changed,
        "exactIndependentFrameKeys": true, "authoredProjectUnchanged": true,
        "limits": "Explicit local native scheduling/pixel control, not whole-app performance or all temporal content."
    })).unwrap()).unwrap();
}
