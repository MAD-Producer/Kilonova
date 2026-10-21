use motion_core::{render_sampling::*, vector::*, *};

fn fixture() -> Project {
    let mut p = Project::new(128, 128, 30, 60).unwrap();
    let vector = VectorContent::shape(ShapeKind::Ellipse);
    let mut layer = Layer::solid(1, "Vector", [24.; 2], [64., 64., 0.], [1.; 4]);
    layer.content = Content::Vector { vector };
    p.layers.push(layer);
    p
}
fn continuous() -> LayerSampling {
    LayerSampling {
        continuous_rasterization: true,
        ..Default::default()
    }
}

#[test]
fn current_settings_are_explicit_and_roundtrip_with_undo() {
    let original = fixture();
    let bytes = serde_json::to_vec(&original).unwrap();
    let migrated: Project = serde_json::from_slice::<Project>(&bytes)
        .unwrap();
    assert_eq!(serde_json::to_vec(&migrated).unwrap(), bytes);
    assert!(serde_json::to_value(&migrated).unwrap()["layers"][0]
        .as_object()
        .unwrap()
        .contains_key("sampling"));
    let mut e = Engine::new(migrated).unwrap();
    let command = Command::SetLayerSampling {
        object: 1,
        sampling: continuous(),
    };
    let parsed = parse_commands(&serde_json::to_string(&command).unwrap()).unwrap();
    e.apply_batch(parsed).unwrap();
    assert_eq!(e.project().version, motion_model::CURRENT_PROJECT_VERSION);
    assert_eq!(e.project().layers[0].sampling, continuous());
    let folder = tempfile::tempdir().unwrap();
    storage::save(folder.path(), e.project()).unwrap();
    let frozen = storage::load(folder.path()).unwrap();
    assert_eq!(&frozen, e.project());
    e.undo().unwrap();
    assert_eq!(e.project(), &original);
    e.redo().unwrap();
    assert_eq!(e.project(), &frozen);
    e.begin_gesture().unwrap();
    e.apply(Command::SetLayerSampling {
        object: 1,
        sampling: Default::default(),
    })
    .unwrap();
    e.end_gesture(false).unwrap();
    assert_eq!(e.project(), &frozen);
}

#[test]
fn add_retains_current_format_and_invalid_locked_or_nonvisual_targets_are_atomic() {
    let mut e = Engine::new(fixture()).unwrap();
    let before = e.project().clone();
    for sampling in [
        LayerSampling {
            frame_blend: FrameBlend::Linear,
            ..Default::default()
        },
        LayerSampling {
            motion_blur: Some(Shutter {
                frames: f64::NAN,
                ..Default::default()
            }),
            ..Default::default()
        },
        LayerSampling {
            motion_blur: Some(Shutter {
                samples: 1,
                ..Default::default()
            }),
            ..Default::default()
        },
    ] {
        assert!(e
            .apply(Command::SetLayerSampling {
                object: 1,
                sampling
            })
            .is_err());
        assert_eq!(e.project(), &before);
    }
    let mut layer = before.layers[0].clone();
    layer.id = 2;
    layer.sampling = continuous();
    e.apply(Command::Add { layer }).unwrap();
    assert_eq!(e.project().version, motion_model::CURRENT_PROJECT_VERSION);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let mut p = before.clone();
    p.layers[0].locked = true;
    let mut locked = Engine::new(p.clone()).unwrap();
    assert!(matches!(
        locked.apply(Command::SetLayerSampling {
            object: 1,
            sampling: continuous()
        }),
        Err(Error::Locked(1))
    ));
    assert_eq!(locked.project(), &p);
    for content in [
        Content::Null,
        Content::Audio {
            audio: AudioClip::new(1),
        },
        Content::Adjustment,
    ] {
        assert!(LayerSampling {
            motion_blur: Some(Default::default()),
            ..Default::default()
        }
        .validate_content(&content)
        .is_err());
        assert!(continuous().validate_content(&content).is_err());
    }
    let mut downgraded = before;
    downgraded.layers[0].sampling = continuous();
    downgraded.validate().unwrap();
    downgraded.version = 20;
    assert!(downgraded.validate().unwrap_err().to_string().contains("unsupported project format"));
}
