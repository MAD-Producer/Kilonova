//! Composition coordinates must reach continuous spatial storage through the
//! same property clock as sampling. Stable IDs never encode displayed frames.
use motion_core::{
    layer_time::*, storage, Command, Ease, Engine, KeyframeTarget, Layer, LayerTimeline, Project,
    Property, SpatialKeyEdit,
};

fn fixture(reverse: bool) -> Engine {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Spatial clock", [32.; 2], [0.; 3], [1.; 4]);
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
            if reverse { 0. } else { -12. },
        )
        .unwrap(),
        remap: None,
    });
    for (frame, x) in [(-12, 0.), (-9, 1.), (-6, 7.), (0, 10.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}

fn command(edit: SpatialKeyEdit) -> Command {
    Command::SpatialKeyframes {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        },
        edit,
    }
}
fn clock(p: &Project) -> LayerClock {
    p.layers[0].time_mapping.as_ref().unwrap().clock
}
fn spatial_engine(reverse: bool) -> Engine {
    let mut p = fixture(reverse).snapshot();
    p.layers[0].transform.position.enable_roving(&[-9]).unwrap();
    Engine::new(p).unwrap()
}
fn roundtrip(p: &Project) {
    let temp = tempfile::tempdir().unwrap();
    storage::save(temp.path(), p).unwrap();
    assert_eq!(storage::load(temp.path()).unwrap(), *p);
    let package = temp.path().join("spatial.msproj");
    storage::export_package(temp.path(), p, &package).unwrap();
    assert_eq!(
        storage::import_package(&package, &temp.path().join("import")).unwrap(),
        *p
    );
}

#[test]
fn composition_conversion_finds_signed_local_keys_in_both_directions() {
    for reverse in [false, true] {
        let mut p = fixture(reverse).snapshot();
        // Use exactly projectable integer source keys. Fractional composition
        // projections require a signed-local source address, not an integer cast.
        for (key, frame) in p.layers[0]
            .transform
            .position
            .keys
            .iter_mut()
            .zip([-12, -8, -4, 0])
        {
            key.frame = frame;
        }
        let mut e = Engine::new(p).unwrap();
        let before = e.snapshot();
        // Both source keys have integer composition projections, in the reverse
        // order when the layer plays backwards. The model keeps authored order.
        let mut frames = [-8., -4.].map(|local| {
            let frame = clock(&before).composition_frame(local, 2).unwrap();
            assert_eq!(frame.fract(), 0.);
            frame as u32
        });
        frames.sort();
        e.apply(command(SpatialKeyEdit::Enable {
            frames: frames.to_vec(),
        }))
        .unwrap();
        let mut expected = before.clone();
        expected.layers[0]
            .transform
            .position
            .enable_roving(&[-8, -4])
            .unwrap();
        assert_eq!(e.project(), &expected);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        e.redo().unwrap();
        assert_eq!(e.project(), &expected);
        roundtrip(&expected);
    }
}

#[test]
fn continuous_time_edits_use_property_clock_and_preserve_complete_history() {
    for reverse in [false, true] {
        let mut e = spatial_engine(reverse);
        let before = e.snapshot();
        let composition = clock(&before).composition_frame(-10.25, 2).unwrap();
        e.apply(command(SpatialKeyEdit::Time {
            key: 2,
            time: composition,
        }))
        .unwrap();
        let mut expected = before.clone();
        expected.layers[0]
            .transform
            .position
            .spatial_animation
            .as_mut()
            .unwrap()
            .set_fixed_time(2, -10.25)
            .unwrap();
        assert_eq!(e.project(), &expected);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        e.redo().unwrap();
        assert_eq!(e.project(), &expected);
        roundtrip(&expected);
    }
}

