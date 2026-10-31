use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    parse_commands, Ease, Engine, Layer, LayerTimeline, Project,
};
use serde_json::{json, Value};

fn fixture() -> Engine {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(7, "Local keys", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            TimeDirection::Forward,
            0.,
            0.,
        )
        .unwrap(),
        remap: None,
    });
    for frame in [0, 3, 6, 9] {
        layer
            .transform
            .position
            .upsert(frame, [frame as f32, 16., 0.], Ease::Linear)
            .unwrap();
        layer
            .transform
            .opacity
            .upsert(frame, 0.2 + frame as f32 / 20., Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn target(property: &str) -> Value {
    json!({"kind":"property", "object":7, "property":property, "axis":null})
}
fn command(target: Value, edit: Value) -> Value {
    json!({"op":"local_keyframes", "target":target, "edit":edit})
}
fn apply(engine: &mut Engine, value: Value) {
    engine
        .apply_batch(parse_commands(&value.to_string()).unwrap())
        .unwrap();
}

#[test]
fn fractional_projection_moves_by_local_identity_and_preserves_undo_and_save() {
    let mut engine = fixture();
    let before = engine.snapshot();
    assert_eq!(
        before.layers[0]
            .transform
            .position
            .timeline_for_layer(&before.layers[0])
            .unwrap()
            .keys[1]
            .frame,
        6.5
    );
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"translate", "frames":[3],
        "delta_frame":2, "copy":false, "value_offset":null}),
        ),
    );
    let after = engine.snapshot();
    assert_eq!(
        after.layers[0]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [0, 5, 6, 9]
    );
    assert_eq!(
        after.layers[0]
            .transform
            .position
            .timeline_for_layer(&after.layers[0])
            .unwrap()
            .keys[1]
            .frame,
        9.5
    );
    assert_eq!(
        after.layers[0].transform.position.keys[1].value,
        before.layers[0].transform.position.keys[1].value
    );
    assert_eq!(after.layers[0].time_mapping, before.layers[0].time_mapping);
    assert_eq!(engine.revision(), 1);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), &after).unwrap();
    assert_eq!(motion_core::storage::load(directory.path()).unwrap(), after);
}

fn rejected(engine: &mut Engine, value: Value, cause: &str) {
    let before = engine.snapshot();
    let revision = engine.revision();
    let history = engine.history_diagnostics();
    let commands = parse_commands(&value.to_string()).unwrap();
    let error = engine.apply_batch(commands).unwrap_err();
    assert!(error.to_string().contains(cause), "{error}");
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.history_diagnostics(), history);
}

#[test]
fn overlapping_selection_moves_without_replacing_unselected_keys_and_copy_offsets_one_component() {
    let mut engine = fixture();
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"translate", "frames":[0,3,6,9],
        "delta_frame":3,"copy":false}),
        ),
    );
    let keys = &engine.project().layers[0].transform.position.keys;
    assert_eq!(
        keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        [3, 6, 9, 12]
    );
    assert_eq!(
        keys.iter().map(|k| k.value[0]).collect::<Vec<_>>(),
        [0., 3., 6., 9.]
    );
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"translate", "frames":[3],
        "delta_frame":1,"copy":true,"value_offset":{"component":1,"delta":2.5}}),
        ),
    );
    let keys = &engine.project().layers[0].transform.position.keys;
    assert_eq!(keys[0].value, [0., 16., 0.]);
    assert_eq!(keys[1].value, [0., 18.5, 0.]);
    assert_eq!(keys[1].frame, 4);
    for (frames, delta, copy) in [
        (json!([3]), 1, true),
        (json!([3]), 3, false),
        (json!([3, 6]), 3, false),
        (json!([3]), 0, true),
    ] {
        rejected(
            &mut engine,
            command(
                target("position"),
                json!({"kind":"translate","frames":frames,
            "delta_frame":delta,"copy":copy}),
            ),
            "occupied",
        );
    }
}

