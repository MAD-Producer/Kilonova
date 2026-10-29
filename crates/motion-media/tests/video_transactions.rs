//! Transaction tests inject probing; real platform decode is covered by Android instrumentation.
use motion_core::{Command, Content, Engine, Project, VideoAsset};
use motion_media::{AudioMixer, ProbeVideo, VideoImportOptions, VideoJobs, VideoProbe};
use std::{
    fs::File,
    path::Path,
    sync::{Arc,Mutex},
    time::{Duration, Instant},
};
const VIDEO: &str = "tests/fixtures/video/sound-24fps.mp4";
// These cases share the process-wide two-worker import budget. Concurrency
// inside each transaction remains tested; unrelated cases cannot occupy it.
static TEST_LOCK:Mutex<()>=Mutex::new(());
fn probe_stub() -> ProbeVideo {
    Arc::new(|source, _, audio, check| {
        check()?;
        let pts = (0..72)
            .map(|n| (n * 1_000_000 + 12) / 24)
            .collect::<Vec<_>>();
        Ok(VideoProbe {
            asset: VideoAsset {
                id: 1,
                path: "assets/probe.mp4".into(),
                bytes: std::fs::metadata(source).unwrap().len(),
                mime: "video/avc".into(),
                track: 0,
                width: 256,
                height: 144,
                rotation: 0,
                display_width: 256,
                display_height: 144,
                video_start_us: 0,
                video_end_us: 3_000_000,
                duration_us: 3_000_000,
                frame_count: 72,
                variable_frame_rate: false,
                nominal_frame_rate: 24.0,
                color_standard: 1,
                color_range: 2,
                audio_asset: None,
            },
            timestamps: pts,
            audio_track: if audio == Some(u32::MAX) {
                None
            } else {
                Some(1)
            },
            first_rgba: vec![255; 256 * 144 * 4],
            tracks: serde_json::json!([]),
        })
    })
}
/// Same 24 fps source, but the first visible frame is at 0.250 s (leading
/// edit) and the visible span ends at 3.000 s. No audio track is reported.
fn probe_delayed_start() -> ProbeVideo {
    Arc::new(|source, _, _, check| {
        check()?;
        let timestamps = (0..66)
            .map(|n| 250_000 + (n * 1_000_000 + 12) / 24)
            .collect::<Vec<_>>();
        Ok(VideoProbe {
            asset: VideoAsset {
                id: 1,
                path: "assets/probe.mp4".into(),
                bytes: std::fs::metadata(source).unwrap().len(),
                mime: "video/avc".into(),
                track: 0,
                width: 256,
                height: 144,
                rotation: 0,
                display_width: 256,
                display_height: 144,
                video_start_us: 250_000,
                video_end_us: 3_000_000,
                duration_us: 3_000_000,
                frame_count: 66,
                variable_frame_rate: false,
                nominal_frame_rate: 24.0,
                color_standard: 1,
                color_range: 2,
                audio_asset: None,
            },
            timestamps,
            audio_track: None,
            first_rgba: vec![255; 256 * 144 * 4],
            tracks: serde_json::json!([]),
        })
    })
}
fn options(id: &str) -> VideoImportOptions {
    VideoImportOptions {
        request_id: id.into(),
        at_frame: 0,
        name: "video".into(),
        track: None,
        audio_track: None,
        with_audio: true,
    }
}
fn start(jobs: &VideoJobs, id: &str) {
    start_probe(jobs, id, true, probe_stub());
}
fn start_probe(jobs: &VideoJobs, id: &str, with_audio: bool, probe: ProbeVideo) {
    let mut import = options(id);
    import.with_audio = with_audio;
    jobs.start(
        || {
            let f = File::open(Path::new(env!("CARGO_MANIFEST_DIR")).join(VIDEO)).unwrap();
            let bytes = f.metadata().unwrap().len();
            Ok((Box::new(f), Some(bytes)))
        },
        import,
        false,
        probe,
    )
    .unwrap();
}
fn wait(jobs: &VideoJobs, id: &str) -> motion_media::VideoTaskStatus {
    let time = Instant::now();
    loop {
        let t = jobs.status(id).unwrap();
        if t.state != "running" {
            return t;
        }
        assert!(time.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn video_import_preserves_unknown_audio_progress_until_the_adapter_reports_a_total() {
    let _guard = TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let continue_rx = Mutex::new(continue_rx);
    let decode: motion_media::DecodeAudio = Arc::new(move |source, pcm, track, limit, check| {
        check(None)?;
        entered_tx.send(()).unwrap();
        continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        motion_media::decode_audio(source, pcm, track, limit, check)
    });
    let jobs = VideoJobs::with_audio_decoder(root.path().into(), decode).unwrap();
    start(&jobs, "unknown-video-audio");
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let pending = jobs.status("unknown-video-audio").unwrap();
    assert_eq!(pending.state, "running");
    assert_eq!(pending.phase, "decoding_audio");
    assert!(pending.progress.is_none());
    let wire = serde_json::to_value(&pending).unwrap();
    assert!(wire.as_object().unwrap().contains_key("progress"));
    assert!(wire["progress"].is_null());
    continue_tx.send(()).unwrap();
    let ready = wait(&jobs, "unknown-video-audio");
    assert_eq!(ready.state, "ready", "{}", serde_json::to_value(&ready).unwrap());
    assert_eq!(ready.progress, Some(1.0));
    jobs.cancel("unknown-video-audio").unwrap();
    jobs.release("unknown-video-audio").unwrap();
}

#[test]
fn blocked_thumbnail_keeps_old_video_index_and_retry_rebuilds_both_files() {
    let _guard = TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    start(&jobs, "cache-source");
    assert_eq!(wait(&jobs, "cache-source").state, "ready");
    assert_eq!(jobs.commit("cache-source", &mut engine).unwrap().state, "succeeded");
    let before = engine.snapshot();
    let asset = before.video_assets[0].clone();
    let index = motion_media::video_cache_path(root.path(), &asset).unwrap();
    let thumbnail = index.with_extension("png");
    let original = std::fs::read(&index).unwrap();
    std::fs::remove_file(&thumbnail).unwrap();
    std::fs::create_dir(&thumbnail).unwrap();
    jobs.prepare_cache("cache-blocked", asset.clone(), probe_stub()).unwrap();
    let failed = wait(&jobs, "cache-blocked");
    assert_eq!(failed.state, "failed");
    assert!(failed.error.unwrap().contains("not a regular file"));
    assert_eq!(std::fs::read(&index).unwrap(), original);
    assert_eq!(engine.snapshot(), before);
    std::fs::remove_dir(&thumbnail).unwrap();
    jobs.prepare_cache("cache-retry", asset.clone(), probe_stub()).unwrap();
    assert_eq!(wait(&jobs, "cache-retry").state, "succeeded");
    assert_eq!(std::fs::read(&index).unwrap(), original);
    assert_eq!(motion_media::load_video_index(root.path(), &asset).unwrap().len(), 72);
    assert!(thumbnail.is_file());
}

#[test]
fn source_and_original_audio_share_one_owned_file_and_one_history_entry() {
    let _guard=TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    start(&jobs, "import");
    assert_eq!(wait(&jobs, "import").state, "ready");
    let t = jobs.commit("import", &mut engine).unwrap();
    assert_eq!(t.state, "succeeded", "{:?}", t.error);
    assert_eq!(
        std::fs::read_dir(root.path().join("assets"))
            .unwrap()
            .count(),
        1
    );
    let p = engine.project();
    assert_eq!(p.version,motion_model::CURRENT_PROJECT_VERSION);
    assert_eq!(p.layers[0].rotation_order,motion_core::RotationOrder::FixedXyz);
    assert_eq!(p.video_assets[0].audio_asset, Some(p.audio_assets[0].id));
    assert_eq!(p.video_assets[0].path, p.audio_assets[0].path);
    let mut mixer = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut pcm = vec![0.; 2048];
    mixer.mix(6000, &mut pcm).unwrap();
    assert!(pcm.iter().any(|s| s.abs() > 0.03));
    let old = p.clone();
    engine
        .apply_batch(vec![Command::SplitLayerClip {
            object: 1,
            frame: 15,
        }])
        .unwrap();
    let mut split = AudioMixer::new(engine.snapshot(), root.path()).unwrap();
    let mut after = vec![0.; 2048];
    split.mix(6000, &mut after).unwrap();
    assert_eq!(pcm, after);
    engine.undo().unwrap();
    assert_eq!(*engine.project(), old);
    engine.undo().unwrap();
    assert!(engine.project().layers.is_empty());
    assert!(root.path().join(&old.video_assets[0].path).is_file());
    engine.redo().unwrap();
    assert_eq!(*engine.project(), old);
    let package = root.path().join("project.msproj");
    motion_core::storage::export_package(root.path(), engine.project(), &package).unwrap();
    let mut archive = zip::ZipArchive::new(File::open(&package).unwrap()).unwrap();
    assert_eq!(archive.len(), 2);
    assert!(archive.by_name(&old.video_assets[0].path).is_ok());
    let destination = root.path().join("restored");
    assert_eq!(
        motion_core::storage::import_package(&package, &destination).unwrap(),
        old
    );
    jobs.prepare_cache("rebuild", old.video_assets[0].clone(), probe_stub())
        .unwrap();
    assert_eq!(wait(&jobs, "rebuild").state, "succeeded");
}

#[test]
fn cancelled_probe_and_failed_save_preserve_the_original_project() {
    let _guard=TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    let before = engine.snapshot();
    start(&jobs, "cancel");
    assert_eq!(wait(&jobs, "cancel").state, "ready");
    assert_eq!(jobs.cancel("cancel").unwrap().state, "cancelled");
    jobs.commit("cancel", &mut engine).unwrap();
    assert_eq!(before, engine.snapshot());
    start(&jobs, "save-fail");
    assert_eq!(wait(&jobs, "save-fail").state, "ready");
    std::fs::create_dir(root.path().join("project.json")).unwrap();
    let t = jobs.commit("save-fail", &mut engine).unwrap();
    assert_eq!(t.state, "failed");
    assert_eq!(before, engine.snapshot());
    assert!(!engine.can_undo());
    assert_eq!(
        std::fs::read_dir(root.path().join("assets"))
            .unwrap()
            .count(),
        0
    );
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".video-import")));
}

