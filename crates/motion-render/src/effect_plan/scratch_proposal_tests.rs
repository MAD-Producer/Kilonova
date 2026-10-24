//! Exercise the proposed def/use graph with actual, unchanged PlanBuilder
//! output. This is planning evidence, not native allocation or pixel evidence.
use super::{scratch_graph as graph, scratch_lifetimes as placement, *};
use motion_core::{EffectInstance, Layer, Project};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Class {
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    working_space: u32,
    alpha: u32,
}

fn proposed(frame: &EffectFramePlan) -> graph::Graph<Class> {
    let passes: Vec<_> = frame
        .passes
        .iter()
        .map(|p| {
            // This adapter mirrors the current native slot contract. A future
            // descriptor table must make format explicit instead of extending
            // these slot rules when project precision changes.
            let format = if p.output == 7 {
                wgpu::TextureFormat::Rgba16Float
            } else if (1..=3).contains(&p.output) {
                wgpu::TextureFormat::Rgba8Unorm
            } else {
                wgpu::TextureFormat::Rgba8UnormSrgb
            };
            graph::Pass {
                input: p.input,
                source: p.source,
                output: p.output as usize,
                descriptor: placement::Descriptor {
                    class: Class {
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_SRC
                            | wgpu::TextureUsages::COPY_DST,
                        working_space: p.uniform.output_mode[0].to_bits(),
                        alpha: p.uniform.output_mode[1].to_bits(),
                    },
                    width: p.width,
                    height: p.height,
                    bytes_per_pixel: format.block_copy_size(None).unwrap(),
                },
                // Every current native effect render pass clears the complete
                // attachment, including sprite passes. Their intra-pass blending
                // is not a later attachment-load dependency.
                load: graph::Load::Clear,
            }
        })
        .collect();
    let draws: Vec<_> = frame
        .draws
        .iter()
        .map(|d| d.pass_start..d.pass_end)
        .collect();
    graph::build(&passes, &draws, 8, frame.passes.len()).unwrap()
}

fn instance(package: &EffectPackage, name: &str, id: u64, size: [f32; 2]) -> EffectInstance {
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == name)
        .unwrap();
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        size,
    )
}

fn fixture() -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(512, 512, 30, 60).unwrap();
    for (index, size) in [[256., 16.], [16., 256.], [256., 16.]]
        .into_iter()
        .enumerate()
    {
        let id = index as u64 + 1;
        let mut layer = Layer::solid(
            id,
            "scratch lifetime fixture",
            size,
            [256., 256., 0.],
            [1., 0.25, 0.5, 0.75],
        );
        for (i, effect) in ["polar_coordinates", "gaussian_blur", "tint"]
            .into_iter()
            .enumerate()
        {
            layer
                .effects
                .push(instance(&package, effect, id * 10 + i as u64, size));
        }
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    project
}

#[test]
fn actual_mixed_size_effect_plans_have_complete_def_use_and_terminal_consumers() {
    let project = fixture();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let frame = builder.build(&scene, &[0], 512, 512, true).unwrap();
    let mut original = vec![0; frame.buffer_bytes(&scene)];
    frame.write(&scene, &mut original).unwrap();
    let proposal = proposed(frame);
    assert_eq!(proposal.passes.len(), frame.passes.len());
    let placed = placement::place(
        &proposal.resources,
        placement::Limits {
            resources: frame.passes.len(),
            allocations: frame.passes.len(),
            bytes: u64::MAX,
        },
    )
    .unwrap();
    assert!(
        placed.bytes < scratch_capacity_bytes(&frame.scratch_sizes),
        "fixture must expose actual planned aspect-ratio waste, not an assumed percentage"
    );
    println!(
        "SCRATCH_PLACEMENT_PROPOSAL {}",
        serde_json::json!({
            "scope": "planning only; no native allocation or pixel measurement",
            "legacySemanticBytes": scratch_capacity_bytes(&frame.scratch_sizes),
            "proposedPhysicalBytes": placed.bytes,
            "draws": frame.draws.len(),
            "passes": frame.passes.len(),
            "logicalResources": proposal.resources.len(),
            "physicalAllocations": placed.allocations.iter().map(|allocation| serde_json::json!({
                "width": allocation.width, "height": allocation.height,
                "format": format!("{:?}", allocation.class.format),
                "bytesPerPixel": allocation.bytes_per_pixel,
                "workingSpaceBits": allocation.class.working_space,
                "alphaBits": allocation.class.alpha,
            })).collect::<Vec<_>>(),
        })
    );
    for (index, draw) in frame.draws.iter().enumerate() {
        let output = proposal.draw_outputs[index].unwrap();
        assert_eq!(output, proposal.passes[draw.pass_end - 1].output);
        assert!(proposal.resources[output].last_use > proposal.resources[output].first_use);
        let final_pass = &frame.passes[draw.pass_end - 1];
        assert_eq!(
            (
                proposal.resources[output].descriptor.width,
                proposal.resources[output].descriptor.height
            ),
            (final_pass.width, final_pass.height)
        );
    }
    for (index, pass) in proposal.passes.iter().enumerate() {
        for input in [pass.input, pass.source] {
            if let graph::Input::Resource(input) = input {
                assert_ne!(
                    placed.assignments[input], placed.assignments[pass.output],
                    "read/write native aliases would be invalid at pass {index}"
                );
            }
        }
    }
    let mut after = vec![0; original.len()];
    frame.write(&scene, &mut after).unwrap();
    assert_eq!(
        original, after,
        "proposal must not mutate the current packed/native contract"
    );
}

#[test]
fn actual_effected_image_dependencies_remain_external_to_scratch_lifetimes() {
    let mut project = fixture();
    let source = project.layers[0].id;
    let size = project.layers[1].size;
    let effect = &mut project.layers[1].effects[2];
    // Displacement explicitly accepts another layer after its effects.
    let package = motion_effects::builtin::package().unwrap();
    *effect = instance(&package, "displacement_map", 99, size);
    effect.image_input = Some(motion_model::EffectImageInput::Layer {
        layer: source,
        stage: motion_model::EffectImageStage::Effects,
    });
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut builder = PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
    let frame = builder.build(&scene, &[0], 512, 512, true).unwrap();
    assert!(!image_input_order(frame).unwrap().is_empty());
    assert!(frame.passes.iter().any(|p| p.resource_input > 0));
    let proposal = proposed(frame);
    assert_eq!(
        proposal.resources.len(),
        frame.passes.len(),
        "snapshot images are separately owned, not more scratch values"
    );
    assert!(proposal.draw_outputs.iter().all(Option::is_some));
}
