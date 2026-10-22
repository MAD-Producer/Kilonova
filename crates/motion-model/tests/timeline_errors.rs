use motion_model::{
    AudioAsset, AudioClip, AxisTracks, Content, Ease, Error, Layer, Project, Track, VideoAsset,
    VideoClip,
};
use std::error::Error as _;

fn axes() -> Box<AxisTracks> {
    Box::new(AxisTracks {
        x: Track::constant(1.),
        y: Track::constant(2.),
        z: Track::constant(3.),
    })
}

#[test]
fn missing_video_asset_does_not_panic() {
    let mut project = Project::new(100, 100, 30, 60).unwrap();
    let mut layer = Layer::solid(91, "test", [10.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(82),
    };
    project.layers.push(layer);
    let before = project.clone();
    let error = project.timeline_layers(7.).unwrap_err();
    let Error::Timeline { object, source } = &error else {
        panic!("{error}")
    };
    assert_eq!(*object, 91);
    assert!(matches!(source.as_ref(), Error::Invalid(_)));
    assert!(error.source().is_some());
    assert!(error.to_string().contains("object=91 asset=82 frame=7"));
    assert_eq!(project, before);
}

#[test]
fn scalar_dimensions_do_not_panic() {
    let mut track = Track::constant(1.);
    track.axes = Some(axes());
    let before = track.clone();
    let error = track.timeline(0).unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(error.to_string().contains("type=f32"));
    assert_eq!(track, before);
}

#[test]
fn nested_scalar_dimensions_do_not_panic() {
    let mut track = Track::constant([1., 2., 3.]);
    track.axes = Some(axes());
    track.axes.as_mut().unwrap().y.axes = Some(axes());
    let before = track.clone();
    assert!(matches!(track.timeline(0), Err(Error::Invalid(_))));
    assert_eq!(track, before);
}

#[test]
fn malformed_layer_properties_preserve_object_and_source_identity() {
    for property in ["opacity", "anchor", "position"] {
        let mut project = Project::new(100, 100, 30, 60).unwrap();
        let mut layer = Layer::solid(91, "not-in-error-text", [10.; 2], [0.; 3], [1.; 4]);
        match property {
            "opacity" => layer.transform.opacity.axes = Some(axes()),
            "anchor" => layer.transform.anchor.axes = Some(axes()),
            "position" => {
                layer.transform.position.axes = Some(axes());
                layer.transform.position.axes.as_mut().unwrap().z.axes = Some(axes());
            }
            _ => unreachable!(),
        }
        project.layers.push(layer);
        let before = project.clone();
        let error = project.timeline_layers(7.).unwrap_err();
        let Error::Timeline { object, source } = &error else {
            panic!("{error}")
        };
        assert_eq!(*object, 91);
        assert!(matches!(source.as_ref(), Error::Invalid(_)));
        assert!(!error.to_string().contains("not-in-error-text"));
        assert_eq!(project, before);
    }
}

#[test]
fn healthy_separated_timeline_keeps_values_key_metadata_and_signed_offsets() {
    let mut track = Track::constant([1., 2., 3.]);
    track.upsert(-4, [4., 5., 6.], Ease::In).unwrap();
    track.upsert(9, [7., 8., 9.], Ease::Hold).unwrap();
    let coupled = serde_json::to_value(track.timeline(-20).unwrap()).unwrap();
    assert_eq!(coupled["value"], serde_json::json!([1., 2., 3.]));
    assert_eq!(
        coupled["keys"][0],
        serde_json::json!({"frame":-24,"local_frame":-4,"value":[4.,5.,6.],"ease":"in"})
    );
    assert_eq!(
        coupled["keys"][1],
        serde_json::json!({"frame":-11,"local_frame":9,"value":[7.,8.,9.],"ease":"hold"})
    );
    assert_eq!(coupled["separated"], false);
    assert!(coupled.get("axes").is_none());
    track.separate().unwrap();
    let before = track.clone();
    let separated = serde_json::to_value(track.timeline(-20).unwrap()).unwrap();
    assert_eq!(separated["value"], coupled["value"]);
    assert_eq!(separated["separated"], true);
    assert_eq!(separated["keys"], serde_json::json!([]));
    for (axis, index) in [("x", 0), ("y", 1), ("z", 2)] {
        assert_eq!(separated["axes"][axis]["value"], coupled["value"][index]);
        for key in 0..2 {
            let mut expected = coupled["keys"][key].clone();
            expected["value"] = expected["value"][index].clone();
            assert_eq!(separated["axes"][axis]["keys"][key], expected);
        }
    }
    assert_eq!(track, before);
}

#[test]
fn invalid_time_is_rejected_even_for_an_empty_project() {
    let project = Project::new(100, 100, 30, 60).unwrap();
    let before = project.clone();
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = project.timeline_layers(frame).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(error.to_string().contains("requires finite time"));
        assert_eq!(project, before);
    }
    assert!(project.timeline_layers(-10.).unwrap().is_empty());
    assert!(project.timeline_layers(100.).unwrap().is_empty());
}

