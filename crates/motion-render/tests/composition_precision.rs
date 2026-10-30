use motion_core::{
    Content, EffectImageInput, EffectImageStage, EffectInstance, Layer, Project, Track,
    compositing::{BlendMode, BlendSpace, LayerBlend, MatteMode, TrackMatte},
    masks::LayerMask,
    vector::{PathNode, VectorPath},
};
use motion_effects::{
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{Renderer, Scene, TextureCategory};
use std::sync::Arc;

const SIZE: u32 = 17;

// Independent constant shaders isolate storage/compositing precision from
// image decoding, display conversion, and primitive-edge antialiasing.
fn package() -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let base = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    manifest.id = "com.motionstudio.tests.composition-precision".into();
    manifest.effects.clear();
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::create_dir(directory.path().join("assets")).unwrap();
    image::save_buffer(
        directory.path().join("assets/fallback.png"),
        &[255; 4],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    for (id, body) in [
        ("back", "return vec4<f32>(2.0, -0.25, 1.0001, 1.0);"),
        ("front", "return vec4<f32>(0.5, 0.25, 0.5, 1.0);"),
        ("half", "return vec4<f32>(1.0, -0.125, 0.5, 0.5);"),
        (
            "external",
            "return textureSampleLevel(resource0,resource_sampler0,vec2<f32>(0.5),0.0) + vec4<f32>(1.0,0.0,0.0,0.0);",
        ),
        (
            "adjust",
            "return sample_input(p) + vec4<f32>(1.0, -0.25, 0.0, 0.0);",
        ),
    ] {
        let mut definition = base.clone();
        definition.id = id.into();
        definition.working_space = WorkingSpace::Linear;
        definition.alpha_mode = AlphaMode::Premultiplied;
        if id == "external" {
            definition.required_capabilities.push("image_input".into());
            definition.resources = vec!["assets/fallback.png".into()];
        }
        definition.passes = vec![PassDefinition {
            shader: format!("shaders/{id}.wgsl"),
            entry: "main_fx".into(),
        }];
        std::fs::write(
            directory.path().join(&definition.passes[0].shader),
            format!("fn main_fx(p:vec2<f32>)->vec4<f32> {{ {body} }}"),
        )
        .unwrap();
        manifest.effects.push(definition);
    }
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    )
}

fn layer(id: u64, effect: &str, package: &EffectPackage) -> Layer {
    let mut layer = Layer::solid(
        id,
        "precision receiver",
        [SIZE as f32 * 2.; 2],
        [SIZE as f32 / 2., SIZE as f32 / 2., 0.],
        [1.; 4],
    );
    if !effect.is_empty() {
        layer.effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            package
                .manifest
                .effects
                .iter()
                .find(|d| d.id == effect)
                .unwrap(),
            layer.size,
        ));
    }
    layer
}

