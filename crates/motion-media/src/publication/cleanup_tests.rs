use super::*;
use crate::poison_observation::tests::{case, unlocked, Capture};
use std::sync::{atomic::AtomicBool, Arc, Mutex};

#[test]
fn backup_cleanup_warnings_are_deferred_until_unlock_even_on_unwind() {
    let Some(mode) = case(
        "publication::cleanup_tests::backup_cleanup_warnings_are_deferred_until_unlock_even_on_unwind",
        &["complete", "unwind"],
    ) else { return; };
    let directory = tempfile::tempdir().unwrap();
    let task = Arc::new(Mutex::new(()));
    let observed = task.clone();
    let capture = Capture::install(move |_| unlocked(&observed));
    let sources = [
        directory.path().join("new-a"),
        directory.path().join("new-b"),
    ];
    let targets = [directory.path().join("a"), directory.path().join("b")];
    for i in 0..2 {
        fs::write(&sources[i], b"new").unwrap();
        fs::write(&targets[i], b"old").unwrap();
    }
    let mut recovery = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cleanup = PublicationCleanup::default();
        let _state = task.lock().unwrap();
        cleanup
            .publish_cache_pair((&sources[0], &targets[0]), (&sources[1], &targets[1]))
            .unwrap();
        let path = cleanup.cache.as_ref().unwrap().directory.clone();
        assert_eq!(fs::read(path.join("old-0")).unwrap(), b"old");
        fs::remove_file(path.join("old-0")).unwrap();
        fs::create_dir(path.join("old-0")).unwrap();
        recovery = Some(path);
        assert!(capture.matching("cleanup failed").is_empty());
        if mode == "unwind" {
            panic!("controlled post-publication panic");
        }
    }));
    assert_eq!(result.is_err(), mode == "unwind");
    for target in targets {
        assert_eq!(fs::read(target).unwrap(), b"new");
    }
    let warnings = capture.matching("cleanup failed");
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().all(|(level, _)| *level == log::Level::Warn));
    let recovery = recovery.unwrap();
    assert!(recovery.join("old-0").is_dir());
    assert!(!recovery.join("old-1").exists());
}

struct ObserveBeforeUnlock<F: Fn()>(F);
impl<F: Fn()> Drop for ObserveBeforeUnlock<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

#[test]
fn unwind_rolls_back_before_unlock_and_emits_restore_failure_after_unlock() {
    let Some(mode) = case(
        "publication::cleanup_tests::unwind_rolls_back_before_unlock_and_emits_restore_failure_after_unlock",
        &["restore", "restore-failure"],
    ) else { return; };
    let directory = tempfile::tempdir().unwrap();
    let task = Arc::new(Mutex::new(()));
    let observed = task.clone();
    let capture = Capture::install(move |_| unlocked(&observed));
    let target = directory.path().join("cache.pts");
    let source = directory.path().join("new.pts");
    fs::write(&target, b"old timestamps").unwrap();
    fs::write(&source, b"new timestamps").unwrap();
    let before_unlock = AtomicBool::new(false);
    let mut recovery = None;
    let failure = mode == "restore-failure";
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut cleanup = PublicationCleanup::default();
        cleanup.cache = Some(Publication::new(directory.path()).unwrap());
        recovery = Some(cleanup.cache.as_ref().unwrap().directory.clone());
        let _state = task.lock().unwrap();
        let _check = ObserveBeforeUnlock(|| {
            let restored = fs::read(&target).is_ok_and(|bytes| bytes == b"old timestamps");
            before_unlock.store(restored, Ordering::Release);
        });
        let attempt = Attempt(cleanup.cache.as_mut().unwrap());
        attempt.0.publish(&source, &target).unwrap();
        if failure {
            fs::remove_file(&target).unwrap();
            fs::create_dir(&target).unwrap();
        }
        assert!(capture.matching("cache rollback failed").is_empty());
        panic!("controlled between-file publication panic");
    }));
    assert_eq!(
        crate::panic_reason(result.unwrap_err()),
        "controlled between-file publication panic"
    );
    assert_eq!(before_unlock.load(Ordering::Acquire), !failure);
    let errors = capture.matching("cache rollback failed");
    assert_eq!(errors.len(), usize::from(failure));
    let recovery = recovery.unwrap();
    if failure {
        assert_eq!(errors[0].0, log::Level::Error);
        assert!(errors[0].1.contains(&recovery.display().to_string()));
        assert_eq!(fs::read(recovery.join("old-0")).unwrap(), b"old timestamps");
        assert!(target.is_dir());
    } else {
        assert!(!recovery.exists());
        assert_eq!(fs::read(target).unwrap(), b"old timestamps");
    }
}

#[test]
fn import_rollback_preserves_primary_and_bounds_deferred_evidence() {
    let Some(_) = case(
        "publication::cleanup_tests::import_rollback_preserves_primary_and_bounds_deferred_evidence",
        &["rollback"],
    ) else { return; };
    let directory = tempfile::tempdir().unwrap();
    let task = Arc::new(Mutex::new(()));
    let observed = task.clone();
    let capture = Capture::install(move |_| unlocked(&observed));
    let paths: Vec<_> = (0..10)
        .map(|i| directory.path().join(format!("{}-{i}", "x".repeat(300))))
        .collect();
    let borrowed: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    let mut cleanup = PublicationCleanup::default();
    let state = task.lock().unwrap();
    let error = cleanup.rollback_files("original save failure", &borrowed);
    assert!(error.starts_with("original save failure; rollback remove "));
    for path in paths {
        assert!(error.contains(&path.display().to_string()));
    }
    assert!(cleanup.rollback_warning.as_ref().unwrap().chars().count() <= 2048);
    assert!(capture.matching("rollback remove").is_empty());
    drop(state);
    drop(cleanup);
    let warnings = capture.matching("rollback remove");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].0, log::Level::Warn);
    assert!(warnings[0].1.chars().count() <= 2048);
}
