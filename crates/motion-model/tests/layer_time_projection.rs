use motion_model::{
    layer_time::{self, LayerTimeAction, TimeStretch},
    Ease, Layer, LayerTimeline, Project,
};
use serde_json::{json, Value};

fn project() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "keys", [20.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 70,
        offset_frame: 10,
    });
    for (frame, value, ease) in [(0, 0., Ease::In), (1, 1., Ease::Hold), (40, 40., Ease::Out)] {
        layer
            .transform
            .position
            .upsert(frame, [value, 2. * value, 0.], ease)
            .unwrap();
    }
    project.layers.push(layer);
    project
}
fn stretch(project: &mut Project) {
    layer_time::apply(
        &mut project.layers[0],
        120,
        LayerTimeAction::Stretch {
            stretch: TimeStretch::new(3, 2).unwrap(),
            anchor_frame: 10.,
            resize_clip: true,
        },
    )
    .unwrap();
}
fn view(project: &Project) -> Value {
    serde_json::to_value(project.timeline_layers(20.).unwrap().remove(0)).unwrap()
}

#[test]
fn timeline_keys_preserve_fractional_composition_times_and_full_local_metadata() {
    let mut project = project();
    let original = view(&project);
    stretch(&mut project);
    let before = project.clone();
    let current = view(&project);
    for (index, time) in [10., 11.5, 70.].into_iter().enumerate() {
        let key = &current["properties"]["position"]["keys"][index];
        assert_eq!(key["frame"].as_f64().unwrap(), time);
        let mut expected = original["properties"]["position"]["keys"][index].clone();
        expected["frame"] = key["frame"].clone();
        assert_eq!(*key, expected);
        let local = project.layers[0].checked_local_frame(time).unwrap();
        assert_eq!(local, key["local_frame"].as_f64().unwrap());
    }
    assert_eq!(
        current["properties"]["position"]["keys"][1]["frame"],
        json!(11.5)
    );
    assert_eq!(
        project, before,
        "snapshot queries cannot reauthor the project"
    );
}

#[test]
fn reverse_projection_keeps_local_interval_ownership_while_composition_order_reverses() {
    let mut project = project();
    stretch(&mut project);
    let tracks = project.layers[0].transform.clone();
    layer_time::apply(&mut project.layers[0], 120, LayerTimeAction::Reverse {}).unwrap();
    let current = view(&project);
    for (index, time) in [100., 98.5, 40.].into_iter().enumerate() {
        let key = &current["properties"]["position"]["keys"][index];
        assert_eq!(key["frame"].as_f64().unwrap(), time);
        assert_eq!(
            project.layers[0].checked_local_frame(time).unwrap(),
            key["local_frame"].as_f64().unwrap()
        );
    }
    assert_eq!(current["properties"]["position"]["keys"][0]["ease"], "in");
    assert_eq!(current["properties"]["position"]["keys"][1]["ease"], "hold");
    assert_eq!(project.layers[0].transform, tracks);
}

#[test]
fn separated_axes_use_the_same_inverse_clock_and_keep_axis_key_metadata() {
    let mut project = project();
    project.layers[0].transform.position.separate().unwrap();
    let original = view(&project);
    stretch(&mut project);
    layer_time::apply(&mut project.layers[0], 120, LayerTimeAction::Reverse {}).unwrap();
    let before = project.clone();
    let current = view(&project);
    for axis in ["x", "y", "z"] {
        for (index, time) in [100., 98.5, 40.].into_iter().enumerate() {
            let key = &current["properties"]["position"]["axes"][axis]["keys"][index];
            assert_eq!(key["frame"].as_f64().unwrap(), time);
            let mut expected =
                original["properties"]["position"]["axes"][axis]["keys"][index].clone();
            expected["frame"] = key["frame"].clone();
            assert_eq!(*key, expected);
        }
    }
    assert_eq!(project, before);
}

#[test]
fn mask_values_and_six_component_path_nodes_project_to_the_same_fractional_instant() {
    use motion_model::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut project = project();
    let mut geometry = Track::constant([0.; 6]);
    geometry
        .upsert(1, [1., 2., 0., 0., 0., 0.], Ease::InOut)
        .unwrap();
    let mut mask = LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: false,
            tangents: Default::default(),
            nodes: vec![
                PathNode { id: 1, geometry },
                PathNode {
                    id: 2,
                    geometry: Track::constant([10., 0., 0., 0., 0., 0.]),
                },
            ],
        },
    );
    mask.opacity.upsert(1, 50., Ease::In).unwrap();
    mask.feather.upsert(1, [3., 4.], Ease::Out).unwrap();
    mask.expansion.upsert(1, 5., Ease::Linear).unwrap();
    project.layers[0].masks.push(mask);
    project.validate().unwrap();
    stretch(&mut project);
    let before = project.clone();
    let current = view(&project);
    for track in [
        &current["masks"][0]["opacity"],
        &current["masks"][0]["feather"],
        &current["masks"][0]["expansion"],
        &current["masks"][0]["nodes"][0]["geometry"],
    ] {
        assert_eq!(track["keys"][0]["frame"], json!(11.5));
        assert_eq!(track["keys"][0]["local_frame"], 1);
    }
    assert_eq!(
        current["masks"][0]["nodes"][0]["geometry"]["keys"][0]["value"],
        json!([1., 2., 0., 0., 0., 0.])
    );
    assert_eq!(project, before);
}