#[test]
fn mixed_vector_keys_and_nested_scalar_axes_are_rejected_by_static_projection() {
    let mut track = Track::constant([1., 2., 3.]);
    track.upsert(7, [4., 5., 6.], Ease::Linear).unwrap();
    track.axes = Some(axes());
    let before = track.clone();
    assert!(track
        .static_value()
        .unwrap_err()
        .to_string()
        .contains("vector keys"));
    // PartialEq intentionally compares only authoritative axes; also check keys.
    assert_eq!(track.keys, before.keys);
    assert_eq!(track.value, before.value);
    assert_eq!(track.axes, before.axes);
    track.keys.clear();
    track.axes.as_mut().unwrap().y.axes = Some(axes());
    assert!(track
        .static_value()
        .unwrap_err()
        .to_string()
        .contains("scalar axis tracks"));
}

#[test]
fn missing_audio_asset_does_not_become_a_zero_duration_success() {
    let mut project = Project::new(100, 100, 30, 60).unwrap();
    let mut layer = Layer::solid(91, "test", [10.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip {
            asset: 83,
            source_offset_us: 0,
            volume: 1.,
            muted: false,
        },
    };
    project.layers.push(layer);
    let before = project.clone();
    let error = project.timeline_layers(7.).unwrap_err();
    assert!(
        matches!(&error, Error::Timeline { object: 91, source } if matches!(source.as_ref(), Error::Invalid(_)))
    );
    assert!(error
        .to_string()
        .contains("audio asset missing: object=91 asset=83 frame=7"));
    assert_eq!(project, before);
}

#[test]
fn silent_video_is_normal_and_missing_linked_audio_recovers_after_registration() {
    let mut project = Project::new(100, 100, 30, 60).unwrap();
    let mut layer = Layer::solid(91, "test", [32., 24.], [0.; 3], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(82),
    };
    project.layers.push(layer);
    project.video_assets.push(VideoAsset {
        id: 82,
        path: "assets/video.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 1,
        width: 32,
        height: 24,
        rotation: 0,
        display_width: 32,
        display_height: 24,
        video_start_us: 0,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 30,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    });
    let silent = project.timeline_layers(7.).unwrap();
    assert!(silent[0].audio.is_none());
    assert!(!silent[0].video.as_ref().unwrap().has_audio);
    assert_eq!(silent[0].video.as_ref().unwrap().source_time_us, 233_333);
    project.video_assets[0].audio_asset = Some(83);
    let before = project.clone();
    assert!(project
        .timeline_layers(7.)
        .unwrap_err()
        .to_string()
        .contains("audio asset missing"));
    assert_eq!(project, before);
    project.audio_assets.push(AudioAsset {
        id: 83,
        path: "assets/video.mp4".into(),
        mime: "audio/mp4".into(),
        bytes: 1,
        track: 2,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 48_000,
        duration_us: 1_000_000,
    });
    let healthy = project.timeline_layers(7.).unwrap();
    assert_eq!(
        healthy[0].audio.as_ref().unwrap().source_duration_us,
        1_000_000
    );
    assert!(healthy[0].video.as_ref().unwrap().has_audio);
}
