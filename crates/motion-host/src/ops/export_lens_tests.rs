use super::*;
use motion_core::{
    tracking::{asset::Observation, plate::Calibration, shared::SharedData},
    *,
};

fn camera_fixture() -> Project {
    use motion_core::{
        media_assets::{AssetKind, AssetRef},
        tracking::{asset as a, camera::*, plate::*, point::*},
    };
    let mut p = Project::new(640, 480, 24, 120).unwrap();
    p.camera.created = true;
    let extent = Extent::new(320, 240).unwrap();
    p.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/authored.mp4".into(),
        bytes: 100,
        mime: "video/avc".into(),
        track: 0,
        width: 320,
        height: 240,
        rotation: 0,
        display_width: 320,
        display_height: 240,
        video_start_us: 0,
        video_end_us: 41667,
        duration_us: 41667,
        frame_count: 1,
        variable_frame_rate: false,
        nominal_frame_rate: 24.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    });
    let data = SharedData::new(a::Asset {
        version: a::VERSION,
        source_id: "Independent host lens guard".into(),
        source_sha256: [7; 32],
        source_extent: extent,
        coordinates: a::Coordinates::RightDownForward,
        ticks_per_second: 24,
        nominal_rate: a::Rate::new(24, 1).unwrap(),
        source_pts: vec![0],
        meters_per_world_unit: Some(1.),
        tracks: vec![a::Track {
            id: "camera".into(),
            kind: a::Kind::Camera,
            reference_index: None,
            samples: vec![a::Sample {
                source_index: 0,
                visibility: a::Visibility::Visible,
                confidence: 1.,
                manual_correction: false,
                observation: Some(Observation::Camera {
                    camera_to_world: a::Pose {
                        position: [0., 0., -5.],
                        orientation_xyzw: [0., 0., 0., 1.],
                    },
                    calibration: Calibration::new(
                        extent,
                        [210., 180.],
                        [142., 130.],
                        [0.; 3],
                        [0.; 2],
                    )
                    .unwrap(),
                }),
            }],
        }],
    })
    .unwrap();
    p.tracking_assets.push(TrackingAsset {
        id: 2,
        path: "assets/tracking/guard.mstrack".into(),
        bytes: 100,
        sha256: "0".repeat(64),
        format: TrackingFormat::MotionTrackingJsonV1,
        source: AssetRef {
            kind: AssetKind::Video,
            id: 1,
        },
        source_sha256: "07".repeat(32),
        data,
    });
    p.camera.tracking = Some(CameraBinding {
        asset: 2,
        track: "camera".into(),
        anchor_local_frame: 0,
        source_anchor_pts: 0,
        speed_numerator: 0,
        speed_denominator: 1,
        world_basis: [1, -2, -3],
        target_meters_per_unit: 0.01,
        target_origin: [0.; 3],
        correction_reference: CorrectionReference::capture(&p.camera, 0., [640, 480]).unwrap(),
        crop: [0., 0., 320., 240.],
        rotation: Rotation::None,
        near: 0.5,
        far: 100000.,
        interpolation: Interpolation::Linear,
        max_gap_ticks: 0,
        minimum_confidence: 0.8,
        missing: Missing::Error,
    });
    p.validate().unwrap();
    p
}

#[test]
fn tracking_lens_portable_host_guard_checks_nested_ownership_and_plain_recovery() {
    let mut project = camera_fixture();
    let mut raw = (*project.tracking_assets[0].data).clone();
    for sample in &mut raw.tracks[0].samples {
        if let Some(Observation::Camera {
            ref mut calibration,
            ..
        }) = sample.observation
        {
            *calibration = Calibration::new(
                raw.source_extent,
                calibration.focal(),
                calibration.principal(),
                [0.01, 0., 0.],
                [0.; 2],
            )
            .unwrap();
        }
    }
    project.tracking_assets[0].data = SharedData::new(raw).unwrap();
    let sample = |p: &Project| {
        let mut scene = Scene::new(p).unwrap();
        scene
            .sample(p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        scene
    };
    let error = require_portable_depth(&sample(&project))
        .unwrap_err()
        .to_string();
    assert!(error.contains("phase=lens_pack"), "{error}");
    project.compositions.push(Composition {
        id: "comp-lens-host".into(),
        name: "Owned nested lens".into(),
        width: project.width,
        height: project.height,
        fps: project.fps,
        frames: project.frames,
        work_area: project.work_area,
        guides: vec![],
        background: project.background,
        camera: project.camera.clone(),
        layers: vec![],
        expressions: vec![],
    });
    project.camera = Camera::new(project.width, project.height);
    let mut layer = Layer::solid(
        3,
        "Child reference",
        [640., 480.],
        [320., 240., 0.],
        [1.; 4],
    );
    layer.content = Content::Composition {
        clip: CompositionClip::new("comp-lens-host".into()),
    };
    project.layers.push(layer);
    let nested = sample(&project);
    assert!(nested.native_lens().is_none());
    assert!(nested.has_native_lens());
    assert!(require_portable_depth(&nested)
        .unwrap_err()
        .to_string()
        .contains("phase=lens_pack"));
    project.layers.clear();
    assert!(require_portable_depth(&sample(&project)).is_ok());
}