fn scene(layers: Vec<Layer>) -> Scene {
    let mut project = Project::new(SIZE, SIZE, 30, 60).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.background = [0.; 4];
    project.layers = layers;
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

fn renderer(precision: ChannelPrecision, package: Arc<EffectPackage>) -> Renderer {
    let format = match precision {
        ChannelPrecision::Float16 => wgpu::TextureFormat::Rgba16Float,
        ChannelPrecision::Float32 => wgpu::TextureFormat::Rgba32Float,
        ChannelPrecision::Unorm8 | ChannelPrecision::Unorm16 => unreachable!(),
    };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
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

fn read(renderer: &Renderer, target: &motion_render::ImageCaptureTarget) -> Vec<[f32; 4]> {
    let image = renderer.read_image_target(target).unwrap();
    let channels: Vec<f32> = if image.contract.precision == ChannelPrecision::Float16 {
        image
            .pixels
            .chunks_exact(2)
            .map(|b| {
                let bits = u16::from_le_bytes(b.try_into().unwrap());
                let sign = if bits & 0x8000 == 0 { 1. } else { -1. };
                let exponent = (bits >> 10) & 31;
                let fraction = f32::from(bits & 1023);
                if exponent == 0 {
                    sign * fraction * 2_f32.powi(-24)
                } else if exponent == 31 {
                    panic!("nonfinite Float16 pixel: {bits:#x}")
                } else {
                    sign * (1. + fraction / 1024.) * 2_f32.powi(i32::from(exponent) - 15)
                }
            })
            .collect()
    } else {
        image
            .pixels
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect()
    };
    channels
        .chunks_exact(4)
        .map(|p| p.try_into().unwrap())
        .collect()
}

fn exact(pixels: &[[f32; 4]], expected: [f32; 4], context: &str) {
    assert_eq!(pixels.len(), (SIZE * SIZE) as usize);
    for (i, pixel) in pixels.iter().enumerate() {
        assert_eq!(*pixel, expected, "{context}: pixel {i}");
    }
}

fn resources(
    renderer: &Renderer,
    category: TextureCategory,
    format: &str,
    pixel_bytes: u64,
) -> u64 {
    let ledger = renderer.texture_resource_ledger();
    let entries: Vec<_> = ledger
        .resources()
        .filter(|r| r.references.iter().any(|v| v.category == category))
        .collect();
    assert!(!entries.is_empty(), "missing {category:?}");
    for entry in &entries {
        assert_eq!(entry.format, format, "{category:?}");
        assert_eq!(
            entry.logical_bytes,
            Some(u64::from(entry.width) * u64::from(entry.height) * pixel_bytes)
        );
    }
    entries.iter().map(|r| r.logical_bytes.unwrap()).sum()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effected_layer_snapshots_preserve_hdr_sign_and_fine_detail_in_both_stack_orders() {
    let package = package();
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision, package.clone());
        let target = target(&renderer, precision);
        let mut source = layer(1, "back", &package);
        source.visible = false;
        let mut receiver = layer(2, "external", &package);
        receiver.effects[0].image_input = Some(EffectImageInput::Layer {
            layer: 1,
            stage: EffectImageStage::Effects,
        });
        let blue = if precision == ChannelPrecision::Float16 {
            1.
        } else {
            1.0001
        };
        for layers in [
            vec![source.clone(), receiver.clone()],
            vec![receiver, source],
        ] {
            let scene = scene(layers);
            renderer.preflight_effects(&scene, SIZE, SIZE).unwrap();
            renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
            exact(
                &read(&renderer, &target),
                [3., -0.25, blue, 1.],
                "layer snapshot",
            );
            let (format, bytes) = if precision == ChannelPrecision::Float16 {
                ("Rgba16Float", 8)
            } else {
                ("Rgba32Float", 16)
            };
            let capacity = resources(
                &renderer,
                TextureCategory::EffectInputSnapshots,
                format,
                bytes,
            );
            assert_eq!(
                renderer.texture_memory_breakdown().effect_input_snapshots,
                capacity
            );
        }
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_transfer_and_adjustment_accumulators_preserve_extended_color() {
    let package = package();
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision, package.clone());
        let target = target(&renderer, precision);
        let blue = if precision == ChannelPrecision::Float16 {
            1.
        } else {
            1.0001
        };
        for (mode, expected) in [
            (BlendMode::Multiply, [1., -0.0625, blue * 0.5, 1.]),
            (BlendMode::Add, [2.5, 0., blue + 0.5, 1.]),
            (
                BlendMode::Screen,
                [1.5, 0.0625, blue + 0.5 - blue * 0.5, 1.],
            ),
        ] {
            let mut front = layer(2, "front", &package);
            front.blend = LayerBlend {
                mode,
                space: BlendSpace::Linear,
            };
            let scene = scene(vec![layer(1, "back", &package), front]);
            renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
            exact(
                &read(&renderer, &target),
                expected,
                &format!("{precision:?} {mode:?}"),
            );
            let (format, bytes) = if precision == ChannelPrecision::Float16 {
                ("Rgba16Float", 8)
            } else {
                ("Rgba32Float", 16)
            };
            let capacity = resources(
                &renderer,
                TextureCategory::LayerCompositingScratch,
                format,
                bytes,
            );
            assert_eq!(capacity, u64::from(SIZE * SIZE) * bytes * 2);
            assert_eq!(
                renderer
                    .texture_memory_breakdown()
                    .layer_compositing_scratch,
                capacity
            );
        }
        let mut adjustment = layer(2, "adjust", &package);
        adjustment.content = Content::Adjustment;
        let scene = scene(vec![layer(1, "back", &package), adjustment]);
        renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        exact(
            &read(&renderer, &target),
            [3., -0.5, blue, 1.],
            "adjustment",
        );
        renderer.check_health().unwrap();
    }
}

fn full_mask() -> LayerMask {
    LayerMask::new(
        1,
        VectorPath {
            tangents: Default::default(), id: 1,
            closed: true,
            nodes: [[0., 0.], [34., 0.], [34., 34.], [0., 34.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_mask_and_projected_matte_coverage_avoid_unorm8_quantization() {
    let package = package();
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision, package.clone());
        let target = target(&renderer, precision);
        let (scalar_format, scalar_bytes) = if precision == ChannelPrecision::Float16 {
            ("R16Float", 2)
        } else {
            ("R32Float", 4)
        };
        let mut masked = layer(1, "", &package);
        let mut mask = full_mask();
        mask.opacity.value = 50.;
        masked.masks.push(mask);
        let masked_scene = scene(vec![masked]);
        renderer
            .draw(&masked_scene, target.view(), SIZE, SIZE)
            .unwrap();
        exact(&read(&renderer, &target), [0.5; 4], "50 percent mask");
        resources(
            &renderer,
            TextureCategory::MaskOutputs,
            scalar_format,
            scalar_bytes,
        );
        resources(
            &renderer,
            TextureCategory::MaskScratch,
            scalar_format,
            scalar_bytes,
        );

        let mut matte = layer(1, "half", &package);
        matte.visible = false;
        let mut receiver = layer(2, "half", &package);
        receiver.track_matte = Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
            hide_source: true,
        });
        let scene = scene(vec![matte, receiver]);
        renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        exact(
            &read(&renderer, &target),
            [0.5, -0.0625, 0.25, 0.25],
            "projected alpha matte",
        );
        // No transfer source is allocated for Normal; only scalar matte images.
        let capacity = resources(
            &renderer,
            TextureCategory::MatteBlendScratch,
            scalar_format,
            scalar_bytes,
        );
        assert_eq!(
            renderer.texture_memory_breakdown().matte_blend_scratch,
            capacity
        );
        let raster = renderer
            .materialize_layer(&scene, 2, SIZE, SIZE, false)
            .unwrap()
            .unwrap()
            .0;
        let ledger = raster.texture_resource_ledger();
        assert_eq!(ledger.total_known_bytes(), raster.texture_bytes());
        assert!(ledger.resources().any(|r| r.format == scalar_format));
        let color_format = if precision == ChannelPrecision::Float16 {
            "Rgba16Float"
        } else {
            "Rgba32Float"
        };
        assert!(ledger.resources().any(|r| r.format == color_format));
        renderer
            .draw_layer_rasters(&scene, &[&raster], target.view(), SIZE, SIZE)
            .unwrap();
        exact(
            &read(&renderer, &target),
            [0.5, -0.0625, 0.25, 0.25],
            "retained local image and matte",
        );
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_snapshot_admission_refuses_an_eight_bit_allowance_and_recovers() {
    let package = package();
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision, package.clone());
        let target = target(&renderer, precision);
        let original = renderer.memory_policy();
        let mut policy = original;
        // One 34x34 RGBA8 snapshot fits; its native float equivalent does not.
        policy.source_texture_budget_bytes = 5000;
        renderer.set_memory_policy(policy).unwrap();
        let mut source = layer(1, "back", &package);
        source.visible = false;
        let mut receiver = layer(2, "external", &package);
        receiver.effects[0].image_input = Some(EffectImageInput::Layer {
            layer: 1,
            stage: EffectImageStage::Effects,
        });
        let scene = scene(vec![source, receiver]);
        let error = renderer
            .draw(&scene, target.view(), SIZE, SIZE)
            .unwrap_err();
        let rejection = error.resource_rejection().unwrap();
        assert!(
            matches!(rejection, motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::SourceTextures,
            required_bytes, budget_bytes: 5000,
        } if *required_bytes > 5000),
            "{error}"
        );
        assert!(error.to_string().contains("snapshots"), "{error}");
        assert_eq!(
            renderer.texture_memory_breakdown().effect_input_snapshots,
            0
        );
        renderer.check_health().unwrap();
        renderer.set_memory_policy(original).unwrap();
        renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
        let blue = if precision == ChannelPrecision::Float16 {
            1.
        } else {
            1.0001
        };
        exact(
            &read(&renderer, &target),
            [3., -0.25, blue, 1.],
            "budget recovery",
        );
        assert!(renderer.texture_memory_breakdown().effect_input_snapshots > 5000);
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn ordinary_nested_composition_keeps_native_color_storage_and_capacity() {
    let package = package();
    let mut project = Project::new(SIZE, SIZE, 30, 60).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.background = [0.; 4];
    project.layers = vec![layer(1, "back", &package)];
    project.rebuild_plugin_dependencies();
    let mut engine = motion_core::Engine::new(project).unwrap();
    engine
        .apply_batch(
            motion_core::parse_commands(
                &serde_json::json!({
                    "op": "composition", "composition": motion_core::MAIN_COMPOSITION,
                    "action": {"kind": "precompose", "objects": [1], "name": "precision child"}
                })
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let mut scene = Scene::new(engine.project()).unwrap();
    scene
        .sample(
            engine.project(),
            0.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    assert_eq!(scene.nested.len(), 1);
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision, package.clone());
        let target = target(&renderer, precision);
        let (format, bytes) = if precision == ChannelPrecision::Float16 {
            ("Rgba16Float", 8)
        } else {
            ("Rgba32Float", 16)
        };
        let blue = if precision == ChannelPrecision::Float16 {
            1.
        } else {
            1.0001
        };
        let mut previous = None;
        for _ in 0..2 {
            renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
            // This oracle targets storage inside the child plane. Its edge
            // coverage is exercised separately by existing geometry tests.
            let pixels = read(&renderer, &target);
            assert_eq!(
                pixels[(SIZE * (SIZE / 2) + SIZE / 2) as usize],
                [2., -0.25, blue, 1.]
            );
            let capacity = resources(
                &renderer,
                TextureCategory::NestedCompositions,
                format,
                bytes,
            );
            assert_eq!(
                renderer.texture_memory_breakdown().nested_compositions,
                capacity
            );
            assert_eq!(capacity, u64::from(SIZE * SIZE) * bytes);
            let ledger = renderer.texture_resource_ledger();
            let ids: Vec<_> = ledger
                .resources()
                .filter(|r| {
                    r.references
                        .iter()
                        .any(|v| v.category == TextureCategory::NestedCompositions)
                })
                .map(|r| r.id.clone())
                .collect();
            if let Some(previous) = &previous {
                assert_eq!(&ids, previous);
            }
            previous = Some(ids);
        }
        renderer.check_health().unwrap();
    }
}