#[test]
fn local_failures_are_atomic_and_keep_context_including_locks_bounds_missing_keys_and_numeric_overflow(
) {
    let mut engine = fixture();
    for (edit, cause) in [
        (json!({"kind":"delete","frames":[]}), "empty"),
        (json!({"kind":"delete","frames":[3,3]}), "sorted and unique"),
        (json!({"kind":"delete","frames":[6,3]}), "sorted and unique"),
        (json!({"kind":"delete","frames":[4]}), "does not exist"),
        (
            json!({"kind":"translate","frames":[3],"delta_frame":i32::MAX,"copy":false}),
            "overflow",
        ),
        (
            json!({"kind":"translate","frames":[0],"delta_frame":-2,"copy":false}),
            "outside composition",
        ),
        (
            json!({"kind":"translate","frames":[3],"delta_frame":40,"copy":false}),
            "outside composition",
        ),
        (
            json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false,
            "value_offset":{"component":3,"delta":1.}}),
            "component out of range",
        ),
        (
            json!({"kind":"ease","frames":[3],"incoming":false,"outgoing":false}),
            "at least one",
        ),
        (
            json!({"kind":"curve","frames":[3,9],"easing":{"ease":"out"}}),
            "two adjacent",
        ),
    ] {
        rejected(&mut engine, command(target("position"), edit), cause);
    }
    let mut project = engine.snapshot();
    project.layers[0].locked = true;
    rejected(
        &mut Engine::new(project).unwrap(),
        command(target("position"), json!({"kind":"delete","frames":[3]})),
        "locked",
    );
    let mut project = engine.snapshot();
    project.layers[0].transform.position.keys[1].value[0] = f32::MAX;
    let before = project.clone();
    let overflow = command(
        target("position"),
        json!({"kind":"translate","frames":[3],
        "delta_frame":1,"copy":false,"value_offset":{"component":0,"delta":f32::MAX}}),
    );
    let motion_core::Command::LocalKeyframes {
        target: channel,
        edit,
    } = serde_json::from_value(overflow).unwrap()
    else {
        panic!()
    };
    let error =
        motion_model::keyframe_clipboard::local::edit(&mut project, &channel, &edit).unwrap_err();
    assert!(error.to_string().contains("overflow"), "{error}");
    assert!(error.to_string().contains("local_translate"), "{error}");
    assert_eq!(project, before);
    let mut missing = target("position");
    missing["object"] = 999.into();
    rejected(
        &mut engine,
        command(missing, json!({"kind":"delete","frames":[3]})),
        "999",
    );
    // Failure in a later channel restores all earlier channel edits in the batch.
    rejected(
        &mut engine,
        json!([
            command(
                target("position"),
                json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false})
            ),
            command(target("opacity"), json!({"kind":"delete","frames":[4]}))
        ]),
        "does not exist",
    );
}

#[test]
fn reverse_projection_keeps_local_interval_sides_and_automatic_policy() {
    let mut project = fixture().snapshot();
    project.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
        TimeStretch::new(3, 2).unwrap(),
        TimeDirection::Reverse,
        0.,
        30.,
    )
    .unwrap();
    let mut engine = Engine::new(project).unwrap();
    assert_eq!(
        engine.project().layers[0]
            .property_composition_frame(3.)
            .unwrap(),
        42.5
    );
    let before = engine.snapshot();
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"ease","frames":[3],
        "incoming":true,"outgoing":false}),
        ),
    );
    let track = &engine.project().layers[0].transform.position;
    let expected = motion_core::Easing {
        ease: Ease::Linear,
        curve: None,
    }
    .ease_sides(true, false);
    assert_eq!(track.keys[0].curve, expected.curve);
    assert_eq!(
        track.keys[1..],
        before.layers[0].transform.position.keys[1..]
    );
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"automatic","frames":[3],"enabled":true}),
        ),
    );
    assert!(engine.project().layers[0].transform.position.keys[1].auto_bezier);
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"curve","frames":[3],
        "easing":{"ease":"in_out"}}),
        ),
    );
    let track = &engine.project().layers[0].transform.position;
    assert!(!track.keys[1].auto_bezier);
    assert_eq!(track.keys[1].ease, Ease::InOut);
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"translate","frames":[3],"delta_frame":2,"copy":false}),
        ),
    );
    assert_eq!(
        engine.project().layers[0]
            .property_composition_frame(5.)
            .unwrap(),
        39.5
    );
    assert_eq!(
        engine.project().layers[0].time_mapping,
        before.layers[0].time_mapping
    );
}

