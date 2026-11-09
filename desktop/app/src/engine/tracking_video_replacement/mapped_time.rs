//! Actual decoder and host publication retain authored clocks independently of original solve identity.
use super::*;
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Easing, Project,
};

fn fixture(
    root: &Path,
    bytes: &[u8],
) -> (
    Project,
    motion_host::tracking::asset::Asset,
    motion_core::tracking::camera::CameraBinding,
) {
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/plate.mp4"), bytes).unwrap();
    let source = root.join("assets/plate.mp4");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
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
    let original_timebase = data.ticks_per_second;
    let original_gap_ticks = binding.max_gap_ticks;
    data.ticks_per_second = timing["timescale"].as_u64().unwrap() as u32;
    // Retain the authored 4000 / 48000 second policy in the video's clock.
    // Floor the conversion so the physical gap limit never increases.
    binding.max_gap_ticks = (u128::from(original_gap_ticks) * u128::from(data.ticks_per_second)
        / u128::from(original_timebase))
    .try_into()
    .unwrap();
    assert!(
        u128::from(binding.max_gap_ticks) * u128::from(original_timebase)
            <= u128::from(original_gap_ticks) * u128::from(data.ticks_per_second)
    );
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
        out_frame: project
            .fps
            .frame_at_tick(
                (probe.asset.video_end_us - probe.asset.video_start_us)
                    .try_into()
                    .unwrap(),
                1_000_000,
                motion_core::ClockRounding::Floor,
            )
            .unwrap()
            .try_into()
            .unwrap(),
        offset_frame: 0,
    });
    project.layers.push(layer);
    (project, data, binding)
}

