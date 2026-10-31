//! Explicit native controls for policy pressure and document ownership.
use super::*;

fn high(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        s.preview.set_mode(motion_render::PreviewMode::High);
        Ok(serde_json::json!({}))
    })
}
fn pressure(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        assert!(s.preview_memory.refused(
            motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::PreviewTarget,
                required_bytes: 1024,
                budget_bytes: 0,
            },
            0,
        ));
        Ok(serde_json::json!({}))
    })
}
fn release_pressure(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        // Release this test's explicit policy fixture; this does not claim
        // physical adapter pressure or automatic production recovery.
        s.preview_memory = Default::default();
        Ok(serde_json::json!({}))
    })
}
fn study(
    events: &mut EventLoop<()>,
    engine: &Engine,
    operation: fn(i64) -> motion_host::Result<Value>,
) {
    receive(
        events,
        &engine
            .request(|reply| Command::TestStudy { operation, reply })
            .unwrap(),
    );
}
fn warm(engine: &Engine, minimum_completed: u64) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if status["cache"]["resident_frames"].as_u64() == Some(2)
            && status["completed"].as_u64().unwrap() >= minimum_completed
        {
            return state;
        }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn solid(color: [f32; 4]) -> motion_core::Project {
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Owned control",
        [16.; 2],
        [8., 8., 0.],
        color,
    ));
    project
}
fn pixels(front: &Value, expected: [u64; 4]) {
    assert!(
        front["pixels"]
            .as_array()
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel == expected),
        "{front}"
    );
}

#[test]
#[ignore = "explicit native prerender policy-pressure and project switch/reopen control"]
fn native_pressure_holds_admission_and_project_reopen_preserves_pixel_ownership() {
    const NAME: &str = "engine::prerender::tests::pressure_lifecycle::native_pressure_holds_admission_and_project_reopen_preserves_pixel_ownership";
    const CHILD: &str = "MOTION_PRERENDER_LIFECYCLE_CHILD";
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
        if let Some(target) = std::env::var_os("MOTION_PRERENDER_LIFECYCLE_ARTIFACTS") {
            let target = PathBuf::from(target);
            std::fs::create_dir_all(&target).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, target.join(name)).unwrap();
                }
            }
            std::fs::write(target.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(target.join("stderr.log"), &output.stderr).unwrap();
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
    let directory = root.join("default");
    let red = solid([1., 0., 0., 1.]);
    motion_core::storage::save(&directory, &red).unwrap();
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
    study(&mut events, &engine, high);
    present(&mut events, &engine);
    let control = crate::mcp::ToolRouter {
        engine: &engine,
        access: crate::mcp::Access::Edit,
    };
    control
        .call(
            "configure_prerender",
            serde_json::json!({"enabled":true,"window":2}),
        )
        .unwrap();
    let red_warm = warm(&engine, 2);
    engine.seek(1.).unwrap();
    present(&mut events, &engine);
    let red_front = read_front(&mut events, &engine);
    pixels(&red_front, [255, 0, 0, 255]);
    assert_eq!(red_front["cachedPresented"], 1);

    study(&mut events, &engine, pressure);
    let began = Instant::now();
    let held = loop {
        let state = engine.state().unwrap();
        assert!(state["desktop"]["prerender"]["fault"].is_null(), "{state}");
        let observation = engine.prerender_observation().unwrap().unwrap();
        if state["desktop"]["prerender"]["hold"] == "pressure"
            && observation.in_flight.is_none()
            && !observation.completion_waiting
            && observation.retained_cpu_bytes == 0
        {
            break state;
        }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    };
    std::thread::sleep(Duration::from_millis(150));
    let held_again = engine.state().unwrap();
    assert_eq!(
        held_again["desktop"]["prerender"]["started"],
        held["desktop"]["prerender"]["started"]
    );
    assert_eq!(held_again["desktop"]["prerender"]["pending"], 0);
    assert_eq!(
        engine
            .prerender_observation()
            .unwrap()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
    engine.seek(2.).unwrap();
    present(&mut events, &engine);
    let pressure_front = read_front(&mut events, &engine);
    pixels(&pressure_front, [255, 0, 0, 255]);
    assert_eq!(
        pressure_front["cachedPresented"],
        red_front["cachedPresented"]
    );
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(&red).unwrap()
    );

    study(&mut events, &engine, release_pressure);
    // Supply forward demand; backward lookahead at frame zero is empty by design.
    engine.seek(3.).unwrap();
    let recovered = warm(
        &engine,
        held_again["desktop"]["prerender"]["completed"]
            .as_u64()
            .unwrap()
            + 2,
    );
    engine.seek(4.).unwrap();
    present(&mut events, &engine);
    let recovered_front = read_front(&mut events, &engine);
    pixels(&recovered_front, [255, 0, 0, 255]);
    assert_eq!(recovered_front["cachedPresented"], 2);

    let green = solid([0., 1., 0., 1.]);
    engine
        .new_project(serde_json::to_string(&green).unwrap())
        .unwrap();
    study(&mut events, &engine, high);
    present(&mut events, &engine);
    let green_cold = read_front(&mut events, &engine);
    pixels(&green_cold, [0, 255, 0, 255]);
    let before_green = engine.state().unwrap()["desktop"]["prerender"]["completed"]
        .as_u64()
        .unwrap();
    engine.seek(1.).unwrap();
    let green_warm = warm(&engine, before_green + 2);
    engine.seek(2.).unwrap();
    present(&mut events, &engine);
    let green_front = read_front(&mut events, &engine);
    pixels(&green_front, [0, 255, 0, 255]);
    assert!(green_front["cachedPresented"].as_u64() > green_cold["cachedPresented"].as_u64());
    engine.open_project("default".into()).unwrap();
    study(&mut events, &engine, high);
    present(&mut events, &engine);
    let reopened_front = read_front(&mut events, &engine);
    pixels(&reopened_front, [255, 0, 0, 255]);
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(&red).unwrap()
    );
    control
        .call("configure_prerender", serde_json::json!({"enabled":false}))
        .unwrap();
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "redWarm":red_warm,"redFront":red_front,"pressure":held,"pressureAfter":held_again,
        "pressureFront":pressure_front,"recovered":recovered,"recoveredFront":recovered_front,
        "greenWarm":green_warm,"greenCold":green_cold,"greenFront":green_front,"reopened":reopened_front,
        "limits":"Injected typed preview refusal and explicit fixture release; actual native pixels and project lifecycle; not physical low-memory-device qualification"
    })).unwrap()).unwrap();
}
