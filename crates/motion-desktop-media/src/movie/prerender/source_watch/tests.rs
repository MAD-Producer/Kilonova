use super::*;
use std::{fs, sync::Barrier, thread, time::Duration};

fn wait_cancelled(observation: &Observation, control: &ExportControl) {
    let began = Instant::now();
    while observation.valid() || control.check().is_ok() {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "native source event did not invalidate and cancel: {:?}",
            observation.snapshot()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    let relative = PathBuf::from("materials/source.bin");
    fs::create_dir_all(root.join("materials")).unwrap();
    fs::write(root.join(&relative), b"old!").unwrap();
    (temporary, root, relative)
}

#[test]
fn procedural_sources_do_not_require_an_os_watcher_and_keep_real_job_lifetimes() {
    let temporary = tempfile::tempdir().unwrap();
    let calls = std::cell::Cell::new(0);
    let watch = SourceWatch::arm_with_backend(201, temporary.path(), &[], |_| {
        calls.set(calls.get() + 1);
        Err(notify::Error::generic("native watcher unavailable"))
    });
    assert!(watch.is_ok(), "procedural sources must not depend on a native watcher");
    assert_eq!(calls.get(), 0, "no OS backend or event thread is needed");
    let watch = watch.unwrap();
    let observation = watch.observation();
    assert!(observation.valid());
    let first_control = ExportControl::default();
    let first = observation.attach(first_control.clone()).unwrap().unwrap();
    assert!(observation.attach(ExportControl::default()).is_err());
    drop(first);
    assert!(first_control.check().is_ok());
    let next_control = ExportControl::default();
    let next = observation.attach(next_control.clone()).unwrap().unwrap();
    drop(watch);
    assert!(!observation.valid());
    assert!(observation.snapshot().unwrap().retired);
    assert!(next_control.check().is_err());
    assert!(observation.attach(ExportControl::default()).unwrap().is_none());
    drop(next);
    assert_eq!(observation.snapshot().unwrap().active_ticket, None);
}

#[test]
fn visual_sources_still_require_the_backend_and_preserve_its_original_failure() {
    let (_temporary, root, relative) = fixture();
    let calls = std::cell::Cell::new(0);
    let result = SourceWatch::arm_with_backend(202, &root, &[relative], |_| {
        calls.set(calls.get() + 1);
        Err(notify::Error::generic("owned backend failure 202"))
    });
    assert_eq!(calls.get(), 1);
    let error = result.err().expect("a visual source cannot bypass watch setup failure");
    assert!(error.contains("create native source watcher"), "{error}");
    assert!(error.contains("owned backend failure 202"), "{error}");
}

#[test]
fn lease_retains_one_real_job_until_ack_and_refuses_stale_reuse() {
    let observation = Observation::new(17);
    let first_control = ExportControl::default();
    let lease = observation.attach(first_control.clone()).unwrap().unwrap();
    assert_eq!(observation.snapshot().unwrap().active_ticket, Some(1));
    assert!(observation.attach(ExportControl::default()).is_err());
    assert!(observation.finish(2).is_err());
    assert_eq!(observation.snapshot().unwrap().active_ticket, Some(1));
    drop(lease);
    assert_eq!(observation.snapshot().unwrap().active_ticket, None);
    let next_control = ExportControl::default();
    let next = observation.attach(next_control.clone()).unwrap().unwrap();
    assert_eq!(observation.snapshot().unwrap().active_ticket, Some(2));
    observation.invalidate("replacement".into(), false);
    assert!(next_control.check().is_err());
    assert!(
        first_control.check().is_ok(),
        "an acknowledged job must not be cancelled"
    );
    assert!(observation
        .attach(ExportControl::default())
        .unwrap()
        .is_none());
    observation.invalidate("later unrelated fault".into(), true);
    assert_eq!(
        observation.snapshot().unwrap().cause.as_deref(),
        Some("replacement")
    );
    drop(next);
}

#[test]
fn a_normal_change_cannot_hide_a_later_watcher_fault_before_retirement() {
    let observation = Observation::new(18);
    let control = ExportControl::default();
    let lease = observation.attach(control.clone()).unwrap().unwrap();
    observation.invalidate("normal source replacement".into(), false);
    assert!(control.check().is_err());
    assert!(!observation.snapshot().unwrap().fault);
    observation.invalidate("native watcher lost its event stream".into(), true);
    let snapshot = observation.snapshot().unwrap();
    assert!(
        snapshot.fault,
        "a prior normal change must not hide a real fault"
    );
    assert_eq!(snapshot.cause.as_deref(), Some("normal source replacement"));
    assert!(
        observation
            .attach(ExportControl::default())
            .unwrap()
            .is_none()
    );
    drop(lease);
    observation.retire();
    observation.invalidate("retired callback".into(), true);
    assert!(observation.snapshot().unwrap().retired);
}

#[test]
fn fault_after_change_is_logged_once_at_info_without_holding_observation_lock() {
    const NAME: &str = "movie::prerender::source_watch::tests::fault_after_change_is_logged_once_at_info_without_holding_observation_lock";
    const CHILD: &str = "MOTION_SOURCE_FAULT_ORDER_LOG_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
        return;
    }
    struct Capture {
        observation: Observation,
        records: Mutex<Vec<(log::Level, String)>>,
    }
    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                assert!(
                    self.observation.0.state.try_lock().is_ok(),
                    "logger called with observation lock held"
                );
                let mut records = self.records.lock().unwrap();
                assert!(
                    records.len() < 4,
                    "repeated source events must be suppressed"
                );
                records.push((record.level(), record.args().to_string()));
            }
        }
        fn flush(&self) {}
    }
    let observation = Observation::new(8204);
    let logger = Box::leak(Box::new(Capture {
        observation: observation.clone(),
        records: Mutex::new(Vec::new()),
    }));
    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    observation.invalidate("normal replacement".into(), false);
    let fault = format!("native stream failure {}", "证".repeat(2000));
    observation.invalidate(fault.clone(), true);
    for _ in 0..128 {
        observation.invalidate("repeated normal event".into(), false);
        observation.invalidate("repeated fault".into(), true);
    }
    observation.retire();
    observation.invalidate("late retired fault".into(), true);
    let records = logger.records.lock().unwrap();
    assert_eq!(records.len(), 2, "one change and one terminal fault only");
    assert_eq!(records[0].0, log::Level::Info);
    assert!(records[0].1.contains("normal replacement"));
    assert_eq!(records[1].0, log::Level::Error);
    assert!(
        records[1]
            .1
            .ends_with(&fault.chars().take(MAX_CAUSE_CHARS).collect::<String>())
    );
    assert!(!records[1].1.contains("repeated fault"));
    assert!(observation.snapshot().unwrap().fault);
}

