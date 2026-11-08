use super::*;
use motion_render::{ColorImageRenderSourceError, RenderError, Scene, TextureState};
use std::error::Error;

fn renderer(precision: ChannelPrecision) -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        precision,
        Default::default(),
    ))
    .unwrap()
}

fn scene(pixels: &[[f32; 4]]) -> (Scene, motion_effects::Registry) {
    use motion_core::{EffectInstance, Layer, Project};
    use motion_effects::{EffectPackage, PassDefinition, Registry};

    assert_eq!(pixels.len(), 12);
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.passes = vec![PassDefinition {
        shader: "shaders/native-input.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.ocio-native-source".into();
    manifest.effects = vec![definition];
    let values = pixels
        .iter()
        .map(|v| format!("vec4<f32>({:?},{:?},{:?},{:?})", v[0], v[1], v[2], v[3]))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(directory.path().join("shaders/native-input.wgsl"), format!(
        "fn main_fx(p:vec2<f32>)->vec4<f32>{{let values=array({values}); return values[u32(p.y)*3u+u32(p.x)];}}"
    )).unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut project = Project::new(3, 4, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "Native OCIO source", [3., 4.], [1.5, 2., 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (scene, registry)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_scene_output_enters_ocio_display_and_export_without_copy_or_intermediate_readback() {
    let reference = reference();
    let configs = configs(&reference);
    let mut renderer = renderer(ChannelPrecision::Float32);
    let mut cases = 0;
    for case in reference.cases.iter().filter(|case| {
        case.processor.phase != ColorProcessorPhase::InputToWorking
            && case.processor.input_alpha == AlphaMode::Premultiplied
    }) {
        cases += 1;
        let (scene, registry) = scene(&case.input);
        renderer.set_effect_registry(registry);
        let (config, selection) = &configs[case.config];
        let processor = case.processor.compile_image(config, selection).unwrap();
        let native = renderer.render_target(3, 4).unwrap();
        let mut original = motion_render::TextureResourceLedger::default();
        native.collect_texture_resources(&mut original, "native/source", TextureState::Retained);
        let ids: Vec<_> = original.resources().map(|r| r.id.clone()).collect();
        let mut initial = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        renderer
            .encode_preview(&scene, &native.view, 3, 4, &mut initial, None)
            .unwrap();
        renderer
            .submit_allocated(initial.finish(), |ledger| ledger.merge(original))
            .unwrap();
        let admitted = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let reads = renderer.image_readback_info();
        let source = renderer
            .color_image_from_render_target(native, processor.input_space())
            .unwrap();
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            admitted
        );
        assert_eq!(renderer.image_readback_info(), reads);
        assert_eq!(source.image().readback_bytes(), 0);
        assert_eq!(source.contract().precision, ChannelPrecision::Float32);
        assert_eq!(source.contract().alpha, AlphaMode::Premultiplied);
        assert_eq!(
            source
                .texture_resource_ledger()
                .resources()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        let retained = read(&renderer, &source);
        for (pixel, (got, expected)) in retained.iter().zip(&case.input).enumerate() {
            for channel in 0..4 {
                assert!(
                    (got[channel] - expected[channel]).abs() <= 0.000001,
                    "adoption changed existing native frame case={cases} pixel={pixel} channel={channel}: actual={got:?} expected={expected:?}"
                );
            }
        }
        let output = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    space: processor.output_space(),
                    alpha: processor.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let binding =
            ColorProcessorOutput::new(&renderer, &source, &processor, output.contract()).unwrap();
        for frame in 0..2 {
            let reads = renderer.image_readback_info();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            let stats = renderer
                .encode_preview(&scene, source.image().view(), 3, 4, &mut encoder, None)
                .unwrap();
            assert!(
                stats.draw_calls >= 2,
                "actual native effect and composition must execute"
            );
            binding.encode(&renderer, &mut encoder, &output).unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(source.texture_resource_ledger());
                    ledger.merge(binding.texture_resource_ledger());
                    ledger.merge(output.texture_resource_ledger());
                })
                .unwrap();
            assert_eq!(
                renderer.image_readback_info(),
                reads,
                "native composition and OCIO must not read back between passes"
            );
            let actual = read(&renderer, &output);
            for (pixel, (got, expected)) in actual.iter().zip(&case.expected).enumerate() {
                for channel in 0..4 {
                    let tolerance = reference.absolute_tolerance
                        + reference.relative_tolerance * expected[channel].abs();
                    assert!(got[channel].is_finite() && (got[channel] - expected[channel]).abs() <= tolerance,
                        "case={cases} frame={frame} pixel={pixel} channel={channel}: actual={got:?} expected={expected:?}");
                }
            }
            let actual_source = read(&renderer, &source);
            for (pixel, (got, expected)) in actual_source.iter().zip(&case.input).enumerate() {
                for channel in 0..4 {
                    assert!((got[channel] - expected[channel]).abs() <= 0.000001,
                        "native source case={cases} pixel={pixel} channel={channel}: actual={got:?} expected={expected:?}");
                }
            }
        }
    }
    assert_eq!(cases, 7);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn render_source_refuses_real_nonfloat32_storage_and_foreign_owner_then_recovers() {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    let processor = case.processor.compile_image(config, selection).unwrap();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
    ] {
        let renderer = renderer(precision);
        let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let native = renderer.render_target(3, 4).unwrap();
        let error = renderer
            .color_image_from_render_target(native, processor.input_space())
            .err()
            .unwrap();
        assert!(
            matches!(&error, RenderError::ColorImageRenderSource(ColorImageRenderSourceError { format })
            if *format == motion_render::image_formats::native_format(motion_render::image_formats::color_storage(precision)))
        );
        assert!(error.source().unwrap().is::<ColorImageRenderSourceError>());
        assert_eq!(renderer.image_readback_info().requested_calls, 0);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
        renderer.check_health().unwrap();
    }
    let renderer = renderer(ChannelPrecision::Float32);
    let foreign = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float32,
        renderer.allocation_budget().clone(),
    ))
    .unwrap();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let native = foreign.render_target(3, 4).unwrap();
    let error = renderer
        .color_image_from_render_target(native, processor.input_space())
        .err()
        .unwrap();
    assert!(error.to_string().contains("belongs to another renderer"));
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    let mut native = renderer.render_target(3, 4).unwrap();
    native.width = 999;
    native.height = 0;
    let source = renderer
        .color_image_from_render_target(native, processor.input_space())
        .unwrap();
    assert_eq!(source.image().width(), 3);
    assert_eq!(source.image().height(), 4);
    assert_eq!(source.image().texture_bytes(), 192);
    assert_eq!(read(&renderer, &source), vec![[0.; 4]; 12]);
    drop(source);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn render_source_refuses_replaced_view_without_retaining_foreign_allocation_then_recovers() {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    let processor = case.processor.compile_image(config, selection).unwrap();
    let renderer = renderer(ChannelPrecision::Float32);
    let foreign = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float32,
        renderer.allocation_budget().clone(),
    ))
    .unwrap();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    for other in [&renderer, &foreign] {
        let mut native = renderer.render_target(3, 4).unwrap();
        let mut replacement = other.render_target(3, 4).unwrap();
        std::mem::swap(&mut native.view, &mut replacement.view);
        let error = renderer
            .color_image_from_render_target(native, processor.input_space())
            .err()
            .unwrap();
        assert!(error
            .to_string()
            .contains("view belongs to another texture"));
        drop(replacement);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
        assert_eq!(renderer.image_readback_info().requested_calls, 0);
        renderer.check_health().unwrap();
        foreign.check_health().unwrap();
    }
    let source = renderer
        .color_image_from_render_target(
            renderer.render_target(3, 4).unwrap(),
            processor.input_space(),
        )
        .unwrap();
    assert_eq!(read(&renderer, &source), vec![[0.; 4]; 12]);
    drop(source);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    renderer.check_health().unwrap();
}