#[test]
fn group_move_scales_and_reverses_delta_without_changing_spatial_values_or_ids() {
    for reverse in [false, true] {
        let mut e = spatial_engine(reverse);
        let before = e.snapshot();
        let delta = 1.125;
        e.apply(command(SpatialKeyEdit::Move {
            keys: vec![1, 2, 3, 4],
            delta,
        }))
        .unwrap();
        let mut expected = before.clone();
        expected.layers[0]
            .transform
            .position
            .spatial_animation
            .as_mut()
            .unwrap()
            .move_keys(&[1, 2, 3, 4], if reverse { -0.75 } else { 0.75 })
            .unwrap();
        assert_eq!(e.project(), &expected);
        // The same composition shift preserves sampling, independently of the
        // property clock, without remapping IDs through integer frames.
        for local in [-12., -10.125, -9., -6., -0.5, 0.] {
            let composition = clock(&before).composition_frame(local, 2).unwrap();
            let shifted = clock(&expected)
                .local_frame(composition + delta, 2)
                .unwrap();
            assert_eq!(
                before.layers[0].transform.position.sample(local).unwrap(),
                expected.layers[0]
                    .transform
                    .position
                    .sample(shifted)
                    .unwrap()
            );
        }
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        e.redo().unwrap();
        assert_eq!(e.project(), &expected);
        roundtrip(&expected);
    }
}

#[test]
fn exact_projected_fixed_time_does_not_drift_on_inverse_roundoff() {
    let mut p = spatial_engine(false).snapshot();
    p.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
        TimeStretch::new(7, 3).unwrap(),
        TimeDirection::Forward,
        0.1,
        -12.25,
    )
    .unwrap();
    let local = -5.123456789;
    p.layers[0]
        .transform
        .position
        .spatial_animation
        .as_mut()
        .unwrap()
        .set_fixed_time(3, local)
        .unwrap();
    let composition = clock(&p).projected_composition_frame(local, 2).unwrap();
    assert_ne!(
        clock(&p).local_frame(composition, 2).unwrap(),
        local,
        "fixture must expose inverse roundoff"
    );
    let mut e = Engine::new(p.clone()).unwrap();
    let history = e.history_diagnostics();
    e.apply(command(SpatialKeyEdit::Time {
        key: 3,
        time: composition,
    }))
    .unwrap();
    assert_eq!(e.project(), &p);
    assert_eq!(e.history_diagnostics(), history);
}

#[test]
fn failed_mapped_edits_preserve_project_revision_redo_and_batch_atomicity() {
    for reverse in [false, true] {
        let mut e = spatial_engine(reverse);
        let frame = clock(e.project()).composition_frame(-10.25, 2).unwrap();
        e.apply(command(SpatialKeyEdit::Time {
            key: 2,
            time: frame,
        }))
        .unwrap();
        let redo = e.snapshot();
        e.undo().unwrap();
        let before = e.snapshot();
        let revision = e.revision();
        let history = e.history_diagnostics();
        for edit in [
            SpatialKeyEdit::Time {
                key: 999,
                time: frame,
            },
            SpatialKeyEdit::Time {
                key: 2,
                time: f64::NAN,
            },
            SpatialKeyEdit::Time { key: 2, time: 1e20 },
            SpatialKeyEdit::Move {
                keys: vec![2, 999],
                delta: 1.125,
            },
            SpatialKeyEdit::Move {
                keys: vec![2],
                delta: f64::INFINITY,
            },
            SpatialKeyEdit::Move {
                keys: vec![2],
                delta: 100.,
            },
        ] {
            let error = e
                .apply_batch(vec![
                    command(SpatialKeyEdit::Time {
                        key: 2,
                        time: frame,
                    }),
                    command(edit),
                ])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("operation=spatial_keyframes") && error.contains("object=7"),
                "{error}"
            );
            assert_eq!(e.project(), &before);
            assert_eq!(e.revision(), revision);
            assert_eq!(e.history_diagnostics(), history);
        }
        e.redo().unwrap();
        assert_eq!(e.project(), &redo);
        let mut p = fixture(reverse).snapshot();
        p.layers[0].time_mapping.as_mut().unwrap().clock = LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            TimeDirection::Forward,
            0.,
            -12.25,
        )
        .unwrap();
        let mut e = Engine::new(p.clone()).unwrap();
        let error = e
            .apply(command(SpatialKeyEdit::Enable { frames: vec![7] }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("integer_key"), "{error}");
        assert_eq!(e.project(), &p);
        assert!(!e.undo().unwrap());
    }
}
