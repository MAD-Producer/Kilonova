//! Native video preparation must not silently reuse another plate's solve.
#![cfg(feature = "ffmpeg")]
use super::*;
#[path = "tracking_video_replacement/mapped_time.rs"]
mod mapped_time;
use motion_core::media_assets::{AssetKind, AssetRef};
use motion_host::tracking::{asset::Sample, plate::Extent};
use serde_json::json;
use std::{fs, path::Path, time::Instant};
#[path = "../../../../crates/motion-model/tests/support/tracking_camera.rs"]
mod authored;
#[path = "tracking_video_pair_tests.rs"]
mod paired;

fn media(engine: &Engine, request: Value) -> Result<Value, String> {
    engine.media(request.to_string())
}

fn ready(engine: &Engine, request: &str) {
    let began = Instant::now();
    loop {
        let status = media(engine, json!({"op":"media_status","request_id":request})).unwrap();
        if status["state"] == "ready" {
            return;
        }
        assert_eq!(status["state"], "running", "{status}");
        assert!(began.elapsed() < Duration::from_secs(30), "{status}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn prepare(engine: &Engine, request: &str, path: &Path) {
    media(
        engine,
        json!({"op":"import_media","request_id":request,"path":path,
        "kind":"video","at_frame":0,"with_audio":false}),
    )
    .unwrap();
    ready(engine, request);
}

fn frame(engine: &Engine) -> image::RgbaImage {
    let began = Instant::now();
    let captured = loop {
        match engine.render_preview(Some(0.)) {
            Ok(frame) => break frame,
            Err(pending) if pending == "video capture pending; request frames and retry" => {
                assert!(began.elapsed() < Duration::from_secs(30), "{pending}");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(cause) => panic!("native video capture failed: {cause}"),
        }
    };
    image::open(captured["path"].as_str().unwrap())
        .unwrap()
        .to_rgba8()
}

#[test]
fn tracked_video_replacement_refuses_changed_plate_and_preserves_identical_source() {
    let observations = std::env::var_os("MOTION_TRACKING_VIDEO_NATIVE_DIR").map(PathBuf::from);
    let _logger = observations
        .as_ref()
        .map(|p| crate::logging::init(p.join("logs")).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let bytes = fs::read(fixtures.join("video-ctts.mp4")).unwrap();
    let source = root.join("assets/plate.mp4");
    fs::write(&source, &bytes).unwrap();
    let platform = motion_desktop_media::platform();
    let mut probe = platform
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/plate.mp4".into();
    let index = motion_media::video_cache_path(&root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let timing: Value =
        serde_json::from_slice(&fs::read(fixtures.join("native-timing.json")).unwrap()).unwrap();
    let timing = &timing["video-ctts.mp4"];
    let mut pts: Vec<_> = timing["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s[0].as_i64().unwrap())
        .collect();
    pts.sort_unstable();
    let mut project = authored::camera_fixture();
    let mut data = (*project.tracking_assets[0].data).clone();
    let mut binding = project.camera.tracking.take().unwrap();
    project.tracking_assets.clear();
    project.video_assets = vec![probe.asset.clone()];
    data.source_extent = Extent::new(probe.asset.width, probe.asset.height).unwrap();
    let digest = motion_core::storage::resource_sha256(&bytes);
    data.source_sha256 =
        std::array::from_fn(|i| u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap());
    data.ticks_per_second = timing["timescale"].as_u64().unwrap() as u32;
    data.source_pts = pts;
    let mut sample = data.tracks[0].samples[0].clone();
    if let Some(motion_host::tracking::asset::Observation::Camera { calibration, .. }) =
        &mut sample.observation
    {
        *calibration = motion_host::tracking::plate::Calibration::new(
            data.source_extent,
            [180., 160.],
            [128., 72.],
            [0.; 3],
            [0.; 2],
        )
        .unwrap();
    }
    data.tracks[0].samples = (0..data.source_pts.len())
        .map(|i| Sample {
            source_index: i as u32,
            ..sample.clone()
        })
        .collect();
    binding.source_anchor_pts = data.source_pts[0];
    binding.crop = [0., 0., probe.asset.width as f64, probe.asset.height as f64];
    let mut layer = motion_core::Layer::solid(
        50,
        "Native tracked video",
        [256., 144.],
        [320., 240., 0.],
        [1.; 4],
    );
    layer.three_d = true;
    layer.content = motion_core::Content::Video {
        video: motion_core::VideoClip::for_source(&probe.asset, false),
    };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: ((probe.asset.video_end_us - probe.asset.video_start_us)
            * u64::from(project.fps)
            / 1_000_000) as i32,
        offset_frame: 0,
    });
    project.layers.push(layer);
    motion_core::storage::save(&root, &project).unwrap();
    let document = temp.path().join("original.mstrack");
    let original = motion_host::tracking::encode::to_vec(&data).unwrap();
    fs::write(&document, &original).unwrap();
    let engine = Engine::start(root.clone(), platform, 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let imported = engine
        .import_tracking(
            document,
            AssetRef {
                kind: AssetKind::Video,
                id: 1,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    engine
        .edit_at_revision(
            json!([{"op":"set_tracking_camera","object":0,"binding":binding}]).to_string(),
            imported["revision"].as_u64().unwrap(),
        )
        .unwrap();
    let before = engine.state().unwrap();
    let warm = frame(&engine);
    assert_eq!(frame(&engine), warm);
    assert!(warm.pixels().filter(|p| p[3] > 0).count() > 100);
    // A valid free atom changes container identity without corrupting decoding.
    let wrong = temp.path().join("wrong.mp4");
    let mut changed = bytes.clone();
    changed.extend_from_slice(&16_u32.to_be_bytes());
    changed.extend_from_slice(b"freeauthored");
    fs::write(&wrong, &changed).unwrap();
    prepare(&engine, "tracked-video-wrong", &wrong);
    let cause = media(
        &engine,
        json!({"op":"finish_media_replacement","request_id":"tracked-video-wrong",
        "target":{"kind":"video","id":1},"revision":before["revision"]}),
    )
    .unwrap_err();
    assert!(cause.contains("tracking source content differs"), "{cause}");
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(engine.state().unwrap()["revision"], before["revision"]);
    assert_eq!(frame(&engine), warm);
    media(
        &engine,
        json!({"op":"cancel_media_import","request_id":"tracked-video-wrong"}),
    )
    .unwrap();
    media(
        &engine,
        json!({"op":"release_media_task","request_id":"tracked-video-wrong"}),
    )
    .unwrap();
    let same = temp.path().join("same.mp4");
    fs::write(&same, &bytes).unwrap();
    prepare(&engine, "tracked-video-same", &same);
    media(
        &engine,
        json!({"op":"finish_media_replacement","request_id":"tracked-video-same",
        "target":{"kind":"video","id":1},"revision":before["revision"]}),
    )
    .unwrap();
    let after = engine.state().unwrap();
    assert_eq!(after["project"]["camera"], before["project"]["camera"]);
    assert_eq!(
        after["project"]["tracking_assets"],
        before["project"]["tracking_assets"]
    );
    assert_eq!(frame(&engine), warm);
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    assert_eq!(frame(&engine), warm);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    assert_eq!(frame(&engine), warm);
    // The batched relink route independently performs native clock admission.
    for (request, path, accepted) in [
        ("tracked-video-relink-wrong", &wrong, false),
        ("tracked-video-relink-same", &same, true),
    ] {
        let current = engine.state().unwrap();
        media(
            &engine,
            json!({"op":"start_asset_relink","request_id":request,
            "revision":current["revision"],"document_generation":current["document_generation"],
            "candidates":[{"target":{"kind":"video","id":1},"path":path}]}),
        )
        .unwrap();
        ready(&engine, request);
        let result = media(
            &engine,
            json!({"op":"finish_asset_relink","request_id":request}),
        );
        if accepted {
            result.unwrap();
            let relinked = engine.state().unwrap();
            assert_eq!(relinked["project"]["camera"], current["project"]["camera"]);
            assert_eq!(
                relinked["project"]["tracking_assets"],
                current["project"]["tracking_assets"]
            );
            assert_eq!(engine.history(0).unwrap()["project"], current["project"]);
            assert_eq!(frame(&engine), warm);
            assert_eq!(engine.history(1).unwrap()["project"], relinked["project"]);
        } else {
            let cause = result.unwrap_err();
            assert!(cause.contains("tracking source content differs"), "{cause}");
            assert_eq!(engine.state().unwrap()["project"], current["project"]);
            assert_eq!(engine.state().unwrap()["revision"], current["revision"]);
        }
        assert_eq!(frame(&engine), warm);
        media(
            &engine,
            json!({"op":"release_asset_relink","request_id":request}),
        )
        .unwrap();
    }
    let after = engine.state().unwrap();
    engine.save().unwrap();
    drop(engine);
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
    assert_eq!(frame(&reopened), warm);
    assert_eq!(
        fs::read(
            root.join(
                after["project"]["tracking_assets"][0]["path"]
                    .as_str()
                    .unwrap()
            )
        )
        .unwrap(),
        original
    );
    if let Some(dir) = observations {
        warm.save(dir.join("before.png")).unwrap();
        let log = fs::read_to_string(dir.join("logs/motion-studio.log")).unwrap();
        assert!(log.contains("media replacement tracking start request=tracked-video-same"));
        assert!(log.contains("asset relink tracking start request=tracked-video-relink-same"));
        assert!(log.contains("phase=decode_tracking"));
    }
}
