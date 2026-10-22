use motion_core::{Command, Content, Ease, Engine, Layer, LayerTimeline, Project, Property, Track};
use motion_render::Scene;

fn fixture() -> Project {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut parent = Layer::solid(1, "Parent", [1.; 2], [250., 150., 0.], [0.; 4]);
    parent.content = Content::Null;
    parent.transform.rotation = Track::constant([0., 0., 17.]);
    parent.transform.scale = Track::constant([120., 80., 100.]);
    let mut child = Layer::solid(
        2,
        "Child",
        [160., 90.],
        [320., 180., 0.],
        [0.2, 0.5, 0.8, 1.],
    );
    child.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 100,
        offset_frame: 10,
    });
    p.layers = vec![parent, child];
    p
}
fn model(p: &Project, frame: f64) -> glam::Mat4 {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s.layers.iter().find(|l| l.id == 2).unwrap().model
}
fn close(a: glam::Mat4, b: glam::Mat4) {
    for (x, y) in a.to_cols_array().into_iter().zip(b.to_cols_array()) {
        assert!((x - y).abs() < 0.002, "matrix mismatch {a:?} {b:?}");
    }
}
fn animate(e: &mut Engine) {
    e.apply(Command::Animate {
        object: 2,
        property: Property::Anchor,
        axis: None,
        frame: 10,
        enabled: true,
    })
    .unwrap();
    e.apply(Command::SetVector {
        object: 2,
        property: Property::Anchor,
        frame: 50,
        value: [0.1, 0.9, 0.],
    })
    .unwrap();
}
#[test]
fn static_anchor_array_keeps_format_pose_and_noncentral_pivot() {
    let mut p = fixture();
    p.layers[1].transform.anchor = Track::constant([0.25, 0.8]);
    let value = serde_json::to_value(&p).unwrap();
    assert_eq!(
        value["layers"][1]["transform"]["anchor"],
        serde_json::json!([0.25f32, 0.8f32])
    );
    let migrated = serde_json::from_value::<Project>(value.clone())
        .unwrap();
    assert_eq!(serde_json::to_value(&migrated).unwrap(), value);
    for frame in [10., 30.5, 80.] {
        close(model(&p, frame), model(&migrated, frame));
    }
    let before = model(&p, 20.);
    let mut e = Engine::new(p).unwrap();
    e.apply(Command::Animate {
        object: 2,
        property: Property::Anchor,
        axis: None,
        frame: 20,
        enabled: true,
    })
    .unwrap();
    assert_eq!(e.project().version, motion_model::CURRENT_PROJECT_VERSION);
    close(before, model(e.project(), 20.));
}
#[test]
fn anchor_uses_local_time_and_survives_save_reopen_and_frozen_snapshot() {
    let mut e = Engine::new(fixture()).unwrap();
    animate(&mut e);
    assert_eq!(
        e.project().layers[1]
            .transform
            .anchor
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [0, 40]
    );
    let a = e.project().layers[1].transform.anchor.sample(20.).unwrap();
    assert!((a[0] - 0.3).abs() < 1e-6 && (a[1] - 0.7).abs() < 1e-6);
    let timeline = e.project().timeline_layers(30.).unwrap();
    assert_eq!(
        timeline[1]
            .properties
            .anchor
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [10, 50]
    );
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), e.project()).unwrap();
    let frozen = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(&frozen, e.project());
    for frame in [10., 20., 30.25, 50., 80.] {
        close(model(&frozen, frame), model(e.project(), frame));
    }
    let old = model(&frozen, 30.);
    e.apply(Command::SetVector {
        object: 2,
        property: Property::Anchor,
        frame: 30,
        value: [0.9, 0.1, 0.],
    })
    .unwrap();
    assert_ne!(model(e.project(), 30.), old);
    close(model(&frozen, 30.), old);
}
#[test]
fn animated_anchor_compensation_preserves_parented_separated_pose_and_cancel_undo() {
    let mut e = Engine::new(fixture()).unwrap();
    e.apply(Command::Parent {
        object: 2,
        parent: Some(1),
        frame: 10,
    })
    .unwrap();
    e.apply(Command::SeparateDimensions {
        object: 2,
        property: Property::Position,
    })
    .unwrap();
    e.apply(Command::Animate {
        object: 2,
        property: Property::Rotation,
        axis: None,
        frame: 10,
        enabled: true,
    })
    .unwrap();
    e.apply(Command::SetVector {
        object: 2,
        property: Property::Rotation,
        frame: 50,
        value: [0., 0., 63.],
    })
    .unwrap();
    animate(&mut e);
    let before = e.snapshot();
    let pose = model(&before, 30.);
    let anchor = before.layers[1].transform.anchor.sample(20.).unwrap();
    e.begin_gesture().unwrap();
    for a in [[0.35, 0.8], [0.6, 0.2], anchor] {
        e.apply(Command::AnchorAt {
            object: 2,
            frame: 30,
            anchor: a,
        })
        .unwrap();
        close(pose, model(e.project(), 30.));
    }
    assert!(e.project().layers[1].transform.position.axes.is_some());
    e.end_gesture(false).unwrap();
    assert_eq!(e.project(), &before);
    e.apply(Command::AnchorAt {
        object: 2,
        frame: 30,
        anchor: [0.8, 0.2],
    })
    .unwrap();
    close(pose, model(e.project(), 30.));
    let after = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    e.redo().unwrap();
    assert_eq!(e.project(), &after);
}
#[test]
fn anchor_keys_move_copy_ease_disable_and_reject_invalid_edits_atomically() {
    let mut e = Engine::new(fixture()).unwrap();
    animate(&mut e);
    e.apply(Command::CopyKey {
        object: 2,
        property: Property::Anchor,
        axis: None,
        from: 50,
        to: 70,
    })
    .unwrap();
    e.apply(Command::MoveKey {
        object: 2,
        property: Property::Anchor,
        axis: None,
        from: 70,
        to: 80,
    })
    .unwrap();
    e.apply(Command::Ease {
        object: 2,
        property: Property::Anchor,
        axis: None,
        frame: 50,
        ease: Ease::Hold,
    })
    .unwrap();
    assert_eq!(
        e.project().layers[1].transform.anchor.sample(50.).unwrap(),
        [0.1, 0.9]
    );
    for command in [
        Command::SetVector {
            object: 2,
            property: Property::Anchor,
            frame: 30,
            value: [1001., 0., 0.],
        },
        Command::SetVector {
            object: 2,
            property: Property::Anchor,
            frame: 30,
            value: [0., 0., 1.],
        },
        Command::AnchorAt {
            object: 2,
            frame: 120,
            anchor: [0.; 2],
        },
        Command::SeparateDimensions {
            object: 2,
            property: Property::Anchor,
        },
        Command::SetComponent {
            object: 2,
            property: Property::Anchor,
            axis: motion_core::Axis::X,
            frame: 30,
            value: 0.2,
        },
    ] {
        let before = e.snapshot();
        assert!(e.apply(command).is_err());
        assert_eq!(e.project(), &before);
    }
    e.apply(Command::Animate {
        object: 2,
        property: Property::Anchor,
        axis: None,
        frame: 30,
        enabled: false,
    })
    .unwrap();
    assert!(!e.project().layers[1].transform.anchor.is_animated());
    let json = serde_json::to_value(e.project()).unwrap();
    assert!(json["layers"][1]["transform"]["anchor"].is_array());
    let mut invalid = fixture();
    invalid.layers[1]
        .transform
        .anchor
        .upsert(0, [0.5; 2], Ease::Linear)
        .unwrap();
    invalid.validate().unwrap();
    invalid.version = 19;
    assert!(invalid.validate().unwrap_err().to_string().contains("unsupported project format"));
}