#[test]
fn attachment_racing_a_change_cannot_escape_cancellation() {
    for request in 0..32 {
        let observation = Observation::new(request);
        let control = ExportControl::default();
        let barrier = Arc::new(Barrier::new(2));
        let other = observation.clone();
        let wait = barrier.clone();
        let thread = thread::spawn(move || {
            wait.wait();
            other.invalidate("concurrent replacement".into(), false);
        });
        barrier.wait();
        let lease = observation.attach(control.clone()).unwrap();
        thread.join().unwrap();
        assert!(!observation.valid());
        if lease.is_some() {
            assert!(control.check().is_err());
        } else {
            assert_eq!(observation.snapshot().unwrap().active_ticket, None);
        }
    }
}

#[test]
fn retired_observation_cannot_cancel_a_new_closure_and_retains_bounded_first_cause() {
    let old = Observation::new(1);
    let control = ExportControl::default();
    let _lease = old.attach(control.clone()).unwrap().unwrap();
    old.invalidate("x".repeat(10000), true);
    assert_eq!(
        old.snapshot().unwrap().cause.unwrap().len(),
        MAX_CAUSE_CHARS
    );
    old.retire();
    assert!(old.snapshot().unwrap().retired);
    assert!(old.attach(ExportControl::default()).unwrap().is_none());
    let new = Observation::new(2);
    let fresh = ExportControl::default();
    let _new_lease = new.attach(fresh.clone()).unwrap().unwrap();
    old.invalidate("late old callback".into(), true);
    assert!(new.valid());
    assert!(fresh.check().is_ok());
    new.retire();
    assert!(fresh.check().is_err());
}

