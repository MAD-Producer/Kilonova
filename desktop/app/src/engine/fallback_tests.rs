use super::*;
#[test]
fn poisoned_engine_caches_keep_drop_policy_and_terminal_messages() {
    const CHILD: &str = "MOTION_ENGINE_CACHE_POISON_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let pool = engine.paint_pool.clone();
        assert!(std::thread::spawn(move || {
            let _guard = pool.lock().unwrap();
            panic!("controlled paint pool poison");
        })
        .join()
        .is_err());
        for _ in 0..120 {
            let paint = engine.take_paint();
            assert!(paint.order.is_empty());
            engine.recycle_paint(paint);
            recycle_paint_pool(
                &engine.paint_pool,
                motion_ui::paint::PaintList::default(),
                true,
            );
        }
        assert!(engine
            .paint_pool
            .lock()
            .err()
            .expect("poison retained")
            .into_inner()
            .is_empty());
        let failure = engine.failure.clone();
        assert!(std::thread::spawn(move || {
            let _guard = failure.lock().unwrap();
            panic!("controlled failure lock poison");
        })
        .join()
        .is_err());
        for _ in 0..120 {
            assert!(engine.check_running().is_ok());
            assert!(engine.stopped_error().contains("reopen the project"));
        }
        *engine.failure.lock().unwrap_err().into_inner() = Some("original terminal reason".into());
        assert_eq!(
            engine.check_running().unwrap_err(),
            "original terminal reason"
        );
        assert_eq!(engine.stopped_error(), "original terminal reason");
        // This fixture injected a terminal value into a live worker. Restore
        // its lifecycle invariant before Drop sends the ordinary Shutdown.
        *engine.failure.lock().unwrap_err().into_inner() = None;
        let text = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        for site in [
            "engine.paint.take",
            "engine.paint.recycle",
            "engine.paint.worker_recycle",
            "engine.failure.check",
            "engine.failure.stopped",
        ] {
            let records = text
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect::<Vec<_>>();
            assert_eq!(records.len(), 2, "{site}: {text}");
            assert!(records[0].contains("WARN") && records[1].contains("ERROR"));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine::fallback_tests::poisoned_engine_caches_keep_drop_policy_and_terminal_messages",
        ])
        .env_remove("MOTION_LOG")
        .env(CHILD, temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=engine.paint.take"));
}

#[test]
fn bounded_dispatch_and_shutdown_emit_after_releasing_the_flight_gate() {
    const CHILD: &str = "MOTION_ENGINE_BOUNDED_LOG_GATE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        struct Probe {
            flight: Arc<Mutex<Option<Flight>>>,
            failure: Failure,
            active: AtomicBool,
            records: Mutex<Vec<String>>,
        }
        impl log::Log for Probe {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, record: &log::Record<'_>) {
                if !self.active.load(Ordering::Acquire) {
                    return;
                }
                let text = record.args().to_string();
                if !(text.contains("site=engine.failure.")
                    || text.starts_with("dispatch #")
                    || text.starts_with("shutdown will follow"))
                {
                    return;
                }
                for available in [
                    !matches!(
                        self.flight.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ),
                    !matches!(
                        self.failure.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ),
                ] {
                    assert!(
                        available,
                        "logger entered while a business lock was held: {text}"
                    );
                }
                self.records.lock().unwrap().push(text);
            }
            fn flush(&self) {}
        }
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            dir.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let (entered, waiting) = channel();
        let (release, gate) = channel();
        let blocked = engine
            .request(|reply| Command::TestWait {
                entered,
                release: gate,
                reply,
            })
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let probe = Box::leak(Box::new(Probe {
            flight: engine.in_flight.clone(),
            failure: engine.failure.clone(),
            active: AtomicBool::new(true),
            records: Mutex::new(Vec::new()),
        }));
        log::set_logger(probe).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
        let failure = engine.failure.clone();
        assert!(std::thread::spawn(move || {
            let _guard = failure.lock().unwrap();
            panic!("controlled failure-state poison during a blocked command");
        })
        .join()
        .is_err());
        let mut pending = Vec::new();
        for _ in 0..COMMAND_QUEUE_CAPACITY {
            pending.push(engine.request(Command::State).unwrap());
        }
        assert_eq!(engine.queue_depth(), COMMAND_QUEUE_CAPACITY);
        for _ in 0..120 {
            let began = Instant::now();
            assert!(engine
                .request(Command::State)
                .unwrap_err()
                .contains("command was not queued"));
            assert!(began.elapsed() < Duration::from_secs(1));
        }
        assert_eq!(engine.queue_depth(), COMMAND_QUEUE_CAPACITY);
        engine.request_shutdown().unwrap();
        engine.request_shutdown().unwrap();
        assert!(engine
            .request(Command::State)
            .unwrap_err()
            .contains("shutdown requested"));
        let records = probe.records.lock().unwrap().clone();
        let recovery = records
            .iter()
            .filter(|l| l.contains("site=engine.failure.dispatch "))
            .collect::<Vec<_>>();
        assert_eq!(recovery.len(), 2);
        assert!(recovery[0].contains("count=1 ") && recovery[1].contains("count=60 "));
        assert!(records
            .iter()
            .any(|l| l.starts_with("shutdown will follow")));
        probe.active.store(false, Ordering::Release);
        release.send(()).unwrap();
        blocked
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        for reply in pending {
            reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !engine.worker.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(engine.queue_depth(), 0);
        assert!(engine.in_flight().is_none());
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine::fallback_tests::bounded_dispatch_and_shutdown_emit_after_releasing_the_flight_gate"])
        .env(CHILD,"1").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
