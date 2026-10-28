use super::*;
use std::time::Duration;
fn state(root: &std::path::Path) -> Value {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    serde_json::json!({"root":root,"document_generation":1,"project":{"assets":[{"id":1,"path":"assets/source.png","width":16,"height":8}]}})
}
fn key() -> RowKey {
    RowKey::Item(ProjectItem::Image { id: 1 })
}
fn settle(service: &mut Thumbnails) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while service.busy() {
        service.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn thumbnails_decode_real_sources_once_and_explicit_refresh_recovers_a_missing_source() {
    let temp = tempfile::tempdir().unwrap();
    let state = state(temp.path());
    let mut service = Thumbnails {
        enabled: true,
        ..Default::default()
    };
    assert!(service.get(&state, &key()).is_none());
    settle(&mut service);
    assert!(service.message.take().unwrap().contains("os error 2"));
    for _ in 0..100 {
        assert!(service.get(&state, &key()).is_none());
    }
    assert_eq!(service.request, 1, "repaint restarted failed decode");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([45, 110, 200, 255]))
        .save(temp.path().join("assets/source.png"))
        .unwrap();
    service.refresh();
    service.get(&state, &key());
    settle(&mut service);
    assert!(service.message.is_none(), "{:?}", service.message);
    let bitmap = service.get(&state, &key()).unwrap();
    assert_eq!(bitmap.dimensions(), [16, 8]);
    for _ in 0..100 {
        assert!(Arc::ptr_eq(&bitmap, &service.get(&state, &key()).unwrap()));
    }
    assert_eq!(service.request, 2);
    assert!(service.message.is_none());
    // A changed source identity gets a new image without polling the filesystem.
    let mut next = state.clone();
    next["document_generation"] = serde_json::json!(2);
    service.get(&next, &key());
    settle(&mut service);
    assert!(!Arc::ptr_eq(&bitmap, &service.get(&next, &key()).unwrap()));
}
#[test]
fn thumbnail_cancel_keeps_blocked_worker_bounded_and_publishes_state_before_decode() {
    let temp = tempfile::tempdir().unwrap();
    let state = state(temp.path());
    let mut service = Thumbnails {
        enabled: true,
        ..Default::default()
    };
    let mut releases = Vec::new();
    for _ in 0..2 {
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        service.before = Some(Box::new(move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
        }));
        service.get(&state, &key());
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(service.busy());
        assert!(matches!(
            service.records.values().next().unwrap().state,
            Status::Pending { .. }
        ));
        service.refresh();
        releases.push(release);
    }
    assert_eq!(service.permits.load(Ordering::Acquire), 2);
    service.get(&state, &key());
    assert!(!service.busy());
    assert_eq!(service.request, 2);
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.permits.load(Ordering::Acquire) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !temp.path().join("cache").exists(),
        "cancelled decode touched source/cache"
    );
}
#[test]
fn thumbnail_failures_and_panics_reach_default_error_channels_without_stdout_logs() {
    const CHILD: &str = "MOTION_PROJECT_THUMBNAIL_ERROR_CHILD";
    const NAME:&str="panels::project::thumbnails::tests::thumbnail_failures_and_panics_reach_default_error_channels_without_stdout_logs";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let state = state(&root);
        let mut service = Thumbnails {
            enabled: true,
            ..Default::default()
        };
        service.get(&state, &key());
        settle(&mut service);
        assert!(service.message.take().is_some());
        for _ in 0..100 {
            service.get(&state, &key());
        }
        assert_eq!(service.request, 1);
        service.refresh();
        service.before = Some(Box::new(|| panic!("controlled project thumbnail panic")));
        service.get(&state, &key());
        settle(&mut service);
        assert!(service.message.take().unwrap().contains("disconnected"));
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_THUMBNAIL_ERROR_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for marker in [
        "project thumbnail failed",
        "source.png",
        "controlled project thumbnail panic",
        "thumbnail worker disconnected",
    ] {
        assert!(file.contains(marker), "file missing {marker}");
        assert!(stderr.contains(marker), "stderr missing {marker}");
    }
    assert_eq!(file.matches("project thumbnail failed").count(), 1);
    assert!(
        file.contains("phase=queued")
            && file.contains("phase=decode")
            && file.contains("unwinding=true")
    );
    assert!(file.contains("Backtrace") || file.contains("backtrace"));
    assert!(
        !stdout.contains("INFO ")
            && !stdout.contains("ERROR ")
            && !stdout.contains("project thumbnail queued")
    );
}
