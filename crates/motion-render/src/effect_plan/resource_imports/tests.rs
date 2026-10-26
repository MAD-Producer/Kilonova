use super::*;
#[path = "../../../tests/support/import_fixture.rs"]
mod fixture;

fn scene(p: &motion_core::Project) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
#[test]
fn numeric_package_values_have_one_checked_resource_local_producer_and_two_consumers() {
    for private in [false, true] {
        let package = fixture::package(private);
        let p = fixture::project(&package, ChannelPrecision::Float32);
        let mut registry = Registry::new_with_builtins().unwrap();
        registry.insert(package).unwrap();
        let mut builder = PlanBuilder::new(registry).unwrap();
        builder.build(&scene(&p), &[0], 30, 24, true).unwrap();
        let execution = builder.effect_execution().unwrap();
        let imports: Vec<_> = execution
            .bindings()
            .iter()
            .filter(|b| b.imported_image.is_some())
            .collect();
        assert_eq!(imports.len(), 1);
        let binding = imports[0];
        let ImportedImage::Package(import) = binding.imported_image.as_ref().unwrap() else {
            panic!("expected a package import");
        };
        assert_eq!(
            import.value_type.image,
            ImageContract::new(
                ImageMeaning::Data {
                    channels: DataChannels::Vector4
                },
                ChannelPrecision::Unorm8
            )
        );
        assert_eq!(import.value_type.coordinates, CoordinateDomain::Resource);
        assert_eq!(
            import.value_type.rect.size,
            if private { [256, 1] } else { [3, 2] }
        );
        assert_eq!(import.storage, ImageStorage::Rgba8Unorm);
        assert_eq!(
            import.encoding,
            if private {
                ResourceEncoding::LittleEndianF32 { scalars: 1 }
            } else {
                ResourceEncoding::NumericChannels
            }
        );
        let lifetime = execution
            .checked()
            .lifetimes()
            .iter()
            .find(|l| l.value.0 == binding.node.0)
            .unwrap();
        assert_eq!(lifetime.consumer_count, 2);
        assert!(!lifetime.terminal);
        assert!(execution
            .plan()
            .nodes
            .iter()
            .filter(|n| n.origin.effect.is_some())
            .all(|n| n.inputs.iter().any(|p| p.value.0 == binding.node.0)));
        assert_eq!(
            execution.native_tasks().len(),
            2,
            "imports add no fake native draws"
        );
    }
}
#[test]
fn native_import_contract_refuses_changed_bindings_storage_and_extent_and_recovers() {
    let package = fixture::package(false);
    let p = fixture::project(&package, ChannelPrecision::Unorm8);
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    let mut builder = PlanBuilder::new(registry).unwrap();
    let s = scene(&p);
    builder.build(&s, &[0], 30, 24, true).unwrap();
    let program = builder
        .programs
        .iter()
        .position(|p| !p.imports.is_empty())
        .unwrap();
    let imports = builder.programs[program].imports.clone();
    assert!(imports[0]
        .validate_texture(2, 3, ImageStorage::Rgba8Unorm)
        .is_err());
    assert!(imports[0]
        .validate_texture(3, 2, ImageStorage::Rgba8UnormSrgb)
        .is_err());
    builder.programs[program].resources[0] = "assets/other.png".into();
    let e = builder.build(&s, &[0], 30, 24, true).unwrap_err();
    assert!(e.to_string().contains("resource_import"), "{e}");
    assert!(builder.effect_execution().is_none());
    builder.programs[program].resources[0] = imports[0].path.clone();
    builder.build(&s, &[0], 30, 24, true).unwrap();
    let mut changed = imports.to_vec();
    changed[0].storage = ImageStorage::Rgba8UnormSrgb;
    builder.programs[program].imports = changed.into();
    assert!(builder
        .build(&s, &[0], 30, 24, true)
        .unwrap_err()
        .to_string()
        .contains("identity/storage"));
    builder.programs[program].imports = imports;
    builder.build(&s, &[0], 30, 24, true).unwrap();
    assert_eq!(
        builder
            .effect_execution()
            .unwrap()
            .bindings()
            .iter()
            .filter(|b| b.imported_image.is_some())
            .count(),
        1
    );
}

#[test]
fn invalid_package_header_retains_the_original_image_cause_and_bounded_path_context() {
    let mut package = Arc::try_unwrap(fixture::package(false)).ok().unwrap();
    package
        .files
        .insert("assets/pixels.png".into(), vec![0; 24]);
    let error = package_images(&package, &package.manifest.effects[0]).unwrap_err();
    assert!(error.to_string().contains("resource_import_dimensions"));
    assert!(error.to_string().contains("assets/pixels.png"));
    match &error {
        RenderError::Diagnostic { cause, .. } => {
            assert!(matches!(cause.as_ref(), RenderError::Image(_)))
        }
        other => panic!("lost the native image-header cause: {other:?}"),
    }
}
