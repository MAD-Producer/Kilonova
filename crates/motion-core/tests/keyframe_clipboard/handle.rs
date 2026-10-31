use super::*;
use motion_core::{CurveShape, Easing};

fn animated(automatic: bool) -> Engine {
    let mut p = fixture().snapshot();
    let track = &mut p.layers[0].transform.opacity;
    track.keys.clear();
    for (frame, value) in [(0, 0.), (10, 0.2), (40, 0.9), (70, 1.)] {
        track.upsert(frame, value, Ease::Linear).unwrap();
    }
    if automatic {
        for frame in [10, 40] {
            track.set_auto_bezier(frame, true).unwrap();
        }
    }
    Engine::new(p).unwrap()
}

fn command(target: KeyframeTarget, frame: u32, incoming: bool, control: [f64; 2]) -> Command {
    Command::KeyframeHandle {
        target,
        frame,
        incoming,
        control,
    }
}

fn opacity(frame: u32, incoming: bool, control: [f64; 2]) -> Command {
    command(
        property(7, Property::Opacity, None),
        frame,
        incoming,
        control,
    )
}

fn controls(easing: Easing) -> [[f64; 2]; 2] {
    let Some(motion_core::Curve {
        shape: CurveShape::Cubic {
            control1, control2, ..
        },
        ..
    }) = easing.curve
    else {
        panic!("expected resolved cubic: {easing:?}");
    };
    [control1, control2]
}

#[test]
fn each_handle_converts_only_its_owner_and_freezes_that_keys_other_incident_curve() {
    for incoming in [false, true] {
        let mut e = animated(true);
        let before = e.snapshot();
        let original = &before.layers[0].transform.opacity;
        let index = usize::from(incoming);
        let control = [[0.23, 0.13], [0.78, 0.92]][index];
        e.apply(opacity(17, incoming, control)).unwrap();
        let current = &e.project().layers[0].transform.opacity;
        let changed = controls(current.effective_easing(1).unwrap());
        assert_eq!(changed[index], control);
        assert_eq!(
            changed[1 - index],
            controls(original.effective_easing(1).unwrap())[1 - index]
        );
        assert!(!current.keys[1 + index].auto_bezier);
        assert!(current.keys[2 - index].auto_bezier);
        let frozen = if incoming { 2 } else { 0 };
        assert_eq!(
            current.effective_easing(frozen).unwrap(),
            original.effective_easing(frozen).unwrap()
        );
        for (a, b) in current.keys.iter().zip(&original.keys) {
            assert_eq!(
                (a.frame, a.value, &a.spatial),
                (b.frame, b.value, &b.spatial)
            );
        }
        assert_eq!(e.project().layers[1], before.layers[1]);
        // The unselected automatic endpoint must continue deriving its slope.
        let neighbour_interval = if incoming { 0 } else { 2 };
        let prior = current.effective_easing(neighbour_interval).unwrap();
        let mut changed = current.clone();
        changed
            .set_at(
                if incoming { 0 } else { 70 },
                if incoming { 0.1 } else { 0.95 },
            )
            .unwrap();
        assert_ne!(prior, changed.effective_easing(neighbour_interval).unwrap());
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
    }
}

#[test]
fn handle_payload_resolves_the_opposite_endpoint_at_apply_time() {
    let mut e = animated(false);
    // Prepare both requests before either edit. Neither carries a stale copy
    // of the other control, even when their application order is reversed.
    let outgoing = opacity(17, false, [0.24, 0.17]);
    let incoming = opacity(17, true, [0.77, 0.88]);
    e.apply(incoming).unwrap();
    e.apply(outgoing).unwrap();
    assert_eq!(
        controls(
            e.project().layers[0]
                .transform
                .opacity
                .effective_easing(1)
                .unwrap()
        ),
        [[0.24, 0.17], [0.77, 0.88]]
    );
    let value = serde_json::to_value(opacity(17, true, [0.77, 0.88])).unwrap();
    let restored: Command = serde_json::from_value(value).unwrap();
    assert!(
        matches!(restored, Command::KeyframeHandle { frame:17,incoming:true,control,.. } if control==[0.77,0.88])
    );
}