#[test]
fn new_source_cap_accepts_large_declared_lengths_but_still_requires_exact_bytes() {
    let _guard=TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    for (id, bytes, message) in [
        ("above-old-limit", 512 * 1024 * 1024 + 1, "length mismatch"),
        (
            "at-new-limit",
            motion_core::storage::MAX_MEDIA_ASSET,
            "length mismatch",
        ),
        (
            "above-new-limit",
            motion_core::storage::MAX_MEDIA_ASSET + 1,
            "exceeds 2 GiB",
        ),
    ] {
        jobs.start(
            move || Ok((Box::new(std::io::Cursor::new(vec![1u8; 9])), Some(bytes))),
            options(id),
            false,
            probe_stub(),
        )
        .unwrap();
        let t = wait(&jobs, id);
        assert_eq!(t.state, "failed");
        assert!(t.error.unwrap().contains(message));
        jobs.release(id).unwrap();
    }
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".video-import")));
}

/// The clip must map layer-local time zero to the first visible source frame and
/// cover only the visible span; a source that starts at 0 keeps the old geometry.
/// Probing is injected, so no decoder runs.
#[test]
fn imported_clip_maps_local_time_to_the_visible_source_interval() {
    let _guard = TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    start_probe(&jobs, "visible-head", false, probe_delayed_start());
    assert_eq!(wait(&jobs, "visible-head").state, "ready");
    let t = jobs.commit("visible-head", &mut engine).unwrap();
    assert_eq!(t.state, "succeeded", "{:?}", t.error);
    jobs.release("visible-head").unwrap();
    {
        let p = engine.project();
        assert_eq!(p.fps, 60);
        let layer = &p.layers[0];
        let Content::Video { video } = &layer.content else {
            panic!("import did not create a video layer");
        };
        let clip = layer.clip(p.frames);
        assert_eq!(clip.in_frame, 0);
        assert_eq!(video.source_offset_us, 250_000);
        assert_eq!(video.source_time_us(layer.local_frame(0.0), p.fps), 250_000);
        assert_eq!(clip.out_frame - clip.in_frame, 165);
        let last = video.source_time_us(layer.local_frame(f64::from(clip.out_frame - 1)), p.fps);
        assert!((250_000..3_000_000).contains(&last), "last sample {last}");
    }
    start_probe(&jobs, "visible-zero", false, probe_stub());
    assert_eq!(wait(&jobs, "visible-zero").state, "ready");
    let t = jobs.commit("visible-zero", &mut engine).unwrap();
    assert_eq!(t.state, "succeeded", "{:?}", t.error);
    jobs.release("visible-zero").unwrap();
    let p = engine.project();
    let layer = p.layers.last().unwrap();
    let Content::Video { video } = &layer.content else {
        panic!("second import did not create a video layer");
    };
    let clip = layer.clip(p.frames);
    assert_eq!(video.source_offset_us, 0);
    assert_eq!(clip.out_frame - clip.in_frame, 180);
}

