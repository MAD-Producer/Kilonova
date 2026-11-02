#![cfg(feature = "ffmpeg")]
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Content, Layer, LayerTimeline, Project, VideoClip,
};
use motion_desktop_media::{
    av::{probe, Decoder},
    platform,
};
use motion_host::{
    preview_inputs::{InputPixels, InputPoll, InputRequest, InputSource, PreviewInputReader},
    video_frames::VideoFrames,
    Session,
};
use motion_render::Scene;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../motion-media/tests/fixtures/video")
        .join(name)
}
fn project(root: &Path, name: &str) -> (Project, Vec<u64>) {
    std::fs::create_dir(root.join("assets")).unwrap();
    let path = root.join("assets/source.mp4");
    std::fs::copy(fixture(name), &path).unwrap();
    let mut found = probe(&path, None, Some(u32::MAX), &|| Ok(())).unwrap();
    found.asset.id = 1;
    found.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &found.asset).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &found.asset, &found.timestamps).unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(fixture("manifest.json")).unwrap()).unwrap();
    let reference: Vec<_> = manifest[name]["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (f["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                * 1_000_000.)
                .round() as u64
        })
        .collect();
    assert_eq!(
        found.timestamps, reference,
        "native probe must match fixture's reference index"
    );
    let mut p = Project::new(32, 32, if name == "variable.mp4" { 48 } else { 24 }, 72).unwrap();
    p.background = [0., 0., 0., 1.];
    let mut layer = Layer::solid(7, "video", [32.; 2], [16., 16., 0.], [1.; 4]);
    let mut video = VideoClip::new(1);
    video.source_offset_us = 250_000;
    layer.content = Content::Video { video };
    layer.timeline = Some(LayerTimeline {
        in_frame: 6,
        out_frame: 54,
        offset_frame: 6,
    });
    p.layers.push(layer);
    p.video_assets.push(found.asset);
    p.validate().unwrap();
    (p, reference)
}
fn mapping(mode: &str) -> Option<LayerTimeMapping> {
    let (clock, remap) = match mode {
        "identity" => return None,
        "stretch" => (
            LayerClock::new(
                TimeStretch::new(3, 2).unwrap(),
                TimeDirection::Forward,
                0.,
                0.,
            )
            .unwrap(),
            None,
        ),
        "reverse" => (
            LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 48.).unwrap(),
            None,
        ),
        "stationary" => (
            LayerClock::default(),
            Some(
                TimeRemap::new(vec![
                    TimeRemapKey {
                        frame: 0.,
                        source_frame: 48.,
                        easing: motion_core::Easing {
                            ease: motion_core::Ease::In,
                            curve: None,
                        },
                    },
                    TimeRemapKey {
                        frame: 48.,
                        source_frame: 0.,
                        easing: Default::default(),
                    },
                ])
                .unwrap(),
            ),
        ),
        "remap" => (
            LayerClock::default(),
            Some(
                TimeRemap::new(
                    [(0., 0.), (12., 36.), (24., 12.), (36., 12.), (48., 48.)]
                        .into_iter()
                        .map(|(frame, source_frame)| TimeRemapKey {
                            frame,
                            source_frame,
                            easing: Default::default(),
                        })
                        .collect(),
                )
                .unwrap(),
            ),
        ),
        _ => panic!("unknown fixture mode"),
    };
    Some(LayerTimeMapping { clock, remap })
}
fn cases(mode: &str, fps: u32) -> Vec<(f64, u64)> {
    if fps == 48 {
        return match mode {
            "identity" => vec![(6., 250_000), (30., 750_000), (53.5, 1_239_583)],
            "stretch" => vec![(6., 250_000), (30., 583_333), (53.5, 909_722)],
            "reverse" => vec![(6., 1_249_999), (30., 749_999), (53.5, 260_416)],
            "stationary" => vec![(6., 1_249_999), (30., 999_999)],
            "remap" => vec![
                (6., 250_000),
                (12., 625_000),
                (18., 999_999),
                (24., 749_999),
                (30., 500_000),
                (36., 500_000),
                (42., 500_000),
                (48., 875_000),
            ],
            _ => panic!("unknown fixture mode"),
        };
    }
    assert_eq!(fps, 24);
    match mode {
        "identity" => vec![(6., 250_000), (30., 1_250_000), (53.5, 2_229_167)],
        "stretch" => vec![(6., 250_000), (30., 916_667), (53.5, 1_569_444)],
        "reverse" => vec![(6., 2_249_999), (30., 1_249_999), (53.5, 270_832)],
        "stationary" => vec![(6., 2_249_999), (30., 1_749_999)],
        "remap" => vec![
            (6., 250_000),
            (12., 1_000_000),
            (18., 1_749_999),
            (24., 1_249_999),
            (30., 750_000),
            (36., 750_000),
            (42., 750_000),
            (48., 1_500_000),
        ],
        _ => panic!("unknown fixture mode"),
    }
}