#[test]
fn preview_cancel_restores_policy_and_commit_is_one_undo_with_identical_reopened_samples() {
    let mut e = animated(true);
    let before = e.snapshot();
    for commit in [false, true] {
        e.begin_gesture().unwrap();
        for i in 0..20 {
            e.apply(opacity(17, false, [0.25, 0.1 + f64::from(i) / 100.]))
                .unwrap();
        }
        e.end_gesture(commit).unwrap();
        if !commit {
            assert_eq!(e.project(), &before);
            assert!(!e.can_undo());
            continue;
        }
        let after = e.snapshot();
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), &after).unwrap();
        let reopened = motion_core::storage::load(root.path()).unwrap();
        assert_eq!(after, reopened);
        for i in 0..701 {
            let frame = f64::from(i) / 10.;
            assert_eq!(
                after.layers[0].transform.opacity.sample(frame).unwrap(),
                reopened.layers[0].transform.opacity.sample(frame).unwrap()
            );
        }
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        e.redo().unwrap();
        assert_eq!(e.project(), &after);
    }
}

#[test]
fn bad_target_frame_control_and_locked_or_unsupported_interpolation_are_atomic() {
    let mut e = animated(true);
    for frame in [18, 77, 120] {
        let cause = if frame == 77 {
            "following keyframe"
        } else if frame == 18 {
            "does not exist"
        } else {
            "outside the composition"
        };
        rejected(&mut e, opacity(frame, false, [0.25, 0.2]), cause);
    }
    for control in [
        [f64::NAN, 0.],
        [-0.1, 0.],
        [1.1, 0.],
        [0.5, 9.],
        [0.5, f64::INFINITY],
    ] {
        rejected(&mut e, opacity(17, false, control), "numeric range");
    }
    rejected(
        &mut e,
        command(
            property(999, Property::Opacity, None),
            17,
            false,
            [0.2, 0.1],
        ),
        "does not exist",
    );
    let mut p = e.snapshot();
    p.layers[0].locked = true;
    rejected(
        &mut Engine::new(p).unwrap(),
        opacity(17, false, [0.2, 0.1]),
        "locked",
    );
    let mut p = animated(false).snapshot();
    p.layers[0]
        .transform
        .opacity
        .set_ease(10, Ease::Hold)
        .unwrap();
    rejected(
        &mut Engine::new(p).unwrap(),
        opacity(17, false, [0.2, 0.1]),
        "choose Bezier explicitly",
    );
}

#[test]
fn endpoint_edits_use_typed_effect_vector_and_geometry_channels_without_mutating_values() {
    let mut fixtures = vec![vector_fixture(false), vector_fixture(true)];
    for effect in ["posterize", "curves"] {
        let (engine, target) = effects_fixture(effect);
        fixtures.push((engine, vec![target]));
    }
    for (mut seeded, targets) in fixtures {
        let selected: Vec<_> = targets
            .iter()
            .map(|t| selection(t.clone(), &[10]))
            .collect();
        let keys = KeyframeClipboard::capture(seeded.project(), &selected).unwrap();
        seeded
            .apply(paste(keys, targets.clone(), Some(20)))
            .unwrap();
        seeded
            .apply(Command::KeyframeInterpolation {
                selections: selected.clone(),
                ease: Ease::Linear,
            })
            .unwrap();
        for (target, selected) in targets.into_iter().zip(selected) {
            let mut e = Engine::new(seeded.snapshot()).unwrap();
            let before = e.snapshot();
            let keys =
                KeyframeClipboard::capture(&before, std::slice::from_ref(&selected)).unwrap();
            e.apply(command(target, 10, false, [0.23, 0.12])).unwrap();
            let actual =
                KeyframeClipboard::capture(e.project(), std::slice::from_ref(&selected)).unwrap();
            let mut expected = serde_json::to_value(keys).unwrap();
            let actual = serde_json::to_value(actual).unwrap();
            expected["tracks"][0]["data"]["keys"][0]["curve"] =
                actual["tracks"][0]["data"]["keys"][0]["curve"].clone();
            assert_eq!(expected, actual, "typed value or metadata changed");
            let curve: motion_core::Curve =
                serde_json::from_value(actual["tracks"][0]["data"]["keys"][0]["curve"].clone())
                    .unwrap();
            assert_eq!(
                controls(Easing {
                    ease: Ease::Linear,
                    curve: Some(curve)
                }),
                [[0.23, 0.12], [2. / 3., 1. - 1. / 3.]]
            );
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            assert!(!e.can_undo());
        }
    }
}

