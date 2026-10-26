use super::*;
#[path = "interpolation.rs"]
mod interpolation;
#[path = "easing.rs"]
mod easing;
#[path = "transform.rs"]
mod transform;
#[path = "timing.rs"]
mod timing;
use motion_core::{
    masks::{LayerMask, MaskProperty},
    vector::{groups::*, *},
    Content, EffectAction, EffectInstance, Track, Tween,
};

fn seed<T: Tween>(track: &mut Track<T>) {
    track.keys = vec![Keyframe {
        frame: 3,
        value: track.value,
        ease: Ease::InOut,
        curve: None,
        spatial: None,
    }];
}
fn node(id: u64, x: f32, y: f32) -> PathNode {
    let mut geometry = Track::constant([x, y, 0., 0., 0., 0.]);
    seed(&mut geometry);
    PathNode { id, geometry }
}
fn path() -> VectorPath {
    VectorPath {
        id: 10,
        closed: true,
        nodes: vec![node(11, 0., 0.), node(12, 40., 0.), node(13, 20., 40.)],
    }
}
fn retarget(target: &KeyframeTarget, object: u64) -> KeyframeTarget {
    let mut result = target.clone();
    match &mut result {
        KeyframeTarget::Property { object: id, .. }
        | KeyframeTarget::Effect { object: id, .. }
        | KeyframeTarget::Mask { object: id, .. }
        | KeyframeTarget::Audio { object: id, .. }
        | KeyframeTarget::Vector { object: id, .. } => *id = object,
    }
    result
}
fn assert_transfer(a: &KeyframeClipboard, b: &KeyframeClipboard, delta: i32) {
    let mut expected = serde_json::to_value(a).unwrap();
    let actual = serde_json::to_value(b).unwrap();
    for (expected, actual) in expected["tracks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(actual["tracks"].as_array().unwrap())
    {
        expected["source"] = actual["source"].clone();
        for key in expected["data"]["keys"].as_array_mut().unwrap() {
            key["frame"] = (key["frame"].as_i64().unwrap() + i64::from(delta)).into();
        }
    }
    assert_eq!(
        expected, actual,
        "all values, easing and tangents must survive unchanged"
    );
}

#[test]
fn masks_and_path_geometry_transfer_without_cross_property_fallback() {
    let mut p = fixture().snapshot();
    let mut mask = LayerMask::new(1, path());
    seed(&mut mask.opacity);
    seed(&mut mask.feather);
    seed(&mut mask.expansion);
    for l in &mut p.layers {
        l.masks.push(mask.clone());
    }
    let mut e = Engine::new(p).unwrap();
    let targets = [
        MaskProperty::Opacity,
        MaskProperty::Feather,
        MaskProperty::Expansion,
        MaskProperty::Node(11),
    ]
    .into_iter()
    .map(|property| KeyframeTarget::Mask {
        object: 7,
        mask: 1,
        property,
    })
    .collect::<Vec<_>>();
    let frozen = KeyframeClipboard::capture(
        e.project(),
        &targets
            .iter()
            .map(|t| selection(t.clone(), &[10]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let dest = targets.iter().map(|t| retarget(t, 9)).collect::<Vec<_>>();
    let before = e.snapshot();
    e.apply(paste(frozen.clone(), dest.clone(), Some(50)))
        .unwrap();
    let pasted = KeyframeClipboard::capture(
        e.project(),
        &dest
            .iter()
            .map(|t| selection(t.clone(), &[50]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_transfer(&frozen, &pasted, 40);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let mut wrong = dest.clone();
    wrong[0] = dest[2].clone();
    rejected(
        &mut e,
        paste(frozen.clone(), wrong, Some(50)),
        "properties differ",
    );
    e.apply(Command::RemoveKeyframes {
        clipboard: Box::new(frozen),
    })
    .unwrap();
    assert!(e.project().layers[0].masks[0].opacity.keys.is_empty());
    assert!(e.project().layers[0].masks[0].path.nodes[0]
        .geometry
        .keys
        .is_empty());
    assert_eq!(
        e.project().layers[0].masks[0].path.nodes[1]
            .geometry
            .keys
            .len(),
        1
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}

fn vector_fixture(grouped: bool) -> (Engine, Vec<KeyframeTarget>) {
    let mut p = fixture().snapshot();
    let mut vector = VectorContent::shape(ShapeKind::RoundedRectangle);
    seed(vector.fill.as_mut().unwrap());
    vector.stroke = Some(Stroke {
        color: Track::constant([1.; 4]),
        width: Track::constant(2.),
        cap: LineCap::Butt,
        join: LineJoin::Miter,
        miter_limit: 4.,
        dashes: Some(StrokeDashes::default()),
    });
    let stroke = vector.stroke.as_mut().unwrap();
    seed(&mut stroke.color);
    seed(&mut stroke.width);
    let dash = stroke.dashes.as_mut().unwrap();
    seed(&mut dash.offset);
    for t in &mut dash.pattern {
        seed(t);
    }
    vector.trim = Some(TrimPaths::default());
    let trim = vector.trim.as_mut().unwrap();
    seed(&mut trim.start);
    seed(&mut trim.end);
    seed(&mut trim.offset);
    let VectorSource::Shape { parameters, .. } = &mut vector.source else {
        unreachable!()
    };
    for t in parameters.values_mut() {
        seed(t);
    }
    let parameters = [
        "fill",
        "stroke_color",
        "stroke_width",
        "trim_start",
        "trim_end",
        "trim_offset",
        "dash_offset",
        "dash_0",
        "shape:angle",
        "shape:corner_ratio",
    ];
    let mut targets = parameters
        .iter()
        .map(|parameter| KeyframeTarget::Vector {
            object: 7,
            item: grouped.then_some(2),
            parameter: (*parameter).into(),
        })
        .collect::<Vec<_>>();
    if grouped {
        vector = VectorGroup::wrap(vector, [64.; 2]);
        let VectorSource::Group { group } = &mut vector.source else {
            unreachable!()
        };
        seed(&mut group.transform.position);
        seed(&mut group.transform.opacity);
        let GroupItem::Geometry { size, position, .. } = &mut group.items[0] else {
            unreachable!()
        };
        seed(size);
        seed(position);
        let mut repeater = Repeater::default();
        seed(&mut repeater.copies);
        seed(&mut repeater.position);
        let mut nested = VectorGroup {
            id: 7,
            name: "Nested".into(),
            transform: Default::default(),
            items: vec![],
        };
        seed(&mut nested.transform.scale);
        group.items.extend([
            GroupItem::Repeater {
                id: 6,
                name: "Repeater".into(),
                repeater,
            },
            GroupItem::Group {
                group: Box::new(nested),
            },
            GroupItem::Geometry {
                id: 8,
                name: "Path".into(),
                vector: Box::new(VectorContent {
                    source: VectorSource::Paths {
                        paths: vec![path()],
                    },
                    fill: None,
                    stroke: None,
                    trim: None,
                    fill_rule: FillRule::NonZero,
                }),
                size: Track::constant([64.; 2]),
                position: Track::constant([0.; 2]),
            },
        ]);
        targets.extend(
            [
                (1, "position"),
                (1, "opacity"),
                (2, "size"),
                (2, "position"),
                (6, "copies"),
                (6, "position"),
                (7, "scale"),
                (8, "node:10:11"),
            ]
            .into_iter()
            .map(|(item, parameter)| KeyframeTarget::Vector {
                object: 7,
                item: Some(item),
                parameter: parameter.into(),
            }),
        );
    }
    for l in &mut p.layers {
        l.content = Content::Vector {
            vector: vector.clone(),
        };
    }
    (Engine::new(p).unwrap(), targets)
}

#[test]
fn flat_and_nested_vector_channels_preserve_typed_values_modifiers_and_single_undo() {
    for grouped in [false, true] {
        let (mut e, targets) = vector_fixture(grouped);
        let selected = targets
            .iter()
            .map(|t| selection(t.clone(), &[10]))
            .collect::<Vec<_>>();
        let frozen = KeyframeClipboard::capture(e.project(), &selected).unwrap();
        let dest = targets.iter().map(|t| retarget(t, 9)).collect::<Vec<_>>();
        let before = e.snapshot();
        e.apply(paste(frozen.clone(), dest.clone(), Some(50)))
            .unwrap();
        let pasted = KeyframeClipboard::capture(
            e.project(),
            &dest
                .iter()
                .map(|t| selection(t.clone(), &[50]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_transfer(&frozen, &pasted, 40);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        e.apply(Command::RemoveKeyframes {
            clipboard: Box::new(frozen),
        })
        .unwrap();
        assert!(KeyframeClipboard::capture(e.project(), &selected).is_err());
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
    }
}

fn effects_fixture(effect: &str) -> (Engine, KeyframeTarget) {
    let mut e = fixture();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == effect)
        .unwrap();
    let instance = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    let param = instance
        .params
        .iter()
        .find(|(_, p)| p.animatable && p.implemented && (effect != "curves" || p.curve.is_some()))
        .unwrap()
        .0
        .clone();
    for object in [7, 9] {
        e.apply(Command::Effect {
            object,
            action: EffectAction::Insert {
                instance: instance.clone(),
            },
        })
        .unwrap();
    }
    let mut p = e.snapshot();
    let param_data = p.layers[0].effects[0].params.get_mut(&param).unwrap();
    if let Some(c) = &mut param_data.curve {
        c.keys = vec![Keyframe {
            frame: 3,
            value: c.value.clone(),
            ease: Ease::Hold,
            curve: None,
            spatial: None,
        }];
    } else {
        seed(&mut param_data.track);
    }
    (
        Engine::new(p).unwrap(),
        KeyframeTarget::Effect {
            object: 7,
            effect: 1,
            param,
        },
    )
}

#[test]
fn effect_and_color_curve_keys_preserve_values_and_reject_foreign_parameter_schema() {
    for effect in ["posterize", "curves"] {
        let (mut e, target) = effects_fixture(effect);
        let frozen =
            KeyframeClipboard::capture(e.project(), &[selection(target.clone(), &[10])]).unwrap();
        let dest = retarget(&target, 9);
        let before = e.snapshot();
        e.apply(paste(frozen.clone(), vec![dest.clone()], Some(50)))
            .unwrap();
        let pasted = KeyframeClipboard::capture(e.project(), &[selection(dest, &[50])]).unwrap();
        assert_transfer(&frozen, &pasted, 40);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        e.apply(Command::RemoveKeyframes {
            clipboard: Box::new(frozen),
        })
        .unwrap();
        assert!(KeyframeClipboard::capture(e.project(), &[selection(target, &[10])]).is_err());
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        let mut bad = pasted;
        bad.tracks[0].signature.push_str("-foreign-plugin");
        rejected(
            &mut e,
            paste(bad, vec![retarget(&bad_target(&before), 9)], Some(60)),
            "properties differ",
        );
    }
}
fn bad_target(p: &Project) -> KeyframeTarget {
    let effect = &p.layers[0].effects[0];
    let param = effect
        .params
        .iter()
        .find(|(_, p)| {
            p.animatable && p.implemented && (effect.effect != "curves" || p.curve.is_some())
        })
        .unwrap()
        .0
        .clone();
    KeyframeTarget::Effect {
        object: 7,
        effect: effect.id,
        param,
    }
}

#[test]
fn camera_keys_use_their_own_channel_and_layer_local_time() {
    let mut e = fixture();
    let result = e
        .apply_batch(vec![Command::CreateCamera, Command::CreateCamera])
        .unwrap();
    let ids = result
        .iter()
        .map(|r| match r {
            motion_core::EditResult::CreateCamera { object } => *object,
            _ => panic!("wrong result"),
        })
        .collect::<Vec<_>>();
    e.apply(Command::Animate {
        object: ids[0],
        property: Property::Fov,
        axis: None,
        frame: 10,
        enabled: true,
    })
    .unwrap();
    e.apply(Command::SetScalar {
        object: ids[0],
        property: Property::Fov,
        frame: 10,
        value: 45.,
    })
    .unwrap();
    let target = property(ids[0], Property::Fov, None);
    let frozen = KeyframeClipboard::capture(e.project(), &[selection(target, &[10])]).unwrap();
    let before = e.snapshot();
    e.apply(paste(
        frozen.clone(),
        vec![property(ids[1], Property::Fov, None)],
        Some(50),
    ))
    .unwrap();
    assert_eq!(
        e.project().camera_for(ids[1]).unwrap().fov.keys[0].value,
        45.
    );
    assert_eq!(
        e.project().camera_for(ids[1]).unwrap().fov.keys[0].frame,
        50
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    rejected(
        &mut e,
        paste(
            frozen,
            vec![property(ids[1], Property::Roll, None)],
            Some(50),
        ),
        "properties differ",
    );
}

#[test]
fn audio_envelopes_copy_between_stereo_channels_without_spatial_fallback() {
    let mut p = fixture().snapshot();
    p.audio_assets.push(motion_core::AudioAsset {
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
    for l in &mut p.layers {
        l.content = Content::Audio {
            audio: motion_core::AudioClip::new(1),
        };
        l.size = [0.; 2];
        l.transform = motion_core::Transform::new([0.; 3]);
    }
    let mut e = Engine::new(p).unwrap();
    e.apply(Command::AudioLevels {
        object: 7,
        action: motion_core::AudioLevelsAction::Set {
            channel: motion_core::AudioChannel::Left,
            frame: 10,
            value: -12.,
            animated: Some(true),
        },
    })
    .unwrap();
    let target = KeyframeTarget::Audio {
        object: 7,
        channel: motion_core::AudioChannel::Left,
    };
    let frozen = KeyframeClipboard::capture(e.project(), &[selection(target, &[10])]).unwrap();
    let before = e.snapshot();
    e.apply(paste(
        frozen.clone(),
        vec![KeyframeTarget::Audio {
            object: 9,
            channel: motion_core::AudioChannel::Right,
        }],
        Some(50),
    ))
    .unwrap();
    let levels = e.project().layers[1].audio_levels.as_ref().unwrap();
    assert!(levels.left.keys.is_empty());
    assert_eq!(levels.right.keys[0].frame, 10);
    assert_eq!(levels.right.keys[0].value, -12.);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let mut bad = frozen.clone();
    if let KeyframeData::Scalar(keys) = &mut bad.tracks[0].data {
        keys[0].value = 100.;
    }
    rejected(
        &mut e,
        paste(
            bad,
            vec![KeyframeTarget::Audio {
                object: 9,
                channel: motion_core::AudioChannel::Left,
            }],
            Some(50),
        ),
        "-192..24",
    );
    rejected(
        &mut e,
        paste(frozen, vec![property(9, Property::Opacity, None)], Some(50)),
        "properties differ",
    );
}

#[test]
fn numeric_aliases_cannot_duplicate_a_source_or_mutate_a_destination_twice() {
    let (mut e, targets) = vector_fixture(true);
    let first = targets.last().unwrap().clone();
    let mut alias = first.clone();
    if let KeyframeTarget::Vector { parameter, .. } = &mut alias {
        *parameter = "node:010:011".into();
    }
    let error = KeyframeClipboard::capture(
        e.project(),
        &[selection(first.clone(), &[10]), selection(alias, &[10])],
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("duplicate clipboard source channel"),
        "{error}"
    );
    let mut second = first.clone();
    if let KeyframeTarget::Vector { parameter, .. } = &mut second {
        *parameter = "node:10:12".into();
    }
    let frozen = KeyframeClipboard::capture(
        e.project(),
        &[selection(first.clone(), &[10]), selection(second, &[10])],
    )
    .unwrap();
    let destination = retarget(&first, 9);
    let mut alias = destination.clone();
    if let KeyframeTarget::Vector { parameter, .. } = &mut alias {
        *parameter = "node:010:011".into();
    }
    rejected(
        &mut e,
        paste(frozen, vec![destination, alias], Some(50)),
        "duplicate clipboard destination channel",
    );
}

#[test]
fn color_curve_cut_retains_the_final_removed_value_and_undo_restores_both_keys() {
    let (e, target) = effects_fixture("curves");
    let mut p = e.snapshot();
    let KeyframeTarget::Effect { param, .. } = &target else {
        unreachable!()
    };
    let curve = p.layers[0].effects[0]
        .params
        .get_mut(param)
        .unwrap()
        .curve
        .as_mut()
        .unwrap();
    let mut final_key = curve.keys[0].clone();
    final_key.frame = 13;
    final_key.value.channels[0] = vec![[0., 0.], [0.5, 0.2], [1., 1.]];
    let expected = final_key.value.clone();
    curve.keys.push(final_key);
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    let frozen =
        KeyframeClipboard::capture(e.project(), &[selection(target.clone(), &[10, 20])]).unwrap();
    e.apply(Command::RemoveKeyframes {
        clipboard: Box::new(frozen),
    })
    .unwrap();
    let curve = e.project().layers[0].effects[0].params[param]
        .curve
        .as_ref()
        .unwrap();
    assert!(curve.keys.is_empty());
    assert_eq!(curve.value, expected);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}

#[test]
fn clipboard_channel_count_is_independent_of_the_layer_count_limit() {
    let (e, _) = vector_fixture(true);
    let mut p = e.snapshot();
    let Content::Vector { vector } = &mut p.layers[0].content else {
        unreachable!()
    };
    let VectorSource::Group { group } = &mut vector.source else {
        unreachable!()
    };
    let mut targets = vec![];
    for i in 0..=motion_core::MAX_LAYERS {
        let id = 100 + i as u64;
        let mut color = Track::constant([0.5; 4]);
        seed(&mut color);
        group.items.push(GroupItem::Fill {
            id,
            name: "Fill".into(),
            color,
            fill_rule: FillRule::NonZero,
            composite: Composite::Above,
        });
        targets.push(KeyframeTarget::Vector {
            object: 7,
            item: Some(id),
            parameter: "color".into(),
        });
    }
    let mut destination = p.layers[0].clone();
    destination.id = 9;
    destination.timeline = p.layers[1].timeline;
    p.layers[1] = destination;
    let mut e = Engine::new(p).unwrap();
    let frozen = KeyframeClipboard::capture(
        e.project(),
        &targets
            .iter()
            .map(|t| selection(t.clone(), &[10]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(frozen.tracks.len(), motion_core::MAX_LAYERS + 1);
    let before = e.snapshot();
    e.apply(paste(
        frozen,
        targets.iter().map(|t| retarget(t, 9)).collect(),
        Some(50),
    ))
    .unwrap();
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
}
