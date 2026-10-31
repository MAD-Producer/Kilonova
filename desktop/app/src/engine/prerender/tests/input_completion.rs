//! A real completed native result must respect input preceding action enqueue.
use super::*;
use crate::engine::prerender::{InputActivity, Runtime, Signals};
use motion_desktop_media::movie::prerender::{PrerenderPlan, worker::YieldControl};

fn wait_for_completion(owner: &motion_desktop_media::movie::prerender::coordinator::Coordinator) {
    let began = Instant::now();
    loop {
        let observed = owner.observation().unwrap().snapshot().unwrap();
        assert!(
            observed.last_failure.is_none(),
            "{:?}",
            observed.last_failure
        );
        if observed.completion_waiting {
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "completion deadline; phase={:?} retained_cpu_bytes={}",
            observed.in_flight.as_ref().map(|flight| flight.phase),
            observed.retained_cpu_bytes
        );
        std::thread::yield_now();
    }
}

fn completed_input_boundary(id: i64) -> motion_host::Result<Value> {
    completed_input_boundary_at(id, false)
}

fn late_completed_input_boundary(id: i64) -> motion_host::Result<Value> {
    completed_input_boundary_at(id, true)
}

fn completed_input_boundary_at(id: i64, after_demand: bool) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        s.preview.set_mode(motion_render::PreviewMode::High);
        let signals = Arc::new(Signals::default());
        let mut runtime = Runtime::new(signals.clone());
        runtime
            .configure(motion_host::prerender::Policy {
                enabled: true,
                window: 1,
                idle_delay: Duration::ZERO,
                ..Default::default()
            })
            .unwrap();
        runtime.reconcile(s).unwrap();
        let renderer = &s.graphics.as_ref().unwrap().renderer;
        let plan = PrerenderPlan::capture(
            s,
            s.document_generation,
            s.surface_epoch,
            82,
            64 << 20,
            &Default::default(),
        )
        .unwrap();
        let owner = runtime.owner.as_mut().unwrap();
        assert!(
            owner
                .prepare_with_yield(plan, YieldControl::default())
                .unwrap()
        );
        wait_for_completion(owner);
        // Observation publication precedes the bounded mailbox send. Consume
        // the actual acknowledgment before asserting the prepared profile.
        let began = Instant::now();
        while owner.busy() {
            assert!(!owner.poll(renderer).unwrap());
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        assert!(owner.profile().is_some());

        let demand = runtime.demand(s, false).unwrap();
        let generation = signals.generation().unwrap();
        let token = YieldControl::default();
        runtime.lease = signals.publish_yield(generation, token.clone()).unwrap();
        assert!(runtime.lease.is_some());
        let owner = runtime.owner.as_mut().unwrap();
        owner.update(demand).unwrap();
        assert!(owner.schedule_with_yield(renderer, token.clone()).unwrap());
        wait_for_completion(owner);
        let began = Instant::now();
        while !token.completed() {
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        assert!(
            token.completed(),
            "the test requires real completed pixels, not a canceled render"
        );
        assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
        assert!(
            owner
                .observation()
                .unwrap()
                .snapshot()
                .unwrap()
                .retained_cpu_bytes
                > 0
        );
        renderer.set_submission_tracking(true);
        let submitted = renderer
            .submission_memory_info(&Default::default())
            .submitted;

        // No command or edit is enqueued. The genuine dispatch input gate is
        // the only hold; completed tokens deliberately survive preemption.
        if after_demand {
            // Keep the pre-input demand stale to select the real interleaving
            // between reconciliation and completion polling, without an edit.
            let before_input = runtime.demand(s, false).unwrap();
            runtime.owner.as_mut().unwrap().update(before_input).unwrap();
        }
        signals.input(InputActivity::Pulse).unwrap();
        assert!(signals.input_pending().unwrap());
        assert!(token.control().check().is_ok());
        if after_demand {
            let began = Instant::now();
            let owner = runtime.owner.as_mut().unwrap();
            while owner.busy() {
                assert!(
                    !owner.poll(renderer).unwrap(),
                    "late input must reject completed optional pixels after demand reconciliation"
                );
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
        }
        runtime
            .tick_inner(s, false, 83, signals.generation().unwrap())
            .unwrap();
        // Drain this same job's real reply; do not treat waiting=true as
        // proof that its completion has already entered the mailbox.
        let began = Instant::now();
        while runtime.owner.as_ref().unwrap().busy() {
            runtime
                .tick_inner(s, false, 83, signals.generation().unwrap())
                .unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        let held = runtime.status();
        assert_eq!(
            held["cache"]["resident_frames"], 0,
            "pending native input must reject completed optional pixels before upload"
        );
        assert_eq!(held["completed"], 0);
        assert_eq!(held["canceled"], 1);
        assert_eq!(held["hold"], "input");
        assert!(held["fault"].is_null());
        assert!(
            runtime.lease.is_none(),
            "real acknowledgment must release the publication slot"
        );
        assert_eq!(
            renderer
                .submission_memory_info(&Default::default())
                .submitted,
            submitted
        );
        assert_eq!(
            runtime
                .owner
                .as_ref()
                .unwrap()
                .observation()
                .unwrap()
                .snapshot()
                .unwrap()
                .retained_cpu_bytes,
            0
        );

        // An acknowledged input frame reopens admission without retiring the
        // reusable immutable native owner or losing the completion slot.
        runtime.foreground_presented(signals.preempt_for_present().unwrap());
        assert!(!signals.input_pending().unwrap());
        runtime
            .tick_inner(s, false, 84, signals.generation().unwrap())
            .unwrap();
        wait_for_completion(runtime.owner.as_ref().unwrap());
        runtime
            .tick_inner(s, false, 85, signals.generation().unwrap())
            .unwrap();
        // Drain this same job's real reply; do not treat waiting=true as
        // proof that its completion has already entered the mailbox.
        let began = Instant::now();
        while runtime.owner.as_ref().unwrap().busy() {
            runtime
                .tick_inner(s, false, 85, signals.generation().unwrap())
                .unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        let recovered = runtime.status();
        assert_eq!(recovered["cache"]["resident_frames"], 1);
        assert_eq!(recovered["completed"], 1);
        assert!(recovered["fault"].is_null());
        let frame = runtime
            .owner
            .as_mut()
            .unwrap()
            .get(renderer, 1.)
            .unwrap()
            .unwrap();
        assert_eq!(
            renderer
                .read_native_render_target(frame.source())
                .unwrap()
                .pixels,
            [255, 0, 0, 255].repeat(16 * 16)
        );
        runtime.configure(Default::default()).unwrap();
        Ok(serde_json::json!({"held":held,"recovered":recovered,
            "limits":"Actual native completion, input hold, upload exclusion and pixel recovery; not whole-app P95 acceptance"}))
    })
}

#[test]
#[ignore = "explicit native completed-result input admission and recovery control"]
fn native_completed_result_waits_for_input_acknowledgment_before_upload() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Input completion control",
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
    let report = receive(
        &mut events,
        &engine
            .request(|reply| Command::TestStudy {
                operation: completed_input_boundary,
                reply,
            })
            .unwrap(),
    );
    if let Some(path) = std::env::var_os("MOTION_PRERENDER_INPUT_COMPLETION_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "explicit native completed-result input admission and recovery control"]
fn native_late_input_rejects_completed_result_after_demand_reconciliation() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Input completion control",
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
    let report = receive(
        &mut events,
        &engine
            .request(|reply| Command::TestStudy {
                operation: late_completed_input_boundary,
                reply,
            })
            .unwrap(),
    );
    if let Some(path) = std::env::var_os("MOTION_PRERENDER_LATE_INPUT_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
