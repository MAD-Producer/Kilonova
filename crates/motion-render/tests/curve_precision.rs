use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
};
use motion_model::color_curves::ColorInterpolation;
use motion_model::{CurveObject, Ease, Keyframe};
use motion_render::{Renderer, Scene, TextureCategory};
use std::sync::Arc;

const SIZE: u32 = 17;
const LUT_BYTES: u64 = motion_model::HIGH_PRECISION_CURVE_SAMPLES as u64 * 16;

fn fixture(body: &str) -> (Project, Registry) {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "curves")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.resources.clear();
    definition.passes = vec![PassDefinition {
        shader: "shaders/curve.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.curve-precision".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("shaders/curve.wgsl"),
        format!("fn main_fx(p: vec2<f32>) -> vec4<f32> {{ {body} }}"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut project = Project::new(SIZE, SIZE, 30, 60).unwrap();
    let mut layer = Layer::solid(
        1,
        "curve probe",
        [SIZE as f32 * 2.; 2],
        [SIZE as f32 / 2., SIZE as f32 / 2., 0.],
        [1.; 4],
    );
    layer.effects.push(EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (project, registry)
}
fn curve(project: &mut Project) -> &mut motion_model::CurveTrack {
    project.layers[0].effects[0]
        .params
        .get_mut("p0001")
        .unwrap()
        .curve
        .as_mut()
        .unwrap()
}
fn sample(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn renderer(precision: ChannelPrecision, registry: Registry) -> Renderer {
    let format = match precision {
        ChannelPrecision::Unorm8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        ChannelPrecision::Unorm16 => wgpu::TextureFormat::Rgba16Unorm,
        ChannelPrecision::Float16 => wgpu::TextureFormat::Rgba16Float,
        ChannelPrecision::Float32 => wgpu::TextureFormat::Rgba32Float,
    };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
    renderer.set_effect_registry(registry);
    renderer
}
fn target(renderer: &Renderer, precision: ChannelPrecision) -> motion_render::ImageCaptureTarget {
    renderer
        .image_capture_target(
            SIZE,
            SIZE,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                precision,
            ),
        )
        .unwrap()
}
fn f32_pixels(bytes: &[u8]) -> Vec<[f32; 4]> {
    bytes
        .chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|c| f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect()
}
fn constant(red: f32) -> CurveObject {
    let mut value = CurveObject::default();
    value.channels[1] = vec![[0., red], [1., red]];
    value
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dense_authored_features_reach_high_precision_without_requiring_f32_filtering_for_half() {
    let (mut project, registry) =
        fixture("return curve_lookup(vec4<f32>(2049.0/4080.0,0.25,0.75,1.0));");
    let x = 2049. / 4080.;
    let value = &mut curve(&mut project).value;
    value.interpolation = [ColorInterpolation::Linear; 5];
    value.channels[1] = vec![
        [0., 0.],
        [x - 0.0001, 0.],
        [x, 1.],
        [x + 0.0001, 0.],
        [1., 0.],
    ];
    let scene = sample(&project, 0.);
    assert!(scene.curve_luts[0].iter().all(|row| row[0] == 0.));
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = renderer(precision, registry.clone());
        if precision != ChannelPrecision::Float32 {
            assert!(!renderer
                .device
                .features()
                .contains(wgpu::Features::FLOAT32_FILTERABLE));
        }
        let target = target(&renderer, precision);
        let stats = renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        let expected_bytes = if precision == ChannelPrecision::Unorm8 {
            1024
        } else {
            LUT_BYTES
        };
        assert_eq!(stats.parameter_resource_upload_bytes, expected_bytes);
        assert_eq!(
            renderer.texture_memory_breakdown().curve_luts,
            expected_bytes
        );
        let image = renderer.read_image_target(&target).unwrap();
        match precision {
            ChannelPrecision::Unorm8 => assert!(image
                .pixels
                .chunks_exact(4)
                .all(|p| p[0] == 0 && p[3] == 255)),
            ChannelPrecision::Unorm16 => assert!(image
                .pixels
                .chunks_exact(8)
                .all(|p| u16::from_le_bytes(p[..2].try_into().unwrap()) >= 65534
                    && p[6..8] == u16::MAX.to_le_bytes())),
            ChannelPrecision::Float16 => assert!(image
                .pixels
                .chunks_exact(8)
                .all(|p| u16::from_le_bytes(p[..2].try_into().unwrap()) == 0x3c00
                    && p[6..8] == 0x3c00_u16.to_le_bytes())),
            ChannelPrecision::Float32 => {
                assert!(f32_pixels(&image.pixels)
                    .iter()
                    .all(|p| (p[0] - 1.).abs() < 1e-6 && p[3] == 1.))
            }
        }
        let ledger = renderer.texture_resource_ledger();
        let tables: Vec<_> = ledger
            .resources()
            .filter(|r| {
                r.references
                    .iter()
                    .any(|owner| owner.category == TextureCategory::CurveLuts)
            })
            .collect();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].logical_bytes, Some(expected_bytes));
        assert_eq!(
            tables[0].format,
            if precision == ChannelPrecision::Unorm8 {
                "Rgba8Unorm"
            } else {
                "Rgba32Float"
            }
        );
        assert_eq!(
            renderer
                .draw(&scene, target.view(), SIZE, SIZE)
                .unwrap()
                .parameter_resource_upload_bytes,
            0
        );
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn fine_gradients_and_temporal_seeks_reuse_numeric_tables_and_pipelines() {
    let (mut project, registry) =
        fixture("return curve_lookup(vec4<f32>(p.x/34.0,0.25,0.75,1.0));");
    curve(&mut project).value.channels[1] = vec![[0., 0.5001], [1., 0.5003]];
    let mut renderer = renderer(ChannelPrecision::Float32, registry.clone());
    let target = target(&renderer, ChannelPrecision::Float32);
    let scene = sample(&project, 0.);
    renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
    let first = renderer.read_image_target(&target).unwrap().pixels;
    let row: Vec<_> = f32_pixels(&first)
        .iter()
        .take(SIZE as usize)
        .map(|p| p[0])
        .collect();
    assert!(
        row.windows(2).all(|p| p[1] > p[0]),
        "flattened gradient: {row:?}"
    );
    assert!(row.iter().all(|v| *v > 0.5001 && *v < 0.5003));
    let builds = renderer.effect_pipeline_builds();
    let ids = renderer.texture_resource_ledger();
    assert_eq!(
        renderer
            .draw(&scene, target.view(), SIZE, SIZE)
            .unwrap()
            .parameter_resource_upload_bytes,
        0
    );
    assert_eq!(renderer.read_image_target(&target).unwrap().pixels, first);
    assert_eq!(renderer.texture_resource_ledger(), ids);
    curve(&mut project).keys = vec![
        Keyframe {
            auto_bezier: false,
            frame: 0,
            value: constant(0.5001),
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        },
        Keyframe {
            auto_bezier: false,
            frame: 10,
            value: constant(0.5003),
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        },
    ];
    for frame in [5., 10., 0., 5.] {
        let scene = sample(&project, frame);
        let expected = 0.5001_f32 + (0.5003_f32 - 0.5001) * (frame as f32 / 10.);
        assert_eq!(
            renderer
                .draw(&scene, target.view(), SIZE, SIZE)
                .unwrap()
                .parameter_resource_upload_bytes,
            LUT_BYTES
        );
        let image = renderer.read_image_target(&target).unwrap();
        assert!(f32_pixels(&image.pixels)
            .iter()
            .all(|p| (p[0] - expected).abs() < 1e-7));
        let mut fresh = self::renderer(ChannelPrecision::Float32, registry.clone());
        let reference = self::target(&fresh, ChannelPrecision::Float32);
        fresh.draw(&scene, reference.view(), SIZE, SIZE).unwrap();
        assert_eq!(
            image.pixels,
            fresh.read_image_target(&reference).unwrap().pixels
        );
        assert_eq!(renderer.effect_pipeline_builds(), builds);
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dense_table_admission_refusal_preserves_health_and_recovers_without_downcasting() {
    let (project, registry) = fixture("return curve_lookup(vec4<f32>(0.5,0.25,0.75,1.0));");
    for precision in [
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = renderer(precision, registry.clone());
        let budget = renderer.allocation_budget().clone();
        let original = budget.info().unwrap();
        let scene = sample(&project, 0.);
        let mut builder = motion_render::effect_plan::PlanBuilder::new(registry.clone()).unwrap();
        builder.set_precision(precision);
        let scratch = builder
            .build(&scene, &[0], SIZE, SIZE, true)
            .unwrap()
            .scratch_bytes();
        let required = original.admitted_bytes + scratch + LUT_BYTES;
        budget.set_limit(required - 1).unwrap();
        let error = renderer.preflight_effects(&scene, SIZE, SIZE).unwrap_err();
        assert!(error.to_string().contains("curve index=0"), "{error}");
        assert_eq!(
            error.resource_rejection(),
            Some(&motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::RenderAllocations,
                required_bytes: required,
                budget_bytes: required - 1,
            })
        );
        assert_eq!(renderer.texture_memory_breakdown().curve_luts, 0);
        renderer.check_health().unwrap();
        budget.set_limit(original.budget_bytes).unwrap();
        let target = target(&renderer, precision);
        renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        assert_eq!(renderer.texture_memory_breakdown().curve_luts, LUT_BYTES);
        renderer.read_image_target(&target).unwrap();
        renderer.check_health().unwrap();
    }
}

#[test]
fn packed_numeric_curves_publish_encoding_native_float_bits_and_refuse_missing_sources() {
    use motion_render::effect_plan::{PlanBuilder, PLAN_VERSION};
    let (mut project, registry) = fixture("return curve_lookup(vec4<f32>(0.5));");
    curve(&mut project).value = constant(0.50001);
    let mut scene = sample(&project, 0.);
    let mut builder = PlanBuilder::new(registry).unwrap();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        builder.set_precision(precision);
        let plan = builder.build(&scene, &[0], SIZE, SIZE, true).unwrap();
        let mut bytes = vec![0xa5; plan.buffer_bytes(&scene)];
        plan.write(&scene, &mut bytes).unwrap();
        let word = |p| u32::from_ne_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        assert_eq!(word(4), PLAN_VERSION as usize);
        assert_eq!(PLAN_VERSION, 12);
        let stride = if precision == ChannelPrecision::Unorm8 {
            1024
        } else {
            LUT_BYTES as usize
        };
        assert_eq!(
            word(156),
            usize::from(precision != ChannelPrecision::Unorm8)
        );
        assert_eq!(word(64) - word(44), stride);
        if precision != ChannelPrecision::Unorm8 {
            let expected = curve(&mut project)
                .sampled_curve(0.)
                .unwrap()
                .high_precision_lut();
            let packed = &bytes[word(44)..word(44) + stride];
            assert_eq!(packed, bytemuck::cast_slice::<_, u8>(&expected));
            assert_eq!(
                f32::from_ne_bytes(packed[..4].try_into().unwrap()).to_bits(),
                0.50001_f32.to_bits()
            );
        }
    }
    let plan = builder.build(&scene, &[0], SIZE, SIZE, true).unwrap();
    scene.curve_luts.push([[0.; 4]; 256]);
    let mut bytes = vec![0xa5; plan.buffer_bytes(&scene)];
    let error = plan.write(&scene, &mut bytes).unwrap_err();
    assert!(error.to_string().contains("sources=1"));
    assert!(bytes.iter().all(|b| *b == 0xa5));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn removed_and_reintroduced_luts_replace_bindings_and_release_owned_capacity() {
    let (mut project, registry) = fixture("return curve_lookup(vec4<f32>(0.5,0.25,0.75,1.0));");
    curve(&mut project).value = constant(0.1);
    let mut second = project.layers[0].effects[0].clone();
    second.id = 2;
    second
        .params
        .get_mut("p0001")
        .unwrap()
        .curve
        .as_mut()
        .unwrap()
        .value = constant(0.9);
    project.layers[0].effects.push(second);
    project.rebuild_plugin_dependencies();
    let mut renderer = renderer(ChannelPrecision::Float32, registry);
    let target = target(&renderer, ChannelPrecision::Float32);
    let mut previous_ids = Vec::new();
    for (iteration, count, expected) in [(0, 2, 0.9), (1, 1, 0.1), (2, 2, 0.7)] {
        let second = &mut project.layers[0].effects[1];
        second.enabled = iteration != 1;
        if iteration == 2 {
            second
                .params
                .get_mut("p0001")
                .unwrap()
                .curve
                .as_mut()
                .unwrap()
                .value = constant(0.7);
        }
        let scene = sample(&project, 0.);
        renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        let pixels = renderer.read_image_target(&target).unwrap().pixels;
        assert!(f32_pixels(&pixels)
            .iter()
            .all(|p| (p[0] - expected).abs() < 1e-7));
        assert_eq!(
            renderer.texture_memory_breakdown().curve_luts,
            count * LUT_BYTES
        );
        let ledger = renderer.texture_resource_ledger();
        let ids: Vec<_> = ledger
            .resources()
            .filter(|r| {
                r.references
                    .iter()
                    .any(|owner| owner.category == TextureCategory::CurveLuts)
            })
            .map(|r| r.id.clone())
            .collect();
        if iteration == 0 {
            previous_ids = ids;
        } else if iteration == 1 {
            assert_eq!(ids, previous_ids[..1]);
        } else {
            assert_eq!(ids[0], previous_ids[0]);
            assert_ne!(ids[1], previous_ids[1]);
        }
        renderer.check_health().unwrap();
    }
}
