use super::*;
use std::sync::atomic::AtomicBool;
mod yielding;
mod disk_cache;
mod scheduling;

fn session(root: &Path) -> Session {
    let mut p = Project::new(32, 24, 30, 8).unwrap();
    p.background = [0., 0., 0., 1.];
    p.layers.push(motion_core::Layer::solid(
        1,
        "Owned worker control",
        [32., 24.],
        [16., 12., 0.],
        [1., 0., 0., 1.],
    ));
    Session::new(p, root.into(), crate::platform()).unwrap()
}
fn plan(session: &Session) -> PrerenderPlan {
    PrerenderPlan::capture(session, 1, 2, 82, 32 * 24 * 4, &ExportControl::default()).unwrap()
}
fn poll(worker: &mut Worker) -> Completion {
    let began = Instant::now();
    loop {
        if let Some(result) = worker.poll().unwrap() {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "worker did not acknowledge: {:?}",
            worker.snapshot().unwrap().in_flight
        );
        std::thread::yield_now();
    }
}
fn stop(worker: &mut Worker) {
    worker.shutdown();
    let began = Instant::now();
    while !worker.stopped() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    worker.shutdown();
}

#[test]
fn cpu_admission_is_bounded_and_follows_the_actual_completion_owner() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Observation>();
    let budget = Arc::new(CpuBudget {
        limit: 64,
        live: AtomicU64::new(0),
    });
    let first = budget.reserve(48).unwrap();
    assert_eq!(budget.live.load(Ordering::Acquire), 48);
    assert!(budget.reserve(17).is_none());
    let second = budget.reserve(16).unwrap();
    assert!(budget.reserve(1).is_none());
    drop(first);
    assert_eq!(budget.live.load(Ordering::Acquire), 16);
    let full = budget.reserve(48).unwrap();
    drop(second);
    drop(full);
    assert_eq!(budget.live.load(Ordering::Acquire), 0);
    assert!(budget.reserve(u64::MAX).is_none());
    assert!(Worker::start(0).is_err());
}

#[test]
fn caller_cancellation_published_before_enqueue_refuses_without_leaking_cpu_admission() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let mut worker = Worker::start(32 * 24 * 4).unwrap();
    let control = ExportControl::default();
    assert!(control.cancel());
    let cause = worker
        .prepare_with_control(plan(&session), control.clone())
        .unwrap_err();
    assert_eq!(cause, "movie export cancelled");
    let watch = worker.observation();
    let snapshot = thread::spawn(move || watch.snapshot().unwrap())
        .join()
        .unwrap();
    assert!(snapshot.in_flight.is_none() && !snapshot.completion_waiting);
    assert_eq!(snapshot.retained_cpu_bytes, 0);
    worker.prepare(plan(&session)).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    assert_eq!(
        worker.render_with_control(1., control).unwrap_err(),
        "movie export cancelled"
    );
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    assert!(worker.profile.is_some());
    stop(&mut worker);
}

