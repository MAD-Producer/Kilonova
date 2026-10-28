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
fn maximum_supported_asset_inventory_remains_virtualized_and_searchable_while_thumbnail_decode_is_blocked(
) {
    use motion_ui::{
        input::{Input, Rect},
        paint::PaintList,
        text::TextAtlas,
    };
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("assets")).unwrap();
    image::RgbaImage::from_pixel(16, 8, image::Rgba([45, 110, 200, 255]))
        .save(temp.path().join("assets/source.png"))
        .unwrap();
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    let asset_count = motion_core::MAX_LAYERS * 2;
    project.assets = (1..=asset_count)
        .map(|id| motion_core::Asset {
            id: id as u64,
            path: "assets/source.png".into(),
            width: 16,
            height: 8,
        })
        .collect();
    project.validate().unwrap();
    let state = serde_json::json!({"root":temp.path(),"document_generation":1,"revision":0,"project":project,"frame":0});
    let mut panels = crate::panels::Panels::default();
    panels.project.thumbnails.enabled = true;
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    panels.project.thumbnails.before = Some(Box::new(move || {
        entered.send(()).unwrap();
        gate.recv().unwrap();
    }));
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut paint = PaintList::default();
    let mut actions = Vec::new();
    super::super::render(
        &mut paint,
        &mut atlas,
        Rect::new(0., 0., 700., 500.),
        &mut panels,
        &state,
        &Input::default(),
        &mut actions,
    );
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(panels.project.thumbnails.busy());
    assert_eq!(panels.project.thumbnails.records.len(), 2);
    assert!(
        panels.project.browser.rows.len() < 32,
        "inventory painted every asset"
    );
    assert_eq!(
        panels
            .project
            .browser
            .cache
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .entries
            .len(),
        asset_count + 1
    );
    // Search input and another paint complete before the controlled decoder returns.
    panels.project.query = "no-match".into();
    paint.clear();
    super::super::render(
        &mut paint,
        &mut atlas,
        Rect::new(0., 0., 700., 500.),
        &mut panels,
        &state,
        &Input::default(),
        &mut actions,
    );
    assert!(panels.project.browser.rows.is_empty());
    assert_eq!(
        panels.project.thumbnails.request, 2,
        "search launched a decode for an invisible asset"
    );
    panels.project.thumbnails.refresh();
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while panels.project.thumbnails.permits.load(Ordering::Acquire) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
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

#[cfg(feature = "ffmpeg")]
#[test]
fn video_poster_cache_miss_decodes_source_and_only_source_failure_uses_error_channels() {
    use motion_host::platform::Platform;
    const CHILD: &str = "MOTION_PROJECT_VIDEO_POSTER_CHILD";
    const NAME: &str = "panels::project::thumbnails::tests::video_poster_cache_miss_decodes_source_and_only_source_failure_uses_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let path = root.join("assets/source.mp4");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/motion-media/tests/fixtures/video/sound-24fps.mp4"),
            &path,
        )
        .unwrap();
        let mut asset = motion_desktop_media::DesktopPlatform::new()
            .probe_video(&path, None, Some(u32::MAX), &|| Ok(()))
            .unwrap()
            .asset;
        asset.path = "assets/source.mp4".into();
        let state = serde_json::json!({"root":root,"document_generation":1,"project":{"video_assets":[asset]}});
        let key = RowKey::Item(ProjectItem::Video { id: 1 });
        let mut service = Thumbnails {
            enabled: true,
            ..Default::default()
        };
        assert!(!root.join("cache").exists());
        service.get(&state, &key);
        settle(&mut service);
        let bitmap = service
            .get(&state, &key)
            .expect("cache absence did not regenerate poster");
        assert!(bitmap.dimensions().iter().all(|n| *n > 0 && *n <= 128));
        assert!(service.message.is_none());
        for _ in 0..100 {
            assert!(Arc::ptr_eq(&bitmap, &service.get(&state, &key).unwrap()));
        }
        assert_eq!(service.request, 1);
        assert!(decode(
            &root.to_string_lossy(),
            1,
            &state["project"]["video_assets"][0],
            &AtomicBool::new(true)
        )
        .unwrap()
        .is_none());
        crate::logging::flush();
        let healthy = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(
            !healthy.contains("ERROR"),
            "normal cache miss logged a fault: {healthy}"
        );
        std::fs::remove_file(path).unwrap();
        service.refresh();
        service.get(&state, &key);
        settle(&mut service);
        assert!(service
            .message
            .as_deref()
            .is_some_and(|m| m.contains("os error 2")));
        for _ in 0..100 {
            service.get(&state, &key);
        }
        assert_eq!(service.request, 2);
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_VIDEO_POSTER_EVIDENCE")
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
    for marker in [
        "project thumbnail failed",
        "asset=1 kind=1",
        "source.mp4",
        "request=2",
        "os error 2",
    ] {
        assert!(file.contains(marker), "file missing {marker}");
        assert!(stderr.contains(marker), "stderr missing {marker}");
    }
    assert_eq!(file.matches("project thumbnail failed").count(), 1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("ERROR ")
            && !stdout.contains("INFO ")
            && !stdout.contains("project thumbnail queued")
    );
}
