//! Persisted package templates must drive native work, not just report a DAG.
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::execution_template::{
    ExecutionTemplate, TemplateNode, TemplateValue, EXECUTION_TEMPLATE_CAPABILITY,
};
use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
use motion_effects::{AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace};
use motion_render::{
    effect_plan::PlanBuilder, RenderError, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
};
use std::sync::Arc;

#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

const W: u32 = 32;
const H: u32 = 24;
const SOURCES: [&str; 4] = [
    "fn main_fx(p:vec2<f32>)->vec4<f32>{let a=sample_input(p);return vec4(a.r+0.125,a.g,a.b,1.0);}",
    "fn main_fx(p:vec2<f32>)->vec4<f32>{let b=sample_input(p);let c=sample_source(p);return vec4(b.r,c.g,b.b,1.0);}",
    "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4(0.2,0.4,0.6,1.0);}",
    "fn main_fx(p:vec2<f32>)->vec4<f32>{let a=sample_input(p);return vec4(a.r,1.0-a.g,a.b,1.0);}",
];

fn fixture_package(pressure: bool, enabled: bool) -> Arc<EffectPackage> {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    manifest.id = format!(
        "test.runtime-template.{}.{enabled}",
        if pressure { "pressure" } else { "diamond" }
    );
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "invert")
        .unwrap()
        .clone();
    definition.id = "runtime".into();
    definition.params.clear();
    definition.resources.clear();
    definition.shader_data = None;
    definition.native_editor = None;
    definition.editor = None;
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.output_bounds = None;
    definition.passes.clear();
    definition.required_capabilities.clear();
    let sources: Vec<_> = if pressure {
        vec![SOURCES[2]; 5]
    } else {
        SOURCES.to_vec()
    };
    let mut identities = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let path = format!("shaders/pass-{i}.wgsl");
        std::fs::write(directory.path().join(&path), source).unwrap();
        let shader = motion_effects::shader::compile(source, "main_fx").unwrap();
        identities.push(motion_effects::shader::compiled_code_identity(&shader).digest);
        definition.passes.push(PassDefinition {
            shader: path,
            entry: "main_fx".into(),
        });
    }
    let mut template = ExecutionTemplate::single_pass(
        &definition,
        &motion_effects::shader::compile(sources[0], "main_fx").unwrap(),
    );
    let node = |id, pass, input, source| TemplateNode {
        id,
        pass,
        input,
        source,
        code_identity: identities[usize::from(pass)],
    };
    let input = TemplateValue::Input;
    let value = |id| TemplateValue::Node { id };
    if pressure {
        // Three branches need four simultaneous working images. Admission must
        // depend on their real dimensions/format and the caller's byte budget.
        template.nodes = vec![
            node(1, 0, input, input),
            node(2, 1, input, input),
            node(3, 2, input, input),
            node(4, 3, value(1), value(2)),
            node(5, 4, value(4), value(3)),
        ];
        template.output = 5;
    } else {
        // Neither the manifest pass list B,D,A,C nor the serialized node vector
        // D,C,A,B is executable order. A must be prepared once for both B and C.
        template.nodes = vec![
            node(40, 1, value(20), value(30)),
            node(30, 3, value(10), value(10)),
            node(10, 2, input, input),
            node(20, 0, value(10), value(10)),
        ];
        template.output = 40;
    }
    if enabled {
        definition.execution = Some(template);
        definition
            .required_capabilities
            .push(EXECUTION_TEMPLATE_CAPABILITY.into());
    }
    manifest.effects = vec![definition];
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = motion_effects::package_directory(directory.path()).unwrap();
    Arc::new(EffectPackage::from_bytes(bytes).unwrap())
}

