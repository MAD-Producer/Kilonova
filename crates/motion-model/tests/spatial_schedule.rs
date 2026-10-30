use motion_model::{
    Ease, SpatialAnimation, SpatialKeyTimes, SpatialPathBudget, SpatialTangents, Track,
};

fn source(points: &[[f32; 3]], frames: &[i32]) -> Track<[f32; 3]> {
    let mut track = Track::constant(points[0]);
    for (point, frame) in points.iter().zip(frames) {
        track.upsert(*frame, *point, Ease::Linear).unwrap();
    }
    track
}
fn base() -> SpatialAnimation<[f32; 3]> {
    SpatialAnimation::from_track(&source(
        &[[0.; 3], [1., 0., 0.], [3., 0., 0.], [9., 0., 0.]],
        &[0, 3, 6, 10],
    ))
    .unwrap()
}
fn times(animation: &SpatialAnimation<[f32; 3]>) -> Vec<(u64, f64)> {
    animation.prepared().keys().collect()
}
fn bytes(animation: &SpatialAnimation<[f32; 3]>) -> Vec<u8> {
    serde_json::to_vec(animation).unwrap()
}
fn capture(name: &str, animation: &SpatialAnimation<[f32; 3]>) {
    let Some(root) = std::env::var_os("MOTION_SPATIAL_TIMING_ARTIFACTS") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec_pretty(animation).unwrap(),
    )
    .unwrap();
    let keys: Vec<_> = animation.prepared().keys().collect();
    let start = keys[0].1;
    let end = keys[keys.len() - 1].1;
    let samples:Vec<_>=(0..=100).map(|index| {
        let frame=start+(end-start)*f64::from(index)/100.;
        let sample=animation.sample(frame).unwrap();
        serde_json::json!({"frame":frame,"position":sample.position,"interval":sample.interval,"parameter":sample.parameter})
    }).collect();
    std::fs::write(
        root.join(format!("{name}.samples.json")),
        serde_json::to_vec_pretty(&serde_json::json!({"keys":keys,"samples":samples})).unwrap(),
    )
    .unwrap();
}
fn assert_samples_equal(a: &SpatialAnimation<[f32; 3]>, b: &SpatialAnimation<[f32; 3]>) {
    for index in 0..=1000 {
        let frame = f64::from(index) / 100.;
        let left = a.sample(frame).unwrap().position;
        let right = b.sample(frame).unwrap().position;
        for axis in 0..3 {
            assert!(
                (left[axis] - right[axis]).abs() < 1e-12,
                "frame={frame} axis={axis} {left:?} {right:?}"
            );
        }
    }
}

