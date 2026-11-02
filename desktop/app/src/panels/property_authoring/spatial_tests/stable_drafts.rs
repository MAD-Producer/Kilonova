use super::*;

#[test]
fn bound_spatial_updates_follow_the_id_after_roving_moves_its_time() {
    for channel in [
        Channel::Position,
        Channel::Anchor,
        Channel::CameraPosition,
        Channel::CameraTarget,
        Channel::GlobalPosition,
        Channel::GlobalTarget,
    ] {
        for mode in ["plain", "forward", "reverse"] {
            if channel.object() == 0 && mode != "plain" {
                continue;
            }
            let (p, at) = fixture(channel, mode, true);
            let binding = SpatialDraft::read(
                &state(&p, at),
                channel.object(),
                channel.property(),
                Some(0),
            )
            .unwrap();
            assert!(binding.is_some());
            let mut engine = Engine::new(p.clone()).unwrap();
            for x in [0.25, 0.75, 0.375] {
                let s = state(engine.project(), at);
                let mut old = command(channel, at);
                if let Command::SetVector { value, .. } = &mut old {
                    value[0] = if channel.property() == Property::Anchor {
                        x
                    } else {
                        340. + x
                    };
                    // This is deliberately the wrong sampled companion value.
                    value[1] = 99.;
                }
                let c = SpatialDraft::command(&binding, &s, old).unwrap();
                let value = if channel.property() == Property::Anchor {
                    TrackValue::Vec2([x, 0.5])
                } else {
                    TrackValue::Vec3([
                        340. + x,
                        180.,
                        if matches!(channel, Channel::CameraPosition | Channel::GlobalPosition) {
                            -900.
                        } else {
                            0.
                        },
                    ])
                };
                assert!(
                    matches!(&c, Command::TrackAt { edit: TrackEdit::SetSpatialKey {key:2,value:v}, .. } if *v==value),
                    "{c:?}"
                );
                engine.apply(c).unwrap();
                assert!(SpatialDraft::current(&binding, &state(engine.project(), at)).unwrap());
            }
            let mut restored = engine.project().clone();
            // Removing history must restore every field, not just sampled output.
            for _ in 0..3 {
                engine.undo().unwrap();
            }
            assert_eq!(engine.project(), &p);
            for _ in 0..3 {
                engine.redo().unwrap();
            }
            assert_eq!(engine.project(), &restored);
            let temp = tempfile::tempdir().unwrap();
            motion_core::storage::save(temp.path(), &restored).unwrap();
            restored = motion_core::storage::load(temp.path()).unwrap();
            assert_eq!(&restored, engine.project());
        }
    }
}

#[test]
fn explicit_spatial_value_rejections_preserve_the_entire_project_and_history() {
    let (p, at) = fixture(Channel::Position, "plain", true);
    for (target, key, value) in [
        (Channel::Position.target(), 0, TrackValue::Vec3([1.; 3])),
        (Channel::Position.target(), 999, TrackValue::Vec3([1.; 3])),
        (Channel::Position.target(), 2, TrackValue::Vec2([1.; 2])),
        (
            Channel::Position.target(),
            2,
            TrackValue::Vec3([f32::NAN; 3]),
        ),
        (Channel::Position.target(), 2, TrackValue::Vec3([1e10; 3])),
        (
            KeyframeTarget::Property {
                object: 7,
                property: Property::Scale,
                axis: None,
            },
            2,
            TrackValue::Vec3([1.; 3]),
        ),
        (
            KeyframeTarget::Property {
                object: 7,
                property: Property::Position,
                axis: Some(motion_core::Axis::X),
            },
            2,
            TrackValue::Scalar(1.),
        ),
    ] {
        let mut e = Engine::new(p.clone()).unwrap();
        let error = e
            .apply(Command::TrackAt {
                target,
                frame: at,
                edit: TrackEdit::SetSpatialKey { key, value },
            })
            .unwrap_err();
        assert!(!error.to_string().is_empty());
        assert_eq!(e.project(), &p);
        assert!(!e.can_undo());
    }
}

#[test]
fn deleted_identity_cancels_instead_of_rebinding_and_coincident_keys_are_rejected() {
    let (mut p, at) = fixture(Channel::Position, "plain", true);
    let binding = SpatialDraft::read(&state(&p, at), 7, Property::Position, Some(0)).unwrap();
    p.layers[0]
        .transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap()
        .delete_key(2)
        .unwrap();
    let s = state(&p, at);
    assert!(!SpatialDraft::current(&binding, &s).unwrap());
    assert!(SpatialDraft::command(&binding, &s, command(Channel::Position, at)).is_err());
    let (mut p, _) = fixture(Channel::Position, "plain", true);
    p.layers[0]
        .transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_value(2, [320., 180., 0.])
        .unwrap();
    let at = p.layers[0].property_composition_frame(0.).unwrap();
    assert!(SpatialDraft::read(&state(&p, at), 7, Property::Position, Some(0)).is_err());
}

#[test]
fn new_spatial_key_protocol_is_in_the_catalog_and_roundtrips_json() {
    let c = Command::TrackAt {
        target: Channel::Position.target(),
        frame: 2.5,
        edit: TrackEdit::SetSpatialKey {
            key: 2,
            value: TrackValue::Vec3([1., 2., 3.]),
        },
    };
    let payload = serde_json::to_string(&c).unwrap();
    assert!(
        !c.composition_transaction(),
        "value previews must join the active gesture"
    );
    assert_eq!(
        serde_json::to_value(serde_json::from_str::<Command>(&payload).unwrap()).unwrap(),
        serde_json::to_value(c).unwrap()
    );
    assert!(
        serde_json::to_string(&motion_core::CommandRegistry::catalogue())
            .unwrap()
            .contains("set_spatial_key")
    );
}
