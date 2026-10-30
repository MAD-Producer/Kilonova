use motion_model::{
    Ease, Easing, Layer, LayerTimeline, Project, SpatialAnimation, SpatialTangents, Track,
};

fn ordinary() -> Track<[f32; 3]> {
    let mut track = Track::constant([0.; 3]);
    for (time, x) in [(0, 0.), (4, 1.), (10, 7.), (15, 9.), (20, 15.)] {
        track.upsert(time, [x, 0., 0.], Ease::Linear).unwrap();
    }
    track
}

#[test]
fn conversion_preserves_manual_intervals_and_roving_only_changes_its_incident_span() {
    let mut original = ordinary();
    original.set_ease(10, Ease::InOut).unwrap();
    original.set_ease(15, Ease::Hold).unwrap();
    original
        .set_spatial(
            10,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([1., 5., 0.]),
            }),
        )
        .unwrap();
    let imported = SpatialAnimation::from_track(&original).unwrap();
    for tick in -100..=2100 {
        let time = f64::from(tick) / 100.;
        assert_eq!(
            imported.sample_value(time).unwrap(),
            original.sample(time).unwrap(),
            "{time}"
        );
    }
    let mut roving = original.clone();
    roving.enable_roving(&[4]).unwrap();
    assert!(roving.keys.is_empty() && roving.axes.is_none());
    assert!(roving.is_animated());
    assert_eq!(
        roving.key_frames().collect::<Vec<_>>(),
        vec![0., 10. / 7., 10., 15., 20.]
    );
    for tick in 1000..=2100 {
        let time = f64::from(tick) / 100.;
        assert_eq!(
            roving.sample(time).unwrap(),
            original.sample(time).unwrap(),
            "unselected interval {time}"
        );
    }
    assert!((roving.sample(5.).unwrap()[0] - 3.5).abs() < 1e-5);
    assert_ne!(
        roving.sample(5.).unwrap(),
        original.sample(5.).unwrap(),
        "integer-timing negative control"
    );
    let frozen = roving.clone();
    roving
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_roving(&[2], false)
        .unwrap();
    let reopened: Track<[f32; 3]> =
        serde_json::from_slice(&serde_json::to_vec(&roving).unwrap()).unwrap();
    for tick in 0..=2000 {
        let time = f64::from(tick) / 100.;
        assert_eq!(roving.sample(time).unwrap(), frozen.sample(time).unwrap());
        assert_eq!(reopened.sample(time).unwrap(), frozen.sample(time).unwrap());
    }
}

#[test]
fn nonlinear_incident_timing_rejects_conversion_without_changing_any_track_state() {
    for ease in [Ease::Hold, Ease::In, Ease::Out, Ease::InOut] {
        let mut track = ordinary();
        track.set_ease(0, ease).unwrap();
        let before = track.clone();
        let error = track.enable_roving(&[4]).unwrap_err();
        assert!(
            error.to_string().contains("linear distance timing"),
            "{error}"
        );
        assert_eq!(track, before);
    }
}

#[test]
fn real_project_round_trip_and_timeline_preserve_continuous_time_ids_and_local_offsets() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "spatial", [32.; 2], [0.; 3], [1.; 4]);
    layer.transform.position = ordinary();
    layer.transform.position.enable_roving(&[4]).unwrap();
    layer.timeline = Some(LayerTimeline {
        in_frame: 7,
        out_frame: 100,
        offset_frame: 7,
    });
    for (time, value) in [(0, [0., 0.]), (4, [0.2, 0.1]), (10, [0.8, 0.4])] {
        layer
            .transform
            .anchor
            .upsert(time, value, Ease::Linear)
            .unwrap();
    }
    layer.transform.anchor.enable_roving(&[4]).unwrap();
    project.layers.push(layer);
    project.validate().unwrap();
    let stored = serde_json::to_vec(&project).unwrap();
    let reopened: Project = serde_json::from_slice(&stored).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened, project);
    let timeline = serde_json::to_value(reopened.timeline_layers(12.).unwrap()).unwrap();
    let keys = &timeline[0]["properties"]["position"]["spatial_keys"];
    assert_eq!(keys[1]["id"], 2);
    assert_eq!(keys[1]["roving"], true);
    assert_eq!(keys[1]["local_frame"], 10. / 7.);
    assert_eq!(keys[1]["frame"], 7. + 10. / 7.);
    assert_eq!(
        timeline[0]["properties"]["position"]["keys"],
        serde_json::json!([])
    );
    for tick in 0..=2000 {
        let time = f64::from(tick) / 100.;
        for (old, new) in [(
            &project.layers[0].transform.position,
            &reopened.layers[0].transform.position,
        )] {
            assert_eq!(old.sample(time).unwrap(), new.sample(time).unwrap());
        }
        assert_eq!(
            project.layers[0].transform.anchor.sample(time).unwrap(),
            reopened.layers[0].transform.anchor.sample(time).unwrap()
        );
    }
    // Malformed persistence cannot make two stores authoritative or drop the
    // per-interval policy on reopen. These are complete Project decodes.
    let value: serde_json::Value = serde_json::from_slice(&stored).unwrap();
    for corruption in ["keys", "arc", "easing", "id"] {
        let mut bad = value.clone();
        let track = &mut bad["layers"][0]["transform"]["position"];
        match corruption {
            "keys" => track["keys"] = serde_json::json!([]),
            "id" => track["spatial_animation"]["keys"][1]["id"] = serde_json::json!(1),
            field => {
                track["spatial_animation"]["keys"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
            }
        }
        assert!(
            serde_json::from_value::<Project>(bad).is_err(),
            "{corruption}"
        );
    }
}