#[test]
fn scopes_include_source_and_ancestor_replacement_but_ignore_access_and_sibling_logs() {
    let (temporary, root, relative) = fixture();
    let scopes = Scopes::new(&root, &[relative.clone()]).unwrap();
    let root = root.canonicalize().unwrap();
    let changed = |p| Event::new(EventKind::Any).add_path(p);
    assert!(scopes.relevant(&changed(root.join(&relative))));
    assert!(scopes.relevant(&changed(root.join("materials"))));
    assert!(scopes.relevant(&changed(root.clone())));
    assert!(!scopes.relevant(&changed(
        temporary
            .path()
            .canonicalize()
            .unwrap()
            .join("motion-studio.log")
    )));
    assert!(!scopes.relevant(
        &Event::new(EventKind::Access(notify::event::AccessKind::Any))
            .add_path(root.join(relative))
    ));
    assert!(
        scopes.relevant(&Event::new(EventKind::Any)),
        "lost path information fails closed"
    );
    assert!(scopes.relevant(
        &Event::new(EventKind::Access(notify::event::AccessKind::Close(
            notify::event::AccessMode::Write
        )))
        .add_path(root.join("materials/source.bin"))
    ));
    assert!(
        scopes.relevant(
            &Event::new(EventKind::Access(notify::event::AccessKind::Any))
                .add_path(temporary.path().join("unrelated.log"))
                .set_flag(notify::event::Flag::Rescan)
        ),
        "rescan precedes kind/path filtering"
    );
    #[cfg(windows)]
    {
        assert!(path_matches(
            Path::new(r"E:\Project\Source.png"),
            Path::new(r"\\?\e:\project\source.PNG")
        ));
        assert!(path_matches(
            Path::new(r"\\server\share\file"),
            Path::new(r"\\?\UNC\server\share\FILE")
        ));
        assert!(!path_matches(Path::new("unrelated"), Path::new("素材")));
        assert!(path_matches(
            Path::new("素材/Ä.PNG"),
            Path::new("素材/ä.png")
        ));
    }
}

#[test]
fn unsafe_missing_and_over_budget_inputs_are_rejected_before_trusting_sources() {
    let (_temporary, root, relative) = fixture();
    for sources in [
        vec![PathBuf::from("../outside")],
        vec![PathBuf::from("missing.bin")],
        vec![relative; MAX_SOURCES + 1],
    ] {
        assert!(SourceWatch::arm(9, &root, &sources).is_err());
    }
    let sources: Vec<_> = (0..MAX_DIRECTORIES)
        .map(|i| {
            let directory = root.join(format!("set{i}"));
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("item"), b"x").unwrap();
            PathBuf::from(format!("set{i}/item"))
        })
        .collect();
    assert!(Scopes::new(&root, &sources).is_err());
}

#[test]
fn native_atomic_replacement_cancels_even_with_equal_size_and_restored_mtime() {
    let (_temporary, root, relative) = fixture();
    let source = root.join(&relative);
    let old_handle = fs::File::open(&source).unwrap();
    let old_metadata = old_handle.metadata().unwrap();
    // Prepare the replacement before arming. Its creation cannot satisfy the
    // source event assertion; publication is the only filesystem mutation below.
    let replacement = tempfile::NamedTempFile::new_in(source.parent().unwrap()).unwrap();
    fs::write(replacement.path(), b"new!").unwrap();
    replacement
        .as_file()
        .set_times(fs::FileTimes::new().set_modified(old_metadata.modified().unwrap()))
        .unwrap();
    let watcher = SourceWatch::arm(101, &root, &[relative]).unwrap();
    let observation = watcher.observation();
    let control = ExportControl::default();
    let lease = observation.attach(control.clone()).unwrap().unwrap();
    assert!(observation.valid());
    replace_test_source(replacement, &source);
    assert_eq!(fs::metadata(&source).unwrap().len(), old_metadata.len());
    assert_eq!(
        fs::metadata(&source).unwrap().modified().unwrap(),
        old_metadata.modified().unwrap()
    );
    wait_cancelled(&observation, &control);
    assert_eq!(fs::read(&source).unwrap(), b"new!");
    let mut old_bytes = Vec::new();
    std::io::Read::read_to_end(&mut &old_handle, &mut old_bytes).unwrap();
    assert_eq!(old_bytes, b"old!", "the old reader still owns the old file");
    assert!(observation.snapshot().unwrap().cause.is_some());
    drop(lease);
    drop(watcher);
    assert!(observation.snapshot().unwrap().retired);
}

#[test]
fn native_in_place_write_and_ancestor_rename_cancel_without_an_owner_queue() {
    for rename in [false, true] {
        let (_temporary, root, relative) = fixture();
        let watcher = SourceWatch::arm(102, &root, &[relative.clone()]).unwrap();
        let observation = watcher.observation();
        let control = ExportControl::default();
        let _lease = observation.attach(control.clone()).unwrap().unwrap();
        if rename {
            fs::rename(root.join("materials"), root.join("moved")).unwrap();
        } else {
            fs::write(root.join(relative), b"new!").unwrap();
        }
        wait_cancelled(&observation, &control);
    }
}

#[test]
fn native_watcher_drop_retires_observations_and_cancels_its_owned_job() {
    let (_temporary, root, relative) = fixture();
    let watcher = SourceWatch::arm(103, &root, &[relative]).unwrap();
    let observation = watcher.observation();
    let control = ExportControl::default();
    let _lease = observation.attach(control.clone()).unwrap().unwrap();
    drop(watcher);
    assert!(!observation.valid());
    assert!(observation.snapshot().unwrap().retired);
    assert!(control.check().is_err());
}