/// Opt-in disk/PCM transaction test. Video probing is injected; this does not
/// measure MediaCodec, preview FPS or decoded video fidelity. The selected MP4
/// must have an AAC first audio track at index 1 (same contract as the fixture).
#[test]
#[ignore = "requires MOTION_LARGE_VIDEO_SOURCE (>512 MiB MP4/AAC) and several GiB of free disk"]
fn large_source_roundtrip_with_shared_audio_and_bounded_read_requests() {
    let _guard=TEST_LOCK.lock().unwrap();
    use std::{
        io::{Read, Write},
        sync::atomic::{AtomicUsize, Ordering},
    };
    let source = std::path::PathBuf::from(
        std::env::var_os("MOTION_LARGE_VIDEO_SOURCE").expect("set MOTION_LARGE_VIDEO_SOURCE"),
    );
    let bytes = source.metadata().unwrap().len();
    assert!(bytes > 512 * 1024 * 1024 && bytes <= motion_core::storage::MAX_MEDIA_ASSET);
    let root = tempfile::tempdir().unwrap();
    let maximum = Arc::new(AtomicUsize::new(0));
    struct BoundedReader {
        file: File,
        maximum: Arc<AtomicUsize>,
    }
    impl Read for BoundedReader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.maximum.fetch_max(output.len(), Ordering::Relaxed);
            self.file.read(output)
        }
    }
    let m = maximum.clone();
    let input = source.clone();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 36000).unwrap()).unwrap();
    let begin = Instant::now();
    jobs.start(
        move || {
            Ok((
                Box::new(BoundedReader {
                    file: File::open(input).unwrap(),
                    maximum: m,
                }),
                Some(bytes),
            ))
        },
        options("large"),
        false,
        probe_stub(),
    )
    .unwrap();
    let mut task;
    loop {
        task = jobs.status("large").unwrap();
        if task.state != "running" {
            break;
        }
        assert!(begin.elapsed() < Duration::from_secs(900));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(task.state, "ready", "{:?}", task.error);
    assert_eq!(
        jobs.commit("large", &mut engine).unwrap().state,
        "succeeded"
    );
    let imported_ms = begin.elapsed().as_millis();
    let p = engine.project();
    assert_eq!(p.video_assets[0].bytes, bytes);
    assert_eq!(p.video_assets[0].path, p.audio_assets[0].path);
    assert_eq!(
        std::fs::read_dir(root.path().join("assets"))
            .unwrap()
            .count(),
        1
    );
    assert!(maximum.load(Ordering::Relaxed) <= motion_core::storage::TRANSFER_BUFFER_BYTES);
    let package = root.path().join("large.msproj");
    let begin = Instant::now();
    motion_core::storage::export_package(root.path(), p, &package).unwrap();
    let packed_ms = begin.elapsed().as_millis();
    assert!(package.metadata().unwrap().len() > bytes);
    let mut archive = zip::ZipArchive::new(File::open(&package).unwrap()).unwrap();
    assert_eq!(archive.len(), 2);
    assert_eq!(
        archive
            .by_name(&p.video_assets[0].path)
            .unwrap()
            .compression(),
        zip::CompressionMethod::Stored
    );
    let restored = root.path().join("restored");
    let begin = Instant::now();
    assert_eq!(
        motion_core::storage::import_package(&package, &restored).unwrap(),
        *p
    );
    let restored_ms = begin.elapsed().as_millis();
    // Compare the full source using fixed buffers; never materialize a large file.
    let mut a = File::open(&source).unwrap();
    let mut b = File::open(restored.join(&p.video_assets[0].path)).unwrap();
    let mut ab = [0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    let mut bb = [0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    loop {
        let n = a.read(&mut ab).unwrap();
        b.read_exact(&mut bb[..n]).unwrap();
        assert_eq!(&ab[..n], &bb[..n]);
        if n == 0 {
            assert_eq!(b.read(&mut bb).unwrap(), 0);
            break;
        }
    }
    let report = serde_json::json!({"bytes":bytes,"max_read_request_bytes":maximum.load(Ordering::Relaxed),"import_ms":imported_ms,"package_ms":packed_ms,"restore_ms":restored_ms,"archive_entries":2,"audio_video_share_source":true,"full_source_comparison":true,"video_probe":"injected","video_decode_verified":false});
    println!("{report}");
    if let Some(output) = std::env::var_os("MOTION_LARGE_VIDEO_REPORT") {
        File::create(output)
            .unwrap()
            .write_all(serde_json::to_string_pretty(&report).unwrap().as_bytes())
            .unwrap();
    }
}