#[test]
fn stable_ids_and_fractional_times_drive_real_sampling_without_editing_source_geometry() {
    let source = source(
        &[[0.; 3], [1., 0., 0.], [3., 0., 0.], [9., 0., 0.]],
        &[0, 3, 6, 10],
    );
    let original = serde_json::to_vec(&source).unwrap();
    let mut animation = SpatialAnimation::from_track(&source).unwrap();
    animation.set_roving(&[2, 3], true).unwrap();
    capture("linear-roving", &animation);
    assert_eq!(
        times(&animation),
        [(1, 0.), (2, 10. / 9.), (3, 10. / 3.), (4, 10.)]
    );
    assert_eq!(
        animation
            .keys()
            .iter()
            .map(|key| key.id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(
        animation
            .keys()
            .iter()
            .map(|key| key.fixed)
            .collect::<Vec<_>>(),
        [Some(0.), None, None, Some(10.)]
    );
    for index in 0..=1000 {
        let frame = f64::from(index) / 100.;
        assert!((animation.sample(frame).unwrap().position[0] - frame * 0.9).abs() < 1e-12);
    }
    assert_eq!(serde_json::to_vec(&source).unwrap(), original);
    // A metadata-only Roving flag would continue following the old key grid.
    assert!(
        (f64::from(source.sample(3.).unwrap()[0]) - animation.sample(3.).unwrap().position[0])
            .abs()
            > 1.
    );
}

#[test]
fn save_reopen_neighbor_edits_and_frozen_snapshots_share_one_time_definition() {
    let mut animation = base();
    animation.set_roving(&[2, 3], true).unwrap();
    let snapshot = animation.prepared().clone();
    let stored = bytes(&animation);
    let reopened: SpatialAnimation<[f32; 3]> = serde_json::from_slice(&stored).unwrap();
    assert_eq!(animation, reopened);
    assert_eq!(bytes(&reopened), stored);
    assert_samples_equal(&animation, &reopened);
    animation.set_value(2, [2., 0., 0.]).unwrap();
    assert_eq!(animation.keys()[1].id, 2);
    assert_eq!(animation.keys()[1].fixed, None);
    assert_eq!(animation.prepared().effective_time(2).unwrap(), 20. / 9.);
    assert_eq!(snapshot.effective_time(2).unwrap(), 10. / 9.);
    assert_eq!(snapshot.sample(10. / 9.).unwrap().position, [1., 0., 0.]);
    assert_eq!(reopened.sample(10. / 9.).unwrap().position, [1., 0., 0.]);
}

#[test]
fn freezing_selected_keys_preserves_samples_and_reference_order_is_independent_of_fixed_times() {
    let mut animation = SpatialAnimation::from_track(&source(
        &[[0.; 3], [8., 0., 0.], [9., 0., 0.], [10., 0., 0.]],
        &[0, 3, 6, 10],
    ))
    .unwrap();
    animation.set_roving(&[2, 3], true).unwrap();
    let before = animation.clone();
    animation.set_roving(&[2], false).unwrap();
    assert_eq!(animation.keys()[1].fixed, Some(8.));
    assert_eq!(animation.keys()[2].fixed, None);
    // The next free key's reference is 6, behind fixed time 8. These are
    // different coordinates; sorting author storage by either would lose IDs.
    assert_eq!(animation.keys()[2].reference, 6.);
    assert_samples_equal(&before, &animation);
    animation.set_roving(&[3], false).unwrap();
    assert_samples_equal(&before, &animation);
    assert_eq!(animation.keys()[2].fixed, Some(9.));
    let mut fractional = base();
    fractional.set_roving(&[2, 3], true).unwrap();
    let before = fractional.clone();
    fractional.set_roving(&[3], false).unwrap();
    assert_eq!(fractional.keys()[2].fixed, Some(10. / 3.));
    capture("partially-frozen", &fractional);
    assert_samples_equal(&before, &fractional);
    let reopened: SpatialAnimation<[f32; 3]> = serde_json::from_slice(&bytes(&fractional)).unwrap();
    assert_eq!(reopened.keys()[2].fixed, Some(10. / 3.));
    assert_samples_equal(&before, &reopened);
}

#[test]
fn coincident_effective_times_keep_ids_and_invalid_edits_preserve_primary_cause_and_state() {
    let mut animation = SpatialAnimation::from_track(&source(
        &[[0.; 3], [0.; 3], [3., 0., 0.], [9., 0., 0.]],
        &[0, 3, 6, 10],
    ))
    .unwrap();
    animation.set_roving(&[2, 3], true).unwrap();
    assert_eq!(
        times(&animation),
        [(1, 0.), (2, 0.), (3, 10. / 3.), (4, 10.)]
    );
    animation.set_roving(&[2], false).unwrap();
    assert_eq!(animation.keys()[1].id, 2);
    assert_eq!(animation.keys()[1].fixed, Some(0.));
    assert_eq!(animation.sample(0.).unwrap().position, [0.; 3]);
    let before = bytes(&animation);
    let error = animation.set_value(2, [1., 0., 0.]).unwrap_err();
    assert!(error.to_string().contains("key=Some(2)"));
    let cause = std::error::Error::source(&error).unwrap();
    assert!(cause.to_string().contains("positive duration"));
    assert_eq!(bytes(&animation), before);
    if let Some(root) = std::env::var_os("MOTION_SPATIAL_TIMING_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("rejected-zero-duration-edit.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"error":error.to_string(),"primaryCause":cause.to_string(),
                "completeAuthorStateUnchanged":bytes(&animation)==before}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    animation.set_value(3, [4., 0., 0.]).unwrap();
    capture("coincident-keys-after-valid-edit", &animation);
    assert_eq!(animation.keys()[2].id, 3);
    assert_eq!(animation.sample(0.).unwrap().position, [0.; 3]);
}

#[test]
fn stationary_spans_keep_authored_proportions_and_time_edits_target_stable_id() {
    let mut animation = SpatialAnimation::from_track(&source(&[[7.; 3]; 3], &[0, 2, 10])).unwrap();
    animation.set_roving(&[2], true).unwrap();
    let original = bytes(&animation);
    assert!(animation.set_fixed_time(1, 100.).is_err());
    assert_eq!(bytes(&animation), original);
    animation.set_fixed_time(3, 200.).unwrap();
    animation.set_fixed_time(1, 100.).unwrap();
    assert_eq!(times(&animation), [(1, 100.), (2, 120.), (3, 200.)]);
    animation.set_roving(&[2], false).unwrap();
    assert_eq!(animation.keys()[1].fixed, Some(120.));
    animation.set_fixed_time(2, 126.25).unwrap();
    assert_eq!(animation.keys()[1].id, 2);
    assert_eq!(animation.prepared().effective_time(2).unwrap(), 126.25);
    let reopened: SpatialAnimation<[f32; 3]> = serde_json::from_slice(&bytes(&animation)).unwrap();
    assert_eq!(reopened, animation);
    capture("stationary-fixed", &animation);
    for frame in [0., 100., 120., 126.25, 199.5, 200., 300.] {
        assert_eq!(animation.sample(frame).unwrap().position, [7.; 3]);
    }
}

#[test]
fn rejected_selections_and_malformed_persistence_are_atomic() {
    let mut animation = base();
    let before = bytes(&animation);
    for ids in [
        &[][..],
        &[1][..],
        &[4][..],
        &[2, 2][..],
        &[9][..],
        &[2, 4][..],
    ] {
        assert!(animation.set_roving(ids, true).is_err(), "{ids:?}");
        assert_eq!(bytes(&animation), before);
    }
    for fixed in [f64::NAN, f64::INFINITY, -1., 11.] {
        assert!(animation.set_fixed_time(2, fixed).is_err());
        assert_eq!(bytes(&animation), before);
    }
    let original: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let mutations = [
        ("/keys/1/id", serde_json::json!(1)),
        ("/keys/1/id", serde_json::json!(0)),
        ("/next_id", serde_json::json!(2)),
        ("/keys/0/fixed", serde_json::Value::Null),
        ("/keys/3/fixed", serde_json::Value::Null),
        ("/keys/1/reference", serde_json::json!(0.)),
        ("/keys/1/fixed", serde_json::json!(11.)),
    ];
    for (path, value) in mutations {
        let mut malformed = original.clone();
        *malformed.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<SpatialAnimation<[f32; 3]>>(malformed).is_err(),
            "{path}"
        );
    }
    let mut excess = original.clone();
    excess["keys"] = serde_json::json!(vec![original["keys"][0].clone(); 16_386]);
    assert!(serde_json::from_slice::<SpatialAnimation<[f32; 3]>>(
        &serde_json::to_vec(&excess).unwrap()
    )
    .unwrap_err()
    .to_string()
    .contains("count"));
    assert!(serde_json::from_value::<SpatialAnimation<[f32; 3]>>(excess)
        .unwrap_err()
        .to_string()
        .contains("count"));
    assert_eq!(bytes(&animation), before);
    let mut missing = original;
    missing["keys"][1].as_object_mut().unwrap().remove("fixed");
    assert!(serde_json::from_value::<SpatialAnimation<[f32; 3]>>(missing).is_err());
}

#[test]
fn closed_cubic_roving_uses_geometric_arc_distance_and_retains_authored_handles() {
    let mut track = source(&[[0.; 3], [0.; 3], [10., 0., 0.]], &[0, 5, 10]);
    track
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([20., 30., 5.]),
            }),
        )
        .unwrap();
    track
        .set_spatial(
            5,
            Some(SpatialTangents {
                incoming: Some([-15., 25., -5.]),
                outgoing: None,
            }),
        )
        .unwrap();
    let path = track
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    let mut animation = SpatialAnimation::from_track(&track).unwrap();
    animation.set_roving(&[2], true).unwrap();
    let first_length = path.interval_length(0).unwrap();
    assert!(first_length > 30.);
    assert!(
        (animation.prepared().effective_time(2).unwrap()
            - 10. * first_length / path.total_length())
        .abs()
            < 1e-12
    );
    for index in 0..=40 {
        let frame = f64::from(index) / 4.;
        let expected = path
            .sample_distance(path.total_length() * frame / 10.)
            .unwrap()
            .position;
        let actual = animation.sample(frame).unwrap().position;
        for axis in 0..3 {
            assert!((expected[axis] - actual[axis]).abs() < 1e-10);
        }
    }
    assert_eq!(animation.keys()[0].spatial, track.keys[0].spatial);
    assert_eq!(animation.keys()[1].spatial, track.keys[1].spatial);
    let before = animation.clone();
    animation.set_roving(&[2], false).unwrap();
    capture("cubic-frozen", &animation);
    assert_samples_equal(&before, &animation);
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(animation.sample(frame).is_err());
    }
    assert_eq!(animation.sample(-100.).unwrap().position, [0.; 3]);
    assert_eq!(animation.sample(100.).unwrap().position, [10., 0., 0.]);
}

