use super::*;

fn published(token: &YieldControl) {
    let began = Instant::now();
    while !token.completed() {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "native result did not publish"
        );
        std::thread::yield_now();
    }
}

#[test]
fn foreground_yield_before_enqueue_refuses_preparation_without_source_work() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let mut worker = Worker::start(3072).unwrap();
    let token = YieldControl::default();
    assert!(token.preempt());
    assert_eq!(
        worker
            .prepare_with_yield(plan(&session), token.clone())
            .unwrap_err(),
        "movie export cancelled"
    );
    assert!(!token.completed());
    assert!(!worker.snapshot().unwrap().completion_waiting);
    assert!(worker.snapshot().unwrap().in_flight.is_none());
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    stop(&mut worker);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn completed_native_frame_survives_foreground_yield_but_not_strong_invalidation() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let mut worker = Worker::start(3072).unwrap();
    let prepare = YieldControl::default();
    worker
        .prepare_with_yield(plan(&session), prepare.clone())
        .unwrap()
        .unwrap();
    published(&prepare);
    assert!(!prepare.preempt());
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    assert_eq!(
        worker
            .prepare_with_yield(plan(&session), prepare)
            .unwrap_err(),
        "prerender yield token already submitted"
    );
    assert!(worker.profile.is_some());

    let token = YieldControl::default();
    let id = worker
        .render_with_yield(1., token.clone())
        .unwrap()
        .unwrap();
    published(&token);
    assert!(!token.preempt());
    token.control().check().unwrap();
    let result = poll(&mut worker);
    assert_eq!(result.id, id);
    let Outcome::Frame(frame) = result.outcome else {
        panic!("foreground yield discarded a completed native frame");
    };
    assert_eq!(frame.image().pixels, [255, 0, 0, 255].repeat(32 * 24));
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 3072);
    drop(frame);
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    assert_eq!(
        worker.render_with_yield(2., token).unwrap_err(),
        "prerender yield token already submitted"
    );
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);

    let invalidated = YieldControl::default();
    worker
        .render_with_yield(2., invalidated.clone())
        .unwrap()
        .unwrap();
    published(&invalidated);
    assert!(!invalidated.preempt());
    assert!(invalidated.control().cancel());
    assert!(matches!(poll(&mut worker).outcome, Outcome::Cancelled));
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    stop(&mut worker);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn running_native_draw_yields_but_keeps_its_cpu_slot_until_acknowledgment() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let mut worker = Worker::start_with_phase_hook(3072, move |_, phase| {
        if phase == "draw" {
            entered.send(()).unwrap();
            blocked
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
        }
    })
    .unwrap();
    worker.prepare(plan(&session)).unwrap().unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
    let token = YieldControl::default();
    worker
        .render_with_yield(1., token.clone())
        .unwrap()
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(!token.completed());
    assert!(token.preempt());
    assert!(worker.poll().unwrap().is_none());
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 3072);
    assert!(worker
        .render_with_yield(2., YieldControl::default())
        .unwrap()
        .is_none());
    release.send(()).unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Cancelled));
    assert!(token.completed());
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    stop(&mut worker);
}
