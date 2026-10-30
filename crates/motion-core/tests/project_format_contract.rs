use motion_core::{
    compositing::{LayerBlend, MatteMode, TrackMatte},
    masks::LayerMask,
    render_sampling::LayerSampling,
    vector::VectorPath,
    *,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::fmt::Debug;

fn required<T>(value: T, fields: &[&str])
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(serde_json::from_value::<T>(encoded.clone()).unwrap(), value);
    for field in fields {
        let mut missing = encoded.clone();
        assert!(
            missing.as_object_mut().unwrap().remove(*field).is_some(),
            "writer omitted {field}"
        );
        let error = serde_json::from_value::<T>(missing).unwrap_err();
        assert!(error.to_string().contains(field), "{field}: {error}");
    }
}

#[test]
fn current_project_and_visual_settings_are_required_even_at_constructor_defaults() {
    required(Project::demo().unwrap(), &["version", "composition_id"]);
    required(
        Project::demo().unwrap().layers.remove(0),
        &[
            "three_d",
            "sampling",
            "rotation_order",
            "blend",
            "audio_source_channel",
        ],
    );
    required(Camera::new(64, 64), &["created", "film_width_mm"]);
    required(
        LayerSampling::default(),
        &["frame_blend", "continuous_rasterization"],
    );
    required(LayerBlend::default(), &["mode", "space"]);
    required(
        TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
            hide_source: true,
        },
        &["mode", "hide_source"],
    );
    required(
        LayerMask::new(
            1,
            VectorPath {
                tangents: Default::default(), id: 1,
                closed: false,
                nodes: vec![],
            },
        ),
        &[
            "mode",
            "enabled",
            "inverted",
            "opacity",
            "feather",
            "expansion",
        ],
    );
}

#[test]
fn animation_and_effect_modes_must_not_be_invented_by_deserialization() {
    required(
        Keyframe {
            auto_bezier: false,
            frame: 0,
            value: 1.0_f32,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        },
        &["ease"],
    );
    required(
        Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Elastic {
                oscillations: 2.,
                damping: 6.,
            },
        },
        &["space"],
    );
    required(
        CurveShape::Quadratic {
            control: [0.5, 0.5],
            start: 0.,
            end: 1.,
        },
        &["start", "end"],
    );
    required(
        CurveShape::Cubic {
            control1: [0.2, 0.2],
            control2: [0.8, 0.8],
            start: 0.,
            end: 1.,
        },
        &["start", "end"],
    );
    required(Easing::default(), &["ease"]);
    required(CurveObject::default(), &["interpolation"]);
    required(
        EffectImageInput::Layer {
            layer: 1,
            stage: EffectImageStage::Source,
        },
        &["stage"],
    );
    required(
        PropertyExpression {
            target: ExpressionTarget::Property {
                object: 1,
                property: Property::Position,
                axis: None,
            },
            source: "value".into(),
            enabled: true,
            seed: 0,
            profile: EXPRESSION_PROFILE.into(),
        },
        &["seed"],
    );
}

#[test]
fn audio_video_and_composition_playback_settings_are_required() {
    required(AudioClip::new(1), &["volume", "muted"]);
    required(VideoClip::new(1), &["source_offset_us", "volume", "muted"]);
    required(
        CompositionClip::new(MAIN_COMPOSITION.into()),
        &["source_start_frame", "volume", "muted"],
    );
    required(
        AudioMarker {
            id: 1,
            frame: 0,
            label: String::new(),
            generated: false,
            corrected: false,
        },
        &["generated", "corrected"],
    );
}

#[test]
fn missing_optional_content_means_none_or_empty_without_changing_render_settings() {
    let original = Project::demo().unwrap();
    let mut encoded = serde_json::to_value(&original).unwrap();
    for field in [
        "compositions",
        "fonts",
        "audio_assets",
        "video_assets",
        "plugin_dependencies",
        "expressions",
    ] {
        encoded.as_object_mut().unwrap().remove(field);
    }
    for layer in encoded["layers"].as_array_mut().unwrap() {
        for field in [
            "parent",
            "effects",
            "masks",
            "track_matte",
            "timeline",
            "subframe_range",
            "audio_levels",
            "audio_markers",
        ] {
            layer.as_object_mut().unwrap().remove(field);
        }
        layer["sampling"]
            .as_object_mut()
            .unwrap()
            .remove("motion_blur");
    }
    encoded["camera"].as_object_mut().unwrap().remove("parent");
    let decoded: Project = serde_json::from_value(encoded).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, original);

    let curve = CurveObject::default();
    let mut value = serde_json::to_value(&curve).unwrap();
    value.as_object_mut().unwrap().remove("channel_luts");
    value.as_object_mut().unwrap().remove("sampled_lut");
    assert_eq!(serde_json::from_value::<CurveObject>(value).unwrap(), curve);

    // A partial command preserves the unmentioned setting; it is not a project fallback.
    let commands =
        parse_commands(&json!({"op":"set_audio", "object":1, "muted":true}).to_string()).unwrap();
    let [Command::InComposition {
        composition,
        command,
    }] = commands.as_slice()
    else {
        panic!("partial command lost its composition routing");
    };
    assert_eq!(composition, MAIN_COMPOSITION);
    assert!(matches!(
        command.as_ref(),
        Command::SetAudio {
            object: 1,
            volume: None,
            muted: Some(true)
        }
    ));
}

#[test]
fn missing_version_is_an_error_and_current_version_is_written_explicitly() {
    let p = Project::new(64, 64, 30, 60).unwrap();
    let mut value: Value = serde_json::to_value(&p).unwrap();
    assert_eq!(value["version"], CURRENT_PROJECT_VERSION);
    value.as_object_mut().unwrap().remove("version");
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("project.json");
    let bytes = serde_json::to_vec(&value).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(storage::load(root.path())
        .unwrap_err()
        .to_string()
        .contains("version"));
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(!root.path().join("project.json.tmp").exists());
}
