use motion_model::{
    media_assets::{AssetKind, AssetRef},
    tracking::{
        asset::*,
        plate::{Extent, Rotation},
        point::*,
        shared::SharedData,
        time::Time,
    },
    *,
};

#[path = "tracking_layer_time/point.rs"]
mod mapped_clock;

fn fixture() -> Project {
    let mut p = Project::new(640, 480, 24, 120).unwrap();
    let data = SharedData::new(tracking::asset::Asset {
        version: VERSION,
        source_id: "independently-authored-vfr-control".into(),
        source_sha256: [7; 32],
        source_extent: Extent::new(320, 240).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 48_000,
        nominal_rate: Rate::new(30_000, 1001).unwrap(),
        source_pts: vec![-4000, -2000, 0, 4000],
        meters_per_world_unit: None,
        tracks: vec![tracking::asset::Track {
            id: "feature".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: [[20., 30.], [30., 50.], [50., 70.], [90., 110.]]
                .into_iter()
                .enumerate()
                .map(|(i, point)| Sample {
                    source_index: i as u32,
                    visibility: Visibility::Visible,
                    confidence: 0.9,
                    observation: Some(Observation::Point(point)),
                    manual_correction: i == 2,
                })
                .collect(),
        }],
    })
    .unwrap();
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
        video_end_us: 200_000,
        duration_us: 200_000,
        frame_count: 4,
        variable_frame_rate: true,
        nominal_frame_rate: 30_000. / 1001.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    });
    p.tracking_assets.push(TrackingAsset {
        id: 2,
        path: "assets/tracking/control.mstrack".into(),
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
    let mut l = Layer::solid(7, "Tracked null", [1.; 2], [100., 80., 9.], [0.; 4]);
    l.content = Content::Null;
    l.three_d = true;
    l.tracking_point = Some(PointBinding {
        asset: 2,
        track: "feature".into(),
        anchor_local_frame: 0,
        source_anchor_pts: -4000,
        speed_numerator: 1,
        speed_denominator: 1,
        reference_pixels: [20., 30.],
        crop: [0., 0., 320., 240.],
        rotation: Rotation::None,
        interpolation: Interpolation::Linear,
        max_gap_ticks: 4000,
        minimum_confidence: 0.8,
        missing: Missing::Error,
    });
    p.layers.push(l);
    p.validate().unwrap();
    p
}
fn sampled(p: &Project, frame: f64) -> [f32; 3] {
    p.layers[0].sampled_position(p, frame).unwrap()
}

#[test]
fn rational_project_clock_keeps_point_interpolation_on_original_source_pts() {
    for (numerator, denominator) in [(30_000, 1001), (60_000, 1001), (2997, 100), (24, 1)] {
        let mut project = fixture();
        project.fps = FrameRate::new(numerator, denominator).unwrap();
        project.validate().unwrap();
        // 1/48 second advances 1000 original source ticks, halfway from
        // [-4000, (20,30)] to [-2000, (30,50)], with a 2x plate scale.
        let frame = f64::from(numerator) / (48. * f64::from(denominator));
        let actual = sampled(&project, frame);
        for (value, expected) in actual.into_iter().zip([110., 100., 9.]) {
            assert!((value - expected).abs() < 0.00001, "{numerator}/{denominator}: {actual:?}");
        }
        assert!(project.layers[0].transform.position.keys.is_empty());
    }
}

#[test]
fn original_vfr_pts_drive_fractional_frames_and_manual_corrections_without_baking() {
    let mut p = fixture();
    assert_eq!(sampled(&p, 0.), [100., 80., 9.]);
    assert_eq!(sampled(&p, 0.5), [110., 100., 9.]);
    assert_eq!(sampled(&p, 1.), [120., 120., 9.]);
    assert_eq!(sampled(&p, 2.), [160., 160., 9.]);
    assert_eq!(sampled(&p, 3.), [200., 200., 9.]);
    assert!(p.layers[0].transform.position.keys.is_empty());
    p.layers[0]
        .transform
        .position
        .upsert(0, [100., 80., 9.], Ease::Linear)
        .unwrap();
    p.layers[0]
        .transform
        .position
        .upsert(4, [108., 84., 13.], Ease::Linear)
        .unwrap();
    assert_eq!(sampled(&p, 2.), [164., 162., 11.]);
    assert!(p.tracking_assets[0].data.tracks[0].samples[2].manual_correction);
}
#[test]
fn local_clip_offset_reverse_speed_and_explicit_freeze_use_the_same_source_clock() {
    let mut p = fixture();
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 110,
        offset_frame: 10,
    });
    assert_eq!(sampled(&p, 12.), [160., 160., 9.]);
    let binding = p.layers[0].tracking_point.as_mut().unwrap();
    binding.speed_numerator = -1;
    binding.source_anchor_pts = 4000;
    assert_eq!(sampled(&p, 12.), [160., 160., 9.]);
    p.layers[0].tracking_point.as_mut().unwrap().speed_numerator = 0;
    assert_eq!(sampled(&p, 1000.), [240., 240., 9.]);
}
#[test]
fn explicit_crop_rotation_and_resize_define_local_pixel_displacement() {
    let mut p = fixture();
    let b = p.layers[0].tracking_point.as_mut().unwrap();
    b.crop = [10., 20., 160., 120.];
    b.rotation = Rotation::Clockwise90;
    let v = sampled(&p, 1.);
    assert!((v[0] - (100. - 20. * 640. / 120.)).abs() < 1e-4);
    assert!((v[1] - (80. + 10. * 480. / 160.)).abs() < 1e-4);
    assert_eq!(v[2], 9.);
}
#[test]
fn missing_occluded_lost_low_confidence_and_long_gaps_never_get_linear_repaired() {
    for case in 0..5 {
        let mut p = fixture();
        let mut asset = (*p.tracking_assets[0].data).clone();
        match case {
            0 => {
                asset.tracks[0].samples.remove(1);
            }
            1 => {
                asset.tracks[0].samples[1].visibility = Visibility::Occluded;
            }
            2 => {
                asset.tracks[0].samples[1].visibility = Visibility::Lost;
            }
            3 => {
                asset.tracks[0].samples[1].confidence = 0.1;
            }
            _ => {
                p.layers[0].tracking_point.as_mut().unwrap().max_gap_ticks = 1999;
            }
        }
        p.tracking_assets[0].data = SharedData::new(asset).unwrap();
        let e = p.layers[0]
            .sampled_position(&p, 0.5)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("object=7 asset=2 track=feature frame=0.5"),
            "{e}"
        );
        p.layers[0].tracking_point.as_mut().unwrap().missing = Missing::AuthoredOnly;
        assert_eq!(sampled(&p, 0.5), [100., 80., 9.]);
        // Recovery is immediate; an unusable observation does not poison the next exact sample.
        assert_eq!(sampled(&p, 2.), [160., 160., 9.]);
    }
}
#[test]
fn source_range_and_hold_are_explicit_and_do_not_extrapolate() {
    let mut p = fixture();
    assert!(p.layers[0].sampled_position(&p, -0.5).is_err());
    assert!(p.layers[0].sampled_position(&p, 4.5).is_err());
    p.layers[0].tracking_point.as_mut().unwrap().interpolation = Interpolation::Hold;
    assert_eq!(sampled(&p, 1.5), [120., 120., 9.]);
    p.layers[0].tracking_point.as_mut().unwrap().max_gap_ticks = 0;
    assert!(p.layers[0].sampled_position(&p, 1.5).is_err());
    assert_eq!(sampled(&p, 1.), [120., 120., 9.]);
}
#[test]
fn child_hierarchy_inherits_tracking_once_and_frozen_clones_share_immutable_observations() {
    let mut p = fixture();
    let mut child = Layer::solid(8, "child", [10.; 2], [350., 250., 0.], [1.; 4]);
    child.three_d = true;
    child.parent = Some(ParentLink {
        object: Some(7),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    p.layers.push(child);
    p.validate().unwrap();
    let a = hierarchy::world_matrix_with_cameras(&p, 8, 0., &AuthoredCameraSampler).unwrap();
    let b = hierarchy::world_matrix_with_cameras(&p, 8, 2., &AuthoredCameraSampler).unwrap();
    assert!((b.w_axis.x - a.w_axis.x - 60.).abs() < 1e-5);
    assert!((b.w_axis.y - a.w_axis.y + 80.).abs() < 1e-5);
    let clone = p.clone();
    assert!(p.tracking_assets[0]
        .data
        .shares_storage(&clone.tracking_assets[0].data));
    assert_eq!(
        serde_json::from_slice::<Project>(&serde_json::to_vec(&p).unwrap()).unwrap(),
        p
    );
}
#[test]
fn validation_rejects_dangling_binding_wrong_track_kind_and_mismatched_runtime_source() {
    for case in 0..7 {
        let mut p = fixture();
        match case {
            0 => p.layers[0].tracking_point.as_mut().unwrap().asset = 999,
            1 => p.layers[0].tracking_point.as_mut().unwrap().track = "absent".into(),
            2 => p.layers[0].tracking_point.as_mut().unwrap().crop[2] = 1000.,
            3 => {
                p.layers[0]
                    .tracking_point
                    .as_mut()
                    .unwrap()
                    .speed_denominator = 0
            }
            4 => p.tracking_assets[0].source_sha256 = "08".repeat(32),
            5 => p.video_assets[0].width = 321,
            _ => {
                let mut data = (*p.tracking_assets[0].data).clone();
                data.tracks[0].kind = Kind::Plane;
                data.tracks[0].reference_index = Some(0);
                for sample in &mut data.tracks[0].samples {
                    sample.observation = Some(Observation::Plane([
                        [1., 0., 0.],
                        [0., 1., 0.],
                        [0., 0., 1.],
                    ]));
                }
                p.tracking_assets[0].data = SharedData::new(data).unwrap();
            }
        }
        assert!(p.validate().is_err(), "invalid case {case} accepted");
    }
}
#[test]
fn fractional_frame_clock_matches_exact_halves_and_refuses_nonrepresentable_extremes() {
    let rate = Rate::new(24, 1).unwrap();
    assert_eq!(
        Time::fractional_frame(0.5, rate).unwrap(),
        Time::ticks(1, 48).unwrap()
    );
    assert_eq!(
        Time::fractional_frame(-1.5, rate).unwrap(),
        Time::ticks(-1, 16).unwrap()
    );
    assert!(Time::fractional_frame(f64::NAN, rate).is_err());
    assert!(Time::fractional_frame(f64::MAX, rate).is_err());
    assert!(Time::fractional_frame(f64::from_bits(1), rate).is_err());
}
