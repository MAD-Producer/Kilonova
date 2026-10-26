//! Native consumer acceptance; logical DAG counts alone cannot prove shared work.
use motion_core::{
    Asset, Content, EffectImageInput, EffectImageStage, EffectInstance, ExpressionEvaluator, Layer,
    Project,
};
use motion_effects::{
    execution_plan::{ExecutionCapability, ExecutionPlanError},
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, Registry, WorkingSpace,
};
use motion_render::{
    effect_plan::{native_execution_capabilities, native_snapshot_limits, PlanBuilder},
    RenderError, Renderer, Scene,
};
use std::collections::{BTreeMap, BTreeSet};

#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

const W: u32 = 48;
const H: u32 = 32;

fn effect(id: u64, name: &str, source: Option<u64>) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == name)
        .unwrap();
    let mut instance = EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [W as f32, H as f32],
    );
    instance.image_input = source.map(|layer| EffectImageInput::Layer {
        layer,
        stage: EffectImageStage::Effects,
    });
    instance
}

fn diamond(precision: ChannelPrecision) -> Project {
    let mut project = Project::new(W, H, 30, 60).unwrap();
    project.working_precision = precision;
    project.background = [0.; 4];
    project.assets.push(Asset {
        id: 1,
        path: "assets/typed-dag-pattern.png".into(),
        width: W,
        height: H,
    });
    for id in 1..=4 {
        let mut layer = Layer::solid(
            id,
            "DAG layer",
            [W as f32, H as f32],
            [W as f32 / 2., H as f32 / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset: 1 };
        layer.visible = id == 4;
        layer.effects = match id {
            1 => vec![effect(11, "tint", None), effect(12, "invert", None)],
            2 => vec![effect(21, "displacement_map", Some(1))],
            3 => vec![effect(31, "displacement_map", Some(1))],
            _ => vec![
                effect(41, "displacement_map", Some(2)),
                effect(42, "displacement_map", Some(3)),
            ],
        };
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

fn sample(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &ExpressionEvaluator)
        .unwrap();
    scene
}
fn builder(project: &Project) -> (PlanBuilder, Scene) {
    let scene = sample(project);
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    builder.set_precision(project.working_precision);
    builder.build(&scene, &[1], W, H, true).unwrap();
    (builder, scene)
}
fn original_plan_error(error: &RenderError) -> &ExecutionPlanError {
    match error {
        RenderError::ExecutionPlan(source) => source,
        RenderError::Diagnostic { cause, .. } | RenderError::Context { cause, .. } => {
            original_plan_error(cause)
        }
        _ => panic!("lost typed execution cause: {error:?}"),
    }
}

#[test]
fn saved_authoring_diamond_lowers_to_typed_native_snapshot_inputs_and_lifetimes() {
    let project = diamond(ChannelPrecision::Float16);
    let reopened: Project = serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
    reopened.validate().unwrap();
    let (builder, scene) = builder(&reopened);
    let execution = builder.snapshot_execution(&scene, &[]).unwrap();
    let graph = execution.plan().unwrap();
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(
        execution
            .draw_order()
            .iter()
            .map(|&i| builder.frame.draws[i].layer)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let shared = graph.nodes.iter().find(|n| n.origin.layer == 1).unwrap();
    assert_eq!(
        graph
            .nodes
            .iter()
            .flat_map(|n| &n.inputs)
            .filter(|p| p.value == shared.output)
            .count(),
        2
    );
    assert_eq!(
        execution
            .checked()
            .unwrap()
            .lifetimes()
            .iter()
            .find(|v| v.value == shared.output)
            .unwrap()
            .consumer_count,
        3
    );
    assert_eq!(
        shared.output_type.image.precision,
        ChannelPrecision::Float16
    );
    assert_eq!(shared.output_type.rect.size, [W, H]);
    assert!(shared.cost.passes > 0);
    assert!(shared.cost.shader_bytes > 0);
    assert!(execution
        .checked()
        .unwrap()
        .total_cost()
        .samples_per_fragment
        .is_some_and(|n| n > 0));
    assert_eq!(
        graph.required_capabilities,
        BTreeSet::from([ExecutionCapability::ImageDagV1])
    );
}

#[test]
fn numeric_updates_keep_actual_code_identity_and_structure_changes_only_the_affected_draw() {
    let mut project = diamond(ChannelPrecision::Float32);
    let (mut builder, scene) = builder(&project);
    let identities = |execution: &motion_render::effect_plan::SnapshotExecution| {
        execution
            .plan()
            .unwrap()
            .nodes
            .iter()
            .map(|n| (n.origin.layer, n.code_identity))
            .collect::<BTreeMap<_, _>>()
    };
    let initial = identities(&builder.snapshot_execution(&scene, &[]).unwrap());
    project.layers[0].effects[1]
        .params
        .get_mut("blend")
        .unwrap()
        .track
        .value[0] = 37.5;
    let scene = sample(&project);
    builder.build(&scene, &[1], W, H, true).unwrap();
    assert_eq!(
        identities(&builder.snapshot_execution(&scene, &[]).unwrap()),
        initial
    );
    project.layers[1].effects.push(effect(22, "tint", None));
    project.rebuild_plugin_dependencies();
    let scene = sample(&project);
    builder.build(&scene, &[1], W, H, true).unwrap();
    let changed = identities(&builder.snapshot_execution(&scene, &[]).unwrap());
    assert_eq!(changed[&1], initial[&1]);
    assert_eq!(changed[&3], initial[&3]);
    assert_ne!(changed[&2], initial[&2]);
}

#[test]
fn native_consumer_preserves_cycle_budget_and_capability_causes_and_recovers() {
    let project = diamond(ChannelPrecision::Unorm8);
    let (mut builder, scene) = builder(&project);
    let cap = native_snapshot_limits(builder.device_dimension);
    let supported = native_execution_capabilities();
    let error = builder
        .snapshot_execution_checked(&scene, &[], &BTreeSet::new(), cap)
        .unwrap_err();
    assert!(matches!(
        original_plan_error(&error),
        ExecutionPlanError::UnsupportedCapability {
            capability: ExecutionCapability::ImageDagV1
        }
    ));
    let mut tiny = cap;
    tiny.nodes = 2;
    let error = builder
        .snapshot_execution_checked(&scene, &[], &supported, tiny)
        .unwrap_err();
    assert!(matches!(
        original_plan_error(&error),
        ExecutionPlanError::Budget {
            resource: "nodes",
            required: 3,
            maximum: 2,
            ..
        }
    ));
    let source = builder
        .frame
        .draws
        .iter()
        .position(|d| d.layer == 1)
        .unwrap();
    let reader = builder
        .frame
        .draws
        .iter()
        .position(|d| d.layer == 2)
        .unwrap();
    let pass = builder.frame.draws[source].pass_end - 1;
    let original = builder.frame.passes[pass];
    builder.frame.passes[pass].resource_input = reader as i32 + 1;
    builder.frame.passes[pass].uniform.params[30][1] = 1.;
    let error = builder.snapshot_execution(&scene, &[]).unwrap_err();
    assert!(matches!(
        original_plan_error(&error),
        ExecutionPlanError::Cycle { total: 2, .. }
    ));
    assert!(error.to_string().contains("phase=snapshot_dag"));
    assert!(
        error.to_string().contains("layers=[1, 2]") || error.to_string().contains("layers=[2, 1]")
    );
    builder.frame.passes[pass] = original;
    assert_eq!(
        builder
            .snapshot_execution(&scene, &[])
            .unwrap()
            .draw_order()
            .len(),
        3
    );
    let mut graph = builder
        .snapshot_execution(&scene, &[])
        .unwrap()
        .plan()
        .unwrap()
        .clone();
    graph.nodes[1].inputs[0].expected.image.precision = ChannelPrecision::Float16;
    assert!(matches!(
        graph.check(&supported, cap),
        Err(ExecutionPlanError::TypeMismatch { .. })
    ));
    builder.snapshot_execution(&scene, &[]).unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_gpu_diamond_encodes_the_shared_producer_once_across_four_precisions_and_stack_orders() {
    let mut rows = vec![];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let project = diamond(precision);
        let mut project: Project =
            serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let format = motion_render::image_formats::native_format(
            motion_render::image_formats::color_storage(precision),
        );
        let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
        let pixels: Vec<_> = (0..H)
            .flat_map(|y| {
                (0..W).flat_map(move |x| {
                    [
                        (x * 255 / (W - 1)) as u8,
                        (y * 255 / (H - 1)) as u8,
                        ((x * 43 + y * 71) % 256) as u8,
                        255,
                    ]
                })
            })
            .collect();
        renderer.upload_image(1, W, H, &pixels).unwrap();
        let target = renderer
            .image_capture_target(
                W,
                H,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
            )
            .unwrap();
        let mut first = None;
        for reverse in [false, true] {
            if reverse {
                project.layers.reverse();
            }
            renderer
                .draw(&sample(&project), target.view(), W, H)
                .unwrap();
            let pixels = reference::linear(&renderer.read_image_target(&target).unwrap());
            assert!(pixels.iter().flatten().all(|v| v.is_finite()));
            assert!(pixels.iter().any(|p| p[3] > 0.5));
            assert!(pixels.iter().any(|p| (p[0] - pixels[0][0]).abs() > 0.05));
            if let Some(expected) = &first {
                assert_eq!(&pixels, expected, "stack order changed {precision:?}");
            } else {
                first = Some(pixels);
            }
            let trace = renderer.snapshot_encodes();
            assert_eq!(trace.len(), 3, "actual native encode trace: {trace:?}");
            for layer in [1, 2, 3] {
                assert_eq!(trace.iter().filter(|e| e.layer == layer).count(), 1);
            }
            assert!(trace.iter().all(|e| e.encoded_passes > 0));
            rows.push(serde_json::json!({"precision":precision,"reverse":reverse,"native_encoded_snapshots":trace}));
        }
    }
    if let Some(output) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            std::path::Path::new(&output).join("native-diamond.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
