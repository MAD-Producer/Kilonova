use super::*;
use std::time::Duration;

// These two tests exercise the same process-wide worker limit. Keep their
// players isolated while the rest of the desktop suite runs in parallel.
static PLAYER_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn workers_closed() {
    let deadline = Instant::now() + Duration::from_secs(5);
    while WORKERS.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < deadline,
            "source preview worker did not close"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn state(root: &Path) -> Value {
    serde_json::json!({"root":root,"document_generation":1,"revision":0,"composition":"main","project":{"video_assets":[VideoAsset {
        id:1,path:"assets/source.mp4".into(),bytes:1,mime:"video/avc".into(),track:0,
        width:16,height:16,rotation:0,display_width:16,display_height:16,
        video_start_us:0,video_end_us:1_000_000,duration_us:1_000_000,
        frame_count:24,variable_frame_rate:false,nominal_frame_rate:24.,color_standard:4,color_range:2,audio_asset:None,
    }]}})
}
fn settle(player: &mut Player, now: Option<Instant>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while player.pending.is_some() {
        player.poll(now.unwrap_or_else(Instant::now));
        assert!(Instant::now() < deadline, "source preview did not return");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn cancelled_blocked_decoders_keep_their_permits_and_publish_pending_before_entry() {
    let _isolation = PLAYER_TEST.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path());
    let mut releases = Vec::new();
    for _ in 0..2 {
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut player = Player::default();
        player.before = Some(Box::new(move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
        }));
        player.start(&state, 1).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            player
                .worker
                .as_ref()
                .unwrap()
                .phase
                .load(Ordering::Acquire),
            1
        );
        assert_eq!(player.pending, Some(0));
        player.seek(0.25);
        player.seek(0.75);
        assert_eq!(
            player.pending,
            Some(0),
            "scrub queued an unbounded second decode"
        );
        assert_eq!(player.target(), 750_000);
        player.close();
        releases.push(release);
    }
    assert_eq!(WORKERS.load(Ordering::Acquire), 2);
    assert!(Player::default()
        .start(&state, 1)
        .unwrap_err()
        .contains("still closing"));
    for release in releases {
        release.send(()).unwrap();
    }
    workers_closed();
    assert!(
        std::fs::read_dir(root.path()).unwrap().next().is_none(),
        "cancelled source playback wrote files"
    );
}
#[test]
fn source_preview_failures_and_panics_reach_default_channels_without_restarting() {
    const CHILD: &str = "MOTION_SOURCE_PREVIEW_ERROR_CHILD";
    const NAME:&str="project_preview::tests::source_preview_failures_and_panics_reach_default_channels_without_restarting";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let state = state(&root);
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let mut player = Player::default();
        player.start(&state, 1).unwrap();
        settle(&mut player, None);
        assert!(player.message.as_deref().unwrap().contains("os error 2"));
        assert!(player.notice.take().is_some());
        for _ in 0..100 {
            player.poll(Instant::now());
        }
        assert!(!player.busy() && player.worker.is_none() && player.notice.is_none());
        player.before = Some(Box::new(|| panic!("controlled source preview panic")));
        player.start(&state, 1).unwrap();
        settle(&mut player, None);
        assert!(player.message.as_deref().unwrap().contains("disconnected"));
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SOURCE_PREVIEW_ERROR_EVIDENCE")
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
        "source preview failed",
        "source preview interrupted",
        "source preview worker disconnected",
        "controlled source preview panic",
        "os error 2",
    ] {
        assert!(file.contains(marker), "file missing {marker}");
        assert!(stderr.contains(marker), "stderr missing {marker}");
    }
    assert!(file
        .contains("source preview queued request=1 asset=1 source=assets/source.mp4 phase=queued"));
    assert!(file.contains("phase=opening") && file.contains("unwinding=true"));
    assert!(file.contains("backtrace") || file.contains("Backtrace"));
    assert_eq!(file.matches("source preview failed").count(), 1);
    assert!(
        !stdout.contains("INFO ")
            && !stdout.contains("ERROR ")
            && !stdout.contains("source preview queued")
    );
}

#[cfg(feature = "ffmpeg")]
#[test]
fn real_video_play_pause_scrub_coalescing_end_and_missing_index_preserve_project() {
    let _isolation = PLAYER_TEST.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    let path = root.path().join("assets/source.mp4");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/motion-media/tests/fixtures/video/sound-24fps.mp4"),
        &path,
    )
    .unwrap();
    let probe = motion_desktop_media::DesktopPlatform::new()
        .probe_video(&path, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    let mut asset = probe.asset;
    asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root.path(), &asset).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &asset, &probe.timestamps).unwrap();
    let mut state = state(root.path());
    state["project"]["video_assets"][0] = serde_json::to_value(&asset).unwrap();
    let before = state.clone();
    let mut player = Player::default();
    player.start(&state, 1).unwrap();
    settle(&mut player, None);
    assert!(player.message.is_none(), "{:?}", player.message);
    assert_eq!(player.frame.as_ref().unwrap().pts, asset.video_start_us);
    assert!(!player.playing());
    let first = player.bitmap().unwrap();
    player.toggle();
    let began = player.playing.unwrap().0;
    let middle = began + Duration::from_micros(player.duration / 2);
    player.poll(middle);
    settle(&mut player, Some(middle));
    assert!(player.playing());
    assert_eq!(player.cursor, player.duration / 2);
    assert!(player.frame.as_ref().unwrap().pts > asset.video_start_us);
    assert!(!Arc::ptr_eq(&first, &player.bitmap().unwrap()));
    player.toggle();
    assert!(!player.playing());
    player.seek(0.);
    player.seek(player.duration() * 0.25);
    player.seek(player.duration() * 0.75);
    settle(&mut player, None);
    let target = player.target();
    let frame = player.frame.as_ref().unwrap();
    assert!(
        frame.pts <= target && target < frame.end,
        "published superseded scrub"
    );
    let bitmap = player.bitmap().unwrap();
    for _ in 0..100 {
        player.poll(Instant::now());
    }
    assert!(
        Arc::ptr_eq(&bitmap, &player.bitmap().unwrap()),
        "paused preview repeated decoding"
    );
    let position = player.seconds();
    player.seek(f64::NAN);
    assert_eq!(position, player.seconds());
    player.toggle();
    let end = player.playing.unwrap().0 + Duration::from_micros(player.duration + 1);
    player.poll(end);
    settle(&mut player, Some(end));
    assert!(!player.playing());
    assert_eq!(player.cursor, player.duration);
    assert_eq!(player.frame.as_ref().unwrap().end, asset.video_end_us);
    assert!(player.current(&state, 1));
    state["revision"] = serde_json::json!(1);
    assert!(!player.current(&state, 1));
    state["revision"] = before["revision"].clone();
    player.close();
    std::fs::remove_file(index).unwrap();
    player.start(&state, 1).unwrap();
    settle(&mut player, None);
    assert!(player.message.is_none(), "{:?}", player.message);
    assert!(player.bitmap().is_some());
    assert_eq!(state, before, "source playback edited document");
    player.close();
    workers_closed();
}
