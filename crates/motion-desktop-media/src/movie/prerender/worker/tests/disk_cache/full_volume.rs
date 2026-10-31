//! Explicit Linux CI control: real ENOSPC on a separately mounted 8 MiB tmpfs.
//! The regular unit suite ignores this test and never fills a host filesystem.
use super::*;
use std::io::Write;

#[test]
#[ignore = "requires the isolated bounded tmpfs created by validate_prerender_full_volume.py"]
fn actual_full_volume_keeps_native_frames_project_bytes_and_recovery_intact() {
    assert_eq!(std::env::consts::OS, "linux");
    let volume = std::path::PathBuf::from(
        std::env::var_os("MOTION_PRERENDER_FULL_VOLUME")
            .expect("use the isolated-volume validation launcher"),
    )
    .canonicalize()
    .unwrap();
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    let mount = mountinfo
        .lines()
        .find(|line| line.split_whitespace().nth(4) == volume.to_str())
        .expect("cache volume must be an independent mount");
    let filesystem = mount.split_once(" - ").unwrap().1;
    assert_eq!(filesystem.split_whitespace().next(), Some("tmpfs"));
    assert!(filesystem
        .split_whitespace()
        .nth(2)
        .unwrap()
        .split(',')
        .any(|option| matches!(option, "size=8192k" | "size=8388608")));

    let cache_root = tempfile::tempdir_in(&volume).unwrap();
    let project_root = tempfile::tempdir().unwrap();
    assert!(!project_root
        .path()
        .canonicalize()
        .unwrap()
        .starts_with(&volume));
    let project = project_root.path().join("project");
    let session = disk_session(&project);
    motion_core::storage::save(&project, session.engine.project()).unwrap();
    let original = std::fs::read(project.join("project.json")).unwrap();
    let config = disk::Config::new(cache_root.path().join("cache"), 1 << 20).unwrap();
    let mut worker = Worker::start(32 * 24 * 4).unwrap();
    worker
        .prepare(cached_plan(&session, &config, 82))
        .unwrap()
        .unwrap();
    assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));

    worker.render(1.).unwrap().unwrap();
    let cold = poll(&mut worker);
    let expected = match cold.outcome {
        Outcome::Frame(frame) => frame.image().pixels.clone(),
        _ => panic!("cold native render failed before volume exhaustion"),
    };
    assert_eq!(cold.disk.writes, 1);
    assert_eq!(cold.disk.io_failures, 0);

    // The filler is outside the cache namespace, so cache quota eviction cannot
    // make this write appear successful by deleting the exhaustion fixture.
    let filler_path = cache_root.path().join("owned-volume-fill.bin");
    let mut filler = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(&filler_path)
        .unwrap();
    let chunk = [0x7au8; 4096];
    let mut written = 0usize;
    let original_io_error = loop {
        assert!(
            written < 16 << 20,
            "isolated volume did not enforce its size limit"
        );
        match filler.write(&chunk) {
            Ok(0) => panic!("zero write is not a proven ENOSPC"),
            Ok(bytes) => written += bytes,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => break error,
        }
    };
    assert_eq!(
        original_io_error.raw_os_error(),
        Some(28),
        "{original_io_error}"
    );
    assert_eq!(original_io_error.kind(), std::io::ErrorKind::StorageFull);
    assert!(written > 0 && written <= 8 << 20);

    worker.render(2.).unwrap().unwrap();
    let failed_write = poll(&mut worker);
    let frame = match failed_write.outcome {
        Outcome::Frame(frame) => frame,
        _ => panic!("actual cache ENOSPC must preserve the normal native frame"),
    };
    assert_eq!(frame.image().pixels, expected);
    assert_eq!(failed_write.disk.io_failures, 1);
    assert_eq!(failed_write.disk.writes, 1);
    assert!(failed_write.disk.ready);
    let cause = failed_write.disk.last_failure.as_deref().unwrap();
    assert!(
        cause.contains("os error 28"),
        "lost original ENOSPC: {cause}"
    );
    assert!(
        cause.contains("prerender cache"),
        "missing cache phase: {cause}"
    );
    drop(frame);
    assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
    assert_eq!(
        std::fs::read(project.join("project.json")).unwrap(),
        original
    );
    let namespace = cache_root.path().join("cache/native-v1");
    let entries: Vec<_> = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "failed write published a partial entry: {entries:?}"
    );
    assert!(entries[0].to_str().unwrap().ends_with(".mspc"));

    // An already-valid cache frame remains readable on the full volume.
    worker.render(1.).unwrap().unwrap();
    let warm = poll(&mut worker);
    match warm.outcome {
        Outcome::Frame(frame) => assert_eq!(frame.image().pixels, expected),
        _ => panic!("an existing valid frame was lost after ENOSPC"),
    }
    assert_eq!(warm.disk.hits, 1);
    assert_eq!(warm.disk.io_failures, 1);

    // Free only the file this test created. No host cache or project is removed.
    drop(filler);
    std::fs::remove_file(&filler_path).unwrap();
    worker.render(2.).unwrap().unwrap();
    let recovered = poll(&mut worker);
    match recovered.outcome {
        Outcome::Frame(frame) => assert_eq!(frame.image().pixels, expected),
        _ => panic!("native rendering failed after restoring volume capacity"),
    }
    assert_eq!(recovered.disk.writes, 2);
    assert_eq!(recovered.disk.io_failures, 1);
    worker.render(2.).unwrap().unwrap();
    let rebound = poll(&mut worker);
    match rebound.outcome {
        Outcome::Frame(frame) => assert_eq!(frame.image().pixels, expected),
        _ => panic!("recovered cache entry was not readable"),
    }
    assert_eq!(rebound.disk.hits, 2);
    assert_eq!(
        std::fs::read(project.join("project.json")).unwrap(),
        original
    );
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(&project).unwrap()).unwrap(),
        serde_json::to_value(session.engine.project()).unwrap()
    );
    stop(&mut worker);

    let out = std::path::PathBuf::from(
        std::env::var_os("MOTION_PRERENDER_FULL_VOLUME_ARTIFACTS").unwrap(),
    );
    assert!(!out.starts_with(&volume));
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("native-full-volume.json"), serde_json::to_vec_pretty(
        &serde_json::json!({"schemaVersion":1,"actualOsError":28,
            "actualErrorKind":"StorageFull","filledBytes":written,"mount":mount,
            "nativeFramePreserved":true,"existingCacheHitOnFullVolume":true,
            "projectBytesUnchanged":true,"projectReloaded":true,
            "partialEntryPublished":false,"cacheRecovered":true,
            "failedWriteStats":failed_write.disk,"recoveredStats":rebound.disk,
            "limits":"Actual bounded Linux tmpfs ENOSPC; not a Windows or macOS full-volume qualification"})
    ).unwrap()).unwrap();
}
