use motion_effects::image_contract::ChannelPrecision;
use motion_model::{CURRENT_PROJECT_VERSION, Project};

#[test]
fn project_precision_is_required_and_preserves_four_distinct_serialized_modes() {
    let initial = Project::new(64, 64, 30, 60).unwrap();
    assert_eq!(initial.working_precision, ChannelPrecision::Unorm8);
    assert_eq!(initial.version, CURRENT_PROJECT_VERSION);
    for (precision, name) in [
        (ChannelPrecision::Unorm8, "unorm8"),
        (ChannelPrecision::Unorm16, "unorm16"),
        (ChannelPrecision::Float16, "float16"),
        (ChannelPrecision::Float32, "float32"),
    ] {
        let mut project = initial.clone();
        project.working_precision = precision;
        project.validate().unwrap();
        assert_eq!(
            project
                .composition_frame_view(&project.composition_id)
                .unwrap()
                .working_precision,
            precision
        );
        let encoded = serde_json::to_value(&project).unwrap();
        assert_eq!(encoded["working_precision"], name);
        assert_eq!(serde_json::from_value::<Project>(encoded).unwrap(), project);
    }
    let mut missing = serde_json::to_value(&initial).unwrap();
    missing.as_object_mut().unwrap().remove("working_precision");
    assert!(
        serde_json::from_value::<Project>(missing)
            .unwrap_err()
            .to_string()
            .contains("working_precision")
    );
    for invalid in [
        serde_json::json!("16bit"),
        serde_json::Value::Null,
        serde_json::json!(1),
    ] {
        let mut encoded = serde_json::to_value(&initial).unwrap();
        encoded["working_precision"] = invalid;
        assert!(serde_json::from_value::<Project>(encoded).is_err());
    }
    let mut old = initial;
    old.version = CURRENT_PROJECT_VERSION - 1;
    assert!(
        old.validate()
            .unwrap_err()
            .to_string()
            .contains("unsupported project format")
    );
}
