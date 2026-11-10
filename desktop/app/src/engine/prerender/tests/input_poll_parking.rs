//! Actual optional timer parking and foreground Present recovery.
use super::*;

fn idle(engine: &Engine) {
    let began = Instant::now();
    while engine.in_flight().is_some() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait(engine: &Engine, ready: impl Fn(&Value) -> bool) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if ready(status) { return state; }
        assert!(began.elapsed() < Duration::from_secs(30), "{state}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "explicit native idle input timer parking and actual Present wakeup"]
fn native_input_parks_optional_timer_after_acknowledgment_and_present_rearms_it() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1, "Input timer control", [16.; 2], [8., 8., 0.], [1., 0., 0., 1.],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(
        winit::window::Window::default_attributes().with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
    ).unwrap());
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
    receive(&mut events, &engine.request(|reply| Command::TestStudy {
        operation: |id| motion_host::session::with_session(id, |s| {
            s.preview.set_mode(motion_render::PreviewMode::High);
            Ok(serde_json::json!({}))
        }), reply,
    }).unwrap());
    present(&mut events, &engine);
    engine.configure_prerender(motion_host::prerender::Policy {
        enabled: true, window: 2, idle_delay: Duration::ZERO, ..Default::default()
    }).unwrap();
    let warmed = wait(&engine, |s| s["cache"]["resident_frames"] == 2 && s["pending"] == 0);
    // This is actual Engine protocol publication; the existing input-priority
    // control separately proves native WindowEvent -> Signals routing.
    engine.prerender_signals.input(super::super::InputActivity::Pulse).unwrap();
    let held = wait(&engine, |s| s["hold"] == "input" && s["pending"] == 0);
    let began = Instant::now();
    loop {
        let quiet = engine.prerender_observation().unwrap().is_some_and(|s|
            s.in_flight.is_none() && s.retained_cpu_bytes == 0);
        if quiet { break; }
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(10));
    }
    // State is a real worker barrier. Allow an already selected timer wake to
    // complete, then observe three complete timer intervals without Present.
    std::thread::sleep(super::super::POLL_INTERVAL * 2);
    engine.state().unwrap();
    idle(&engine);
    let before = engine.diagnostics().report();
    let last_request = before["recent"].as_array().unwrap().iter()
        .filter_map(|r| r["request"].as_u64()).max().unwrap();
    std::thread::sleep(super::super::POLL_INTERVAL * 3);
    let still_held = engine.state().unwrap();
    idle(&engine);
    let after = engine.diagnostics().report();
    assert_eq!(after["reporting"]["droppedRecords"], before["reporting"]["droppedRecords"],
        "the observation interval must not lose worker timing records");
    let optional_polls = after["recent"].as_array().unwrap().iter().filter(|r|
        r["category"] == "worker" && r["operation"] == "prerender_poll"
            && r["request"].as_u64().unwrap() > last_request).count();
    assert_eq!(optional_polls, 0,
        "acknowledged native input must park synthetic prerender timer commands");
    assert_eq!(still_held["desktop"]["prerender"]["inputPending"], true);
    assert_eq!(still_held["desktop"]["prerender"]["started"], held["desktop"]["prerender"]["started"]);
    // Require new work, not just old warm pixels surviving the input hold.
    // Seek alone cannot acknowledge input; only its successful Present can.
    receive(&mut events, &engine.seek_async(3.).unwrap());
    assert!(engine.prerender_signals.input_pending().unwrap());
    present(&mut events, &engine);
    assert!(!engine.prerender_signals.input_pending().unwrap());
    let completed = held["desktop"]["prerender"]["completed"].as_u64().unwrap();
    let started = held["desktop"]["prerender"]["started"].as_u64().unwrap();
    let recovered = wait(&engine, |s| s["hold"] != "input"
        && s["cache"]["resident_frames"] == 2 && s["pending"] == 0
        && s["started"].as_u64().unwrap() > started
        && s["completed"].as_u64().unwrap() >= completed + 2);
    let front = read_front(&mut events, &engine);
    let pixels: Vec<u8> = serde_json::from_value(front["pixels"].clone()).unwrap();
    assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    engine.configure_prerender(Default::default()).unwrap();
    if let Some(path) = std::env::var_os("MOTION_PRERENDER_PARKING_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
            "warmed":warmed,"held":held,"stillHeld":still_held,"recovered":recovered,
            "observedIdleIntervals":3,"optionalPolls":optional_polls,"before":before,"after":after,
            "limits":"Native protocol input, actual worker timer records and exact retained pixels; not native event routing or whole-app P95 qualification."
        })).unwrap()).unwrap();
    }
}
