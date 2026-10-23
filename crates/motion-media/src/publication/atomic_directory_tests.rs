use super::*;
use std::{sync::mpsc, time::Duration};

struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn check_publication(replace: bool) {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    let stage = root.path().join("stage");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(&stage).unwrap();
    let live = [cache.join("decoded.pcm"), cache.join("decoded.wave")];
    let prepared = [stage.join("decoded.pcm"), stage.join("decoded.wave")];
    for (index, path) in prepared.iter().enumerate() {
        fs::write(path, format!("new-{index}")).unwrap();
        if replace {
            fs::write(&live[index], format!("old-{index}")).unwrap();
        }
    }
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let _release = Release(release.clone());
    let first = (prepared[0].as_path(), live[0].as_path());
    let second = (prepared[1].as_path(), live[1].as_path());
    let visible = std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            PublicationCleanup::default().publish_cache_pair_at_checkpoint(first, second, || {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(5)).unwrap();
            })
        });
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        // A different thread reads the real live paths while the writer is
        // paused after relocating the first file. No mocked FS result is used.
        let visible: Vec<_> = live
            .iter()
            .map(|path| match fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => panic!("{}: {error}", path.display()),
            })
            .collect();
        release.send(()).unwrap();
        worker.join().unwrap().unwrap();
        visible
    });
    let expected: Vec<_> = (0..2)
        .map(|index| replace.then(|| format!("old-{index}").into_bytes()))
        .collect();
    assert_eq!(
        visible, expected,
        "reader observed a half-published or mixed cache pair"
    );
    for (index, path) in live.iter().enumerate() {
        assert_eq!(fs::read(path).unwrap(), format!("new-{index}").as_bytes());
    }
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".cache-publish-")));
}

#[test]
fn first_publication_does_not_expose_only_one_cache_member() {
    check_publication(false);
}

#[test]
fn replacement_does_not_expose_new_pcm_with_old_waveform() {
    check_publication(true);
}

fn snapshot(directory: &Path, prefix: &str) {
    fs::create_dir(directory).unwrap();
    for name in ["decoded.pcm", "decoded.wave"] {
        fs::write(directory.join(name), format!("{prefix}-{name}")).unwrap();
    }
}

fn assert_snapshot(directory: &Path, prefix: &str) {
    for name in ["decoded.pcm", "decoded.wave"] {
        assert_eq!(
            fs::read(directory.join(name)).unwrap(),
            format!("{prefix}-{name}").as_bytes()
        );
    }
    assert_eq!(fs::read_dir(directory).unwrap().count(), 2);
}

#[test]
fn failed_model_save_restores_the_complete_previous_directory() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    let stage = root.path().join("stage");
    snapshot(&cache, "old");
    snapshot(&stage, "new");
    let mut cleanup = PublicationCleanup::default();
    cleanup
        .publish_cache_pair(
            (&stage.join("decoded.pcm"), &cache.join("decoded.pcm")),
            (&stage.join("decoded.wave"), &cache.join("decoded.wave")),
        )
        .unwrap();
    assert_snapshot(&cache, "new");
    let primary = Error::from(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "controlled model save failure",
    ));
    let error = cleanup.rollback_published_cache(primary);
    assert!(
        matches!(error.primary(), Error::Io(cause) if cause.kind() == io::ErrorKind::PermissionDenied)
    );
    assert_snapshot(&cache, "old");
    let recovery = cleanup.cache.as_ref().unwrap().directory.clone();
    assert_snapshot(&recovery.join("discard-0"), "new");
    drop(cleanup);
    assert!(!recovery.exists());
    assert_snapshot(&cache, "old");
}

#[test]
fn directory_publication_unwind_restores_both_members() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    let stage = root.path().join("stage");
    snapshot(&cache, "old");
    snapshot(&stage, "new");
    let caught = std::panic::catch_unwind(|| {
        let mut transaction = Publication::new(root.path()).unwrap();
        let attempt = Attempt(&mut transaction);
        attempt.0.publish(&stage, &cache).unwrap();
        assert_snapshot(&cache, "new");
        panic!("controlled directory publication panic");
    });
    assert_eq!(
        crate::panic_reason(caught.unwrap_err()),
        "controlled directory publication panic"
    );
    assert_snapshot(&cache, "old");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn failed_directory_restore_preserves_old_bytes_and_the_primary_cause() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    let stage = root.path().join("stage");
    snapshot(&cache, "old");
    snapshot(&stage, "new");
    let mut cleanup = PublicationCleanup::default();
    cleanup
        .publish_cache_pair(
            (&stage.join("decoded.pcm"), &cache.join("decoded.pcm")),
            (&stage.join("decoded.wave"), &cache.join("decoded.wave")),
        )
        .unwrap();
    fs::rename(&cache, root.path().join("external-snapshot")).unwrap();
    fs::write(&cache, b"foreign file").unwrap();
    let error = cleanup.rollback_published_cache("original model save failure");
    let Error::Rollback {
        primary, secondary, ..
    } = &error
    else {
        panic!("missing rollback causes: {error:?}");
    };
    assert!(
        matches!(primary.as_ref(), Error::Message(message) if message == "original model save failure")
    );
    assert!(!secondary.is_empty());
    let recovery = cleanup.cache.as_ref().unwrap().directory.clone();
    assert!(error.to_string().contains(&recovery.display().to_string()));
    drop(cleanup);
    assert_eq!(fs::read(cache).unwrap(), b"foreign file");
    assert_snapshot(&recovery.join("old-0"), "old");
    assert_snapshot(&root.path().join("external-snapshot"), "new");
}
