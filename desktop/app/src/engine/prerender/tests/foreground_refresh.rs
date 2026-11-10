//! A completed edit must reach the foreground before new optional GPU work.
use super::*;

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

fn wait_for(engine: &Engine, phase: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if ready(status) {
            return state;
        }
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{phase} deadline: {state}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "explicit native foreground-refresh admission and retained-front pixel control"]
fn native_edit_waits_for_foreground_refresh_before_warming_its_revision() {
    const NAME: &str = "engine::prerender::tests::foreground_refresh::native_edit_waits_for_foreground_refresh_before_warming_its_revision";
    const CHILD: &str = "MOTION_PRERENDER_REFRESH_CHILD";
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
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_REFRESH_ARTIFACTS") {
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
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Refresh control",
        [16.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    ));
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
    let warmed = wait_for(&engine, "initial warmup", |s| {
        s["cache"]["resident_frames"] == 2 && s["pending"] == 0
    });
    let started = warmed["desktop"]["prerender"]["started"].as_u64().unwrap();
    engine
        .edit(
            serde_json::to_string(&motion_core::Command::SetColor {
                object: 1,
                value: [0., 1., 0., 1.],
            })
            .unwrap(),
        )
        .unwrap();
    // Deliberately withhold Present after the real edit has completed. The queue
    // is idle, but rendering a new optional revision would steal foreground slack.
    let held = wait_for(&engine, "await foreground_refresh", |s| {
        s["hold"] == "foreground_refresh" && s["pending"] == 0
    });
    assert_eq!(held["desktop"]["prerender"]["cache"]["resident_frames"], 0);
    assert_eq!(held["desktop"]["prerender"]["started"], started);
    std::thread::sleep(Duration::from_millis(200));
    let still_held = engine.state().unwrap();
    assert_eq!(still_held["desktop"]["prerender"]["started"], started);
    assert_eq!(
        still_held["desktop"]["prerender"]["hold"],
        "foreground_refresh"
    );
    let retained = read_front(&mut events, &engine);
    pixels(&retained, [255, 0, 0, 255]);
    present(&mut events, &engine);
    let refreshed = read_front(&mut events, &engine);
    pixels(&refreshed, [0, 255, 0, 255]);
    assert_eq!(refreshed["cachedPresented"], cold["cachedPresented"]);
    let resumed = wait_for(&engine, "resumed warmup", |s| {
        s["cache"]["resident_frames"] == 2 && s["pending"] == 0
    });
    assert!(resumed["desktop"]["prerender"]["started"].as_u64().unwrap() > started);
    engine.seek(1.).unwrap();
    present(&mut events, &engine);
    let cached = read_front(&mut events, &engine);
    pixels(&cached, [0, 255, 0, 255]);
    assert_eq!(
        cached["cachedPresented"],
        refreshed["cachedPresented"].as_u64().unwrap() + 1
    );
    engine.configure_prerender(Default::default()).unwrap();
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "cold": cold, "warmed": warmed, "held": held, "stillHeld": still_held,
        "retained": retained, "refreshed": refreshed, "resumed": resumed, "cached": cached,
        "limits": "Native admission and pixel control; not whole-app P95 or native-crash causality."
    })).unwrap()).unwrap();
}
