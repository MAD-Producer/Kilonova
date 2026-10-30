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
/// edit), the picture ends at 3.000 s, and the container duration is 3.250 s
/// because the audio track outlasts the picture (AAC padding).
fn probe_delayed_start() -> ProbeVideo {
    Arc::new(|source, _, audio, check| {
        check()?;
        let timestamps = (0..66)
            .map(|n| 250_000 + (n * 1_000_000 + 12) / 24)
            .collect::<Vec<u64>>();
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
                duration_us: 3_250_000,
                frame_count: 66,
                variable_frame_rate: false,
                nominal_frame_rate: 24.0,
                color_standard: 1,
                color_range: 2,
                audio_asset: None,
            },
            timestamps,
            audio_track: if audio == Some(u32::MAX) { None } else { Some(1) },
            first_rgba: vec![255; 256 * 144 * 4],
            tracks: serde_json::json!([]),
        })
    })
}

#[test]
fn delayed_picture_keeps_audio_and_shows_first_frame_without_shifting_the_clock() {
    let _guard = TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    start_probe(&jobs, "visible-with-audio", true, probe_delayed_start());
    let ready = wait(&jobs, "visible-with-audio");
    assert_eq!(ready.state, "ready", "{:?}", ready.error);
    let done = jobs.commit("visible-with-audio", &mut engine).unwrap();
    assert_eq!(done.state, "succeeded", "{:?}", done.error);
    let project = engine.project().clone();
    let layer = &project.layers[0];
    let Content::Video { video } = &layer.content else { panic!("expected video") };
    let asset = &project.video_assets[0];
    assert!(asset.audio_asset.is_some());
    assert!(video.hold_first_frame);
    assert_eq!(video.source_offset_us, 0);
    assert_eq!(video.picture_time_us(0., project.fps, asset), 250_000);
    assert_eq!(video.picture_time_us(15., project.fps, asset), 250_000);
    assert_eq!(video.picture_time_us(30., project.fps, asset), 500_000);
    assert_eq!(video.picture_time_us(-1., project.fps, asset), -16_667);
    assert_eq!(layer.clip(project.frames).out_frame, 195);
    assert_eq!(project.layer_audio(layer).unwrap().source_offset_us, 0);

    // Compare actual decoded PCM through the video layer against the same
    // source as an ordinary audio clip; this catches both truncation and drift.
    let mut audio_only = project.clone();
    let audio = motion_core::AudioClip::new(asset.audio_asset.unwrap());
    audio_only.layers[0].content = Content::Audio { audio };
    audio_only.layers[0].size = [0.; 2];
    audio_only.layers[0].transform = motion_core::Transform::new([0.; 3]);
    let audio_frames = (audio_only.audio_assets[0].duration_us * u64::from(audio_only.fps)).div_ceil(1_000_000);
    audio_only.layers[0].timeline.as_mut().unwrap().out_frame = audio_frames.min(195) as i32;
    let mut actual = AudioMixer::new(project, root.path()).unwrap();
    let mut reference = AudioMixer::new(audio_only, root.path()).unwrap();
    let mut nonzero = false;
    for start in [0, 12_000, 48_000, 96_000] {
        let mut a = vec![0.; 4096];
        let mut b = vec![0.; 4096];
        actual.mix(start, &mut a).unwrap();
        reference.mix(start, &mut b).unwrap();
        assert_eq!(a, b, "audio drift at sample {start}");
        nonzero |= a.iter().any(|sample| *sample != 0.);
    }
    assert!(nonzero, "audio fixture produced only silence");
}

#[test]
fn audio_tail_after_picture_is_preserved_with_delayed_first_frame() {
    let _guard = TEST_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let decode: motion_media::DecodeAudio = Arc::new(|source, pcm, _, _, check| {
        check(Some(0.0))?;
        std::fs::write(pcm, 0.5f32.to_le_bytes().repeat(156_000)).map_err(motion_media::Error::from)?;
        Ok(motion_core::AudioAsset {
            id: 1, path: "assets/source.mp4".into(), mime: "audio/mp4".into(),
            bytes: std::fs::metadata(source).map_err(motion_media::Error::from)?.len(),
            track: 1, sample_rate: 48_000, channels: 1, sample_frames: 156_000,
            duration_us: 3_250_000,
        })
    });
    let jobs = VideoJobs::with_audio_decoder(root.path().into(), decode).unwrap();
    let mut engine = Engine::new(Project::new(256, 144, 60, 240).unwrap()).unwrap();
    start_probe(&jobs, "audio-tail", true, probe_delayed_start());
    let ready = wait(&jobs, "audio-tail");
    assert_eq!(ready.state, "ready", "{:?}", ready.error);
    let done = jobs.commit("audio-tail", &mut engine).unwrap();
    assert_eq!(done.state, "succeeded", "{:?}", done.error);
    let mut mixer = AudioMixer::new(engine.project().clone(), root.path()).unwrap();
    let mut output = vec![0.; 1024];
    mixer.mix(148_800, &mut output).unwrap(); // 3.1 s, after the 3.0 s picture end.
    assert!(output.iter().all(|sample| *sample > 0.2), "audio tail was lost");
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

/// The offset and the length are independent. The source offset maps layer-local
/// time zero to the first visible source frame (no blank head), while the clip
/// length stays derived from `duration_us`: audio voices are clamped to the layer
/// interval, so sizing from the visible span would cut an AAC tail that outlasts
/// the picture. A source that starts at 0 keeps the old geometry.
/// Probing is injected, so no decoder runs.
#[test]
fn imported_clip_offsets_the_head_but_keeps_the_duration_derived_length() {
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
        // Head: layer-local zero is the first visible sample, not source PTS zero.
        assert_eq!(video.source_offset_us, 250_000);
        assert_eq!(video.source_time_us(layer.local_frame(0.0), p.fps), 250_000);
        // Length is duration-derived, not span-derived: 3.250 s of container
        // duration minus the 0.250 s offset is 3.000 s at 60 fps. A visible-span
        // length would be 2.750 s = 165 frames and would drop the audio padding
        // past video_end_us, which composition clamps to this same layer interval.
        assert_eq!(clip.out_frame - clip.in_frame, 180);
        // Project::validate bounds source_offset_us + length by duration_us, so the
        // clip stays inside the recoverable source: 250_000 + 179 frames.
        let last = video.source_time_us(layer.local_frame(f64::from(clip.out_frame - 1)), p.fps);
        assert_eq!(last, 3_233_333);
        assert!(last < 3_250_000, "clip exceeds duration_us: {last}");
        // The picture itself still ends at 3.000 s: the last frame inside
        // [video_start_us, video_end_us) is frame 164 (165 frames = 2.750 s). Frames
        // 165..180 cover only the audio padding and sample as the pre-existing
        // non-fatal "outside" state; that tail is a known follow-up, not fixed here.
        let visible_frames = (2_750_000u64 * u64::from(p.fps)).div_ceil(1_000_000);
        assert_eq!(visible_frames, 165);
        let last_visible =
            video.source_time_us(layer.local_frame((visible_frames - 1) as f64), p.fps);
        assert!(
            (250_000..3_000_000).contains(&last_visible),
            "last visible sample {last_visible}"
        );
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
    // A source whose first visible PTS is zero keeps the `main` geometry: no
    // offset and the same duration-derived length, so its audio window is
    // bit-identical to `main`.
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
