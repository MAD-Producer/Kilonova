use motion_model::{
    media_assets::{AssetKind, AssetRef},
    tracking::{
        asset::*,
        plate::{Extent, Rotation},
        point::*,
        shared::SharedData,
    },
    *,
};

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
pub fn plane_fixture() -> Project {
    let mut p = fixture();
    let mut data = (*p.tracking_assets[0].data).clone();
    data.tracks[0].kind = Kind::Plane;
    data.tracks[0].reference_index = Some(0);
    for (i, s) in data.tracks[0].samples.iter_mut().enumerate() {
        s.observation = Some(Observation::Plane([
            [1., 0., i as f64 * 10.],
            [0., 1., i as f64 * 5.],
            [0., 0., 1.],
        ]));
    }
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    let definition = motion_effects::builtin::manifest()
        .effects
        .into_iter()
        .find(|e| e.id == "corner_pin")
        .unwrap();
    let effect = EffectInstance::new(
        99,
        motion_effects::builtin::PLUGIN_ID,
        "2.0.0",
        &"0".repeat(64),
        &definition,
        [320., 240.],
    );
    p.plugin_dependencies = vec![effect.dependency()];
    let l = &mut p.layers[0];
    l.content = Content::Solid { color: [1.; 4] };
    l.three_d = false;
    l.size = [320., 240.];
    l.tracking_point = None;
    l.effects = vec![effect];
    l.tracking_plane = Some(tracking::plane::PlaneBinding {
        asset: 2,
        track: "feature".into(),
        effect: 99,
        anchor_local_frame: 0,
        source_anchor_pts: -4000,
        speed_numerator: 1,
        speed_denominator: 1,
        reference_quad: [[0., 0.], [320., 0.], [320., 240.], [0., 240.]],
        crop: [0., 0., 320., 240.],
        rotation: Rotation::None,
        interpolation: Interpolation::Linear,
        max_gap_ticks: 4000,
        minimum_confidence: 0.8,
        missing: Missing::Error,
    });
    p.validate().unwrap();
    p
}