#[test]
fn retained_negative_keys_can_edit_metadata_and_move_to_a_visible_fractional_time() {
    let mut project = fixture().snapshot();
    project.layers[0]
        .transform
        .position
        .upsert(-3, [-3., 10., 0.], Ease::Linear)
        .unwrap();
    let mut engine = Engine::new(project).unwrap();
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"interpolation","frames":[-3],"ease":"hold"}),
        ),
    );
    apply(
        &mut engine,
        command(
            target("position"),
            json!({"kind":"translate","frames":[-3],"delta_frame":2,"copy":false}),
        ),
    );
    assert_eq!(
        engine.project().layers[0].transform.position.keys[0].frame,
        -1
    );
    assert_eq!(
        engine.project().layers[0]
            .property_composition_frame(-1.)
            .unwrap(),
        0.5
    );
    assert_eq!(
        engine.project().layers[0].transform.position.keys[0].ease,
        Ease::Hold
    );
}

#[test]
fn separated_axes_require_an_explicit_channel_and_deleting_all_keys_keeps_the_last_value() {
    let mut project = fixture().snapshot();
    project.layers[0].transform.position.separate().unwrap();
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(target("position"), json!({"kind":"delete","frames":[3]})),
        "separated",
    );
    let mut x = target("position");
    x["axis"] = "x".into();
    let before = engine.snapshot();
    apply(
        &mut engine,
        command(x, json!({"kind":"delete","frames":[0,3,6,9]})),
    );
    let actual = engine.project().layers[0]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    let original = before.layers[0].transform.position.axes.as_ref().unwrap();
    assert!(actual.x.keys.is_empty());
    assert_eq!(actual.x.value, 9.);
    assert_eq!(actual.y, original.y);
    assert_eq!(actual.z, original.z);
}

#[test]
fn transport_rejects_fractional_identities_and_unknown_fields_and_catalogue_describes_local_coordinates(
) {
    for edit in [
        json!({"kind":"delete","frames":[3.5]}),
        json!({"kind":"translate","frames":[3],"delta_frame":1.5,"copy":false}),
        json!({"kind":"delete","frames":[3],"extra":true}),
        json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false,
            "value_offset":{"component":0,"delta":1.,"extra":true}}),
    ] {
        assert!(parse_commands(&command(target("position"), edit).to_string()).is_err());
    }
    let catalogue = motion_core::CommandRegistry::catalogue();
    let entry = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "local_keyframes")
        .unwrap();
    let schema = &entry["inputSchema"]["properties"]["edit"];
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 6);
    assert_eq!(
        schema["oneOf"][0]["properties"]["frames"]["items"]["minimum"],
        i32::MIN
    );
    assert!(schema["description"]
        .as_str()
        .unwrap()
        .contains("local identity"));
}

fn seed<T: motion_core::Tween>(track: &mut motion_core::Track<T>) {
    for frame in [0, 3, 6] {
        track.upsert(frame, track.value, Ease::Linear).unwrap();
    }
}

fn path() -> motion_core::vector::VectorPath {
    use motion_core::{vector::*, Track};
    VectorPath {
        id: 10,
        closed: true,
        tangents: Default::default(),
        nodes: [(11, 0., 0.), (12, 30., 0.), (13, 0., 30.)]
            .into_iter()
            .map(|(id, x, y)| {
                let mut geometry = Track::constant([x, y, 0., 0., 0., 0.]);
                seed(&mut geometry);
                PathNode { id, geometry }
            })
            .collect(),
    }
}

