use motion_core::{CommandRegistry, Content, Engine, Layer, Project, VideoAsset, VideoClip};
use motion_model::depth::*;
use serde_json::json;

fn saved(pts: i64) -> SavedDepthCalibration {
    let resource = DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 74,
                revision: 1,
                pts: DepthTime {
                    ticks: pts,
                    timescale: 1_000_000,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "owned-video".into(),
                revision: "1".into(),
                sha256: [7; 32],
                processing: "test".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        vec![1., 2.],
        None,
        None,
    )
    .unwrap();
    SavedDepthCalibration::new(
        &resource,
        PreparedDepthInput {
            rgb_size: [2, 1],
            rgb_sha256: [17; 32],
        },
        [
            DepthAnchor {
                value: 1.,
                camera_z_meters: 8.,
            },
            DepthAnchor {
                value: 2.,
                camera_z_meters: 4.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
        20.,
    )
    .unwrap()
}
fn project() -> Project {
    let mut p = Project::new(32, 32, 30, 12).unwrap();
    let mut layer = Layer::solid(1, "Indexed video", [2., 1.], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip {
            source_offset_us: 10_000,
            ..VideoClip::new(74)
        },
    };
    layer
        .set_interval(
            12,
            motion_core::ClipRange {
                in_frame: 0.,
                out_frame: 4.,
            },
        )
        .unwrap();
    p.layers.push(layer);
    p.video_assets.push(VideoAsset {
        id: 74,
        path: "assets/owned.mp4".into(),
        bytes: 5,
        mime: "video/avc".into(),
        track: 0,
        width: 2,
        height: 1,
        rotation: 0,
        display_width: 2,
        display_height: 1,
        video_start_us: 10_000,
        video_end_us: 150_000,
        duration_us: 200_000,
        frame_count: 3,
        variable_frame_rate: true,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    p
}
fn command(pts: i64, end: u64) -> motion_core::Command {
    let mut commands = CommandRegistry::parse(
        &json!({"op":"video_depth_calibration", "object":1,
        "sample":{"calibration":saved(pts),"end_us":end}})
        .to_string(),
    )
    .unwrap();
    assert_eq!(commands.len(), 1);
    commands.remove(0)
}

#[test]
fn video_calibration_command_preserves_distinct_pts_in_history_and_storage() {
    let initial = project();
    let mut engine = Engine::new(initial.clone()).unwrap();
    engine.apply(command(10_000, 40_000)).unwrap();
    let first = engine.project().clone();
    engine.apply(command(40_000, 110_000)).unwrap();
    let both = engine.project().clone();
    let json = serde_json::to_value(&both).unwrap();
    assert_eq!(
        json["layers"][0]["video_depth_calibration"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &first);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &both);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    std::fs::write(root.path().join("assets/owned.mp4"), b"owned").unwrap();
    motion_core::storage::save(root.path(), &both).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), both);
    assert!(engine.project().layers[0].depth_calibration.is_none());
    engine
        .apply_batch(
            CommandRegistry::parse(r#"{"op":"depth_calibration","object":1,"calibration":null}"#)
                .unwrap(),
        )
        .unwrap();
    let cleared = serde_json::to_value(engine.project()).unwrap();
    assert!(cleared["layers"][0]
        .get("video_depth_calibration")
        .is_none());
}

#[test]
fn invalid_video_calibration_edits_are_atomic_and_clipboard_remaps_every_sample() {
    let mut engine = Engine::new(project()).unwrap();
    engine.apply(command(10_000, 40_000)).unwrap();
    engine.apply(command(40_000, 110_000)).unwrap();
    let before = engine.project().clone();
    assert!(engine.apply(command(20_000, 50_000)).is_err());
    assert_eq!(engine.project(), &before);
    assert!(CommandRegistry::parse(r#"{"op":"video_depth_calibration","object":1}"#).is_err());
    assert!(
        CommandRegistry::parse(r#"{"op":"video_depth_calibration","object":1,"sample":null}"#)
            .is_err()
    );
    let clipboard = motion_core::LayerClipboard::capture(&before, vec![1]).unwrap();
    let mut target = project();
    target.video_assets[0].path = "assets/other.mp4".into();
    let mut target = Engine::new(target).unwrap();
    target
        .apply(motion_core::Command::PasteLayers {
            clipboard: Box::new(clipboard),
            at_frame: None,
            keep_external_references: false,
        })
        .unwrap();
    let pasted = target.project().layers.last().unwrap();
    let Content::Video { video } = &pasted.content else {
        panic!("video layer required")
    };
    assert_ne!(video.asset, 74);
    let track = pasted.video_depth_calibration.as_ref().unwrap();
    assert_eq!(track.samples().len(), 2);
    assert!(track
        .samples()
        .iter()
        .all(|s| s.calibration().asset() == video.asset));
}