fn make_project(package: &EffectPackage, precision: ChannelPrecision) -> Project {
    make_project_at(package, precision, [W, H])
}
fn make_project_at(
    package: &EffectPackage,
    precision: ChannelPrecision,
    [w, h]: [u32; 2],
) -> Project {
    let mut project = Project::new(w, h, 30, 60).unwrap();
    project.working_precision = precision;
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        47,
        "Runtime template",
        [w as f32, h as f32],
        [w as f32 / 2., h as f32 / 2., 0.],
        [1.; 4],
    );
    layer.effects.push(EffectInstance::new(
        29,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}
fn scene(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &ExpressionEvaluator)
        .unwrap();
    scene
}
fn registry(package: Arc<EffectPackage>) -> Registry {
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    registry
}
fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let format = motion_render::image_formats::native_format(
        motion_render::image_formats::color_storage(precision),
    );
    pollster::block_on(Renderer::new(&instance, None, format)).unwrap()
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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn persisted_package_diamond_controls_actual_native_order_and_four_precision_pixels() {
    let package = fixture_package(false, true);
    let legacy = fixture_package(false, false);
    let mut rows = vec![];
    for precision in reference::MODES {
        let project = make_project(&package, precision);
        let mut builder = PlanBuilder::new(registry(package.clone())).unwrap();
        builder.set_precision(precision);
        builder.build(&scene(&project), &[0], W, H, true).unwrap();
        let passes = &builder.frame.passes;
        assert_eq!(passes.len(), 7);
        assert_eq!(
            passes[2..6]
                .iter()
                .map(|p| p.uniform.clock[2])
                .collect::<Vec<_>>(),
            [2., 0., 3., 1.]
        );
        assert_eq!(passes[3].input, passes[2].output as i32);
        assert_eq!(passes[4].input, passes[2].output as i32);
        assert_eq!(passes[5].input, passes[3].output as i32);
        assert_eq!(passes[5].source, passes[4].output as i32);
        assert!(passes[2..6]
            .iter()
            .all(|p| p.output as i32 != p.input && p.output as i32 != p.source));

        let mut renderer = renderer(precision);
        renderer.set_effect_registry(registry(package.clone()));
        let target = renderer
            .image_capture_target(W, H, contract(precision))
            .unwrap();
        let stats = renderer
            .draw(&scene(&project), target.view(), W, H)
            .unwrap();
        let raw = renderer.read_image_target(&target).unwrap();
        let actual = reference::linear(&raw);
        // Independent storage-aware arithmetic, with the package's four real
        // intermediate stores. No production shader is used as the oracle.
        let a = reference::store([0.2, 0.4, 0.6, 1.], precision);
        let b = reference::store([a[0] + 0.125, a[1], a[2], 1.], precision);
        let c = reference::store([a[0], 1. - a[1], a[2], 1.], precision);
        let expected = reference::store([b[0], c[1], b[2], 1.], precision);
        let tolerance = match precision {
            ChannelPrecision::Unorm8 => 0.015,
            ChannelPrecision::Unorm16 => 0.00025,
            ChannelPrecision::Float16 => 0.002,
            ChannelPrecision::Float32 => 0.000005,
        };
        let worst = actual
            .iter()
            .flat_map(|p| (0..4).map(move |i| (p[i] - expected[i]).abs()))
            .fold(0_f64, f64::max);
        assert!(
            worst <= tolerance,
            "{precision:?} expected={expected:?} actual={:?} worst={worst}",
            actual[0]
        );
        assert_eq!(
            stats.draw_calls, 8,
            "one draw plus seven actual native passes, without repeating shared A"
        );
        let builds = renderer.effect_pipeline_builds();
        renderer
            .draw(&scene(&project), target.view(), W, H)
            .unwrap();
        assert_eq!(renderer.effect_pipeline_builds(), builds);
        assert_eq!(
            renderer.read_image_target(&target).unwrap().pixels,
            raw.pixels
        );
        renderer.set_effect_registry(registry(legacy.clone()));
        renderer
            .draw(
                &scene(&make_project(&legacy, precision)),
                target.view(),
                W,
                H,
            )
            .unwrap();
        assert_ne!(
            renderer.read_image_target(&target).unwrap().pixels,
            raw.pixels,
            "the manifest's ordinary linear order must visibly differ; ignoring the template is a failure"
        );
        rows.push(serde_json::json!({"precision":precision,"shader_order":[2,0,3,1],"native_passes":7,
            "actual_draw_calls":stats.draw_calls,"worst_linear_error":worst,"legacy_control_differs":true,"warm_pipeline_builds":0}));
    }
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            std::path::PathBuf::from(out).join("runtime-template-diamond.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

fn resource_cause(error: &RenderError) -> ResourceRejection {
    match error {
        RenderError::Diagnostic { cause, .. } | RenderError::Context { cause, .. } => {
            resource_cause(cause)
        }
        RenderError::Shared(cause) => resource_cause(cause),
        RenderError::Resource { rejection, .. } => *rejection,
        _ => panic!("lost typed resource error: {error:?}"),
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn four_live_branch_images_use_real_storage_and_packed_bindings_in_every_precision() {
    let package = fixture_package(true, true);
    for precision in reference::MODES {
        let project = make_project(&package, precision);
        let scene = scene(&project);
        let mut builder = PlanBuilder::new(registry(package.clone())).unwrap();
        builder.set_precision(precision);
        builder.build(&scene, &[0], W, H, true).unwrap();
        let frame = &builder.frame;
        assert_eq!(frame.scratch_allocations.len(), 4, "{precision:?}");
        assert!(
            frame.passes.iter().any(|p| p.output >= 8),
            "virtual names must reach the real planner"
        );
        let bpp = contract(precision).preferred_storage().unwrap().bytes_per_pixel();
        assert_eq!(frame.scratch_bytes(), 4 * u64::from(W) * u64::from(H) * bpp);
        let mut packed = vec![0; frame.buffer_bytes(&scene)];
        frame.write(&scene, &mut packed).unwrap();
        let word = |offset| u32::from_le_bytes(packed[offset..offset + 4].try_into().unwrap());
        let pass_start = word(5 * 4) as usize;
        for (i, binding) in frame.scratch_bindings.iter().enumerate() {
            let offset = pass_start + 40 * i;
            assert_eq!(word(offset + 4) as i32, binding.input);
            assert_eq!(word(offset + 8) as i32, binding.source);
            assert_eq!(word(offset + 12), binding.output);
            assert!(
                binding.output < 4,
                "packed outputs use physical allocations, not virtual names"
            );
        }
        let mut renderer = renderer(precision);
        renderer.set_effect_registry(registry(package.clone()));
        let target = renderer
            .image_capture_target(W, H, contract(precision))
            .unwrap();
        let stats = renderer.draw(&scene, target.view(), W, H).unwrap();
        assert_eq!(stats.draw_calls, frame.passes.len() as u32 + 1);
        let actual = reference::linear(&renderer.read_image_target(&target).unwrap());
        let expected = reference::store([0.2, 0.4, 0.6, 1.0], precision);
        let tolerance = match precision {
            ChannelPrecision::Unorm8 => 0.015,
            ChannelPrecision::Unorm16 => 0.00025,
            ChannelPrecision::Float16 => 0.002,
            ChannelPrecision::Float32 => 0.000005,
        };
        let worst = actual
            .iter()
            .flat_map(|p| (0..4).map(move |i| (p[i] - expected[i]).abs()))
            .fold(0_f64, f64::max);
        assert!(
            worst <= tolerance,
            "{precision:?} expected={expected:?} actual={:?} worst={worst}",
            actual[0]
        );
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn runtime_branch_storage_refusal_preserves_previous_gpu_pixels_and_recovers() {
    let healthy = fixture_package(false, true);
    let pressure = fixture_package(true, true);
    let precision = ChannelPrecision::Float16;
    // Each real Float16 image occupies 18 MiB: the healthy diamond fits the
    // 64 MiB policy with three allocations; four live images require 72 MiB.
    let [w, h] = [1536, 1536];
    let healthy_scene = scene(&make_project_at(&healthy, precision, [w, h]));
    let pressure_scene = scene(&make_project_at(&pressure, precision, [w, h]));
    let mut renderer = renderer(precision);
    renderer
        .set_scratch_budget(motion_effects::SCRATCH_BUDGET_FLOOR)
        .unwrap();
    renderer.set_effect_registry(registry(healthy.clone()));
    let target = renderer
        .image_capture_target(w, h, contract(precision))
        .unwrap();
    renderer.draw(&healthy_scene, target.view(), w, h).unwrap();
    let previous = renderer.read_image_target(&target).unwrap().pixels;
    renderer.set_effect_registry(registry(pressure.clone()));
    let before = renderer.effect_pipeline_builds();
    let error = renderer
        .draw(&pressure_scene, target.view(), w, h)
        .unwrap_err();
    assert!(matches!(
        resource_cause(&error),
        ResourceRejection::Budget {
            scope: ResourceBudgetScope::EffectScratch,
            required_bytes,
            budget_bytes,
        } if required_bytes == 4 * u64::from(w) * u64::from(h) * 8
            && budget_bytes == motion_effects::SCRATCH_BUDGET_FLOOR
    ));
    assert!(error.to_string().contains("layer 47, effect 29"), "{error}");
    assert_eq!(
        renderer.effect_pipeline_builds(),
        before,
        "refuse before new native pipelines"
    );
    assert_eq!(
        renderer.read_image_target(&target).unwrap().pixels,
        previous
    );
    renderer.set_effect_registry(registry(healthy.clone()));
    renderer.draw(&healthy_scene, target.view(), w, h).unwrap();
    assert_eq!(
        renderer.read_image_target(&target).unwrap().pixels,
        previous
    );
}