#[test]
fn unsupported_imports_do_not_reinterpret_other_temporal_policies_or_nonspatial_values() {
    let mut track = source(&[[0.; 3], [1.; 3]], &[0, 10]);
    for ease in [Ease::Hold, Ease::In, Ease::Out, Ease::InOut] {
        track.set_ease(0, ease).unwrap();
        let animation=SpatialAnimation::from_track(&track).unwrap();
        // Import now preserves manual timing. Compare the actual samples,
        // including Hold's exact terminal boundary, rather than rejecting it.
        for tick in 0..=1000 {
            let frame=f64::from(tick)/100.;
            assert_eq!(animation.sample_value(frame).unwrap(),track.sample(frame).unwrap());
        }
    }
    track.set_ease(0, Ease::Linear).unwrap();
    track.set_auto_bezier(0, true).unwrap();
    assert!(SpatialAnimation::from_track(&track).is_err());
    let mut color = Track::constant([0.; 4]);
    color.upsert(0, [0.; 4], Ease::Linear).unwrap();
    color.upsert(10, [1.; 4], Ease::Linear).unwrap();
    assert!(SpatialAnimation::from_track(&color).is_err());
    let path = source(&[[0.; 3], [1.; 3]], &[0, 10])
        .prepare_spatial_path(SpatialPathBudget::default())
        .unwrap();
    let wrong_count = SpatialKeyTimes::new(&[0., 5., 10.]).unwrap();
    assert!(wrong_count.resolve(path).is_err());
}