#[test]
fn shutdown_returns_while_the_actual_worker_still_owns_a_controlled_blocked_phase() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let plan = plan(&session);
    let (entered, observe) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let (close, closing) = mpsc::sync_channel(1);
    let (returned, did_return) = mpsc::sync_channel(1);
    let owner = thread::spawn(move || {
        let wait = Mutex::new(wait);
        let mut worker = Worker::start_with_phase_hook(32 * 24 * 4, move |_, phase| {
            if phase == "freeze" {
                entered.send(()).unwrap();
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
            }
        })
        .unwrap();
        worker.prepare(plan).unwrap().unwrap();
        closing.recv_timeout(Duration::from_secs(30)).unwrap();
        worker.shutdown();
        let phase = worker.snapshot().unwrap().in_flight.unwrap().phase;
        returned
            .send((worker.stopped(), phase, worker.state.clone()))
            .unwrap();
        // Drop also must not join a running native owner.
        drop(worker);
    });
    observe.recv_timeout(Duration::from_secs(30)).unwrap();
    close.send(()).unwrap();
    let result = did_return.recv_timeout(Duration::from_secs(5));
    // Release before asserting so a broken blocking join still retires cleanly.
    release.send(()).unwrap();
    owner.join().unwrap();
    let (stopped, phase, state) = result.expect("shutdown joined a blocked worker");
    assert!(!stopped);
    assert_eq!(phase, "freeze");
    let began = Instant::now();
    while !state.lock().unwrap().stopped {
        assert!(began.elapsed() < Duration::from_secs(30));
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_worker_keeps_native_frames_cpu_quota_and_ownership_after_session_close() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let plan = plan(&session);
    drop(session);
    let mut worker = Worker::start(32 * 24 * 4).unwrap();
    let prepare = worker.prepare(plan).unwrap().unwrap();
    let reply = poll(&mut worker);
    assert_eq!(reply.id, prepare);
    let Outcome::Prepared(profile) = reply.outcome else {
        panic!("expected owned preparation");
    };
    assert_eq!(profile.context.document_generation, 1);
    let id = worker.render(1.25).unwrap().unwrap();
    assert!(worker.render(2.).unwrap().is_none());
    let complete = poll(&mut worker);
    assert_eq!(complete.id, id);
    let Outcome::Frame(frame) = complete.outcome else {
        panic!("expected actual native frame");
    };
    assert_eq!(frame.key(), profile.frame_key(1.25).unwrap());
    assert_eq!(
        frame.image().storage,
        motion_effects::image_contract::ImageStorage::Rgba8UnormSrgb
    );
    for pixel in frame.image().pixels.chunks_exact(4) {
        assert_eq!(pixel, [255, 0, 0, 255]);
    }
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 32 * 24 * 4);
    // Draining the mailbox alone must not claim the caller's real bytes freed.
    assert!(worker.render(2.).unwrap().is_none());
    drop(frame);
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    worker.render(2.).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Frame(_)));
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    stop(&mut worker);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn blocked_draw_publishes_before_work_accepts_cancellation_and_releases_only_on_ack() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let original = session.engine.snapshot();
    let (entered, observe) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let wait = Mutex::new(wait);
    let once = AtomicBool::new(false);
    let mut worker = Worker::start_with_phase_hook(32 * 24 * 4, move |_, phase| {
        if phase == "draw" && !once.swap(true, Ordering::AcqRel) {
            entered.send(()).unwrap();
            wait.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
        }
    })
    .unwrap();
    worker.prepare(plan(&session)).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    let id = worker.render(1.).unwrap().unwrap();
    observe.recv_timeout(Duration::from_secs(30)).unwrap();
    let flight = worker.snapshot().unwrap().in_flight.unwrap();
    assert_eq!(
        (flight.id, flight.operation, flight.phase, flight.frame),
        (id, "render", "draw", Some(1.))
    );
    assert_eq!(session.engine.snapshot(), original);
    assert!(worker.cancel());
    assert!(worker.poll().unwrap().is_none());
    assert!(worker.render(2.).unwrap().is_none());
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 32 * 24 * 4);
    release.send(()).unwrap();
    let canceled = poll(&mut worker);
    assert_eq!(canceled.id, id);
    assert!(matches!(canceled.outcome, Outcome::Cancelled));
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    worker.prepare(plan(&session)).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    worker.render(2.).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Frame(_)));
    stop(&mut worker);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_worker_panic_is_acknowledged_and_cannot_reuse_the_native_owner() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let mut worker = Worker::start_with_phase_hook(32 * 24 * 4, |_, phase| {
        if phase == "draw" {
            panic!("controlled native worker phase panic");
        }
    })
    .unwrap();
    worker.prepare(plan(&session)).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    let id = worker.render(1.).unwrap().unwrap();
    let failed = poll(&mut worker);
    assert_eq!(failed.id, id);
    let Outcome::Failed(cause) = failed.outcome else {
        panic!("expected native-owner failure");
    };
    assert!(cause.contains("panicked"));
    let began = Instant::now();
    while !worker.stopped() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    assert!(worker.prepare(plan(&session)).is_err());
    let snapshot = worker.snapshot().unwrap();
    assert!(snapshot.stopped && snapshot.in_flight.is_none());
    assert_eq!(snapshot.retained_cpu_bytes, 0);
    assert!(snapshot.last_failure.unwrap().contains("panicked"));
    stop(&mut worker);
}

mod logging;
