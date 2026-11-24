use super::*;

#[test]
fn image_spaces_resolve_roles_and_do_not_depend_on_unrelated_policy() {
    let config = verified(manifest());
    let direct = config.space_identity(&space("linear")).unwrap();
    assert_eq!(
        direct,
        config.space_identity(&role("scene_linear")).unwrap()
    );
    assert_eq!(direct.config_key(), config.key());
    assert_ne!(direct, config.space_identity(&space("log")).unwrap());
    let mut policy = selection(&config);
    let first_selection = config.resolve(&policy).unwrap();
    policy.display.view = "alternate".into();
    assert_ne!(first_selection.key, config.resolve(&policy).unwrap().key);
    assert_eq!(direct, config.space_identity(&policy.working).unwrap());
    let normal = config
        .display_identity(&selection(&config).display)
        .unwrap();
    assert_ne!(normal, config.display_identity(&policy.display).unwrap());
    assert_ne!(normal, config.space_identity(&space("encoded")).unwrap());

    let mut other = manifest();
    other.context.insert("SHOT".into(), "different".into());
    let other = verified(other);
    assert_ne!(direct.config_key(), other.key());
    assert_ne!(direct, other.space_identity(&space("linear")).unwrap());
}

#[test]
fn data_missing_spaces_and_views_keep_the_original_config_error() {
    let config = verified(manifest());
    assert!(matches!(config.space_identity(&role("data")),
        Err(ColorConfigError::DataSpace { name }) if name == "data"));
    assert!(matches!(config.space_identity(&space("absent")),
        Err(ColorConfigError::Missing { kind: "space", name }) if name == "absent"));
    assert!(matches!(config.space_identity(&role("absent")),
        Err(ColorConfigError::Missing { kind: "role", name }) if name == "absent"));
    let missing = DisplayView {
        display: "monitor".into(),
        view: "absent".into(),
    };
    assert!(matches!(config.display_identity(&missing),
        Err(ColorConfigError::Missing { kind: "view", name }) if name == "absent"));
    assert!(config.space_identity(&space("linear")).is_ok());
    assert!(config.display_identity(&selection(&config).display).is_ok());
}
