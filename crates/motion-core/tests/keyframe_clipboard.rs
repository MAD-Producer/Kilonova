use motion_core::{
    Axis, Command, Ease, Engine, Keyframe, KeyframeClipboard, KeyframeData, KeyframeSelection,
    KeyframeTarget, Layer, LayerTimeline, Project, Property, SpatialTangents,
};

fn property(object: u64, property: Property, axis: Option<Axis>) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property,
        axis,
    }
}
fn selection(target: KeyframeTarget, frames: &[u32]) -> KeyframeSelection {
    KeyframeSelection {
        target,
        frames: frames.to_vec(),
    }
}
fn fixture() -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    for (id, offset) in [(7, 7), (9, 40)] {
        let mut layer = Layer::solid(id, "Animated", [64.; 2], [30., 20., 0.], [1.; 4]);
        layer.timeline = Some(LayerTimeline {
            in_frame: offset,
            out_frame: 110,
            offset_frame: offset,
        });
        p.layers.push(layer);
    }
    p.layers[0].transform.position.keys = vec![
        Keyframe {
            frame: 3,
            value: [10., 20., 0.],
            ease: Ease::InOut,
            curve: Some(motion_core::Curve {
                space: motion_core::CurveSpace::Progress,
                shape: motion_core::CurveShape::Cubic {
                    control1: [0.2, 0.1],
                    control2: [0.7, 0.9],
                    start: 0.,
                    end: 1.,
                },
            }),
            spatial: Some(SpatialTangents {
                incoming: None,
                outgoing: Some([5., 3., 0.]),
            }),
        },
        Keyframe {
            frame: 13,
            value: [100., 40., 0.],
            ease: Ease::Out,
            curve: None,
            spatial: Some(SpatialTangents {
                incoming: Some([-5., 1., 0.]),
                outgoing: None,
            }),
        },
    ];
    p.layers[0].transform.opacity.keys = vec![Keyframe {
        frame: 8,
        value: 0.3,
        ease: Ease::Hold,
        curve: None,
        spatial: None,
    }];
    Engine::new(p).unwrap()
}
fn copy(e: &Engine) -> KeyframeClipboard {
    KeyframeClipboard::capture(
        e.project(),
        &[
            selection(property(7, Property::Position, None), &[20, 10]),
            selection(property(7, Property::Opacity, None), &[15]),
        ],
    )
    .unwrap()
}
fn paste(
    clipboard: KeyframeClipboard,
    targets: Vec<KeyframeTarget>,
    frame: Option<u32>,
) -> Command {
    Command::PasteKeyframes {
        clipboard: Box::new(clipboard),
        targets,
        at_frame: frame,
    }
}
fn destinations() -> Vec<KeyframeTarget> {
    vec![
        property(9, Property::Position, None),
        property(9, Property::Opacity, None),
    ]
}
fn rejected(e: &mut Engine, command: Command, cause: &str) {
    let before = e.snapshot();
    let revision = e.revision();
    let undo = e.can_undo();
    let history = e.history_diagnostics();
    let error = e.apply(command).unwrap_err().to_string();
    assert!(error.contains(cause), "expected {cause}, received {error}");
    assert_eq!(e.project(), &before);
    assert_eq!(e.revision(), revision);
    assert_eq!(e.can_undo(), undo);
    assert_eq!(e.history_diagnostics(), history);
}

#[test]
fn multi_track_clipboard_freezes_values_tangents_easing_and_uses_one_history_entry() {
    let mut e = fixture();
    let frozen = copy(&e);
    let before = e.snapshot();
    assert!(!e.can_undo());
    e.apply(paste(frozen.clone(), destinations(), Some(50)))
        .unwrap();
    let p = &e.project().layers[1].transform.position;
    assert_eq!(p.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), [10, 20]);
    for (a, b) in p.keys.iter().zip(&before.layers[0].transform.position.keys) {
        assert_eq!(a.value, b.value);
        assert_eq!(a.spatial, b.spatial);
        assert_eq!(a.ease, b.ease);
        assert_eq!(a.curve, b.curve);
    }
    assert_eq!(e.project().layers[1].transform.opacity.keys[0].frame, 15);
    let once = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.project(), &once);
    e.apply(paste(frozen.clone(), destinations(), None))
        .unwrap();
    assert_eq!(
        e.project().layers[1]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [-30, -20, 10, 20]
    );
    e.apply(Command::SetVector {
        object: 7,
        property: Property::Position,
        frame: 10,
        value: [99.; 3],
    })
    .unwrap();
    assert_eq!(frozen, copy(&Engine::new(before).unwrap()));
    assert_eq!(
        e.project().layers[1].transform.position.keys[0].value,
        [10., 20., 0.]
    );
}

