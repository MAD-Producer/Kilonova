//! Native precision probe justified by the retained dense quantization failure.
//! Independently authored procedural input reaches alpha below the PNG grid.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{
    AlphaMode, EdgeMode, EffectPackage, PassDefinition, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{Renderer, Scene, effect_plan::FusionRetainReason};
use std::sync::Arc;

#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

const W: u32 = 65;
const H: u32 = 33;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_preserving_rgb_groups_keep_native_alpha_exact_through_tiny_and_hdr_inputs() {
    let custom = package();
    let builtin = motion_effects::builtin::package().unwrap();
    let mut rows = Vec::new();
    for precision in reference::MODES {
        let mut project = Project::new(W, H, 30, 60).unwrap();
        project.working_precision = precision;
        project.background = [0.; 4];
        let mut layer = Layer::solid(
            47,
            "Alpha-preserving proof",
            [W as f32, H as f32],
            [W as f32 / 2., H as f32 / 2., 0.],
            [1.; 4],
        );
        let invert = effect(11, "invert", &builtin);
        let mut tint = effect(12, "tint", &builtin);
        tint.params.get_mut("p0001").unwrap().track.value = [0.07, 0.19, 0.31, 1.];
        tint.params.get_mut("p0002").unwrap().track.value = [0.91, 0.65, 0.43, 1.];
        tint.params.get_mut("p0003").unwrap().track.value[0] = 37.5;
        layer.effects = vec![effect(10, "producer", &custom), invert, tint];
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let mut registry = motion_effects::Registry::new_with_builtins().unwrap();
        registry.insert(custom.clone()).unwrap();
        let mut original = renderer(precision);
        let mut optimized = renderer(precision);
        original.set_effect_registry(registry.clone());
        optimized.set_effect_registry(registry);
        optimized.set_pointwise_fusion(true);
        let mut source_project = project.clone();
        source_project.layers[0].effects.truncate(1);
        source_project.rebuild_plugin_dependencies();
        let source = raw(&mut original, &source_project);
        assert!(source.iter().any(|p| p[3] == 0.));
        assert!(source.iter().any(|p| p[3] == 1.));
        if matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        ) {
            assert!(
                source.iter().any(|p| p[3] > 0. && p[3] < 1.0e-6),
                "actual floating source must reach the alpha threshold"
            );
        }
        for channel in 1..=4 {
            project.layers[0].effects[1]
                .params
                .get_mut("channel")
                .unwrap()
                .track
                .value[0] = channel as f32;
            let expected = raw(&mut original, &project);
            let actual = raw(&mut optimized, &project);
            let report = optimized.pointwise_fusion_report();
            assert_eq!(
                report.groups.len(),
                1,
                "{precision:?} channel={channel} {report:?}"
            );
            assert_eq!(report.groups[0].effects, [11, 12]);
            assert_eq!(report.original_passes - report.final_passes, 4);
            let mut worst_rgb = 0_f64;
            let mut worst_pixel = None;
            for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
                assert_eq!(
                    a[3], b[3],
                    "alpha must be independently exact: {precision:?} channel={channel} pixel={index}"
                );
                assert!(a.iter().chain(b).all(|v| v.is_finite()));
                for c in 0..3 {
                    let difference = (a[c] - b[c]).abs();
                    if difference > worst_rgb {
                        worst_rgb = difference;
                        worst_pixel = Some((index, c, *a, *b));
                    }
                }
            }
            let bound = match precision {
                ChannelPrecision::Unorm8 => 0.012,
                ChannelPrecision::Unorm16 => 8. / 65535.,
                ChannelPrecision::Float16 => 0.005,
                ChannelPrecision::Float32 => 0.00002,
            };
            assert!(
                worst_rgb <= bound,
                "{precision:?} channel={channel} RGB error={worst_rgb} bound={bound} worst_pixel={worst_pixel:?}"
            );
            rows.push(serde_json::json!({"precision":precision,"channel":channel,"alpha_exact":true,"worst_rgb":worst_rgb,"bound":bound,"fusion":report}));
        }
    }
    if let Some(output) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            std::path::PathBuf::from(output).join("proof-alpha-preserving.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

fn package() -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    manifest.id = "com.motionstudio.tests.subnormal-alpha".into();
    let mut definition = manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap()
        .clone();
    definition.id = "producer".into();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.edge_mode = EdgeMode::Clamp;
    definition.passes = vec![PassDefinition {
        shader: "shaders/producer.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.effects = vec![definition];
    let source = r#"fn main_fx(p:vec2<f32>)->vec4<f32>{
        let x=u32(floor(p.x));let y=u32(floor(p.y));
        var alpha=exp2(f32(i32(x%24u)-25));
        if x%32u==24u {alpha=0.0;} else if x%32u==25u {alpha=1.0/255.0;}
        else if x%32u==26u {alpha=1.0;}
        let color=vec3(0.03125+f32(y%16u)*0.015625,0.25,2.75);
        return vec4(color*alpha,alpha);
    }"#;
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(directory.path().join("shaders/producer.wgsl"), source).unwrap();
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
fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let format = motion_render::image_formats::native_format(
        motion_render::image_formats::color_storage(precision),
    );
    pollster::block_on(Renderer::new(&instance, None, format)).unwrap()
}
fn effect(id: u64, name: &str, package: &EffectPackage) -> EffectInstance {
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == name)
            .unwrap(),
        [W as f32, H as f32],
    )
}
fn raw(r: &mut Renderer, p: &Project) -> Vec<[f64; 4]> {
    let mut scene = Scene::new(p).unwrap();
    scene.sample(p, 0., None, &ExpressionEvaluator).unwrap();
    let target = r
        .image_capture_target(
            W,
            H,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                p.working_precision,
            ),
        )
        .unwrap();
    r.draw(&scene, target.view(), W, H).unwrap();
    reference::linear(&r.read_image_target(&target).unwrap())
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_subnormal_alpha_rounding_stores_and_alpha_lifting_preserve_declared_bounds() {
    let custom = package();
    let builtin = motion_effects::builtin::package().unwrap();
    let mut rows = vec![];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut project = Project::new(W, H, 30, 60).unwrap();
        project.working_precision = precision;
        project.background = [0.; 4];
        let mut layer = Layer::solid(
            47,
            "Subnormal alpha",
            [W as f32, H as f32],
            [W as f32 / 2., H as f32 / 2., 0.],
            [1.; 4],
        );
        let mut tint = effect(11, "tint", &builtin);
        tint.params.get_mut("p0001").unwrap().track.value = [0.07, 0.19, 0.31, 1.];
        tint.params.get_mut("p0002").unwrap().track.value = [0.91, 0.65, 0.43, 1.];
        let mut invert = effect(12, "invert", &builtin);
        invert.params.get_mut("channel").unwrap().track.value[0] = 5.;
        layer.effects = vec![effect(10, "producer", &custom), tint, invert];
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let mut registry = motion_effects::Registry::new_with_builtins().unwrap();
        registry.insert(custom.clone()).unwrap();
        let mut original = renderer(precision);
        let mut optimized = renderer(precision);
        original.set_effect_registry(registry.clone());
        optimized.set_effect_registry(registry);
        optimized.set_pointwise_fusion(true);
        let mut producer_project = project.clone();
        producer_project.layers[0].effects.truncate(1);
        producer_project.rebuild_plugin_dependencies();
        let producer_pixels = raw(&mut original, &producer_project);
        assert!(producer_pixels.iter().flatten().all(|v| v.is_finite()));
        if matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        ) {
            assert!(
                producer_pixels.iter().any(|v| v[3] > 0. && v[3] < 1.0e-6),
                "floating fixture must actually retain tiny alpha: {precision:?}"
            );
        }
        for amount in [0., 15., 37.5, 60., 68.25, 71., 87.5, 100.] {
            project.layers[0].effects[1]
                .params
                .get_mut("p0003")
                .unwrap()
                .track
                .value[0] = amount;
            let expected = raw(&mut original, &project);
            let actual = raw(&mut optimized, &project);
            assert_eq!(actual.len(), expected.len());
            assert!(
                expected
                    .iter()
                    .chain(&actual)
                    .flatten()
                    .all(|v| v.is_finite())
            );
            let worst = expected
                .iter()
                .zip(&actual)
                .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
                .fold(0_f64, f64::max);
            let bound = match precision {
                ChannelPrecision::Unorm8 => 0.012,
                ChannelPrecision::Unorm16 => 8. / 65535.,
                ChannelPrecision::Float16 => 0.005,
                ChannelPrecision::Float32 => 0.00002,
            };
            let report = optimized.pointwise_fusion_report();
            assert_eq!(actual, expected, "alpha lifting retains every real store");
            assert!(report.groups.is_empty());
            assert_eq!(report.final_passes, report.original_passes);
            assert_eq!(report.portable_compiles, 0);
            assert_eq!(
                report.retained.get(&FusionRetainReason::AlphaBoundary),
                Some(&1)
            );
            rows.push(serde_json::json!({"precision":precision,"amount":amount,"worst":worst,"bound":bound,"fusion":report}));
        }
    }
    if let Some(output) = std::env::var_os("MOTION_POINTWISE_EXTREME_ARTIFACTS") {
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            std::path::Path::new(&output).join("subnormal.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
    for row in rows {
        assert!(
            row["worst"].as_f64().unwrap() <= row["bound"].as_f64().unwrap(),
            "{row}"
        );
    }
}
