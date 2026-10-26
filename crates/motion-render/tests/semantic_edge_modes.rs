//! Independently authored ramp and integer-coordinate sampling oracle.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, BoundsExpr, EffectPackage, PassDefinition, Registry, WorkingSpace,
};
use motion_render::{Renderer, Scene};
use std::sync::Arc;

#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

const W: u32 = 32;
const H: u32 = 24;

fn package() -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let base = manifest
        .effects
        .iter()
        .find(|d| d.id == "glow")
        .unwrap()
        .clone();
    manifest.id = "test.semantic-edge-modes".into();
    manifest.effects.clear();
    for (id, source) in [
        (
            "ramp",
            "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(p.x/32.0,0.2,0.1,1.0);}",
        ),
        (
            "offset",
            "fn main_fx(p:vec2<f32>)->vec4<f32>{return sample_input(p+vec2(5.0,0.0));}",
        ),
    ] {
        let mut def = base.clone();
        def.id = id.into();
        def.editor = None;
        def.native_editor = None;
        def.execution = None;
        def.required_capabilities.clear();
        def.working_space = WorkingSpace::Linear;
        def.alpha_mode = AlphaMode::Premultiplied;
        def.padding = BoundsExpr::Constant { value: 0. };
        def.output_bounds = None;
        def.resources.clear();
        def.shader_data = None;
        if id == "ramp" {
            def.params.clear();
            def.edge_param = None;
        }
        let path = format!("shaders/{id}.wgsl");
        std::fs::write(directory.path().join(&path), source).unwrap();
        def.passes = vec![PassDefinition {
            shader: path,
            entry: "main_fx".into(),
        }];
        manifest.effects.push(def);
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

fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn parameter_identity_controls_real_clamp_repeat_and_mirror_pixels_and_recovery() {
    let package = package();
    let mut project = Project::new(W, H, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        47,
        "edge oracle",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    for (i, def) in package.manifest.effects.iter().enumerate() {
        layer.effects.push(EffectInstance::new(
            i as u64 + 11,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            layer.size,
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ))
    .unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    renderer.set_effect_registry(registry);
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        ChannelPrecision::Unorm8,
    );
    let target = renderer.image_capture_target(W, H, contract).unwrap();
    let mut rows = Vec::new();
    let mut clamp = None;
    let mut previous = None;
    let mut builds = 0;
    for mode in [1_u32, 2, 3] {
        project.layers[0].effects[1]
            .params
            .get_mut("edge")
            .unwrap()
            .track
            .value[0] = mode as f32;
        let stats = renderer
            .draw(&sample(&project), target.view(), W, H)
            .unwrap();
        let raw = renderer.read_image_target(&target).unwrap();
        let pixels = reference::linear(&raw);
        let mut worst = 0_f64;
        let mut worst_storage_code = 0_f64;
        for y in 0..H {
            for x in 0..W {
                let shifted = x + 5;
                let source = match mode {
                    1 => shifted.min(W - 1),
                    2 => shifted % W,
                    3 => {
                        let position = shifted % (2 * W);
                        if position >= W {
                            2 * W - 1 - position
                        } else {
                            position
                        }
                    }
                    _ => unreachable!(),
                };
                let expected = reference::store(
                    [(f64::from(source) + 0.5) / f64::from(W), 0.2, 0.1, 1.],
                    ChannelPrecision::Unorm8,
                );
                for channel in 0..4 {
                    let actual = pixels[(y * W + x) as usize][channel];
                    worst = worst.max((actual - expected[channel]).abs());
                    // UNORM8 color is stored as sRGB. One storage code does not
                    // equal 1/255 in decoded linear light, especially near white.
                    let encoded = |value| {
                        if channel < 3 {
                            reference::encode(value)
                        } else {
                            value
                        }
                    };
                    worst_storage_code = worst_storage_code
                        .max((encoded(actual) - encoded(expected[channel])).abs() * 255.);
                }
            }
        }
        assert!(
            worst_storage_code <= 1. + 1e-8,
            "mode={mode} actual sampling differs from integer-coordinate oracle: {worst_storage_code} storage codes, {worst} linear"
        );
        assert_eq!(stats.draw_calls, 8);
        if mode == 1 {
            clamp = Some(raw.pixels.clone());
            builds = renderer.effect_pipeline_builds();
            assert!(builds > 0);
        } else {
            assert_ne!(
                raw.pixels,
                *clamp.as_ref().unwrap(),
                "the patterned source must distinguish mode={mode} from clamp"
            );
            assert_eq!(
                renderer.effect_pipeline_builds(),
                builds,
                "numeric sampling mode must reuse actual native programs"
            );
        }
        rows.push(serde_json::json!({"mode":mode,"worst_linear_error":worst,"worst_storage_code_error":worst_storage_code,"draw_calls":stats.draw_calls,"native_pipeline_builds":renderer.effect_pipeline_builds()}));
        previous = Some(raw.pixels);
    }
    let mut invalid = sample(&project);
    let edge = invalid.effects[1]
        .param_ids
        .iter()
        .position(|id| id == "edge")
        .unwrap();
    invalid.effects[1].values[edge][0] = 999.;
    // Formal output performs strict preflight; interactive drawing preserves
    // the existing per-effect bypass behavior for invalid parameter values.
    let error = renderer.preflight_effects(&invalid, W, H).unwrap_err();
    assert!(error.to_string().contains("parameter edge"), "{error}");
    assert_eq!(
        renderer.read_image_target(&target).unwrap().pixels,
        previous.unwrap()
    );
    project.layers[0].effects[1]
        .params
        .get_mut("edge")
        .unwrap()
        .track
        .value[0] = 1.;
    renderer
        .draw(&sample(&project), target.view(), W, H)
        .unwrap();
    assert_eq!(
        renderer.read_image_target(&target).unwrap().pixels,
        clamp.unwrap()
    );
    assert_eq!(renderer.effect_pipeline_builds(), builds);
    if let Some(directory) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::PathBuf::from(directory).join("semantic-edge-pixels.json"), serde_json::to_vec_pretty(&serde_json::json!({"adapter":format!("{:?}",renderer.adapter_info),"rows":rows,"refusal_pixels_preserved":true,"same_renderer_recovery":true})).unwrap()).unwrap();
    }
}