#[test]
fn cut_is_one_transaction_and_frozen_keys_survive_source_removal_and_save_reopen() {
    let mut e = fixture();
    let frozen = copy(&e);
    let before = e.snapshot();
    e.apply(Command::RemoveKeyframes {
        clipboard: Box::new(frozen.clone()),
    })
    .unwrap();
    assert!(e.project().layers[0].transform.position.keys.is_empty());
    assert_eq!(
        e.project().layers[0].transform.position.value,
        [100., 40., 0.]
    );
    assert_eq!(e.project().layers[0].transform.opacity.value, 0.3);
    let cut = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    e.redo().unwrap();
    assert_eq!(e.project(), &cut);
    let mut target = Engine::new(Project::new(640, 360, 60, 120).unwrap()).unwrap();
    target
        .apply(Command::Add {
            layer: before.layers[1].clone(),
        })
        .unwrap();
    drop(e);
    target
        .apply(paste(frozen.clone(), destinations(), Some(50)))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    motion_core::storage::save(dir.path(), target.project()).unwrap();
    let restored = motion_core::storage::load(dir.path()).unwrap();
    assert_eq!(&restored, target.project());
    let encoded = serde_json::to_string(&frozen).unwrap();
    assert_eq!(
        serde_json::from_str::<KeyframeClipboard>(&encoded).unwrap(),
        frozen
    );
}

#[test]
fn collisions_locked_targets_and_late_failures_roll_back_every_channel_and_history() {
    let mut e = fixture();
    let frozen = copy(&e);
    e.apply(Command::Animate {
        object: 9,
        property: Property::Opacity,
        axis: None,
        frame: 55,
        enabled: true,
    })
    .unwrap();
    rejected(
        &mut e,
        paste(frozen.clone(), destinations(), Some(50)),
        "occupied",
    );
    e.undo().unwrap();
    e.apply(Command::Flags {
        object: 9,
        visible: true,
        locked: true,
    })
    .unwrap();
    rejected(
        &mut e,
        paste(frozen.clone(), destinations(), Some(50)),
        "locked",
    );
    e.undo().unwrap();
    rejected(
        &mut e,
        paste(frozen.clone(), destinations(), Some(115)),
        "outside composition",
    );
    let mut wrong = destinations();
    wrong[1] = property(99, Property::Opacity, None);
    rejected(
        &mut e,
        paste(frozen.clone(), wrong, Some(50)),
        "does not exist",
    );
    rejected(
        &mut e,
        paste(frozen.clone(), vec![destinations()[0].clone(); 2], Some(50)),
        "duplicate",
    );
    e.begin_gesture().unwrap();
    let before = e.snapshot();
    assert!(matches!(
        e.apply(paste(frozen.clone(), destinations(), Some(50))),
        Err(motion_core::Error::Gesture)
    ));
    assert!(matches!(
        e.apply(Command::RemoveKeyframes {
            clipboard: Box::new(frozen)
        }),
        Err(motion_core::Error::Gesture)
    ));
    assert_eq!(e.project(), &before);
    e.end_gesture(false).unwrap();
}