#[test]
fn frame_addressed_edits_reject_but_playhead_edits_preserve_ids_and_static_conversion_samples_once()
{
    let mut track = ordinary();
    track.enable_roving(&[4]).unwrap();
    let before = track.clone();
    for operation in [0, 1, 2, 3, 4, 5, 6, 7] {
        let mut candidate = track.clone();
        let result = match operation {
            0 => candidate.move_key(4, 5),
            1 => candidate.copy_key(4, 5),
            2 => candidate.delete_key(4),
            3 => candidate.set_ease(4, Ease::In),
            4 => candidate.set_curve(4, Easing::default()),
            5 => candidate.set_auto_bezier(4, true),
            6 => candidate.separate(),
            _ => candidate.upsert(4, [2.; 3], Ease::Linear),
        };
        assert!(result.is_err(), "operation={operation}");
        assert_eq!(candidate, before);
    }
    let ids = track
        .spatial_animation
        .as_ref()
        .unwrap()
        .keys()
        .iter()
        .map(|k| k.id)
        .collect::<Vec<_>>();
    track.set_at(10, [8., 1., 0.]).unwrap();
    assert_eq!(
        track
            .spatial_animation
            .as_ref()
            .unwrap()
            .keys()
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        ids
    );
    track.set_at(-1, [-2., 0., 0.]).unwrap();
    track.set_at(25, [20., 0., 0.]).unwrap();
    track.set_at(12, [8., 2., 0.]).unwrap();
    track.validate_local().unwrap();
    assert_eq!(track.spatial_animation.as_ref().unwrap().keys().len(), 8);
    let sampled = track.sample(6.).unwrap();
    track.set_animated(6, false).unwrap();
    assert!(!track.is_animated() && track.spatial_animation.is_none());
    assert_eq!(track.value, sampled);
    for time in [f64::NEG_INFINITY, 0., 20., f64::INFINITY, f64::NAN] {
        assert_eq!(track.sample(time).unwrap(), sampled);
    }
}

#[test]
fn ambiguous_playhead_and_dual_storage_reject_without_losing_authored_values() {
    let mut track = Track::constant([0.; 3]);
    for (time, value) in [(0, [0.; 3]), (4, [0.; 3]), (10, [1.; 3])] {
        track.upsert(time, value, Ease::Linear).unwrap();
    }
    let key = track.keys[0].clone();
    track.enable_roving(&[4]).unwrap();
    let before = track.clone();
    assert!(track
        .set_at(0, [1.; 3])
        .unwrap_err()
        .to_string()
        .contains("explicit identity"));
    assert_eq!(track, before);
    track.keys.push(key);
    assert_ne!(track, before);
    assert!(track.validate_local().is_err());
    assert!(track.sample(0.).is_err());
    assert!(track.timeline(0).is_err());
    assert!(track.set_at(1, [1.; 3]).is_err());
    assert!(serde_json::to_vec(&track).is_err());
}

#[test]
fn property_semantics_and_numeric_limits_are_checked_on_spatial_values_and_controls() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "bounds", [32.; 2], [0.; 3], [1.; 4]);
    layer.transform.position = ordinary();
    layer.transform.position.enable_roving(&[4]).unwrap();
    layer.transform.rotation = layer.transform.position.clone();
    project.layers.push(layer);
    assert!(project
        .validate()
        .unwrap_err()
        .to_string()
        .contains("not rotation or scale"));
    project.layers[0].transform.rotation = Track::constant([0.; 3]);
    let track = &mut project.layers[0].transform.position;
    track
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_spatial(
            1,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([10_000_001., 0., 0.]),
            }),
        )
        .unwrap();
    assert!(project
        .validate()
        .unwrap_err()
        .to_string()
        .contains("numeric range"));
}

#[test]
fn full_project_load_preserves_identity_and_resource_failure_causes_for_position_and_anchor() {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "load", [32.; 2], [0.; 3], [1.; 4]);
    layer.transform.position = ordinary();
    layer.transform.position.enable_roving(&[4]).unwrap();
    for (frame, value) in [(0, [0.; 2]), (4, [0.2; 2]), (10, [0.8; 2])] {
        layer
            .transform
            .anchor
            .upsert(frame, value, Ease::Linear)
            .unwrap();
    }
    layer.transform.anchor.enable_roving(&[4]).unwrap();
    project.layers.push(layer);
    let original = serde_json::to_value(&project).unwrap();
    for property in ["position", "anchor"] {
        for problem in ["identity", "budget"] {
            let mut bad = original.clone();
            let keys = &mut bad["layers"][0]["transform"][property]["spatial_animation"]["keys"];
            let expected = if problem == "identity" {
                keys[1]["id"] = serde_json::json!(1);
                "spatial key identities must be unique"
            } else {
                *keys = serde_json::json!(vec![keys[0].clone(); 16_386]);
                "spatial timing key count"
            };
            let error = serde_json::from_value::<Project>(bad.clone())
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{property} {problem}: {error}");
            let bytes = serde_json::to_vec(&bad).unwrap();
            let error = serde_json::from_slice::<Project>(&bytes)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(expected),
                "stream {property} {problem}: {error}"
            );
        }
    }
    let duplicate = r#"{"value":[0,0,0],"keys":[],"keys":[]}"#;
    assert!(serde_json::from_str::<Track<[f32; 3]>>(duplicate)
        .unwrap_err()
        .to_string()
        .contains("duplicate field"));
    let private_field = "private-user-payload".repeat(1000);
    let mut unknown = serde_json::json!({"value":[0,0,0],"keys":[]});
    unknown
        .as_object_mut()
        .unwrap()
        .insert(private_field.clone(), serde_json::Value::Null);
    let error = serde_json::from_value::<Track<[f32; 3]>>(unknown)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown track field"));
    assert!(!error.contains("private-user-payload"));
    assert!(error.len() < 256);
}
