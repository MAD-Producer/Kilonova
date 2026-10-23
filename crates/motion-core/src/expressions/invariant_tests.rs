use super::*;

#[test]
fn expression_context_preserves_original_io_identity_and_os_code() {
    let target = ExpressionTarget::Effect { object: 7, effect: 11, param: "amount".into() };
    let cause = std::io::Error::from_raw_os_error(2);
    let original = cause.to_string();
    let wrapped = error(&target, 5.5, Error::Io(cause));
    let Error::Expression { source, frame, target: metadata } = &wrapped else {
        panic!("missing expression context: {wrapped:?}");
    };
    assert_eq!(*frame, 5.5);
    assert_eq!(serde_json::from_str::<serde_json::Value>(metadata).unwrap(), serde_json::to_value(&target).unwrap());
    assert!(matches!(source.as_ref(), Error::Io(cause) if cause.raw_os_error() == Some(2)));
    assert_eq!(std::error::Error::source(&wrapped).unwrap().to_string(), original);
    assert_eq!(wrapped.to_string(), format!("expression {metadata} at frame 5.5: {original}"));
}

#[test]
fn effect_write_refuses_missing_owner_instance_and_parameter_without_mutation() {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|e| e.id == "tint").unwrap();
    let mut project = Project::new(100, 100, 30, 120).unwrap();
    let mut layer = crate::Layer::solid(7, "Expression guard", [20.; 2], [10., 20., 0.], [1.; 4]);
    layer.effects.push(crate::EffectInstance::new(
        11, &package.manifest.id, &package.manifest.version, &package.hash, definition, [100.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let original = project.clone();
    for (object, effect, param) in [(999, 11, "missing"), (7, 999, "missing"), (7, 11, "missing")] {
        let target = ExpressionTarget::Effect { object, effect, param: param.into() };
        let failure = write(&mut project, &target, 5.5, &[1.]).unwrap_err();
        if object == 999 {
            assert!(matches!(failure, Error::Missing(999)), "{failure:?}");
        } else {
            assert!(matches!(&failure, Error::Invalid(cause) if cause == "expression effect parameter does not exist"), "{failure:?}");
        }
        assert_eq!(project, original);
    }
}