#[test]
fn stale_cut_cannot_delete_modified_keys_or_other_channels() {
    let mut e = fixture();
    let frozen = copy(&e);
    e.apply(Command::SetScalar {
        object: 7,
        property: Property::Opacity,
        frame: 15,
        value: 0.4,
    })
    .unwrap();
    rejected(
        &mut e,
        Command::RemoveKeyframes {
            clipboard: Box::new(frozen.clone()),
        },
        "source changed",
    );
    e.undo().unwrap();
    e.apply(Command::Flags {
        object: 7,
        visible: true,
        locked: true,
    })
    .unwrap();
    rejected(
        &mut e,
        Command::RemoveKeyframes {
            clipboard: Box::new(frozen.clone()),
        },
        "locked",
    );
    e.undo().unwrap();
    e.apply(Command::DeleteKey {
        object: 7,
        property: Property::Position,
        axis: None,
        frame: 20,
    })
    .unwrap();
    rejected(
        &mut e,
        Command::RemoveKeyframes {
            clipboard: Box::new(frozen),
        },
        "source key missing",
    );
}

#[test]
fn mismatched_properties_types_and_malformed_clipboard_fail_without_history() {
    let mut e = fixture();
    let frozen = copy(&e);
    let mut targets = destinations();
    targets[0] = property(9, Property::Rotation, None);
    rejected(
        &mut e,
        paste(frozen.clone(), targets, Some(50)),
        "properties differ",
    );
    let mut bad = frozen.clone();
    bad.tracks[0].data = KeyframeData::Vec2(vec![Keyframe {
        frame: 10,
        value: [1.; 2],
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    }]);
    rejected(
        &mut e,
        paste(bad, destinations(), Some(50)),
        "value types differ",
    );
    for frame in [-1, 10, motion_core::MAX_FRAMES as i32] {
        let mut bad = frozen.clone();
        if let KeyframeData::Vec3(keys) = &mut bad.tracks[0].data {
            keys[1].frame = frame;
        }
        rejected(
            &mut e,
            paste(bad, destinations(), Some(50)),
            "composition times",
        );
    }
    let mut bad = frozen.clone();
    if let KeyframeData::Vec3(keys) = &mut bad.tracks[0].data {
        keys[0].value[0] = f32::NAN;
    }
    rejected(&mut e, paste(bad, destinations(), Some(50)), "non-finite");
    assert!(KeyframeClipboard::capture(e.project(), &[]).is_err());
    assert!(KeyframeClipboard::capture(
        e.project(),
        &[selection(property(7, Property::Position, None), &[10, 10])]
    )
    .is_err());
    assert!(KeyframeClipboard::capture(
        e.project(),
        &[selection(property(7, Property::Position, None), &[11])]
    )
    .is_err());
    rejected(&mut e, paste(frozen, vec![], Some(50)), "channel count");
}

#[test]
fn separated_axes_remain_explicit_and_can_copy_only_matching_properties() {
    let mut e = fixture();
    // Remove the spatial path before using the ordinary separation command.
    for frame in [10, 20] {
        e.apply(Command::Spatial {
            object: 7,
            property: Property::Position,
            frame,
            tangents: None,
        })
        .unwrap();
    }
    e.apply(Command::SeparateDimensions {
        object: 7,
        property: Property::Position,
    })
    .unwrap();
    e.apply(Command::SeparateDimensions {
        object: 9,
        property: Property::Position,
    })
    .unwrap();
    let clip = KeyframeClipboard::capture(
        e.project(),
        &[selection(
            property(7, Property::Position, Some(Axis::X)),
            &[10, 20],
        )],
    )
    .unwrap();
    e.apply(paste(
        clip.clone(),
        vec![property(9, Property::Position, Some(Axis::Y))],
        Some(50),
    ))
    .unwrap();
    let axes = e.project().layers[1]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    assert_eq!(
        axes.y
            .keys
            .iter()
            .map(|k| (k.frame, k.value))
            .collect::<Vec<_>>(),
        [(10, 10.), (20, 100.)]
    );
    assert!(axes.x.keys.is_empty());
    assert!(axes.z.keys.is_empty());
    assert!(KeyframeClipboard::capture(
        e.project(),
        &[selection(property(7, Property::Position, None), &[10])]
    )
    .unwrap_err()
    .to_string()
    .contains("explicitly"));
    rejected(
        &mut e,
        paste(
            clip,
            vec![property(9, Property::Rotation, Some(Axis::Y))],
            Some(50),
        ),
        "properties differ",
    );
}

