use super::*;
use std::sync::Mutex;

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = record.args().to_string();
        if text.contains("image_proxy.") || text.starts_with("image proxy temporary cleanup") {
            assert!(
                !matches!(
                    CACHE_LOCK.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ),
                "cache lock held during log"
            );
            self.0.lock().unwrap().push((record.level(), text));
        }
    }
    fn flush(&self) {}
}
fn poison() {
    assert!(std::panic::catch_unwind(|| {
        let _guard = CACHE_LOCK.lock().unwrap();
        panic!("controlled cache poison");
    })
    .is_err());
}

#[test]
fn proxy_failures_log_after_unlock_and_keep_filesystem_recovery() {
    const TEST: &str = "image_resources::proxy_lock_observation_tests::proxy_failures_log_after_unlock_and_keep_filesystem_recovery";
    const CHILD: &str = "MOTION_PROXY_LOCK_TEST";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["info", "debug", "unwind"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env(CHILD, mode)
                .env_remove("MOTION_LOG")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!String::from_utf8_lossy(&output.stdout).contains("site=image_proxy."));
        }
        return;
    };
    let capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(if mode == "debug" {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(format!("{:064x}.rgba", 7));
    let pixels = Pixels {
        width: 2,
        height: 1,
        rgba: vec![80; 8],
        cached: false,
    };
    for _ in 0..120 {
        write_proxy(&path, &pixels);
        assert_eq!(read_proxy(&path, 2, 1).unwrap(), pixels.rgba);
    }
    assert!(capture.0.lock().unwrap().is_empty());
    if mode == "unwind" {
        poison();
        assert!(std::panic::catch_unwind(|| {
            let mut events = ProxyEvents::default();
            let _guard = proxy_cache_lock(&mut events);
            panic!("controlled recovery unwind");
        })
        .is_err());
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Warn);
        assert!(records[0].1.contains("lifetime_count=1 "));
        assert!(CACHE_LOCK.is_poisoned());
        return;
    }
    for _ in 0..120 {
        poison();
        assert_eq!(read_proxy(&path, 2, 1).unwrap(), pixels.rgba);
        assert!(!CACHE_LOCK.is_poisoned());
        write_proxy(&path, &pixels);
    }
    {
        let records = capture.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[1].0, log::Level::Error);
        assert!(records[0].1.contains("lifetime_count=1 "));
        assert!(records[1].1.contains("lifetime_count=60 "));
    }
    let broken = root.path().join("missing-parent/cache.rgba");
    for _ in 0..120 {
        write_proxy(&broken, &pixels);
    }
    write_proxy(&path, &pixels); // Real successful write resets only the write streak.
    for _ in 0..59 {
        write_proxy(&broken, &pixels);
    }
    assert_eq!(
        capture
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, text)| *level == log::Level::Error
                && text.contains("site=image_proxy.write "))
            .count(),
        1
    );
    write_proxy(&broken, &pixels);
    {
        let records = capture.0.lock().unwrap();
        let writes: Vec<_> = records
            .iter()
            .filter(|(_, s)| s.contains("site=image_proxy.write "))
            .collect();
        assert_eq!(
            writes
                .iter()
                .filter(|(l, _)| *l == log::Level::Error)
                .count(),
            2
        );
        assert_eq!(
            writes
                .iter()
                .filter(|(l, _)| *l == log::Level::Debug)
                .count(),
            if mode == "debug" { 2 } else { 0 }
        );
        assert!(writes
            .iter()
            .all(|(_, s)| s.contains("path=") && s.contains("error=") && s.len() < 800));
    }
    // A real failed rename keeps the destination and removes the owned temp file.
    let directory = root.path().join(format!("{:064x}.rgba", 8));
    fs::create_dir(&directory).unwrap();
    write_proxy(&directory, &pixels);
    assert!(directory.is_dir());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
    // Force remove_file to fail using a directory, preserving cleanup and suppression.
    let directory = root.path().join("cleanup.tmp");
    fs::create_dir(&directory).unwrap();
    for _ in 0..120 {
        let mut events = ProxyEvents::default();
        let _guard = proxy_cache_lock(&mut events);
        let _temporary = ProxyTemporary(directory.clone(), &mut events);
    }
    let cleanup_count = || {
        capture
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.starts_with("image proxy temporary cleanup failed:"))
            .count()
    };
    assert_eq!(cleanup_count(), 1);
    {
        let mut events = ProxyEvents::default();
        let _guard = proxy_cache_lock(&mut events);
        let _temporary = ProxyTemporary(root.path().join("absent.tmp"), &mut events);
    }
    {
        let mut events = ProxyEvents::default();
        let _guard = proxy_cache_lock(&mut events);
        let _temporary = ProxyTemporary(directory.clone(), &mut events);
    }
    assert_eq!(cleanup_count(), 2);
    assert!(directory.is_dir());
    let error = std::io::Error::other("sensitive-tail".repeat(1000));
    let context = proxy_observation::context(Path::new(&"long-path-敏".repeat(1000)), &error);
    assert!(context.chars().count() <= 524);
    assert!(context.contains("error=sensitive-tail"));
    assert_eq!(read_proxy(&path, 2, 1).unwrap(), pixels.rgba);
}