fn start(
    root: &Path,
    project: &Project,
    data: &motion_host::tracking::asset::Asset,
    binding: &motion_core::tracking::camera::CameraBinding,
) -> Engine {
    motion_core::storage::save(root, project).unwrap();
    let document = root.parent().unwrap().join(format!(
        "{}.mstrack",
        root.file_name().unwrap().to_string_lossy()
    ));
    fs::write(
        &document,
        motion_host::tracking::encode::to_vec(data).unwrap(),
    )
    .unwrap();
    let engine = Engine::start(
        root.to_path_buf(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
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
    engine
}

fn capture(engine: &Engine, composition: f64) -> image::RgbaImage {
    let began = Instant::now();
    loop {
        match engine.render_preview(Some(composition)) {
            Ok(value) => {
                return image::open(value["path"].as_str().unwrap())
                    .unwrap()
                    .to_rgba8()
            }
            Err(cause) if cause.to_string() == "video capture pending; request frames and retry" => {
                assert!(began.elapsed() < Duration::from_secs(30), "{cause}");
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(cause) => panic!("mapped video capture at {composition}: {cause}"),
        }
    }
}

#[test]
fn mapped_original_video_keeps_native_picture_and_authored_clock_across_replacement() {
    let observations = std::env::var_os("MOTION_TRACKING_VIDEO_CLOCK_DIR").map(PathBuf::from);
    let _logger = observations
        .as_ref()
        .map(|dir| crate::logging::init(dir.join("logs")).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let bytes = fs::read(fixtures.join("video-ctts.mp4")).unwrap();
    let same = temp.path().join("same.mp4");
    fs::write(&same, &bytes).unwrap();
    let wrong = temp.path().join("wrong.mp4");
    let mut different = bytes.clone();
    different.extend_from_slice(&16_u32.to_be_bytes());
    different.extend_from_slice(b"freeauthored");
    fs::write(&wrong, different).unwrap();
    let remap = |pairs: &[(f64, f64)]| {
        TimeRemap::new(
            pairs
                .iter()
                .map(|&(frame, source_frame)| TimeRemapKey {
                    frame,
                    source_frame,
                    easing: Easing::default(),
                })
                .collect(),
        )
        .unwrap()
    };
    let cases = [
        ("stretch", TimeDirection::Forward, 0., None, 36.75, 24.5),
        ("reverse", TimeDirection::Reverse, 48., None, 35.25, 24.5),
        (
            "source-remap",
            TimeDirection::Forward,
            0.,
            Some(remap(&[(0., 4.), (48., 20.)])),
            36.75,
            73. / 6.,
        ),
        (
            "source-hold",
            TimeDirection::Forward,
            0.,
            Some(remap(&[(0., 24.5)])),
            36.75,
            24.5,
        ),
    ];
    let mut report = Vec::new();
    for (name, direction, anchor, source_remap, composition, source_frame) in cases {
        let reference_root = temp.path().join(format!("reference-{name}"));
        let (reference_project, reference_data, reference_binding) =
            fixture(&reference_root, &bytes);
        let reference = start(
            &reference_root,
            &reference_project,
            &reference_data,
            &reference_binding,
        );
        let first = capture(&reference, 0.);
        let expected = capture(&reference, source_frame);
        assert_ne!(
            expected, first,
            "{name}: the original video must actually move"
        );
        assert!(expected.pixels().filter(|p| p[3] > 0).count() > 100);
        drop(reference);
        let root = temp.path().join(name);
        let (mut project, data, binding) = fixture(&root, &bytes);
        project.layers[0].time_mapping = Some(LayerTimeMapping {
            clock: LayerClock::new(TimeStretch::new(3, 2).unwrap(), direction, 0., anchor).unwrap(),
            remap: source_remap,
        });
        project.validate().unwrap();
        let authored = project.layers[0].time_mapping.clone();
        let original_pts = data.source_pts.clone();
        let engine = start(&root, &project, &data, &binding);
        let before = engine.state().unwrap();
        let warm = capture(&engine, composition);
        assert_eq!(
            warm, expected,
            "{name}: mapped native image differs from independently addressed original time"
        );
        prepare(&engine, &format!("mapped-{name}-wrong"), &wrong);
        let cause = media(
            &engine,
            json!({"op":"finish_media_replacement","request_id":format!("mapped-{name}-wrong"),
            "target":{"kind":"video","id":1},"revision":before["revision"]}),
        )
        .unwrap_err();
        assert!(
            cause.contains("tracking source content differs"),
            "{name}: {cause}"
        );
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        assert_eq!(engine.state().unwrap()["revision"], before["revision"]);
        assert_eq!(capture(&engine, composition), expected);
        for (operation, request) in [
            ("cancel_media_import", format!("mapped-{name}-wrong")),
            ("release_media_task", format!("mapped-{name}-wrong")),
        ] {
            media(&engine, json!({"op":operation,"request_id":request})).unwrap();
        }
        prepare(&engine, &format!("mapped-{name}-same"), &same);
        media(
            &engine,
            json!({"op":"finish_media_replacement","request_id":format!("mapped-{name}-same"),
            "target":{"kind":"video","id":1},"revision":before["revision"]}),
        )
        .unwrap();
        let after = engine.state().unwrap();
        let saved: Project = serde_json::from_value(after["project"].clone()).unwrap();
        assert_eq!(saved.layers[0].time_mapping, authored);
        assert_eq!(saved.tracking_assets[0].data.source_pts, original_pts);
        assert_eq!(after["project"]["camera"], before["project"]["camera"]);
        assert_eq!(
            after["project"]["tracking_assets"],
            before["project"]["tracking_assets"]
        );
        assert_eq!(capture(&engine, composition), expected);
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        assert_eq!(capture(&engine, composition), expected);
        assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
        assert_eq!(capture(&engine, composition), expected);
        let relink = format!("mapped-{name}-relink");
        let current = engine.state().unwrap();
        media(
            &engine,
            json!({"op":"start_asset_relink","request_id":relink,
                "revision":current["revision"],"document_generation":current["document_generation"],
                "candidates":[{"target":{"kind":"video","id":1},"path":same}]}),
        )
        .unwrap();
        ready(&engine, &relink);
        media(
            &engine,
            json!({"op":"finish_asset_relink","request_id":relink}),
        )
        .unwrap();
        let relinked = engine.state().unwrap();
        let saved: Project = serde_json::from_value(relinked["project"].clone()).unwrap();
        assert_eq!(saved.layers[0].time_mapping, authored);
        assert_eq!(saved.tracking_assets[0].data.source_pts, original_pts);
        assert_eq!(relinked["project"]["camera"], after["project"]["camera"]);
        assert_eq!(capture(&engine, composition), expected);
        assert_eq!(engine.history(0).unwrap()["project"], after["project"]);
        assert_eq!(capture(&engine, composition), expected);
        assert_eq!(engine.history(1).unwrap()["project"], relinked["project"]);
        assert_eq!(capture(&engine, composition), expected);
        media(
            &engine,
            json!({"op":"release_asset_relink","request_id":relink}),
        )
        .unwrap();
        let after = engine.state().unwrap();
        engine.save().unwrap();
        drop(engine);
        let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], after["project"]);
        assert_eq!(capture(&reopened, composition), expected);
        if let Some(dir) = &observations {
            expected
                .save(dir.join(format!("{name}-reference.png")))
                .unwrap();
            capture(&reopened, composition)
                .save(dir.join(format!("{name}-reopened.png")))
                .unwrap();
        }
        report.push(json!({"case":name,"composition_frame":composition,"reference_source_frame":source_frame,
            "equal_native_pixels":true,"original_pts":true,"authored_clock":true,"undo_redo_reopen":true}));
    }
    if let Some(dir) = observations {
        fs::write(
            dir.join("report.json"),
            serde_json::to_vec_pretty(&json!({"cases":report})).unwrap(),
        )
        .unwrap();
    }
}
