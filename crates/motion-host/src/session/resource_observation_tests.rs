use super::{last_error_observation_tests::Stub, *};

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        let text = record.args().to_string();
        if text.starts_with("resource report unresolved roots ") {
            assert!(
                !matches!(
                    sessions().try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ),
                "registry held during resource log"
            );
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}

#[test]
fn unresolved_resource_roots_are_visible_bounded_and_keep_report_scope() {
    const TEST: &str = "session::resource_observation_tests::unresolved_resource_roots_are_visible_bounded_and_keep_report_scope";
    const CHILD: &str = "MOTION_RESOURCE_ROOT_TEST";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("site=host.resource_info.root"));
        return;
    }
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let project = Project::new(64, 64, 30, 120).unwrap();
    let a = open(first.clone(), project.clone(), Arc::new(Stub)).unwrap();
    let b = open(second.clone(), project, Arc::new(Stub)).unwrap();
    for _ in 0..120 {
        assert_eq!(resource_info(temp.path()).unwrap()["sessions"], 2);
    }
    assert_eq!(resource_info(&first).unwrap()["sessions"], 1);
    assert!(capture.0.lock().unwrap().is_empty());
    with_session(a, |s| {
        s.root = temp.path().join("missing");
        Ok(())
    })
    .unwrap();
    for _ in 0..120 {
        let report = resource_info(temp.path()).unwrap();
        assert_eq!(report["sessions"], 1);
        assert_eq!(report["graphics"], 0);
        assert_eq!(report["assetTextureBytes"], 0);
        assert_eq!(report["renderTargetBytes"], 0);
    }
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("count=1 "));
        assert!(records[1].1.contains("count=60 "));
        assert!(records
            .iter()
            .all(|(_, s)| s.contains("unresolved_roots=1 ")
                && s.contains(&format!("first_session={a} "))
                && s.contains("kind=NotFound")
                && s.contains("reason=")
                && s.contains("requested_scope_membership=unknown")));
    }
    // A different requested scope must not hide the unresolved global root.
    for _ in 0..120 {
        assert_eq!(resource_info(&second).unwrap()["sessions"], 1);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 2);
    with_session(a, |s| {
        s.root = first.clone();
        Ok(())
    })
    .unwrap();
    assert_eq!(resource_info(temp.path()).unwrap()["sessions"], 2);
    with_session(a, |s| {
        s.root = temp.path().join("missing-again");
        Ok(())
    })
    .unwrap();
    for _ in 0..59 {
        assert_eq!(resource_info(temp.path()).unwrap()["sessions"], 1);
    }
    assert_eq!(capture.0.lock().unwrap().len(), 3);
    resource_info(temp.path()).unwrap();
    assert_eq!(capture.0.lock().unwrap().len(), 4);
    with_session(a, |s| {
        s.root = first.clone();
        Ok(())
    })
    .unwrap();
    resource_info(temp.path()).unwrap();
    // Real host canonicalization fails for this uncreated, oversized Unicode path.
    let oversized = first.join("long-敏".repeat(10000));
    with_session(b, |s| {
        s.root = oversized.clone();
        Ok(())
    })
    .unwrap();
    assert_eq!(resource_info(temp.path()).unwrap()["sessions"], 1);
    {
        let records = capture.0.lock().unwrap();
        let text = &records.last().unwrap().1;
        assert_eq!(records.len(), 5);
        assert!(text.chars().count() < 850);
        assert!(!text.contains(&oversized.to_string_lossy().to_string()));
        assert!(text.contains("reason="));
    }
    assert!(resource_info(&temp.path().join("invalid-query-root")).is_err());
    with_session(b, |s| {
        s.root = second;
        Ok(())
    })
    .unwrap();
    assert_eq!(resource_info(temp.path()).unwrap()["sessions"], 2);
}
