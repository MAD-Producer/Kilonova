use super::*;
#[allow(dead_code)]
#[path = "../../../tests/support/import_fixture.rs"]
mod fixture;
mod mask_fixture {
    pub fn scene() -> (motion_core::Project, ()) {
        let mut p = motion_core::Project::new(64, 64, 30, 60).unwrap();
        p.layers.push(motion_core::Layer::solid(
            7,
            "coverage",
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        ));
        (p, ())
    }
    pub fn mask(id: u64) -> motion_model::masks::SampledMask {
        motion_model::masks::SampledMask {
            id,
            nodes: [[8., 8.], [24., 8.], [24., 24.], [8., 24.]]
                .into_iter()
                .map(|p| [p[0], p[1], 0., 0., 0., 0.])
                .collect(),
            mode: motion_model::masks::MaskMode::Add,
            inverted: false,
            opacity: 1.,
            feather: [0.; 2],
            expansion: 0.,
        }
    }
}

fn sample(p: &motion_core::Project) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}

#[test]
fn private_instances_keep_distinct_sources_and_shared_companion_edges() {
    let package = fixture::dynamic_package();
    let mut p = fixture::project(&package, ChannelPrecision::Float16);
    for (i, effect) in p.layers[0].effects.iter_mut().enumerate() {
        let mut values = package.manifest.effects[0]
            .shader_data
            .as_ref()
            .unwrap()
            .values(Some(&package.files["assets/pixels.png"]))
            .unwrap();
        values.texture[0] = (i + 1) as f32 * 0.25;
        effect.shader_data = Some(Arc::new(values));
    }
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    let mut builder = PlanBuilder::new(registry).unwrap();
    builder.set_precision(ChannelPrecision::Float16);
    builder.build(&sample(&p), &[0], 30, 24, true).unwrap();
    let execution = builder.effect_execution().unwrap();
    let packages: Vec<_> = execution
        .bindings()
        .iter()
        .filter_map(|binding| match binding.imported_image.as_ref() {
            Some(ImportedImage::Package(image)) => Some((binding, image)),
            _ => None,
        })
        .collect();
    assert_eq!(
        packages.len(),
        1,
        "unused package defaults are not sampled edges"
    );
    assert_eq!(packages[0].1.path(), "assets/companion.png");
    let package_value = ImageValueId(packages[0].0.node.0);
    assert_eq!(
        execution
            .checked()
            .lifetimes()
            .iter()
            .find(|l| l.value == package_value)
            .unwrap()
            .consumer_count,
        2
    );
    for token in [1, 2] {
        let import = execution
            .frame_import(FrameImageSource::InstanceData(token))
            .unwrap();
        assert_eq!(import.value_type().rect.size, [256, 1]);
        assert_eq!(import.storage(), ImageStorage::Rgba8Unorm);
        assert_eq!(
            import.encoding(),
            ResourceEncoding::LittleEndianF32 { scalars: 1 }
        );
        let source = execution
            .plan()
            .nodes
            .iter()
            .find(|n| {
                n.output_type == import.value_type()
                    && execution.bindings()[n.id.0 as usize]
                        .imported_image
                        .as_ref()
                        == Some(&ImportedImage::Frame(import.clone()))
            })
            .unwrap();
        assert_eq!(source.semantics.time, TimeDependency::CurrentSample);
        let consumer = execution
            .plan()
            .nodes
            .iter()
            .find(|n| n.origin.effect == Some(10 + token as u64))
            .unwrap();
        for (index, value) in [(0, source.output), (1, package_value)] {
            assert!(consumer
                .inputs
                .iter()
                .any(|p| p.role == ImagePortRole::Resource { index } && p.value == value));
        }
    }
    assert_eq!(execution.native_tasks().len(), 2);
}

#[test]
fn curves_and_mask_coverage_keep_their_actual_precision_and_coordinate_contracts() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let (mut p, _) = mask_fixture::scene();
        let package = motion_effects::builtin::package().unwrap();
        let def = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == "curves")
            .unwrap();
        let size = p.layers[0].size;
        p.layers[0].effects.push(motion_core::EffectInstance::new(
            13,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            size,
        ));
        p.rebuild_plugin_dependencies();
        let mut scene = sample(&p);
        scene.layers[0].masks = Arc::new(vec![mask_fixture::mask(19)]);
        let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
        builder.set_precision(precision);
        builder.build(&scene, &[0], 64, 64, true).unwrap();
        let execution = builder.effect_execution().unwrap();
        let curve = execution.frame_import(FrameImageSource::Curve(0)).unwrap();
        assert_eq!(curve, &FrameImageImport::curve(0, precision));
        assert_eq!(curve.value_type().coordinates, CoordinateDomain::Resource);
        assert_eq!(
            curve.value_type().image.meaning,
            ImageMeaning::Data {
                channels: DataChannels::Vector4
            }
        );
        assert_eq!(
            curve.value_type().image.precision,
            if precision == ChannelPrecision::Unorm8 {
                precision
            } else {
                ChannelPrecision::Float32
            }
        );
        let mask = execution.frame_import(FrameImageSource::Mask(0)).unwrap();
        assert_eq!(
            mask.value_type().image,
            ImageContract::new(ImageMeaning::Coverage {}, precision)
        );
        assert_eq!(mask.storage(), coverage_storage(precision));
        assert_eq!(
            mask.value_type().coordinates,
            CoordinateDomain::Layer { layer: 7 }
        );
        assert_eq!(mask.value_type().rect.size, [32, 32]);
        for (source, slot) in [
            (FrameImageSource::Curve(0), 4),
            (FrameImageSource::Mask(0), 6),
        ] {
            let node = execution
                .bindings()
                .iter()
                .find(|b| {
                    matches!(b.imported_image.as_ref(),
                Some(ImportedImage::Frame(i)) if i.source() == source)
                })
                .unwrap()
                .node;
            assert!(execution.plan().nodes.iter().any(|n| {
                n.inputs.iter().any(|p| {
                    p.role == ImagePortRole::Resource { index: slot } && p.value.0 == node.0
                })
            }));
        }
    }
}

#[test]
fn missing_explicit_curve_source_refuses_before_native_allocation_and_recovers() {
    let (mut p, _) = mask_fixture::scene();
    let package = motion_effects::builtin::package().unwrap();
    let def = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "curves")
        .unwrap();
    let size = p.layers[0].size;
    p.layers[0].effects.push(motion_core::EffectInstance::new(
        13,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        def,
        size,
    ));
    p.rebuild_plugin_dependencies();
    let mut scene = sample(&p);
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.set_precision(ChannelPrecision::Float16);
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    let sources = std::mem::take(&mut scene.curve_sources);
    let error = builder.build(&scene, &[0], 64, 64, true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("high-precision curve sources=0 do not match LUTs=1"),
        "{error}"
    );
    assert!(builder.effect_execution().is_none());
    scene.curve_sources = sources;
    builder.build(&scene, &[0], 64, 64, true).unwrap();
    assert!(builder
        .effect_execution()
        .unwrap()
        .frame_import(FrameImageSource::Curve(0))
        .is_some());
}