#[test]
fn inserting_and_deleting_keys_keeps_survivor_ids_and_never_reuses_a_deleted_identity() {
    let mut animation = base();
    let id = animation.insert_fixed_after(2, 4.5, [2., 0., 0.]).unwrap();
    assert_eq!(id, 5);
    assert_eq!(
        animation
            .keys()
            .iter()
            .map(|key| key.id)
            .collect::<Vec<_>>(),
        [1, 2, 5, 3, 4]
    );
    assert_eq!(animation.prepared().effective_time(id).unwrap(), 4.5);
    animation.delete_key(id).unwrap();
    assert_eq!(
        animation
            .keys()
            .iter()
            .map(|key| key.id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    let mut reopened: SpatialAnimation<[f32; 3]> =
        serde_json::from_slice(&bytes(&animation)).unwrap();
    assert_eq!(
        reopened.insert_fixed_after(2, 4.75, [2., 0., 0.]).unwrap(),
        6
    );
    assert_eq!(reopened.keys()[2].fixed, Some(4.75));
    let stored = bytes(&reopened);
    assert!(reopened.insert_fixed_after(2, 20., [2., 0., 0.]).is_err());
    assert!(reopened.delete_key(900).is_err());
    assert_eq!(bytes(&reopened), stored);
    reopened.set_roving(&[2, 6, 3], true).unwrap();
    let first_promoted = reopened.prepared().effective_time(2).unwrap();
    reopened.delete_key(1).unwrap();
    assert_eq!(reopened.keys()[0].id, 2);
    assert_eq!(reopened.keys()[0].fixed, Some(first_promoted));
    assert_eq!(reopened.keys()[1].id, 6);
    assert_eq!(reopened.keys()[1].fixed, None);
}

#[test]
fn identity_exhaustion_corrupt_controls_and_two_dimensional_samples_are_explicit() {
    let mut track = Track::constant([0f32; 2]);
    for (frame, value) in [(0, [0., 0.]), (4, [3., 4.]), (10, [9., 12.])] {
        track.upsert(frame, value, Ease::Linear).unwrap();
    }
    let mut animation = SpatialAnimation::from_track(&track).unwrap();
    animation.set_roving(&[2], true).unwrap();
    assert_eq!(animation.prepared().effective_time(2).unwrap(), 10. / 3.);
    assert_eq!(animation.sample(5.).unwrap().position, [4.5, 6., 0.]);
    let stored = serde_json::to_vec(&animation).unwrap();
    assert!(animation
        .set_spatial(
            1,
            Some(SpatialTangents {
                incoming: Some([1., 0.]),
                outgoing: None,
            })
        )
        .is_err());
    assert!(animation.set_value(2, [f32::INFINITY, 0.]).is_err());
    assert_eq!(serde_json::to_vec(&animation).unwrap(), stored);
    let mut definition: serde_json::Value = serde_json::from_slice(&stored).unwrap();
    definition["next_id"] = serde_json::json!(u64::MAX);
    let mut exhausted: SpatialAnimation<[f32; 2]> = serde_json::from_value(definition).unwrap();
    let stored = serde_json::to_vec(&exhausted).unwrap();
    assert!(exhausted
        .insert_fixed_after(1, 2., [1., 1.])
        .unwrap_err()
        .to_string()
        .contains("identity space"));
    assert_eq!(serde_json::to_vec(&exhausted).unwrap(), stored);
}

#[test]
fn tiny_continuous_durations_stay_resolved_and_unrepresentable_timing_rejects_atomically() {
    let mut animation = SpatialAnimation::from_track(&source(
        &[[0.; 3], [1e-25, 0., 0.], [2e-25, 0., 0.]],
        &[0, 5, 10],
    ))
    .unwrap();
    animation.set_roving(&[2], true).unwrap();
    animation.set_fixed_time(3, 1e-300).unwrap();
    let middle = animation.prepared().effective_time(2).unwrap();
    assert_eq!(middle / 1e-300, 0.5);
    let quarter = animation.sample(0.25e-300).unwrap().position[0];
    assert!((quarter / f64::from(1e-25f32) - 0.5).abs() < 1e-12);
    let before = bytes(&animation);
    assert!(animation.set_fixed_time(3, f64::from_bits(1)).is_err());
    assert_eq!(bytes(&animation), before);
}