#[test]
fn masks_vector_groups_and_effects_use_the_same_exact_local_protocol() {
    use motion_core::{
        masks::LayerMask,
        vector::{groups::VectorGroup, ShapeKind, VectorContent, VectorSource},
        Content, EffectInstance,
    };
    let base = fixture().snapshot();
    let mut fixtures = Vec::new();
    let mut project = base.clone();
    let mut mask = LayerMask::new(1, path());
    seed(&mut mask.opacity);
    seed(&mut mask.feather);
    project.layers[0].masks.push(mask);
    fixtures.push((
        project,
        vec![
            json!({"kind":"mask","object":7,"mask":1,"property":"opacity"}),
            json!({"kind":"mask","object":7,"mask":1,"property":"feather"}),
            serde_json::to_value(motion_core::KeyframeTarget::Mask {
                object: 7,
                mask: 1,
                property: motion_core::masks::MaskProperty::Node(11),
            })
            .unwrap(),
        ],
    ));
    let mut project = base.clone();
    let mut vector = VectorGroup::wrap(VectorContent::shape(ShapeKind::Rectangle), [64.; 2]);
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    seed(&mut group.transform.position);
    seed(&mut group.transform.opacity);
    project.layers[0].content = Content::Vector { vector };
    fixtures.push((
        project,
        vec!["position", "opacity"]
            .into_iter()
            .map(|parameter| json!({"kind":"vector","object":7,"item":1,"parameter":parameter}))
            .collect(),
    ));
    let package = motion_effects::builtin::package().unwrap();
    for name in ["curves", "posterize"] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == name)
            .unwrap();
        let mut instance = EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        );
        let (param, channel) = instance
            .params
            .iter_mut()
            .find(|(_, p)| {
                p.animatable
                    && p.implemented
                    && if name == "curves" {
                        p.curve.is_some()
                    } else {
                        !p.kind.discrete()
                    }
            })
            .unwrap();
        let param = param.clone();
        if let Some(curve) = &mut channel.curve {
            curve.keys = [0, 3, 6]
                .into_iter()
                .map(|frame| motion_core::Keyframe {
                    frame,
                    value: curve.value.clone(),
                    ease: Ease::Linear,
                    curve: None,
                    auto_bezier: false,
                    spatial: None,
                })
                .collect();
        } else {
            seed(&mut channel.track);
        }
        let mut project = base.clone();
        project.layers[0].effects.push(instance);
        fixtures.push((
            project,
            vec![json!({"kind":"effect","object":7,"effect":1,"param":param})],
        ));
    }
    for (mut project, targets) in fixtures {
        project.rebuild_plugin_dependencies();
        let mut engine = Engine::new(project).unwrap();
        for channel in targets {
            let before = engine.snapshot();
            // The two corresponding composition times are 6.5 and 8; neither
            // selecting nor translating depends on composition integer parsing.
            apply(
                &mut engine,
                command(
                    channel.clone(),
                    json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false}),
                ),
            );
            let moved = engine.snapshot();
            assert_ne!(moved, before);
            apply(
                &mut engine,
                command(
                    channel.clone(),
                    json!({"kind":"translate","frames":[4],"delta_frame":-1,"copy":false}),
                ),
            );
            assert_eq!(engine.project(), &before);
            apply(
                &mut engine,
                command(
                    channel.clone(),
                    json!({"kind":"ease","frames":[3],"incoming":true,"outgoing":true}),
                ),
            );
            assert_ne!(engine.project(), &before);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            apply(
                &mut engine,
                command(
                    channel.clone(),
                    json!({"kind":"interpolation","frames":[3],"ease":"hold"}),
                ),
            );
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            let root = tempfile::tempdir().unwrap();
            motion_core::storage::save(root.path(), &moved).unwrap();
            assert_eq!(motion_core::storage::load(root.path()).unwrap(), moved);
        }
    }
}

