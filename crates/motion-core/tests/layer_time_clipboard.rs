use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Command, Curve, CurveShape, CurveSpace, Ease, Engine, Keyframe, KeyframeClipboard,
    KeyframeSelection, KeyframeTarget, Layer, LayerTimeline, LocalKeyframeSelection, Project,
    Property,
};

fn target(object: u64) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property: Property::Position,
        axis: None,
    }
}
fn local(object: u64, frames: &[i32]) -> LocalKeyframeSelection {
    LocalKeyframeSelection {
        target: target(object),
        frames: frames.to_vec(),
    }
}
fn fixture() -> Engine {
    let mut p = Project::new(640, 360, 30, 240).unwrap();
    for (id, offset, mapped) in [(7, 2, true), (8, 5, false), (9, 20, true), (11, 5, false)] {
        let mut l = Layer::solid(id, "Time transfer", [64.; 2], [0.; 3], [1.; 4]);
        l.timeline = Some(LayerTimeline {
            in_frame: offset,
            out_frame: 200,
            offset_frame: offset,
        });
        if mapped {
            l.time_mapping = Some(LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    TimeDirection::Forward,
                    0.,
                    0.,
                )
                .unwrap(),
                remap: None,
            });
        }
        if id == 7 || id == 8 {
            l.transform.position.keys = if mapped { vec![-1, 3, 7] } else { vec![0, 4] }
                .into_iter()
                .map(|frame| Keyframe {
                    frame,
                    value: [frame as f32 * 10., 2., 0.],
                    ease: Ease::In,
                    curve: None,
                    spatial: None,
                    auto_bezier: false,
                })
                .collect();
        }
        p.layers.push(l);
    }
    Engine::new(p).unwrap()
}
fn paste(clipboard: KeyframeClipboard, targets: Vec<KeyframeTarget>, at: Option<f64>) -> Command {
    Command::PasteKeyframes {
        clipboard: Box::new(clipboard),
        targets,
        at_frame: at,
    }
}
fn reject(e: &mut Engine, command: Command, cause: &str) {
    let before = e.snapshot();
    let revision = e.revision();
    let history = e.history_diagnostics();
    let error = e.apply(command).unwrap_err().to_string();
    assert!(error.contains(cause), "{error}");
    assert_eq!(e.project(), &before);
    assert_eq!(e.revision(), revision);
    assert_eq!(e.history_diagnostics(), history);
}
#[test]
fn fractional_transfer_preserves_signed_identity_exact_spacing_save_and_full_undo() {
    let mut e = fixture();
    let before = e.snapshot();
    let clipboard =
        KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[7, 3])]).unwrap();
    let encoded = serde_json::to_vec(&clipboard).unwrap();
    assert_eq!(
        serde_json::from_slice::<KeyframeClipboard>(&encoded).unwrap(),
        clipboard
    );
    e.apply(paste(clipboard, vec![target(9)], Some(33.5)))
        .unwrap();
    let l = &e.project().layers[2];
    assert_eq!(
        l.transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [9, 13]
    );
    assert_eq!(l.property_composition_frame(9.).unwrap(), 33.5);
    assert_eq!(l.property_composition_frame(13.).unwrap(), 39.5);
    assert_eq!(
        l.transform.position.keys[0].value,
        before.layers[0].transform.position.keys[1].value
    );
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), e.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(root.path()).unwrap(),
        e.snapshot()
    );
    assert!(e.undo().unwrap());
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    let clipboard =
        KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[-1, 3])]).unwrap();
    e.apply(paste(clipboard, vec![target(9)], Some(11.)))
        .unwrap();
    assert_eq!(
        e.project().layers[2]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [-6, -2]
    );
    assert!(e.undo().unwrap());
    assert_eq!(e.project(), &before);
}
#[test]
fn mixed_coordinate_tracks_preserve_common_alignment_and_late_failures_are_atomic() {
    let mut e = fixture();
    let before = e.snapshot();
    let clipboard = KeyframeClipboard::capture_mixed(
        e.project(),
        &[KeyframeSelection {
            target: target(8),
            frames: vec![5, 9],
        }],
        &[local(7, &[3, 7])],
    )
    .unwrap();
    e.apply(paste(
        clipboard.clone(),
        vec![target(11), target(9)],
        Some(65.),
    ))
    .unwrap();
    assert_eq!(
        e.project().layers[3]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [60, 64]
    );
    assert_eq!(
        e.project().layers[2]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [31, 35]
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    reject(
        &mut e,
        paste(clipboard.clone(), vec![target(11), target(9)], Some(65.25)),
        "continuous key addressing",
    );
    let mut occupied = before.clone();
    occupied.layers[2].transform.position.keys.push(Keyframe {
        frame: 35,
        value: [1.; 3],
        ease: Ease::Linear,
        curve: None,
        spatial: None,
        auto_bezier: false,
    });
    let mut occupied = Engine::new(occupied).unwrap();
    reject(
        &mut occupied,
        paste(clipboard, vec![target(11), target(9)], Some(65.)),
        "occupied",
    );
}
#[test]
fn cut_fingerprints_the_source_clock_and_keys_and_undo_restores_the_entire_document() {
    let mut e = fixture();
    let before = e.snapshot();
    let clipboard =
        KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[-1, 3, 7])]).unwrap();
    e.apply(Command::RemoveKeyframes {
        clipboard: Box::new(clipboard.clone()),
    })
    .unwrap();
    assert!(e.project().layers[0].transform.position.keys.is_empty());
    assert_eq!(
        e.project().layers[0].transform.position.value,
        [70., 2., 0.]
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    e.apply(Command::LayerTime {
        object: 7,
        action: motion_core::layer_time::LayerTimeAction::Reverse {},
    })
    .unwrap();
    reject(
        &mut e,
        Command::RemoveKeyframes {
            clipboard: Box::new(clipboard),
        },
        "cut source changed",
    );
    assert!(KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[3, 3])]).is_err());
    assert!(KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[2])]).is_err());
    assert!(
        KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[3]), local(7, &[7])])
            .is_err()
    );
}
#[test]
fn opposite_clocks_reverse_interval_ownership_curves_and_spatial_tangents_without_changing_samples()
{
    for curve in [
        None,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1: [0.2, -0.1],
                control2: [0.6, 0.7],
                start: 0.,
                end: 1.,
            },
        }),
        Some(Curve {
            space: CurveSpace::Velocity,
            shape: CurveShape::Quadratic {
                control: [0.3, 0.7],
                start: 0.2,
                end: 1.1,
            },
        }),
    ] {
        let mut p = fixture().snapshot();
        p.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            TimeDirection::Reverse,
            0.,
            20.,
        )
        .unwrap();
        p.layers[0].transform.position.keys.remove(0);
        p.layers[0].transform.position.keys[0].curve = curve;
        p.layers[0].transform.position.keys[0].spatial = Some(motion_core::SpatialTangents {
            incoming: None,
            outgoing: Some([4., 8., 0.]),
        });
        p.layers[0].transform.position.keys[1].spatial = Some(motion_core::SpatialTangents {
            incoming: Some([-3., 2., 0.]),
            outgoing: None,
        });
        let mut e = Engine::new(p).unwrap();
        let before = e.snapshot();
        let clipboard =
            KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[3, 7])]).unwrap();
        e.apply(paste(clipboard, vec![target(9)], Some(33.5)))
            .unwrap();
        let source = &before.layers[0].transform.position;
        let dest = &e.project().layers[2].transform.position;
        for i in 0..=48 {
            let t = f64::from(i) / 48.;
            let a = source.sample(7. - 4. * t).unwrap();
            let b = dest.sample(9. + 4. * t).unwrap();
            for (a, b) in a.iter().zip(b) {
                assert!((a - b).abs() < 0.0002, "curve={curve:?} t={t} {a} != {b}");
            }
        }
        assert_eq!(
            dest.keys[0].spatial.as_ref().unwrap().outgoing,
            source.keys[1].spatial.as_ref().unwrap().incoming
        );
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
    }
}
#[test]
fn unrepresentable_reversed_steps_and_elastic_curves_are_rejected_without_partial_results() {
    for curve in [
        None,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Elastic {
                oscillations: 2.,
                damping: 5.,
            },
        }),
    ] {
        let mut p = fixture().snapshot();
        p.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            TimeDirection::Reverse,
            0.,
            20.,
        )
        .unwrap();
        p.layers[0].transform.position.keys.remove(0);
        p.layers[0].transform.position.keys[0].ease = Ease::Hold;
        p.layers[0].transform.position.keys[0].curve = curve;
        let mut e = Engine::new(p).unwrap();
        let clipboard =
            KeyframeClipboard::capture_mixed(e.project(), &[], &[local(7, &[3, 7])]).unwrap();
        reject(
            &mut e,
            paste(clipboard, vec![target(9)], Some(33.5)),
            if curve.is_some() {
                "elastic"
            } else {
                "opposite-sided step"
            },
        );
    }
}
