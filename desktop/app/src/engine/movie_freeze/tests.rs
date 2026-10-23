use super::*;
use std::io::Read;

struct OwnedInput {
    file: std::fs::File,
    released: Arc<AtomicBool>,
}
impl Drop for OwnedInput {
    fn drop(&mut self) {
        self.released.store(true, Ordering::Release);
    }
}

#[test]
fn blocked_freeze_retains_one_source_owner_and_rejects_additional_workers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.bin");
    std::fs::write(&path, b"owned source bytes").unwrap();
    let released_source = Arc::new(AtomicBool::new(false));
    let source = OwnedInput {
        file: std::fs::File::open(path).unwrap(),
        released: released_source.clone(),
    };
    let observation = Arc::new(Mutex::new(None));
    let mut worker = Worker::new(observation.clone());
    let (entered, entry) = channel();
    let (release, released) = channel();
    let (reply, receive) = channel();
    CURRENT_COMMAND.set(Some(Flight {
        request: 31,
        operation: "freeze_movie",
        started: Instant::now(),
    }));
    let observed = observation.clone();
    worker
        .start(reply, ExportControl::default(), move || {
            assert_eq!(observed.lock().unwrap().unwrap().request, 31);
            let mut source = source;
            let mut bytes = vec![];
            source.file.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"owned source bytes");
            entered.send(()).unwrap();
            released.recv().unwrap();
            Ok(bytes)
        })
        .unwrap();
    CURRENT_COMMAND.set(None);
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(worker.status()["request"], 31);
    assert_eq!(worker.status()["workerFinished"], false);
    let called = Arc::new(AtomicUsize::new(0));
    for _ in 0..64 {
        let (reply, _) = channel::<Result<(), String>>();
        let called = called.clone();
        let error = worker
            .start(reply, ExportControl::default(), move || {
                called.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .unwrap_err();
        assert!(error.contains("still running or cancelling"));
    }
    assert_eq!(called.load(Ordering::Relaxed), 0);
    assert!(!released_source.load(Ordering::Acquire));
    assert_eq!(observation.lock().unwrap().unwrap().request, 31);
    release.send(()).unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        b"owned source bytes"
    );
    drop(worker);
    assert!(released_source.load(Ordering::Acquire));
    assert!(observation.lock().unwrap().is_none());
}

