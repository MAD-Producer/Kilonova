use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Content, Easing, Layer, Project, VideoClip,
};
use motion_host::video_frames::VideoFrames;
use motion_render::Scene;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn fixture(root: &Path, name: &str) -> (Project, Vec<u64>) {
    fs::create_dir_all(root.join("assets")).unwrap();
    let path = root.join("assets/source.mp4");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/motion-media/tests/fixtures/video")
            .join(name),
        &path,
    )
    .unwrap();
    let mut found =
        motion_desktop_media::av::probe(&path, None, Some(u32::MAX), &|| Ok(())).unwrap();
    found.asset.id = 1;
    found.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &found.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &found.asset, &found.timestamps).unwrap();
    let mut p = Project::new(32, 32, 24, 12).unwrap();
    let mut layer = Layer::solid(
        7,
        "Native mapped prefetch",
        [32.; 2],
        [16., 16., 0.],
        [1.; 4],
    );
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    p.layers.push(layer);
    p.video_assets.push(found.asset);
    p.validate().unwrap();
    (p, found.timestamps)
}
fn scene(p: &Project, frame: f64) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
fn mapping(mode: &str) -> LayerTimeMapping {
    match mode {
        "stretch" => LayerTimeMapping {
            clock: LayerClock::new(
                TimeStretch::new(1, 2).unwrap(),
                TimeDirection::Forward,
                0.,
                0.,
            )
            .unwrap(),
            remap: None,
        },
        "reverse" => LayerTimeMapping {
            clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 12.)
                .unwrap(),
            remap: None,
        },
        "freeze" | "turn" => LayerTimeMapping {
            clock: LayerClock::default(),
            remap: Some(
                TimeRemap::new(
                    if mode == "freeze" {
                        vec![(0., 6.)]
                    } else {
                        vec![(0., 0.), (4., 12.), (8., 0.), (12., 12.)]
                    }
                    .into_iter()
                    .map(|(frame, source_frame)| TimeRemapKey {
                        frame,
                        source_frame,
                        easing: Easing::default(),
                    })
                    .collect(),
                )
                .unwrap(),
            ),
        },
        _ => panic!("unknown native media fixture"),
    }
}
fn source_time(mode: &str, frame: f64) -> u64 {
    let (source, earlier) = match mode {
        "stretch" => (frame * 2., false),
        "reverse" => (12. - frame, true),
        "freeze" => (6., false),
        "turn" if frame < 4. => (frame * 3., false),
        "turn" if frame < 8. => (24. - frame * 3., true),
        "turn" => ((frame - 8.) * 3., false),
        _ => panic!("unknown oracle fixture"),
    };
    (source / 24. * 1_000_000.).round() as u64 - u64::from(earlier)
}
fn ready(
    reader: &mut VideoFrames,
    p: &Project,
    root: &Path,
    s: &Scene,
    exact: bool,
) -> Vec<(u64, motion_host::video_frame::PreparedVideoFrame)> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let result = if exact {
            reader.prepare_scene_exact(p, root, s)
        } else {
            reader.prepare_scene(p, root, s, s.frame)
        };
        if let Some(frames) = result.unwrap() {
            return frames;
        }
        assert!(
            Instant::now() < deadline,
            "native video pending: {}",
            reader.metrics()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_cfr_and_vfr_prefetch_keep_exact_pixels_for_stretch_reverse_freeze_and_turn() {
    let mut report = vec![];
    for name in ["silent-24fps.mp4", "variable.mp4"] {
        let root = tempfile::tempdir().unwrap();
        let (base, index) = fixture(root.path(), name);
        for mode in ["stretch", "reverse", "freeze", "turn"] {
            let mut p = base.clone();
            p.layers[0].time_mapping = Some(mapping(mode));
            p.validate().unwrap();
            let before = serde_json::to_value(&p).unwrap();
            let mut reader = VideoFrames::new(motion_desktop_media::platform());
            let mut oracle = motion_desktop_media::av::Decoder::open(
                &root.path().join("assets/source.mp4"),
                p.video_assets[0].clone(),
                index.clone(),
            )
            .unwrap();
            for step in 2..10 {
                let frame = f64::from(step) + 0.5;
                let s = scene(&p, frame);
                let expected = source_time(mode, frame);
                assert_eq!(
                    s.video_decode_layers()[0]
                        .video
                        .as_ref()
                        .unwrap()
                        .source_time_us,
                    expected,
                    "{name} {mode} frame={frame}"
                );
                let actual = ready(&mut reader, &p, root.path(), &s, false).remove(0).1;
                let reference = oracle.frame(expected, &|| Ok(())).unwrap();
                assert_eq!(
                    actual.left.pts, reference.pts,
                    "{name} {mode} frame={frame}"
                );
                assert_eq!(
                    actual.left.rgba().unwrap().as_ref(),
                    reference.rgba().unwrap().as_ref(),
                    "{name} {mode} frame={frame}"
                );
                let metrics = reader.metrics();
                assert!(
                    metrics["cacheFrames"].as_u64().unwrap() <= 5
                        && metrics["cacheBytes"].as_u64().unwrap()
                            <= metrics["cacheBudgetBytes"].as_u64().unwrap()
                );
            }
            if mode == "freeze" {
                assert_eq!(reader.metrics()["decodedFrames"], 1);
            }
            report.push(
                serde_json::json!({"file":name,"mode":mode,"points":8,"metrics":reader.metrics()}),
            );
            reader.finish().unwrap();
            assert_eq!(serde_json::to_value(&p).unwrap(), before);
        }
    }
    if let Some(path) = std::env::var_os("MOTION_MAPPED_PREFETCH_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

#[test]
fn mapped_prefetch_planning_fault_uses_default_channels_and_recovers_without_stalling_required_video(
) {
    const CHILD: &str = "MOTION_MAPPED_PREFETCH_LOG_CHILD";
    const TEST:&str="engine::mapped_video_prefetch_tests::mapped_prefetch_planning_fault_uses_default_channels_and_recovers_without_stalling_required_video";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let (p, _) = fixture(&root, "silent-24fps.mp4");
        let s = scene(&p, 6.);
        let mut reader = VideoFrames::new(motion_desktop_media::platform());
        let expected = ready(&mut reader, &p, &root, &s, true).remove(0).1.left.pts;
        let mut bad = p.clone();
        bad.layers.clear();
        // A stale scene deliberately points at a removed model object. The
        // existing required decoded frame stays available; speculation stops.
        for count in 1..=120 {
            let frames = reader.prepare_scene(&bad, &root, &s, 6.).unwrap().unwrap();
            assert_eq!(frames[0].1.left.pts, expected);
            if [1, 59, 60, 120].contains(&count) {
                crate::logging::flush();
                let text = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
                assert_eq!(
                    text.lines()
                        .filter(|l| l.contains("site=video_prefetch_plan "))
                        .count(),
                    usize::from(count >= 60)
                );
            }
        }
        assert!(reader.prepare_scene(&p, &root, &s, 6.).unwrap().is_some());
        for _ in 0..60 {
            assert!(reader.prepare_scene(&bad, &root, &s, 6.).unwrap().is_some());
        }
        reader.finish().unwrap();
        crate::logging::flush();
        let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let faults: Vec<_> = log
            .lines()
            .filter(|l| l.contains("site=video_prefetch_plan "))
            .collect();
        assert_eq!(faults.len(), 2);
        for line in faults {
            assert!(
                line.contains(" ERROR ")
                    && line.contains("count=60 ")
                    && line.contains("frame=6 ")
                    && line.contains("phase=plan ")
                    && line.contains(&motion_core::Error::Missing(7).to_string()),
                "{line}"
            );
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    if let Some(path) = std::env::var_os("MOTION_MAPPED_PREFETCH_LOG_ARTIFACTS") {
        let path = PathBuf::from(path);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("stdout.log"), &output.stdout).unwrap();
        fs::write(path.join("stderr.log"), &output.stderr).unwrap();
        if root.path().join("logs/motion-studio.log").exists() {
            fs::copy(
                root.path().join("logs/motion-studio.log"),
                path.join("motion-studio.log"),
            )
            .unwrap();
        }
    }
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("site=video_prefetch_plan ")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("site=video_prefetch_plan "));
}
