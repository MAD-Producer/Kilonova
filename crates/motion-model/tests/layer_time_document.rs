use motion_model::layer_time::{self, LayerTimeAction, TimeRemap, TimeRemapKey, TimeStretch};
use motion_model::{Content, Easing, Layer, LayerTimeline, Project, VideoClip};
use serde_json::json;

fn layer() -> Layer {
    let mut layer = Layer::solid(7, "timed", [20.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 70,
        offset_frame: 10,
    });
    layer.transform.position.set_animated(0, true).unwrap();
    layer
        .transform
        .position
        .set_at(60, [60., 120., 0.])
        .unwrap();
    layer
}
fn stretch(n: u32, d: u32, anchor: f64, resize: bool) -> LayerTimeAction {
    LayerTimeAction::Stretch {
        stretch: TimeStretch::new(n, d).unwrap(),
        anchor_frame: anchor,
        resize_clip: resize,
    }
}

#[test]
fn stretching_keeps_local_key_data_and_anchor_but_moves_composition_projection() {
    let mut layer = layer();
    let tracks = layer.transform.clone();
    layer_time::apply(&mut layer, 120, stretch(3, 2, 10., true)).unwrap();
    assert_eq!(layer.transform, tracks);
    assert_eq!(layer.interval(120).in_frame, 10.);
    assert_eq!(layer.interval(120).out_frame, 100.);
    assert_eq!(layer.local_frame(10.), 0.);
    assert_eq!(layer.checked_local_frame(55.).unwrap(), 30.);
    assert_eq!(layer.property_composition_frame(30.).unwrap(), 55.);
    assert_eq!(layer.edit_frame(55).unwrap(), 30);
    let error = layer.edit_frame(11).unwrap_err();
    assert!(error.to_string().contains("object=7"));
    assert!(error.to_string().contains("continuous key addressing"));
    assert_eq!(
        layer.transform, tracks,
        "unrepresentable integer edits must not round a key"
    );
}

#[test]
fn repeated_absolute_stretch_and_identity_stretch_do_not_reauthor_the_clock_or_clip() {
    let mut layer = layer();
    let original = layer.clone();
    layer_time::apply(&mut layer, 120, stretch(1, 1, 53., true)).unwrap();
    assert_eq!(layer, original);
    layer_time::apply(&mut layer, 120, stretch(2, 1, 10., true)).unwrap();
    let stretched = layer.clone();
    layer_time::apply(&mut layer, 120, stretch(2, 1, 53., true)).unwrap();
    assert_eq!(layer, stretched);
}

#[test]
fn two_reversals_restore_the_full_identity_mapping_without_touching_authored_tracks() {
    let mut layer = layer();
    let original = layer.clone();
    layer_time::apply(&mut layer, 120, LayerTimeAction::Reverse {}).unwrap();
    assert_eq!(layer.interval(120), original.interval(120));
    assert_eq!(layer.transform, original.transform);
    assert_eq!(layer.local_frame(10.), 60.);
    assert_eq!(layer.local_frame(40.), 30.);
    assert_eq!(layer.local_frame(70.), 0.);
    assert_eq!(layer.source_time_sample(25.).unwrap().rate, -1.);
    layer_time::apply(&mut layer, 120, LayerTimeAction::Reverse {}).unwrap();
    assert_eq!(layer, original);
    assert!(layer.time_mapping.is_none());
}

#[test]
fn source_remap_can_hold_a_source_while_the_property_clock_keeps_its_inverse() {
    let mut layer = layer();
    layer.content = Content::Video {
        video: VideoClip::new(3),
    };
    layer_time::apply(&mut layer, 120, stretch(2, 1, 10., false)).unwrap();
    let before = layer.clone();
    let remap = TimeRemap::new(vec![
        TimeRemapKey {
            frame: 0.,
            source_frame: 12.5,
            easing: Easing::default(),
        },
        TimeRemapKey {
            frame: 60.,
            source_frame: 12.5,
            easing: Easing::default(),
        },
    ])
    .unwrap();
    layer_time::apply(
        &mut layer,
        120,
        LayerTimeAction::Remap { remap: Some(remap) },
    )
    .unwrap();
    assert_eq!(layer.local_frame(40.), before.local_frame(40.));
    assert_eq!(layer.property_composition_frame(15.).unwrap(), 40.);
    let source = layer.source_time_sample(40.).unwrap();
    assert_eq!(source.frame, 12.5);
    assert_eq!(source.rate, 0.);
    assert_eq!(layer.transform, before.transform);
    layer_time::apply(&mut layer, 120, LayerTimeAction::Remap { remap: None }).unwrap();
    assert_eq!(layer, before);
}

