use super::{last_error_observation_tests::Stub, *};
use std::sync::atomic::AtomicUsize;

struct Capture(AtomicUsize);

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record
            .args()
            .to_string()
            .contains("site=host.session.last_error")
        {
            let registry = sessions().try_lock().expect("log holds registry");
            for slot in registry.values() {
                assert!(
                    !matches!(
                        slot.session.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ),
                    "log holds session"
                );
            }
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn flush(&self) {}
}

#[test]
fn handler_poison_is_local_and_retained_slots_do_not_keep_session_resources() {
    const TEST: &str = "session::registry_scope_tests::handler_poison_is_local_and_retained_slots_do_not_keep_session_resources";
    const CHILD: &str = "MOTION_SESSION_REGISTRY_SCOPE_TEST";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let capture = Box::leak(Box::new(Capture(AtomicUsize::new(0))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    let platform: Arc<dyn Platform> = Arc::new(Stub);
    let resources = Arc::downgrade(&platform);
    let id = open(
        root.path().into(),
        Project::new(64, 64, 30, 120).unwrap(),
        platform,
    )
    .unwrap();
    // A report may retain its entry after the owning thread closes the session.
    let retained = sessions().lock().unwrap().get(&id).cloned().unwrap();
    let report_root = root.path().to_path_buf();
    let mut report = None;
    let registry_available = with_session(id, |_| {
        let references = Arc::strong_count(&retained);
        report = Some(thread::spawn(move || resource_info(&report_root)));
        // The report's retained entry proves it has passed registry lookup.
        // Its per-session read cannot finish while this handler owns the lock.
        let began = Instant::now();
        while Arc::strong_count(&retained) == references
            && began.elapsed() < std::time::Duration::from_secs(5)
        {
            thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(Arc::strong_count(&retained) > references && sessions().try_lock().is_ok())
    })
    .unwrap();
    // Release the handler and join the report before testing the failure case.
    assert_eq!(report.unwrap().join().unwrap().unwrap()["sessions"], 1);
    assert!(registry_available, "resource report retained registry");
    with_session(id, |session| {
        session.last_error = Some("controlled retained failure".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(capture.0.load(Ordering::Relaxed), 1);
    let failure = std::panic::catch_unwind(|| {
        with_session(id, |session| -> Result<()> {
            assert!(sessions().try_lock().is_ok(), "handler holds registry");
            session.frame = 19.;
            panic!("controlled session poison");
        })
    });
    assert!(failure.is_err());
    assert_eq!(
        failure.unwrap_err().downcast_ref::<&str>(),
        Some(&"controlled session poison")
    );
    assert!(!sessions().is_poisoned());
    assert!(retained.session.is_poisoned());
    assert_eq!(with_session(id, |session| Ok(session.frame)).unwrap(), 19.);
    assert!(resources.upgrade().is_some());
    close(id);
    assert!(
        resources.upgrade().is_none(),
        "snapshot retained session resources"
    );
    thread::spawn(move || {
        assert!(retained
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none());
        drop(retained);
    })
    .join()
    .unwrap();
}
