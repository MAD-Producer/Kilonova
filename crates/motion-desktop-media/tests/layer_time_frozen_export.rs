#![cfg(feature = "ffmpeg")]
//! Actual native encoding, decoding and frozen GPU output. The source clock
//! oracle below is independent of the model's mapping/evaluation functions.
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Content, Easing, Layer, LayerTimeline, Project, VideoClip,
};
use motion_desktop_media::{
    encode::Mp4Encoder,
    movie::{frame::FrameFreezePlan, ExportControl, FrozenMovie},
};
use motion_host::Session;
use std::{fs, path::Path};

const COLORS: [[u8; 4]; 8] = [
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [255, 255, 0, 255],
    [0, 255, 255, 255],
    [255, 0, 255, 255],
    [255, 255, 255, 255],
    [0, 0, 0, 255],
];

fn source(root: &Path) -> (Project, Vec<u64>) {
    fs::create_dir_all(root.join("assets")).unwrap();
    let path = root.join("assets/source.mp4");
    let mut project = Project::new(32, 32, 24, 12).unwrap();
    project.background = [0., 0., 0., 1.];
    let mut encoder = Mp4Encoder::new(&path, 32, 32, project.fps).unwrap();
    for frame in 0..16 {
        encoder
            .video_frame(
                &COLORS[frame as usize % COLORS.len()].repeat(32 * 32),
                frame,
            )
            .unwrap();
    }
    // Mp4Encoder deliberately requires both streams. This fixture has no
    // audible source, but still supplies the matching two-thirds-second AAC stream.
    let mut sample = 0;
    while sample < 32_000 {
        let length = encoder.audio_frame_size().min((32_000 - sample) as usize);
        encoder.audio_frame(&vec![0.; length * 2], sample).unwrap();
        sample += length as u64;
    }
    encoder.finish().unwrap();
    let mut probe =
        motion_desktop_media::av::probe(&path, None, Some(u32::MAX), &|| Ok(())).unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    assert_eq!(probe.timestamps.len(), 16);
    let mut layer = Layer::solid(7, "Mapped frozen video", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 2,
        out_frame: 10,
        offset_frame: 2,
    });
    project.layers.push(layer);
    project.video_assets.push(probe.asset);
    project.validate().unwrap();
    (project, probe.timestamps)
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
            clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 8.).unwrap(),
            remap: None,
        },
        "freeze" | "turn" => LayerTimeMapping {
            clock: LayerClock::default(),
            remap: Some(
                TimeRemap::new(
                    if mode == "freeze" {
                        vec![(0., 4.)]
                    } else {
                        vec![(0., 0.), (2., 4.), (4., 0.), (8., 4.)]
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
        _ => panic!("invalid fixture mode"),
    }
}

fn expected_time(mode: &str, composition_frame: f64) -> u64 {
    let local = composition_frame - 2.;
    let (source, earlier) = match mode {
        "stretch" => (local * 2., false),
        "reverse" => (8. - local, true),
        "freeze" => (4., false),
        "turn" if local < 2. => (local * 2., false),
        "turn" if local < 4. => (8. - local * 2., true),
        // The final segment covers four source frames in four local frames.
        "turn" => (local - 4., false),
        _ => panic!("invalid oracle mode"),
    };
    ((source / 24. * 1_000_000.).round() as u64).saturating_sub(u64::from(earlier))
}

fn center(rgba: &[u8]) -> [u8; 4] {
    rgba[((16 * 32 + 16) * 4)..((16 * 32 + 16) * 4 + 4)]
        .try_into()
        .unwrap()
}
fn near(actual: [u8; 4], expected: [u8; 4], context: &str) {
    // Native YUV420/H.264 round trips are lossy. Cube-corner colors make an
    // incorrect source frame differ by far more than this codec-only allowance.
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 16),
        "{context}: actual={actual:?} expected={expected:?}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn saved_reopened_frozen_png_and_movie_keep_mapped_video_clocks_after_live_edits() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("MOTION_LAYER_TIME_EXPORT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    fs::create_dir_all(&directory).unwrap();
    let root = directory.join("project");
    let (base, timestamps) = source(&root);
    let original = root.join("assets/source.mp4");
    let mut reference = motion_desktop_media::av::Decoder::open(
        &original,
        base.video_assets[0].clone(),
        timestamps,
    )
    .unwrap();
    let control = ExportControl::default();
    let mut report = Vec::new();
    for mode in ["stretch", "reverse", "freeze", "turn"] {
        let mut project = base.clone();
        project.layers[0].time_mapping = Some(mapping(mode));
        project.validate().unwrap();
        motion_core::storage::save(&root, &project).unwrap();
        let reopened = motion_core::storage::load(&root).unwrap();
        assert_eq!(reopened, project, "saved clock must round-trip: {mode}");
        let mut session =
            Session::new(reopened, root.clone(), motion_desktop_media::platform()).unwrap();
        let before = session.engine.snapshot();
        let mut plans = Vec::new();
        for frame in [1., 1.999, 2., 3.5, 4., 5., 9.999, 10.] {
            session.frame = frame;
            let path = directory.join(format!("{mode}-{frame}.png"));
            let plan = FrameFreezePlan::capture(&session, Some(path.clone()), &control).unwrap();
            plans.push((frame, path, plan));
        }
        let movie = FrozenMovie::from_session(&session).unwrap();
        assert_eq!(
            session.engine.snapshot(),
            before,
            "freezing must not edit {mode}"
        );
        session
            .engine
            .apply(motion_core::Command::LayerTime {
                object: 7,
                action: motion_core::layer_time::LayerTimeAction::Reset {},
            })
            .unwrap();
        assert_ne!(session.engine.snapshot(), before);
        let edited = session.engine.snapshot();
        assert!(session.engine.undo().unwrap());
        assert_eq!(session.engine.snapshot(), before);
        assert!(session.engine.redo().unwrap());
        assert_eq!(session.engine.snapshot(), edited);
        session.frame = 3.5;
        let control_path = directory.join(format!("{mode}-without-mapping.png"));
        let unmapped =
            FrameFreezePlan::capture(&session, Some(control_path.clone()), &control).unwrap();
        let cancelled = ExportControl::default();
        let cancelled_path = directory.join(format!("{mode}-cancelled.png"));
        let cancelled_plan =
            FrameFreezePlan::capture(&session, Some(cancelled_path.clone()), &cancelled).unwrap();
        assert!(cancelled.cancel());
        drop(session);
        assert!(cancelled_plan
            .export(&cancelled)
            .unwrap_err()
            .contains("cancelled"));
        assert!(!cancelled_path.exists(), "cancel must not publish {mode}");
        unmapped.export(&ExportControl::default()).unwrap();
        let control_pixel = center(image::open(control_path).unwrap().to_rgba8().as_raw());
        for (frame, path, plan) in plans {
            plan.export(&ExportControl::default()).unwrap();
            let image = image::open(&path).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (32, 32));
            let expected = if frame < 2. || frame >= 10. {
                [0, 0, 0, 255]
            } else {
                center(
                    reference
                        .frame(expected_time(mode, frame), &|| Ok(()))
                        .unwrap()
                        .rgba()
                        .unwrap()
                        .as_ref(),
                )
            };
            near(
                center(image.as_raw()),
                expected,
                &format!("PNG {mode} frame={frame}"),
            );
            report.push(serde_json::json!({"mode":mode,"format":"png","frame":frame,
                "actual":center(image.as_raw()),"expected":expected}));
            if frame == 3.5 {
                assert!(
                    center(image.as_raw())
                        .into_iter()
                        .zip(control_pixel)
                        .any(|(a, b)| a.abs_diff(b) > 96),
                    "removing the clock must visibly fail the pixel comparison for {mode}"
                );
            }
        }
        let output = directory.join(format!("{mode}.mp4"));
        movie
            .export(&output, &ExportControl::default(), |_| {})
            .unwrap();
        let probe =
            motion_desktop_media::av::probe(&output, None, Some(u32::MAX), &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), 12);
        let mut decoded =
            motion_desktop_media::av::Decoder::open(&output, probe.asset, probe.timestamps)
                .unwrap();
        for frame in [1u32, 2, 5, 9, 10, 11] {
            let time = (f64::from(frame) / 24. * 1_000_000.).round() as u64;
            let actual = center(
                decoded
                    .frame(time, &|| Ok(()))
                    .unwrap()
                    .rgba()
                    .unwrap()
                    .as_ref(),
            );
            let expected = if frame < 2 || frame >= 10 {
                [0, 0, 0, 255]
            } else {
                center(
                    reference
                        .frame(expected_time(mode, f64::from(frame)), &|| Ok(()))
                        .unwrap()
                        .rgba()
                        .unwrap()
                        .as_ref(),
                )
            };
            near(actual, expected, &format!("movie {mode} frame={frame}"));
            report.push(serde_json::json!({"mode":mode,"format":"mp4","frame":frame,
                "actual":actual,"expected":expected}));
        }
        fs::write(
            directory.join("pixels.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