#[test]
fn typed_commands_parse_register_and_route_to_an_inactive_composition() {
    let mut e = fixture();
    let frozen = copy(&e);
    e.apply(Command::Composition {
        action: motion_core::CompositionAction::Create {
            settings: motion_core::CompositionSettings {
                name: "Child".into(),
                width: 640,
                height: 360,
                fps: 30,
                frames: 120,
                timing: "preserve_seconds".into(),
                shorten: "reject".into(),
            },
        },
    })
    .unwrap();
    let child = e.project().compositions[0].id.clone();
    for layer in fixture().project().layers.clone() {
        e.apply(Command::InComposition {
            composition: child.clone(),
            command: Box::new(Command::Add { layer }),
        })
        .unwrap();
    }
    let before = e.snapshot();
    let command = Command::InComposition {
        composition: child,
        command: Box::new(paste(frozen, destinations(), Some(50))),
    };
    let encoded = serde_json::to_string(&command).unwrap();
    let commands = motion_core::CommandRegistry::parse(&encoded).unwrap();
    motion_core::CommandRegistry::run(&mut e, commands).unwrap();
    assert_eq!(e.project().layers, before.layers);
    assert_ne!(e.project().compositions, before.compositions);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let catalogue = motion_core::CommandRegistry::catalogue();
    let paste = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "paste_keyframes")
        .unwrap();
    assert_eq!(
        paste["inputSchema"]["properties"]["targets"]["type"],
        "array"
    );
    assert_eq!(
        motion_core::CommandRegistry::id(&Command::RemoveKeyframes {
            clipboard: Box::new(copy(&e))
        }),
        "remove_keyframes"
    );
}

#[path = "keyframe_clipboard/channels.rs"]
mod channels;

#[test]
fn thousands_of_frozen_keys_cut_as_one_edit_and_keep_the_final_static_value() {
    let mut p = Project::new(64, 64, 30, 6000).unwrap();
    let mut layer = Layer::solid(7, "Dense", [32.; 2], [0.; 3], [1.; 4]);
    layer.transform.opacity.keys = (0..4000)
        .map(|frame| Keyframe {
            frame,
            value: frame as f32 / 4000.,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    let expected = layer.transform.opacity.keys.last().unwrap().value;
    p.layers.push(layer);
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    let frames = (0..4000).collect::<Vec<_>>();
    let frozen = KeyframeClipboard::capture(
        e.project(),
        &[selection(property(7, Property::Opacity, None), &frames)],
    )
    .unwrap();
    e.apply(Command::RemoveKeyframes {
        clipboard: Box::new(frozen),
    })
    .unwrap();
    assert!(e.project().layers[0].transform.opacity.keys.is_empty());
    assert_eq!(e.project().layers[0].transform.opacity.value, expected);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}

#[test]
fn clipboard_errors_retain_the_original_cause_with_bounded_channel_context() {
    let mut p = fixture().snapshot();
    p.layers[1].locked = true;
    let frozen = copy(&Engine::new(p.clone()).unwrap());
    let mut e = Engine::new(p).unwrap();
    let error = e
        .apply(paste(frozen, destinations(), Some(50)))
        .unwrap_err();
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.to_string(), "object 9 is locked");
    match &error {
        motion_core::Error::KeyframeClipboard {
            phase,
            object,
            channel,
            source,
        } => {
            assert_eq!(*phase, "paste");
            assert_eq!(*object, 9);
            assert_eq!(channel, "Position/None");
            assert!(matches!(**source, motion_core::Error::Locked(9)));
        }
        _ => panic!("missing typed clipboard cause: {error}"),
    }
    let mut bad = copy(&e);
    bad.tracks[0].signature = "x".repeat(16 * 1024 * 1024);
    rejected(
        &mut e,
        paste(bad, destinations(), Some(50)),
        "exceeds metadata limit",
    );
}
