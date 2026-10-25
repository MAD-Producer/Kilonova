//! Actual intermediate pixels and physical descriptors, not display PNGs.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
use motion_effects::{AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace};
use motion_render::{
    Renderer, Scene, TextureCategory,
    effect_plan::{PLAN_VERSION, PlanBuilder},
    image_formats,
};
use std::sync::Arc;

const MODES: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];

fn fixture(produce: &str, consume: &str) -> (Scene, Registry) {
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
    definition.passes = ["produce", "consume"]
        .map(|name| PassDefinition {
            shader: format!("shaders/{name}.wgsl"),
            entry: "main_fx".into(),
        })
        .to_vec();
    manifest.id = "com.motionstudio.tests.working-precision".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    for (name, source) in [("produce", produce), ("consume", consume)] {
        std::fs::write(
            directory.path().join(format!("shaders/{name}.wgsl")),
            source,
        )
        .unwrap();
    }
    let package = Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut project = Project::new(17, 17, 30, 60).unwrap();
    // Keep geometric edges outside the capture so every pixel is a precision probe.
    let mut layer = Layer::solid(
        1,
        "working precision probe",
        [34.; 2],
        [8.5, 8.5, 0.],
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
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (scene, registry)
}

const PRODUCE: &str =
    "fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(2.0, -0.25, 1.0001, 1.0); }";
const CONSUME: &str = "fn main_fx(p: vec2<f32>) -> vec4<f32> { return sample_input(p) + vec4<f32>(1.0, 0.0, 0.0, 0.0); }";

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn vector_group_msaa_resolve_and_opacity_keep_the_selected_storage() {
    use motion_core::{
        Content, Track,
        vector::{groups::*, *},
    };
    let mut project = Project::new(17, 17, 30, 60).unwrap();
    project.background = [0.; 4];
    let wrapped = VectorGroup::wrap(VectorContent::shape(ShapeKind::Rectangle), [12.; 2]);
    let VectorSource::Group { mut group } = wrapped.source else {
        unreachable!()
    };
    group.items.push(GroupItem::Fill {
        id: 3,
        name: "Precision fill".into(),
        color: Track::constant([1., 0., 0., 1.]),
        fill_rule: FillRule::NonZero,
        composite: Composite::Below,
    });
    group.transform.opacity.value = 50.001;
    let mut layer = Layer::solid(1, "Precision group", [12.; 2], [8.5, 8.5, 0.], [1.; 4]);
    layer.content = Content::Vector {
        vector: VectorContent {
            source: VectorSource::Group { group },
            fill: None,
            stroke: None,
            trim: None,
            fill_rule: FillRule::NonZero,
        },
    };
    project.layers.push(layer);
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    for precision in MODES {
        let mut renderer = renderer(precision);
        let target = renderer
            .image_capture_target(17, 17, contract(precision))
            .unwrap();
        renderer.draw(&scene, target.view(), 17, 17).unwrap();
        let image = renderer.read_image_target(&target).unwrap();
        let bpp = image.storage.bytes_per_pixel() as usize;
        let center = &image.pixels[(8 * 17 + 8) * bpp..(8 * 17 + 9) * bpp];
        match precision {
            // Coverage is rounded at both the MSAA resolve and composition.
            // Match the existing vector-group contract of one UNORM8 LSB;
            // the other modes below must retain their own finer precision.
            ChannelPrecision::Unorm8 => assert!(center[3].abs_diff(128) <= 1),
            ChannelPrecision::Unorm16 => {
                let alpha = u16::from_le_bytes(center[6..8].try_into().unwrap());
                let expected = (0.50001_f32 * 65535.).round() as u16;
                assert!(alpha.abs_diff(expected) <= 1, "{alpha} expected {expected}");
                assert!(
                    alpha.abs_diff(32896) > 100,
                    "must not retain UNORM8 alpha steps"
                );
            }
            ChannelPrecision::Float16 => {
                assert_eq!(u16::from_le_bytes(center[6..8].try_into().unwrap()), 0x3800)
            }
            ChannelPrecision::Float32 => {
                let alpha = f32::from_le_bytes(center[12..16].try_into().unwrap());
                assert!((alpha - 0.50001).abs() < 0.000001, "{alpha}");
                assert!((alpha - 128. / 255.).abs() > 0.001);
            }
        }
        let ledger = renderer.texture_resource_ledger();
        let raster_format = format!(
            "{:?}",
            image_formats::native_format(image_formats::raster_storage(precision))
        );
        let mut group_count = 0;
        for resource in ledger.resources().filter(|r| {
            r.references.iter().any(|v| {
                matches!(
                    v.category,
                    TextureCategory::VectorRasterScratch | TextureCategory::VectorGroupScratch
                )
            })
        }) {
            assert_eq!(resource.format, raster_format);
            assert_eq!(
                resource.logical_bytes,
                Some(
                    u64::from(resource.width)
                        * u64::from(resource.height)
                        * bpp as u64
                        * u64::from(resource.samples)
                )
            );
            if resource
                .references
                .iter()
                .any(|v| v.category == TextureCategory::VectorGroupScratch)
            {
                group_count += 1;
            }
        }
        assert!(
            group_count >= 2,
            "must exercise actual group MSAA and resolve images"
        );
        renderer.check_health().unwrap();
    }
}

fn contract(precision: ChannelPrecision) -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        precision,
    )
}
fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    // This acceptance requires real support. A capability refusal is not a passed pixel probe.
    let renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        image_formats::native_format(image_formats::color_storage(precision)),
    ))
    .unwrap();
    eprintln!(
        "working precision {precision:?}: {:?}",
        renderer.adapter.get_info()
    );
    renderer
}