#[test]
fn camera_property_tracks_share_the_layer_clock_instead_of_the_global_camera_clock() {
    let mut project = project();
    let mut layer = Layer::camera(7, "timed camera", 64, 64);
    layer.timeline = project.layers[0].timeline;
    let motion_model::Content::Camera { camera } = &mut layer.content else {
        unreachable!()
    };
    camera
        .position
        .upsert(1, [32., 32., -100.], Ease::In)
        .unwrap();
    camera.target.upsert(1, [32., 32., 0.], Ease::Out).unwrap();
    for track in [
        &mut camera.roll,
        &mut camera.fov,
        &mut camera.radius,
        &mut camera.azimuth,
        &mut camera.elevation,
    ] {
        track.upsert(1, 40., Ease::Linear).unwrap();
    }
    project.layers[0] = layer;
    project.validate().unwrap();
    stretch(&mut project);
    let before = project.clone();
    let current = view(&project);
    for property in [
        "position",
        "target",
        "roll",
        "fov",
        "radius",
        "azimuth",
        "elevation",
    ] {
        assert_eq!(
            current["properties"][property]["keys"][0]["frame"],
            json!(11.5),
            "{property}"
        );
    }
    assert_eq!(project, before);
}

#[test]
fn continuous_spatial_keys_keep_ids_local_times_and_interval_policy_under_reverse_stretch() {
    let mut project = project();
    let track = &mut project.layers[0].transform.position;
    track.keys.clear();
    for (time, x) in [(0, 0.), (4, 1.), (10, 7.), (15, 9.), (20, 15.)] {
        track.upsert(time, [x, 0., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4]).unwrap();
    let original = view(&project);
    stretch(&mut project);
    layer_time::apply(&mut project.layers[0], 120, LayerTimeAction::Reverse {}).unwrap();
    let before = project.clone();
    let current = view(&project);
    let keys = current["properties"]["position"]["spatial_keys"]
        .as_array()
        .unwrap();
    assert_eq!(keys.len(), 5);
    for (index, key) in keys.iter().enumerate() {
        let local = key["local_frame"].as_f64().unwrap();
        let expected_time = 100. - 1.5 * local;
        let frame = key["frame"].as_f64().unwrap();
        assert!((frame - expected_time).abs() < 1e-12);
        let mut expected = original["properties"]["position"]["spatial_keys"][index].clone();
        expected["frame"] = key["frame"].clone();
        assert_eq!(*key, expected);
    }
    assert!(keys[1]["frame"].as_f64().unwrap().fract() != 0.);
    assert_eq!(project, before);
}

#[test]
fn retained_offscreen_keys_are_not_clamped_to_the_sampling_request_budget() {
    let mut project = project();
    let track = &mut project.layers[0].transform.position;
    track.keys.clear();
    track.upsert(0, [0.; 3], Ease::Linear).unwrap();
    track.upsert(3_000_000, [1.; 3], Ease::Linear).unwrap();
    layer_time::apply(
        &mut project.layers[0],
        120,
        LayerTimeAction::Stretch {
            stretch: TimeStretch::new(1000, 1).unwrap(),
            anchor_frame: 10.,
            resize_clip: false,
        },
    )
    .unwrap();
    project.validate().unwrap();
    let before = project.clone();
    let current = view(&project);
    assert_eq!(
        current["properties"]["position"]["keys"][1]["frame"],
        json!(3_000_000_010i64)
    );
    assert_eq!(
        current["properties"]["position"]["keys"][1]["local_frame"],
        3_000_000
    );
    // This is a transfer coordinate, not permission to sample a huge time.
    assert!(project.layers[0]
        .time_mapping
        .as_ref()
        .unwrap()
        .clock
        .composition_frame(3_000_000., 10)
        .is_err());
    assert_eq!(project, before);
}

#[test]
fn source_remapping_never_moves_property_keys_and_identity_keeps_full_signed_storage() {
    use motion_model::{
        layer_time::{LayerTimeMapping, TimeRemap, TimeRemapKey},
        Track, VideoClip,
    };
    let mut layer = project().layers.remove(0);
    layer.content = motion_model::Content::Video {
        video: VideoClip::new(3),
    };
    let before =
        serde_json::to_value(layer.transform.position.timeline_for_layer(&layer).unwrap()).unwrap();
    let remap = TimeRemap::new(vec![
        TimeRemapKey {
            frame: 0.,
            source_frame: 40.,
            easing: Default::default(),
        },
        TimeRemapKey {
            frame: 40.,
            source_frame: 0.,
            easing: Default::default(),
        },
    ])
    .unwrap();
    layer_time::apply(
        &mut layer,
        120,
        LayerTimeAction::Remap { remap: Some(remap) },
    )
    .unwrap();
    let after =
        serde_json::to_value(layer.transform.position.timeline_for_layer(&layer).unwrap()).unwrap();
    assert_eq!(after, before);
    let mut edge = Track::constant(0.);
    edge.upsert(i32::MIN, 1., Ease::Linear).unwrap();
    let projected = edge.timeline_for_layer(&layer).unwrap();
    assert_eq!(projected.keys[0].frame, f64::from(i32::MIN) + 10.);
    assert!(matches!(
        layer.time_mapping,
        Some(LayerTimeMapping { remap: Some(_), .. })
    ));
}

#[test]
fn non_finite_projection_is_rejected_and_serialization_cannot_turn_it_into_null() {
    let mut project = project();
    stretch(&mut project);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = project.layers[0]
            .property_composition_frame(bad)
            .unwrap_err();
        assert!(matches!(
            error,
            motion_model::Error::LayerTimeObject {
                object: 7,
                phase: "key_projection",
                ..
            }
        ));
        let key = motion_model::TimelineKey {
            frame: bad,
            local_frame: 0,
            value: 1.,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        };
        let error = serde_json::to_value(key).unwrap_err();
        assert!(error.to_string().contains("non-finite projected key time"));
    }
}
