use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Command, Curve, CurveClipboard, CurveShape, CurveSpace, Ease, Easing, Engine,
    KeyframeSelection, KeyframeTarget, Layer, LayerTimeline, LocalKeyframeSelection, Project,
    Property,
};

fn target(property: Property) -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property,
        axis: None,
    }
}
fn selected(property: Property, frames: &[i32]) -> LocalKeyframeSelection {
    LocalKeyframeSelection {
        target: target(property),
        frames: frames.to_vec(),
    }
}
fn payload() -> CurveClipboard {
    CurveClipboard::new(Easing {
        ease: Ease::Linear,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Quadratic {
                control: [0.4, 0.2],
                start: 0.,
                end: 1.,
            },
        }),
    })
    .unwrap()
}
fn fixture(reverse: bool) -> Engine {
    let mut project = Project::new(64, 64, 30, 90).unwrap();
    let mut layer = Layer::solid(7, "Local easing", [32.; 2], [32., 32., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 90,
        offset_frame: 8,
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
            if reverse { 12. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    for frame in [-3, 3, 9] {
        layer
            .transform
            .position
            .upsert(frame, [frame as f32, 20., 0.], Ease::Linear)
            .unwrap();
        layer
            .transform
            .opacity
            .upsert(frame, (frame + 10) as f32 / 20., Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn command(local_selections: Vec<LocalKeyframeSelection>) -> Command {
    Command::PasteEasing {
        clipboard: Box::new(payload()),
        selections: vec![],
        local_selections,
    }
}
fn reject(engine: &mut Engine, command: Command, cause: &str) {
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let error = engine.apply(command).unwrap_err().to_string();
    assert!(error.contains(cause), "{error}");
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn signed_local_easing_survives_fractional_forward_reverse_projection_save_and_one_undo() {
    for reverse in [false, true] {
        let mut engine = fixture(reverse);
        assert_ne!(
            engine.project().layers[0]
                .property_composition_frame(3.)
                .unwrap()
                .fract(),
            0.
        );
        let before = engine.snapshot();
        engine
            .apply(command(vec![
                selected(Property::Position, &[-3, 3, 9]),
                selected(Property::Opacity, &[3]),
            ]))
            .unwrap();
        let after = engine.snapshot();
        assert_eq!(
            CurveClipboard::capture_local(engine.project(), &selected(Property::Position, &[-3]))
                .unwrap(),
            payload()
        );
        assert_eq!(
            CurveClipboard::capture_local(engine.project(), &selected(Property::Opacity, &[3]))
                .unwrap(),
            payload()
        );
        let mut restored = after.clone();
        restored.layers[0].transform.position = before.layers[0].transform.position.clone();
        restored.layers[0].transform.opacity = before.layers[0].transform.opacity.clone();
        assert_eq!(restored, before);
        for (a, b) in after.layers[0]
            .transform
            .position
            .keys
            .iter()
            .zip(&before.layers[0].transform.position.keys)
        {
            let mut original = a.clone();
            original.ease = b.ease;
            original.curve = b.curve;
            assert_eq!(&original, b);
        }
        assert_eq!(
            after.layers[0].transform.position.keys[2],
            before.layers[0].transform.position.keys[2]
        );
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), engine.project()).unwrap();
        assert_eq!(motion_core::storage::load(root.path()).unwrap(), after);
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
        assert!(engine.redo().unwrap());
        assert_eq!(engine.project(), &after);
    }
}

#[test]
fn mixed_spaces_preflight_every_target_and_reject_aliases_without_partial_write() {
    let mut engine = fixture(false);
    let mut project = engine.snapshot();
    let mut legacy = project.layers[0].clone();
    legacy.id = 8;
    legacy.time_mapping = None;
    project.layers.push(legacy);
    engine = Engine::new(project).unwrap();
    let legacy = KeyframeSelection {
        target: KeyframeTarget::Property {
            object: 8,
            property: Property::Opacity,
            axis: None,
        },
        frames: vec![11],
    };
    for (frames, cause) in [
        (vec![4], "does not exist"),
        (vec![9], "no outgoing"),
        (vec![3, -3], "sorted and unique"),
        (vec![], "no keyframes"),
    ] {
        reject(
            &mut engine,
            Command::PasteEasing {
                clipboard: Box::new(payload()),
                selections: vec![legacy.clone()],
                local_selections: vec![selected(Property::Opacity, &frames)],
            },
            cause,
        );
    }
    reject(
        &mut engine,
        Command::PasteEasing {
            clipboard: Box::new(payload()),
            selections: vec![legacy.clone()],
            local_selections: vec![LocalKeyframeSelection {
                target: legacy.target.clone(),
                frames: vec![3],
            }],
        },
        "duplicate easing channel",
    );
    engine
        .apply(Command::PasteEasing {
            clipboard: Box::new(payload()),
            selections: vec![legacy],
            local_selections: vec![selected(Property::Opacity, &[3])],
        })
        .unwrap();
    assert_eq!(
        engine.project().layers[0].transform.opacity.keys[1].curve,
        payload().easing.curve
    );
    assert_eq!(
        engine.project().layers[1].transform.opacity.keys[1].curve,
        payload().easing.curve
    );
    let mut locked = engine.snapshot();
    locked.layers[0].locked = true;
    let mut locked = Engine::new(locked).unwrap();
    assert_eq!(
        CurveClipboard::capture_local(locked.project(), &selected(Property::Opacity, &[3]))
            .unwrap(),
        payload()
    );
    reject(
        &mut locked,
        command(vec![selected(Property::Opacity, &[3])]),
        "locked",
    );
}

#[test]
fn local_capture_requires_one_existing_outgoing_key_and_command_encodes_signed_addresses() {
    let engine = fixture(true);
    for frames in [vec![], vec![3, 9], vec![4], vec![9]] {
        assert!(CurveClipboard::capture_local(
            engine.project(),
            &selected(Property::Opacity, &frames)
        )
        .is_err());
    }
    let json = serde_json::to_value(command(vec![selected(Property::Opacity, &[-3])])).unwrap();
    assert_eq!(json["local_selections"][0]["frames"][0], -3);
    let parsed: Command = serde_json::from_value(json).unwrap();
    let mut engine = fixture(true);
    engine.apply(parsed).unwrap();
    assert_eq!(
        engine.project().layers[0].transform.opacity.keys[0].curve,
        payload().easing.curve
    );
    let legacy = Command::PasteEasing {
        clipboard: Box::new(payload()),
        selections: vec![],
        local_selections: vec![],
    };
    let json = serde_json::to_value(legacy).unwrap();
    assert!(json.get("local_selections").is_none());
    assert!(serde_json::from_value::<Command>(json).is_ok());
}

#[test]
fn local_easing_requires_explicit_separated_axis_and_preserves_other_axes() {
    let mut project = fixture(true).snapshot();
    project.layers[0].transform.position.separate().unwrap();
    let mut engine = Engine::new(project).unwrap();
    reject(
        &mut engine,
        command(vec![selected(Property::Position, &[3])]),
        "separated",
    );
    let before = engine.snapshot();
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: Some(motion_core::Axis::Y),
    };
    engine
        .apply(Command::PasteEasing {
            clipboard: Box::new(payload()),
            selections: vec![],
            local_selections: vec![LocalKeyframeSelection {
                target: target.clone(),
                frames: vec![3],
            }],
        })
        .unwrap();
    assert_eq!(
        CurveClipboard::capture_local(
            engine.project(),
            &LocalKeyframeSelection {
                target,
                frames: vec![3]
            }
        )
        .unwrap(),
        payload()
    );
    let mut restored = engine.snapshot();
    restored.layers[0]
        .transform
        .position
        .axes
        .as_mut()
        .unwrap()
        .y
        .keys[1] = before.layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap()
        .y
        .keys[1]
        .clone();
    assert_eq!(restored, before);
}