#[test]
fn four_mode_plans_publish_native_formats_capacity_and_current_wire_precision() {
    let (scene, registry) = fixture(PRODUCE, CONSUME);
    let mut builder = PlanBuilder::new(registry).unwrap();
    for precision in MODES {
        builder.set_precision(precision);
        let plan = builder.build_preview(&scene, &[0], 17, 17).unwrap();
        assert_eq!(plan.precision, precision);
        assert!(plan.diagnostics.is_empty(), "{:?}", plan.diagnostics);
        assert!(!plan.scratch_allocations.is_empty());
        let pixel_bytes = image_formats::color_storage(precision).bytes_per_pixel();
        for allocation in &plan.scratch_allocations {
            assert_eq!(
                allocation.class.format.bytes_per_pixel() as u64,
                pixel_bytes
            );
            if precision != ChannelPrecision::Unorm8 {
                assert_eq!(
                    allocation.class.format.native(),
                    image_formats::native_format(image_formats::color_storage(precision))
                );
            }
        }
        assert_eq!(
            plan.scratch_bytes(),
            plan.scratch_allocations
                .iter()
                .map(|a| u64::from(a.width) * u64::from(a.height) * pixel_bytes)
                .sum::<u64>()
        );
        let mut bytes = vec![0xa5; plan.buffer_bytes(&scene)];
        plan.write(&scene, &mut bytes).unwrap();
        assert_eq!(
            u32::from_ne_bytes(bytes[4..8].try_into().unwrap()),
            PLAN_VERSION
        );
        assert_eq!(
            u32::from_ne_bytes(bytes[152..156].try_into().unwrap()),
            precision as u32
        );
        let builds = builder.preview_plan_builds;
        builder.set_precision(precision);
        builder.build_preview(&scene, &[0], 17, 17).unwrap();
        assert_eq!(builder.preview_plan_builds, builds);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_two_pass_images_retain_precision_sign_hdr_and_reuse_physical_allocations() {
    for precision in MODES {
        let (scene, registry) = fixture(PRODUCE, CONSUME);
        let mut renderer = renderer(precision);
        let mut builder = PlanBuilder::new(registry.clone()).unwrap();
        builder.set_precision(precision);
        let expected_capacity = builder
            .build(&scene, &[0], 17, 17, true)
            .unwrap()
            .scratch_bytes();
        renderer.set_effect_registry(registry);
        let target = renderer
            .image_capture_target(17, 17, contract(precision))
            .unwrap();
        let expected: Vec<u8> = match precision {
            ChannelPrecision::Unorm8 => vec![255, 0, 255, 255],
            ChannelPrecision::Unorm16 => [65535_u16, 0, 65535, 65535]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
            ChannelPrecision::Float16 => [0x4200_u16, 0xb400, 0x3c00, 0x3c00]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
            ChannelPrecision::Float32 => [3.0_f32, -0.25, 1.0001, 1.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
        };
        let mut original = None;
        for iteration in 0..2 {
            renderer.preflight_effects(&scene, 17, 17).unwrap();
            renderer.draw(&scene, target.view(), 17, 17).unwrap();
            let image = renderer.read_image_target(&target).unwrap();
            for pixel in image.pixels.chunks_exact(expected.len()) {
                assert_eq!(pixel, expected, "{precision:?} iteration={iteration}");
            }
            let ledger = renderer.texture_resource_ledger();
            let scratch: Vec<_> = ledger
                .resources()
                .filter(|r| {
                    r.references
                        .iter()
                        .any(|reference| reference.category == TextureCategory::EffectScratch)
                })
                .collect();
            assert!(!scratch.is_empty());
            assert_eq!(
                scratch
                    .iter()
                    .map(|r| r.logical_bytes.unwrap())
                    .sum::<u64>(),
                expected_capacity
            );
            if precision != ChannelPrecision::Unorm8 {
                let format = format!(
                    "{:?}",
                    image_formats::native_format(image_formats::color_storage(precision))
                );
                assert!(scratch.iter().all(|r| r.format == format));
            }
            let state = (
                scratch.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
                renderer.effect_pipeline_builds(),
            );
            if let Some(original) = &original {
                assert_eq!(&state, original);
            } else {
                original = Some(state);
            }
            renderer.check_health().unwrap();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn integer_sixteen_retains_adjacent_levels_across_real_effect_intermediates() {
    let (scene, registry) = fixture(
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(32768.0/65535.0,32769.0/65535.0,32770.0/65535.0,1.0);}",
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return sample_input(p);}",
    );
    let mut renderer = renderer(ChannelPrecision::Unorm16);
    renderer.set_effect_registry(registry);
    let target = renderer
        .image_capture_target(17, 17, contract(ChannelPrecision::Unorm16))
        .unwrap();
    renderer.draw(&scene, target.view(), 17, 17).unwrap();
    let image = renderer.read_image_target(&target).unwrap();
    let expected: Vec<u8> = [32768_u16, 32769, 32770, 65535]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    for pixel in image.pixels.chunks_exact(8) {
        assert_eq!(pixel, expected);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn floating_exposure_keeps_hdr_sign_and_fine_detail_through_accumulation_and_resolve() {
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let (scene, registry) = fixture(PRODUCE, CONSUME);
        let mut renderer = renderer(precision);
        renderer.set_effect_registry(registry);
        let mut exposure = renderer.exposure_target(17, 17, &[0.5, 0.5]).unwrap();
        let expected_bytes =
            17 * 17 * image_formats::color_storage(precision).bytes_per_pixel() * 2;
        assert_eq!(exposure.texture_bytes(), expected_bytes);
        assert_eq!(
            exposure.texture_resource_ledger().total_known_bytes(),
            expected_bytes
        );
        assert!(
            renderer
                .capture_exposure_image(&exposure)
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
        renderer
            .accumulate_exposure(&mut exposure, Some(&scene), false)
            .unwrap();
        renderer
            .accumulate_exposure(&mut exposure, Some(&scene), false)
            .unwrap();
        assert!(exposure.complete());
        let image = renderer.capture_exposure_image(&exposure).unwrap();
        let expected: Vec<u8> = if precision == ChannelPrecision::Float16 {
            [0x4200_u16, 0xb400, 0x3c00, 0x3c00]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect()
        } else {
            [3.0_f32, -0.25, 1.0001, 1.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect()
        };
        for pixel in image.pixels.chunks_exact(expected.len()) {
            assert_eq!(pixel, expected, "{precision:?}");
        }
        drop(exposure);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        renderer.check_health().unwrap();
    }
}
