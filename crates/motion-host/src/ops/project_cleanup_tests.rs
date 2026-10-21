use super::*;
use std::path::Path;
use std::{
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Mutex,
};

fn owned(base: &Path, name: &str) -> (PendingProjectDirectory, PathBuf) {
    let base = base.canonicalize().unwrap();
    let path = base.join(name);
    fs::create_dir(&path).unwrap();
    fs::write(path.join("project.json"), b"owned bytes").unwrap();
    let mut pending = PendingProjectDirectory::default();
    pending.own(base, path.clone());
    (pending, path)
}

#[test]
fn rollback_removes_only_owned_directory_and_retains_original_error() {
    let library = tempfile::tempdir().unwrap();
    let foreign = library.path().join("existing-project");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("keep"), b"existing bytes").unwrap();
    let (pending, path) = owned(library.path(), "project-1");
    let result: Result<()> = pending.finish(Err("original replace failure".into()));
    assert_eq!(result.unwrap_err().to_string(), "original replace failure");
    assert!(!path.exists());
    assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"existing bytes");

    let (mut pending, path) = owned(library.path(), "import-1");
    pending.commit();
    drop(pending);
    assert_eq!(fs::read(path.join("project.json")).unwrap(), b"owned bytes");
}

#[test]
fn cleanup_error_preserves_primary_and_does_not_delete_substituted_file() {
    let library = tempfile::tempdir().unwrap();
    let (pending, path) = owned(library.path(), "import-2");
    // Simulate a changed target without deleting a computed directory tree.
    fs::remove_file(path.join("project.json")).unwrap();
    fs::remove_dir(&path).unwrap();
    fs::write(&path, b"foreign replacement").unwrap();
    let result: Result<()> = pending.finish(Err("original import failure".into()));
    let error = result.unwrap_err();
    assert!(
        error.to_string().starts_with("original import failure; cleanup also failed:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("phase=project_cleanup") && error.to_string().contains("cause="),
        "{error}"
    );
    let crate::Error::Cleanup { primary, secondary } = &error else {
        panic!("cleanup flattened the original failure: {error:?}");
    };
    assert!(matches!(primary.as_ref(), crate::Error::Message(message)
        if message == "original import failure"));
    assert!(matches!(secondary.as_ref(), crate::Error::ProjectCleanup { cause, .. }
        if cause.raw_os_error().is_some()));
    assert_eq!(std::error::Error::source(&error).unwrap().to_string(), "original import failure");
    assert_eq!(fs::read(path).unwrap(), b"foreign replacement");
}

#[test]
fn cleanup_refuses_directory_outside_canonical_library() {
    let library = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let mut pending = PendingProjectDirectory::default();
    pending.own(
        library.path().canonicalize().unwrap(),
        foreign.path().canonicalize().unwrap(),
    );
    fs::write(foreign.path().join("keep"), b"foreign bytes").unwrap();
    let result: Result<()> = pending.finish(Err("primary".into()));
    assert!(result.unwrap_err().to_string().contains("refused changed directory"));
    assert_eq!(
        fs::read(foreign.path().join("keep")).unwrap(),
        b"foreign bytes"
    );
}

#[test]
fn unwind_releases_business_lock_then_cleans_without_swallowing_panic() {
    let library = tempfile::tempdir().unwrap();
    let lock = Mutex::new(());
    let path = library.path().canonicalize().unwrap().join("project-3");
    let result = catch_unwind(AssertUnwindSafe(|| {
        let (pending, _) = owned(library.path(), "project-3");
        let _business = lock.lock().unwrap();
        let _keep_outer_guard = &pending;
        panic!("original controlled panic");
    }));
    assert_eq!(
        *result.unwrap_err().downcast::<&str>().unwrap(),
        "original controlled panic"
    );
    assert!(matches!(
        lock.try_lock(),
        Err(std::sync::TryLockError::Poisoned(_))
    ));
    assert!(!path.exists());
}

#[test]
fn unwind_cleanup_failure_is_observable_without_holding_business_lock() {
    const CHILD: &str = "MOTION_PROJECT_CLEANUP_UNWIND_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use std::io::Write;
        struct Capture {
            file: Mutex<fs::File>,
            business: std::sync::Arc<Mutex<()>>,
        }
        impl log::Log for Capture {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() <= log::Level::Info
            }
            fn log(&self, record: &log::Record<'_>) {
                if !self.enabled(record.metadata()) {
                    return;
                }
                assert!(!matches!(
                    self.business.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                let text = format!("{} {}", record.level(), record.args());
                let mut file = self.file.lock().unwrap();
                writeln!(file, "{text}").unwrap();
                file.flush().unwrap();
                if record.level() == log::Level::Error {
                    writeln!(std::io::stderr().lock(), "{text}").unwrap();
                }
            }
            fn flush(&self) {}
        }
        let root = PathBuf::from(root);
        let business = std::sync::Arc::new(Mutex::new(()));
        let sink = Box::leak(Box::new(Capture {
            file: Mutex::new(fs::File::create(root.join("cleanup.log")).unwrap()),
            business: business.clone(),
        }));
        log::set_logger(sink).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let failure = catch_unwind(AssertUnwindSafe(|| {
            let (pending, path) = owned(&root, "project-4");
            let _business = business.lock().unwrap();
            fs::remove_file(path.join("project.json")).unwrap();
            fs::remove_dir(&path).unwrap();
            fs::write(path, b"foreign replacement").unwrap();
            let _keep_outer_guard = &pending;
            panic!("original controlled panic");
        }));
        assert_eq!(
            *failure.unwrap_err().downcast::<&str>().unwrap(),
            "original controlled panic"
        );
        assert_eq!(
            fs::read(root.join("project-4")).unwrap(),
            b"foreign replacement"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ops::project::cleanup_tests::unwind_cleanup_failure_is_observable_without_holding_business_lock"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = fs::read_to_string(root.path().join("cleanup.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        file.contains("ERROR project rollback during unwind failed:")
            && file.contains("phase=project_cleanup")
            && file.contains("cause=")
    );
    assert_eq!(file.lines().count(), 1);
    assert!(stderr.contains(file.trim()));
    assert!(!stdout.contains("project rollback during unwind failed:"));
}

#[cfg(unix)]
#[test]
fn substituted_symlink_cannot_redirect_cleanup() {
    let library = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let (pending, path) = owned(library.path(), "import-3");
    fs::remove_file(path.join("project.json")).unwrap();
    fs::remove_dir(&path).unwrap();
    fs::write(foreign.path().join("keep"), b"foreign bytes").unwrap();
    std::os::unix::fs::symlink(foreign.path(), &path).unwrap();
    let result: Result<()> = pending.finish(Err("primary".into()));
    assert!(result.unwrap_err().contains("refused changed directory"));
    assert_eq!(
        fs::read(foreign.path().join("keep")).unwrap(),
        b"foreign bytes"
    );
}
