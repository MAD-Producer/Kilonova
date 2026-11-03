use super::*;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

pub fn project(reverse: bool, separated: bool, shift: i32) -> Project {
    let mut p = fixture();
    let layer = &mut p.layers[1];
    layer.parent = Some(motion_core::ParentLink {
        object: Some(1),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    layer.three_d = true;
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            f64::from(shift) + if reverse { 6. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    layer.transform.rotation = Track::constant([17., -25., 39.]);
    layer.transform.scale = Track::constant([-140., 80., 120.]);
    for (index, anchor) in [[0.2, 0.8], [0.4, 0.6], [0.6, 0.4]].into_iter().enumerate() {
        let frame = shift + index as i32 * 3;
        layer
            .transform
            .anchor
            .upsert(frame, anchor, Ease::Linear)
            .unwrap();
        layer
            .transform
            .position
            .upsert(
                frame,
                [10. + index as f32 * 10., 20. + index as f32 * 10., 0.],
                Ease::Linear,
            )
            .unwrap();
    }
    if separated {
        layer.transform.position.separate().unwrap();
    }
    p
}

fn only_middle_keys_changed(before: &Project, after: &Project, anchor: [f32; 2], local: i32) {
    let mut expected = before.clone();
    let old = &before.layers[1].transform;
    let new = &after.layers[1].transform;
    assert_eq!(new.anchor.keys.len(), 3);
    assert_eq!(new.anchor.keys[1].frame, local);
    assert_eq!(new.anchor.keys[1].value, anchor);
    expected.layers[1].transform.anchor.keys[1].value = anchor;
    if let (Some(old_axes), Some(new_axes)) = (&old.position.axes, &new.position.axes) {
        let axes = expected.layers[1].transform.position.axes.as_mut().unwrap();
        for (old, next, dest) in [
            (&old_axes.x, &new_axes.x, &mut axes.x),
            (&old_axes.y, &new_axes.y, &mut axes.y),
            (&old_axes.z, &new_axes.z, &mut axes.z),
        ] {
            assert_eq!(next.keys.len(), old.keys.len());
            assert_eq!(next.keys[1].frame, local);
            dest.keys[1].value = next.keys[1].value;
        }
    } else {
        assert_eq!(new.position.keys.len(), 3);
        assert_eq!(new.position.keys[1].frame, local);
        expected.layers[1].transform.position.keys[1].value = new.position.keys[1].value;
    }
    assert_eq!(after, &expected);
}

#[test]
fn exact_fractional_composition_anchor_preserves_parented_mirrored_pose_and_only_local_keys() {
    for reverse in [false, true] {
        for separated in [false, true] {
            for shift in [0, -6] {
                let mut e = Engine::new(project(reverse, separated, shift)).unwrap();
                let before = e.snapshot();
                let pose = model(&before, 6.5);
                let value = [0.75, 0.15];
                e.apply(Command::AnchorAt {
                    object: 2,
                    frame: 6.5,
                    anchor: value,
                })
                .unwrap();
                close(pose, model(e.project(), 6.5));
                only_middle_keys_changed(&before, e.project(), value, shift + 3);
                let after = e.snapshot();
                assert!(e.undo().unwrap());
                assert_eq!(e.snapshot(), before);
                assert!(!e.undo().unwrap());
                assert!(e.redo().unwrap());
                assert_eq!(e.snapshot(), after);
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), &after).unwrap();
                let saved = motion_core::storage::load(folder.path()).unwrap();
                assert_eq!(saved, after);
                close(pose, model(&saved, 6.5));
            }
        }
    }
}

#[test]
fn mapped_anchor_gesture_cancels_both_channels_and_commit_is_one_undo() {
    for reverse in [false, true] {
        let mut e = Engine::new(project(reverse, true, -6)).unwrap();
        let before = e.snapshot();
        let pose = model(&before, 6.5);
        for commit in [false, true] {
            e.begin_gesture().unwrap();
            for anchor in [[0.6, 0.3], [0.7, 0.2]] {
                e.apply(Command::AnchorAt {
                    object: 2,
                    frame: 6.5,
                    anchor,
                })
                .unwrap();
                close(pose, model(e.project(), 6.5));
            }
            e.end_gesture(commit).unwrap();
            if commit {
                assert!(e.undo().unwrap());
            }
            assert_eq!(e.snapshot(), before);
            assert!(!e.undo().unwrap());
        }
    }
}

#[test]
fn anchor_candidate_is_not_published_when_the_compensation_channel_cannot_address_time() {
    let mut p = project(false, false, 0);
    p.layers[1].transform.anchor.enable_roving(&[3]).unwrap();
    let before = p.clone();
    let error =
        motion_model::keyframe_clipboard::authoring::compensate_anchor(&mut p, 2, 8.75, [0.8, 0.2])
            .unwrap_err();
    assert!(
        error.to_string().contains("continuous key addressing"),
        "{error}"
    );
    assert_eq!(
        p, before,
        "first candidate must not leak on second-channel failure"
    );
}

#[test]
fn continuous_anchor_and_position_keep_stable_ids_at_exact_projected_time() {
    for reverse in [false, true] {
        let mut p = project(reverse, false, 0);
        p.layers[1].transform.anchor.spatial_animation = Some(Box::new(
            motion_model::SpatialAnimation::from_track(&p.layers[1].transform.anchor).unwrap(),
        ));
        p.layers[1].transform.position.spatial_animation = Some(Box::new(
            motion_model::SpatialAnimation::from_track(&p.layers[1].transform.position).unwrap(),
        ));
        p.layers[1].transform.anchor.keys.clear();
        p.layers[1].transform.position.keys.clear();
        let track_ids = |p: &Project| {
            let t = &p.layers[1].transform;
            (
                t.anchor
                    .spatial_animation
                    .as_ref()
                    .unwrap()
                    .keys()
                    .iter()
                    .map(|k| k.id)
                    .collect::<Vec<_>>(),
                t.position
                    .spatial_animation
                    .as_ref()
                    .unwrap()
                    .keys()
                    .iter()
                    .map(|k| k.id)
                    .collect::<Vec<_>>(),
            )
        };
        let local = p.layers[1]
            .transform
            .anchor
            .spatial_animation
            .as_ref()
            .unwrap()
            .prepared()
            .keys()
            .nth(1)
            .unwrap()
            .1;
        let frame = p.layers[1].property_composition_frame(local).unwrap();
        let pose = model(&p, frame);
        let ids = track_ids(&p);
        let mut e = Engine::new(p).unwrap();
        e.apply(Command::AnchorAt {
            object: 2,
            frame,
            anchor: [0.7, 0.2],
        })
        .unwrap();
        close(pose, model(e.project(), frame));
        assert_eq!(track_ids(e.project()), ids);
    }
}

#[test]
fn roving_compensation_that_changes_the_current_sample_is_rejected_without_policy_or_pose_changes()
{
    let mut p = project(false, false, 0);
    p.layers[1].transform.anchor.enable_roving(&[3]).unwrap();
    p.layers[1].transform.position.enable_roving(&[3]).unwrap();
    let before = p.clone();
    let error =
        motion_model::keyframe_clipboard::authoring::compensate_anchor(&mut p, 2, 6.5, [0.7, 0.2])
            .unwrap_err();
    assert!(error.to_string().contains("retime spatial keys"), "{error}");
    assert_eq!(p, before);
}

#[test]
fn invalid_anchor_values_times_and_locked_layers_preserve_full_project_and_history() {
    for (frame, anchor, locked) in [
        (6.5, [1001., 0.], false),
        (8.75, [0.7, 0.2], false),
        (-1., [0.7, 0.2], false),
        (120., [0.7, 0.2], false),
        (f64::NAN, [0.7, 0.2], false),
        (6.5, [f32::INFINITY, 0.2], false),
        (6.5, [0.7, 0.2], true),
    ] {
        let mut p = project(false, true, 0);
        p.layers[1].locked = locked;
        let mut e = Engine::new(p).unwrap();
        let before = e.snapshot();
        assert!(e
            .apply(Command::AnchorAt {
                object: 2,
                frame,
                anchor
            })
            .is_err());
        assert_eq!(e.snapshot(), before);
        assert!(!e.can_undo() && !e.can_redo());
    }
}

#[test]
fn anchor_command_catalogue_and_parser_expose_fractional_composition_time() {
    let catalogue = motion_core::CommandRegistry::catalogue();
    let command = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "anchor_at")
        .unwrap();
    assert_eq!(
        command["inputSchema"]["properties"]["frame"]["type"],
        "number"
    );
    let commands = motion_core::CommandRegistry::parse(
        r#"{"op":"anchor_at","object":2,"frame":6.5,"anchor":[0.7,0.2]}"#,
    )
    .unwrap();
    let mut e = Engine::new(project(false, false, 0)).unwrap();
    let pose = model(e.project(), 6.5);
    motion_core::CommandRegistry::run(&mut e, commands).unwrap();
    close(pose, model(e.project(), 6.5));
}

#[test]
fn static_separated_position_uses_owned_axis_values_at_fractional_local_time_without_keys() {
    let mut p = project(false, false, 0);
    p.layers[1].transform.anchor = Track::constant([0.4, 0.6]);
    p.layers[1].transform.position = Track::constant([20., 30., 0.]);
    p.layers[1].transform.position.separate().unwrap();
    let before = p.clone();
    let pose = model(&p, 8.75);
    let mut e = Engine::new(p).unwrap();
    e.apply(Command::AnchorAt {
        object: 2,
        frame: 8.75,
        anchor: [0.7, 0.2],
    })
    .unwrap();
    let after = e.project();
    close(pose, model(after, 8.75));
    assert_eq!(after.layers[1].transform.anchor.value, [0.7, 0.2]);
    let t = &after.layers[1].transform.position;
    assert!(!t.is_animated());
    assert_ne!(
        t.sample(4.5).unwrap(),
        before.layers[1].transform.position.sample(4.5).unwrap()
    );
    assert!(
        t.axes.as_ref().unwrap().x.keys.is_empty() && t.axes.as_ref().unwrap().y.keys.is_empty()
    );
    assert!(e.undo().unwrap());
    assert_eq!(e.snapshot(), before);
}