#[test]
fn shutdown_publishes_cancellation_then_joins_source_work_before_returning() {
    let observation = Arc::new(Mutex::new(None));
    let mut worker = Worker::new(observation.clone());
    let control = ExportControl::default();
    let checked = control.clone();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let (reply, receive) = channel::<Result<(), String>>();
    CURRENT_COMMAND.set(Some(Flight {
        request: 32,
        operation: "freeze_movie",
        started: Instant::now(),
    }));
    worker
        .start(reply, control.clone(), move || {
            entered.send(()).unwrap();
            released.recv().unwrap();
            checked.check()
        })
        .unwrap();
    CURRENT_COMMAND.set(None);
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let (exited, exit) = channel();
    let owner = std::thread::spawn(move || {
        drop(worker);
        exited.send(()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while control.check().is_ok() {
        assert!(
            Instant::now() < deadline,
            "shutdown did not cancel active snapshot IO"
        );
        std::thread::yield_now();
    }
    assert!(matches!(
        exit.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert_eq!(observation.lock().unwrap().unwrap().request, 32);
    release.send(()).unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err(),
        "movie export cancelled"
    );
    exit.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.join().unwrap();
    assert!(observation.lock().unwrap().is_none());
}

#[test]
fn completed_worker_is_reaped_and_explicit_next_request_can_recover() {
    let observation = Arc::new(Mutex::new(None));
    let mut worker = Worker::new(observation.clone());
    let (reply, receive) = channel::<Result<(), String>>();
    worker
        .start(reply, ExportControl::default(), || {
            Err("original freeze failure".into())
        })
        .unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err(),
        "original freeze failure"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.active.as_ref().unwrap().0.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    worker.available().unwrap();
    assert!(worker.active.is_none());
    let (reply, receive) = channel();
    worker
        .start(reply, ExportControl::default(), || Ok(7))
        .unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        7
    );
    drop(worker);
    assert!(observation.lock().unwrap().is_none());
}

#[test]
fn completed_snapshot_handoff_keeps_independent_export_control_usable() {
    let mut worker = Worker::new(Arc::new(Mutex::new(None)));
    let control = ExportControl::default();
    let (reply, receive) = channel();
    worker.start(reply, control.clone(), || Ok(7)).unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        7
    );
    // Do not wait for thread bookkeeping: a reply is already a public handoff.
    drop(worker);
    assert!(control.check().is_ok());
}

#[test]
fn reply_cleanup_panic_is_reported_and_clears_in_flight_before_next_request() {
    struct PanickingReply;
    impl Drop for PanickingReply {
        fn drop(&mut self) {
            panic!("controlled reply drop panic");
        }
    }
    let observation = Arc::new(Mutex::new(None));
    let mut worker = Worker::new(observation.clone());
    let (reply, receive) = channel();
    drop(receive);
    CURRENT_COMMAND.set(Some(Flight {
        request: 33,
        operation: "freeze_movie",
        started: Instant::now(),
    }));
    worker
        .start(reply, ExportControl::default(), || Ok(PanickingReply))
        .unwrap();
    CURRENT_COMMAND.set(None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.active.as_ref().unwrap().0.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let error = worker.available().unwrap_err();
    assert!(error.contains("request=Some(33)"), "{error}");
    assert!(error.contains("controlled reply drop panic"), "{error}");
    assert!(observation.lock().unwrap().is_none());
    let (reply, receive) = channel();
    worker
        .start(reply, ExportControl::default(), || Ok(7))
        .unwrap();
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        7
    );
}

#[cfg(feature = "ffmpeg")]
#[test]
fn real_snapshot_can_export_and_decode_after_engine_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let project = motion_core::Project::new(64, 64, 30, 2).unwrap();
    motion_core::storage::save(&directory, &project).unwrap();
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    let control = ExportControl::default();
    let snapshot = engine
        .freeze_movie(control.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.frames(), 2);
    drop(engine);
    control.check().unwrap();
    let output = root.path().join("snapshot.mp4");
    let result = snapshot.export(&output, &control, |_| {}).unwrap();
    assert_eq!(result.frames, 2);
    assert!(result.bytes > 0);
    let decoded = motion_desktop_media::verify_video_runtime(&output).unwrap();
    assert_eq!(decoded["decoded"], true);
    assert_eq!(decoded["sourceFrames"], 2);
}

#[cfg(feature = "ffmpeg")]
#[test]
fn actual_movie_request_survives_freezing_and_engine_exit_in_all_frame_windows() {
    let root=tempfile::tempdir().unwrap();
    let directory=root.path().join("project");
    let project=motion_core::Project::new(32,24,30,2).unwrap();
    motion_core::storage::save(&directory,&project).unwrap();
    let engine=Engine::start(directory,motion_desktop_media::platform(),8<<30).unwrap();
    let budget=engine.allocation_budget.clone();
    let control=ExportControl::default();
    let (reply,receive)=channel();
    let request=engine.dispatch(Command::FreezeMovie{control:control.clone(),reply}).unwrap();
    let snapshot=receive.recv_timeout(Duration::from_secs(15)).unwrap().unwrap();
    drop(engine);
    let output=root.path().join("observed-snapshot.mp4");
    let result=snapshot.export(&output,&control,|_|{}).unwrap();
    assert_eq!((result.width,result.height,result.frames),(32,24,2));
    assert_eq!(motion_desktop_media::verify_video_runtime(&output).unwrap()["sourceFrames"],2);
    let peaks=budget.frame_peaks().unwrap();
    assert!(peaks.active.is_empty());
    assert_eq!(peaks.recent.len(),3);
    assert!(peaks.recent.iter().all(|w|w.request==request && w.outcome==Some(motion_render::AllocationFrameOutcome::Rendered)));
    assert_eq!(peaks.recent.iter().filter_map(|w|w.source_frame).collect::<Vec<_>>(),[0.,1.]);
    assert_eq!(peaks.recent.last().unwrap().operation,motion_render::FrameOperation::ExportJob);
    assert_eq!(peaks.recent.last().unwrap().end_admitted_bytes,Some(0));
}