#[test]
fn scene_timeline_and_native_reader_share_mapped_cfr_and_vfr_source_frames() {
    for name in ["silent-24fps.mp4", "variable.mp4"] {
        let root = tempfile::tempdir().unwrap();
        let (base, reference) = project(root.path(), name);
        for mode in ["identity", "stretch", "reverse", "remap", "stationary"] {
            let mut p = base.clone();
            p.layers[0].time_mapping = mapping(mode);
            p.validate().unwrap();
            let before = p.clone();
            let mut scene = Scene::new(&p).unwrap();
            let mut reader = VideoFrames::new(platform());
            let mut input_reader = PreviewInputReader::new(platform());
            let mut oracle = Decoder::open(
                &root.path().join("assets/source.mp4"),
                p.video_assets[0].clone(),
                reference.clone(),
            )
            .unwrap();
            for (index, (frame, expected)) in cases(mode, p.fps).into_iter().enumerate() {
                scene
                    .sample(&p, frame, None, &motion_core::ExpressionEvaluator)
                    .unwrap();
                let shown = scene
                    .layers
                    .iter()
                    .find(|l| l.id == 7)
                    .unwrap()
                    .video
                    .as_ref()
                    .unwrap();
                assert_eq!(shown.source_time_us, expected, "{name} {mode} C={frame}");
                assert_eq!(
                    p.timeline_layers(frame).unwrap()[0]
                        .video
                        .as_ref()
                        .unwrap()
                        .source_time_us,
                    expected as i64
                );
                let sequence = index as u64 + 1;
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let state = reader.request(&p, root.path(), 7, frame, sequence).unwrap();
                    assert_ne!(state["state"], "failed", "{state}");
                    assert_eq!(state["source_time_us"], expected);
                    if state["state"] == "ready" {
                        break;
                    }
                    assert!(Instant::now() < deadline, "native frame pending: {state}");
                    std::thread::sleep(Duration::from_millis(1));
                }
                let actual = reader.frame(&p, 7, sequence).unwrap();
                let pts = reference[reference.partition_point(|t| *t <= expected) - 1];
                assert_eq!(actual.pts, pts, "{name} {mode} source={expected}");
                let reference_pixels = oracle.frame(expected, &|| Ok(())).unwrap();
                assert_eq!(
                    actual.rgba().unwrap().as_ref(),
                    reference_pixels.rgba().unwrap().as_ref()
                );
                let info = input_reader
                    .request(
                        &p,
                        root.path(),
                        1,
                        InputRequest {
                            sequence,
                            source: InputSource::Layer {
                                composition: p.composition_id.clone(),
                                object: 7,
                            },
                            frame,
                            max_edge: 256,
                        },
                    )
                    .unwrap();
                assert_eq!(info.clock.source_time_us, Some(expected as i64));
                let deadline = Instant::now() + Duration::from_secs(10);
                let packet = loop {
                    match input_reader.poll(sequence, 1).unwrap() {
                        InputPoll::Ready(packet) => break packet,
                        InputPoll::Pending(_) => {
                            assert!(Instant::now() < deadline, "original video input pending");
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                };
                let InputPixels::Video(original) = packet.pixels else {
                    panic!("video input required")
                };
                assert_eq!(original.pts, pts);
                assert_eq!(
                    original.rgba().unwrap().as_ref(),
                    actual.rgba().unwrap().as_ref()
                );
            }
            assert_eq!(p, before, "sampling must not reauthor clocks or keys");
            if mode != "identity" {
                let cases = cases(mode, p.fps);
                p.layers[0].time_mapping = None;
                let error = match reader.frame(&p, 7, cases.len() as u64) {
                    Err(error) => error,
                    Ok(_) => panic!("a changed clock cannot publish a stale frame"),
                };
                assert!(error.to_string().contains("clip changed"), "{error}");
            }
            reader.finish().unwrap();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_png_matches_an_unmapped_native_source_oracle_after_live_clock_changes() {
    use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
    for (name, mode, frame, source) in [
        ("silent-24fps.mp4", "reverse", 6., 2_249_999),
        ("variable.mp4", "remap", 24., 749_999),
        ("silent-24fps.mp4", "stationary", 6., 2_249_999),
        ("variable.mp4", "stationary", 6., 1_249_999),
    ] {
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("project");
        std::fs::create_dir(&project_dir).unwrap();
        let (mut p, _) = project(&project_dir, name);
        p.layers[0].time_mapping = mapping(mode);
        let mut session = Session::new(p, project_dir.clone(), platform()).unwrap();
        session.frame = frame;
        session.sample().unwrap();
        let control = ExportControl::default();
        let output = root.path().join("mapped.png");
        let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
        // The frozen plan must retain the old clock after a real live edit.
        session
            .apply_commands(r#"{"op":"layer_time","object":7,"action":{"kind":"reset"}}"#)
            .unwrap();
        let oracle_output = root.path().join("oracle.png");
        session.frame =
            6. + (source as f64 - 250_000.) * f64::from(session.engine.project().fps) / 1_000_000.;
        session.sample().unwrap();
        assert_eq!(
            session.scene.layers[0]
                .video
                .as_ref()
                .unwrap()
                .source_time_us,
            source
        );
        let oracle =
            FrameFreezePlan::capture(&session, Some(oracle_output.clone()), &control).unwrap();
        drop(session);
        plan.export(&control).unwrap();
        oracle.export(&control).unwrap();
        let actual = image::open(output).unwrap().to_rgba8();
        let expected = image::open(oracle_output).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (32, 32));
        assert_eq!(actual, expected, "{name} {mode} frozen image");
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn nested_cfr_vfr_native_export_and_gpu_exposure_match_flat_source_pixels() {
    use motion_core::{Camera, Composition, CompositionClip, WorkArea};
    use motion_desktop_media::movie::{frame::FrameFreezePlan, ExportControl};
    use motion_host::{
        preview_video::PreviewVideoCache,
        render_sampling::{ExposureMedia, ExposureMediaPolicy},
    };
    use motion_render::{
        exposure_scene::{ExposureDocument, ExposureScenes},
        ExposureFrame, ExposurePoll, Renderer,
    };
    use std::sync::Arc;

    for name in ["silent-24fps.mp4", "variable.mp4"] {
        let root = tempfile::tempdir().unwrap();
        let project_dir = root.path().join("project");
        std::fs::create_dir(&project_dir).unwrap();
        let (base, _) = project(&project_dir, name);
        let child_fps = base.fps;
        let mut p = base.clone();
        p.fps = child_fps / 2;
        p.compositions.push(Composition {
            id: "comp-child".into(),
            name: "source".into(),
            width: 32,
            height: 32,
            fps: child_fps,
            frames: 72,
            work_area: WorkArea::full(72),
            guides: vec![],
            background: [0.; 4],
            camera: Camera::new(32, 32),
            layers: p.layers.clone(),
            expressions: vec![],
        });
        let mut reference = Layer::solid(10, "reference", [32.; 2], [16., 16., 0.], [1.; 4]);
        reference.content = Content::Composition {
            clip: CompositionClip::new("comp-child".into()),
        };
        reference.timeline = Some(LayerTimeline {
            in_frame: 3,
            out_frame: 27,
            offset_frame: 3,
        });
        p.layers = vec![reference];
        for stationary in [false, true] {
            p.layers[0].time_mapping = Some(if stationary {
                LayerTimeMapping {
                    clock: LayerClock::default(),
                    remap: Some(
                        TimeRemap::new(vec![
                            TimeRemapKey {
                                frame: 0.,
                                source_frame: 24.,
                                easing: motion_core::Easing {
                                    ease: motion_core::Ease::In,
                                    curve: None,
                                },
                            },
                            TimeRemapKey {
                                frame: 24.,
                                source_frame: 0.,
                                easing: Default::default(),
                            },
                        ])
                        .unwrap(),
                    ),
                }
            } else {
                LayerTimeMapping {
                    clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 24.)
                        .unwrap(),
                    remap: None,
                }
            });
            p.validate().unwrap();
            for (frame, expected_source) in match (child_fps, stationary) {
                (24, false) => [(3., 1_999_999), (15., 999_999)],
                (24, true) => [(3., 1_999_999), (15., 1_499_999)],
                (48, false) => [(3., 1_124_999), (15., 624_999)],
                (48, true) => [(3., 1_124_999), (15., 874_999)],
                _ => panic!("unexpected fixture rate"),
            } {
                let control = ExportControl::default();
                let mut session = Session::new(p.clone(), project_dir.clone(), platform()).unwrap();
                session.frame = frame;
                session.sample().unwrap();
                assert_eq!(
                    session.scene.nested[0].scene.layers[0]
                        .video
                        .as_ref()
                        .unwrap()
                        .source_time_us,
                    expected_source
                );
                let output = root.path().join("nested.png");
                let frozen =
                    FrameFreezePlan::capture(&session, Some(output.clone()), &control).unwrap();
                drop(session);
                frozen.export(&control).unwrap();

                let mut oracle_session =
                    Session::new(base.clone(), project_dir.clone(), platform()).unwrap();
                oracle_session.frame =
                    6. + (expected_source as f64 - 250_000.) * f64::from(child_fps) / 1_000_000.;
                oracle_session.sample().unwrap();
                assert_eq!(
                    oracle_session.scene.layers[0]
                        .video
                        .as_ref()
                        .unwrap()
                        .source_time_us,
                    expected_source
                );
                let oracle_output = root.path().join("flat.png");
                let oracle = FrameFreezePlan::capture(
                    &oracle_session,
                    Some(oracle_output.clone()),
                    &control,
                )
                .unwrap();
                drop(oracle_session);
                oracle.export(&control).unwrap();
                let expected = image::open(oracle_output).unwrap().to_rgba8();
                assert_eq!(
                    image::open(output).unwrap().to_rgba8(),
                    expected,
                    "{name} C={frame} ordinary frozen export"
                );

                // The public single-phase exposure path still reconstructs child
                // scenes. Its media callback must receive the inherited direction.
                let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
                renderer.configure_assets(&p, &project_dir).unwrap();
                let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
                let scenes = ExposureScenes::new(
                    document,
                    &p.composition_id,
                    frame,
                    false,
                    None,
                    motion_core::ExpressionEvaluator,
                )
                .unwrap();
                let mut task = ExposureFrame::new(&mut renderer, scenes, 32, 32).unwrap();
                let mut videos = VideoFrames::new(platform());
                let mut proxies = PreviewVideoCache::default();
                let mut media = ExposureMedia {
                    project: &p,
                    root: &project_dir,
                    videos: &mut videos,
                    proxies: &mut proxies,
                    policy: ExposureMediaPolicy::FULL,
                };
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut actual_source_seen = false;
                loop {
                    let poll = task
                        .poll(
                            &mut renderer,
                            Duration::from_millis(8),
                            |renderer, scene| {
                                for layer in &scene.layers {
                                    if let Some(video) = &layer.video {
                                        assert_eq!(
                                            video.source_time_us, expected_source,
                                            "{name} exposure child"
                                        );
                                        actual_source_seen = true;
                                    }
                                }
                                media.prepare(renderer, scene)
                            },
                        )
                        .unwrap();
                    if matches!(poll, ExposurePoll::Ready(_)) {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native nested exposure timed out"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(actual_source_seen, "actual video media callback must run");
                assert_eq!(
                    renderer.capture_exposure(task.target()).unwrap(),
                    expected.into_raw(),
                    "{name} C={frame} nested exposure pixels"
                );
                drop(task);
                videos.finish().unwrap();
            }
        }
    }
}