#[test]
fn failed_resizes_invalid_anchors_and_wrong_content_leave_the_entire_layer_unchanged() {
    let remap = TimeRemap::new(vec![TimeRemapKey {
        frame: 0.,
        source_frame: 0.,
        easing: Easing::default(),
    }])
    .unwrap();
    for action in [
        stretch(1000, 1, 0., true),
        stretch(2, 1, f64::NAN, false),
        LayerTimeAction::Remap { remap: Some(remap) },
    ] {
        let mut layer = layer();
        let before = layer.clone();
        let error = layer_time::apply(&mut layer, 120, action).unwrap_err();
        assert!(matches!(
            error,
            motion_model::Error::LayerTimeObject {
                object: 7,
                phase: "edit",
                ..
            }
        ));
        assert_eq!(layer, before);
    }
    let mut layer = layer();
    layer.locked = true;
    let before = layer.clone();
    let error = layer_time::apply(&mut layer, 120, LayerTimeAction::Reverse {}).unwrap_err();
    assert!(
        matches!(error, motion_model::Error::LayerTimeObject { source, .. } if matches!(*source, motion_model::Error::Locked(7)))
    );
    assert_eq!(layer, before);
}

#[test]
fn reset_preserves_explicit_trim_and_restores_the_unmapped_property_clock() {
    let mut layer = layer();
    let tracks = layer.transform.clone();
    layer_time::apply(&mut layer, 120, stretch(3, 2, 10., true)).unwrap();
    let trim = layer.interval(120);
    layer_time::apply(&mut layer, 120, LayerTimeAction::Reset {}).unwrap();
    assert_eq!(layer.interval(120), trim);
    assert_eq!(layer.transform, tracks);
    assert!(layer.time_mapping.is_none());
    assert_eq!(layer.checked_local_frame(55.).unwrap(), 45.);
}

#[test]
fn current_document_roundtrip_preserves_the_mapping_and_keeps_the_version_required() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(layer());
    let unmapped = serde_json::to_value(&project).unwrap();
    assert!(unmapped["layers"][0].get("time_mapping").is_none());
    layer_time::apply(&mut project.layers[0], 120, stretch(3, 2, 10., true)).unwrap();
    project.validate().unwrap();
    let encoded = serde_json::to_value(&project).unwrap();
    assert_eq!(encoded["version"], motion_model::CURRENT_PROJECT_VERSION);
    assert!(encoded["layers"][0]["time_mapping"].get("clock").is_some());
    let restored: Project = serde_json::from_value(encoded.clone()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, project);
    let mut missing = encoded.clone();
    missing.as_object_mut().unwrap().remove("version");
    assert!(serde_json::from_value::<Project>(missing).is_err());
    let mut stale = encoded;
    stale["version"] = json!(motion_model::CURRENT_PROJECT_VERSION - 1);
    let stale: Project = serde_json::from_value(stale).unwrap();
    assert!(stale.validate().is_err());
}

#[test]
fn command_fields_are_explicit_and_clock_payloads_cannot_invent_or_hide_fields() {
    for value in [
        json!({"kind":"stretch","anchor_frame":0,"resize_clip":true}),
        json!({"kind":"stretch","stretch":{"numerator":2,"denominator":1},"anchor_frame":0}),
        json!({"kind":"remap"}),
        json!({"kind":"reverse","unexpected":true}),
    ] {
        assert!(serde_json::from_value::<LayerTimeAction>(value).is_err());
    }
    assert!(
        serde_json::from_value::<LayerTimeAction>(json!({"kind":"remap","remap":null})).is_ok()
    );
    let mut layer = layer();
    layer_time::apply(&mut layer, 120, stretch(2, 1, 10., false)).unwrap();
    let mut value = serde_json::to_value(&layer).unwrap();
    value["time_mapping"]
        .as_object_mut()
        .unwrap()
        .remove("clock");
    assert!(serde_json::from_value::<Layer>(value).is_err());
}