#[test]
fn mapped_audio_envelope_edits_preserve_the_other_channel_and_source_clock() {
    use motion_core::{AudioAsset, AudioClip, AudioLevels, Content, Transform};
    let mut project = fixture().snapshot();
    project.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/tone.wav".into(),
        bytes: 64,
        mime: "audio/wav".into(),
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 192_000,
        duration_us: 4_000_000,
    });
    let layer = &mut project.layers[0];
    layer.timeline.as_mut().unwrap().in_frame = 2;
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.transform = Transform::new([0.; 3]);
    layer.size = [0.; 2];
    let mut levels = AudioLevels::default();
    seed(&mut levels.left);
    seed(&mut levels.right);
    layer.audio_levels = Some(levels);
    let mut engine = Engine::new(project).unwrap();
    let before = engine.snapshot();
    apply(
        &mut engine,
        command(
            json!({"kind":"audio","object":7,"channel":"left"}),
            json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false,"value_offset":{"component":0,"delta":-6.}}),
        ),
    );
    let levels = engine.project().layers[0].audio_levels.as_ref().unwrap();
    assert_eq!(levels.left.keys[1].frame, 4);
    assert_eq!(levels.left.keys[1].value, -6.);
    assert_eq!(
        levels.right,
        before.layers[0].audio_levels.as_ref().unwrap().right
    );
    assert_eq!(
        engine.project().layers[0].time_mapping,
        before.layers[0].time_mapping
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn explicit_composition_routing_edits_only_the_child_and_retains_one_step_history() {
    let mut engine = fixture();
    let source_layer = engine.project().layers[0].clone();
    apply(
        &mut engine,
        json!({"op":"composition","action":{"kind":"create","settings":{
        "name":"Child","width":64,"height":64,"fps":24,"frames":60,"timing":"preserve_frames","shorten":"reject"}}}),
    );
    let child = engine.project().compositions[0].id.clone();
    apply(
        &mut engine,
        json!({"op":"add","composition":child,"layer":source_layer}),
    );
    let before = engine.snapshot();
    let mut scoped = command(
        target("position"),
        json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false}),
    );
    scoped["composition"] = child.into();
    apply(&mut engine, scoped);
    let after = engine.snapshot();
    assert_eq!(after.layers, before.layers);
    assert_eq!(
        after.compositions[0].layers[0].transform.position.keys[1].frame,
        4
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
}

#[test]
fn discrete_effects_keep_hold_and_reject_temporal_curves_without_partial_edits() {
    use motion_core::EffectInstance;
    let package = motion_effects::builtin::package().unwrap();
    for kind in [
        motion_effects::ParamKind::Bool,
        motion_effects::ParamKind::Enum,
    ] {
        let (definition, parameter) = package
            .manifest
            .effects
            .iter()
            .find_map(|definition| {
                definition
                    .params
                    .iter()
                    .find(|p| p.kind == kind && p.animatable && p.implemented)
                    .map(|p| (definition, p))
            })
            .unwrap();
        let mut instance = EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        );
        let parameter_data = instance.params.get_mut(&parameter.id).unwrap();
        for frame in [0, 3, 6] {
            parameter_data
                .track
                .upsert(frame, parameter_data.default, Ease::Hold)
                .unwrap();
        }
        let mut project = fixture().snapshot();
        project.layers[0].effects.push(instance);
        project.rebuild_plugin_dependencies();
        let mut engine = Engine::new(project).unwrap();
        let channel = json!({"kind":"effect","object":7,"effect":1,"param":parameter.id});
        for edit in [
            json!({"kind":"interpolation","frames":[3],"ease":"linear"}),
            json!({"kind":"curve","frames":[3],"easing":{"ease":"out"}}),
            json!({"kind":"ease","frames":[3],"incoming":true,"outgoing":false}),
            json!({"kind":"automatic","frames":[3],"enabled":true}),
        ] {
            rejected(
                &mut engine,
                command(channel.clone(), edit),
                "hold interpolation only",
            );
        }
        apply(
            &mut engine,
            command(
                channel.clone(),
                json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false}),
            ),
        );
        assert_eq!(
            engine.project().layers[0].effects[0].params[&parameter.id]
                .track
                .keys[1]
                .ease,
            Ease::Hold
        );
        apply(
            &mut engine,
            command(channel, json!({"kind":"delete","frames":[4]})),
        );
        assert_eq!(
            engine.project().layers[0].effects[0].params[&parameter.id]
                .track
                .keys
                .len(),
            2
        );
    }
}

#[test]
fn continuous_spatial_storage_is_explicitly_rejected_and_domain_failures_restore_prior_batch_edits()
{
    let mut project = fixture().snapshot();
    project.layers[0]
        .transform
        .position
        .enable_roving(&[3])
        .unwrap();
    let mut engine = Engine::new(project).unwrap();
    rejected(
        &mut engine,
        command(target("position"), json!({"kind":"delete","frames":[3]})),
        "stable-ID protocol",
    );
    let mut engine = fixture();
    rejected(
        &mut engine,
        json!([
            command(
                target("position"),
                json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false})
            ),
            command(
                target("opacity"),
                json!({"kind":"translate","frames":[3],"delta_frame":1,"copy":false,
            "value_offset":{"component":0,"delta":2.}})
            )
        ]),
        "opacity",
    );
}