#[test]
fn separated_axis_changes_only_the_addressed_component() {
    let mut p = animated(false).snapshot();
    let position = &mut p.layers[0].transform.position;
    for key in &mut position.keys {
        key.spatial = None;
    }
    position.separate().unwrap();
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    e.apply(command(
        property(7, Property::Position, Some(Axis::X)),
        10,
        false,
        [0.2, 0.15],
    ))
    .unwrap();
    let mut expected = before.clone();
    let current = e.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    expected.layers[0]
        .transform
        .position
        .axes
        .as_mut()
        .unwrap()
        .x = current.x.clone();
    assert_eq!(e.project(), &expected);
    assert_eq!(
        controls(current.x.effective_easing(0).unwrap())[0],
        [0.2, 0.15]
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}

#[test]
fn real_boolean_and_enum_effect_parameters_reject_temporal_handles() {
    use motion_effects::ParamKind;
    let package = motion_effects::builtin::package().unwrap();
    for kind in [ParamKind::Bool, ParamKind::Enum] {
        let (definition, parameter) = package
            .manifest
            .effects
            .iter()
            .find_map(|d| {
                d.params
                    .iter()
                    .find(|p| p.kind == kind && p.animatable && p.implemented)
                    .map(|p| (d, p))
            })
            .expect("builtin must supply a real discrete parameter fixture");
        let mut instance = EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        );
        let p = instance.params.get_mut(&parameter.id).unwrap();
        for frame in [3, 13] {
            p.track.upsert(frame, p.default, Ease::Hold).unwrap();
        }
        let mut project = fixture().snapshot();
        project.layers[0].effects.push(instance);
        project.rebuild_plugin_dependencies();
        let mut e = Engine::new(project).unwrap();
        rejected(
            &mut e,
            command(
                KeyframeTarget::Effect {
                    object: 7,
                    effect: 1,
                    param: parameter.id.clone(),
                },
                10,
                false,
                [0.25, 0.2],
            ),
            "boolean and enum parameters have no temporal handles",
        );
    }
}

#[test]
fn handle_registry_targets_inactive_compositions_and_preserves_the_active_composition() {
    let mut e = animated(true);
    e.apply(Command::Composition {
        action: motion_core::CompositionAction::Precompose {
            objects: vec![7],
            name: "Handle child".into(),
            range: "composition".into(),
        },
    })
    .unwrap();
    let before = e.snapshot();
    let request = serde_json::json!({"op":"keyframe_handle","composition":before.compositions[0].id,
        "target":{"kind":"property","object":7,"property":"opacity","axis":null},
        "frame":17,"incoming":false,"control":[0.24,0.13]});
    e.apply_batch(motion_core::CommandRegistry::parse(&request.to_string()).unwrap())
        .unwrap();
    assert_eq!(e.project().layers, before.layers);
    let track = &e.project().compositions[0].layers[0].transform.opacity;
    assert_eq!(
        controls(track.effective_easing(1).unwrap())[0],
        [0.24, 0.13]
    );
    assert!(!track.keys[1].auto_bezier);
    assert!(track.keys[2].auto_bezier);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let mut invalid = request;
    invalid.as_object_mut().unwrap().remove("incoming");
    assert!(motion_core::CommandRegistry::parse(&invalid.to_string()).is_err());
}
