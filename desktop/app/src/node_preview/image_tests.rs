use super::*;
use crate::{node_editor::NodeEditor, node_image_fixture as fixture};
use motion_graph::{GraphDocument, PublicParameter, ValueType};

fn context(renderer: &motion_render::Renderer) -> GpuContext {
    GpuContext {
        allocations: renderer.allocation_context(),
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
    }
}
fn assert_solid(pixels: &[u8], expected: [u8; 4]) {
    assert!(
        pixels
            .chunks_exact(4)
            .all(|p| p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2)),
        "actual center={:?} expected={expected:?}",
        &pixels[(32 * 64 + 32) * 4..][..4]
    );
}
fn compile_and_render(
    renderer: &motion_render::Renderer,
    document: GraphDocument,
    overflow: bool,
    expected: [u8; 4],
) {
    let snapshot = NodeEditor::new(document).snapshot();
    let compiled = snapshot.compile_runtime().unwrap();
    let mut preview = NodePreview::build(&context(renderer), &snapshot, &compiled).unwrap();
    assert_eq!(preview._data.is_some(), overflow);
    assert_eq!(
        preview._images.len() + usize::from(overflow),
        compiled.manifest.effects[0].resources.len()
    );
    assert_solid(&super::tests::pixels(renderer, &mut preview, 0.), expected);
    let baked = snapshot.compile().unwrap();
    let mut comparison = NodePreview::build(&context(renderer), &snapshot, &baked).unwrap();
    assert_eq!(
        super::tests::pixels(renderer, &mut comparison, 0.),
        super::tests::pixels(renderer, &mut preview, 0.)
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn independent_image_preview_uploads_all_slots_alongside_numeric_overflow() {
    let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    compile_and_render(
        &renderer,
        fixture::many_images(4),
        false,
        [3, 100, 200, 255],
    );
    let mut graph = fixture::many_images(3);
    for index in 0..32 {
        graph.public_parameters.push(PublicParameter {
            id: format!("p{index}"),
            name: "参数".into(),
            english_name: "Parameter".into(),
            value_type: ValueType::Float,
            default: [0.; 4],
            min: 0.,
            max: 1.,
            step: 0.1,
            units: "".into(),
            animatable: true,
        });
    }
    compile_and_render(&renderer, graph, true, [2, 100, 200, 255]);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_candidates_replace_content_and_leave_live_preview_intact_on_refusal() {
    let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    let context = context(&renderer);
    let snapshot = NodeEditor::new(fixture::many_images(1)).snapshot();
    let compiled = snapshot.compile_runtime().unwrap();
    let mut active = NodePreview::build(&context, &snapshot, &compiled).unwrap();
    let original = super::tests::pixels(&renderer, &mut active, 0.);
    assert_solid(&original, [0, 100, 200, 255]);
    let mut missing = snapshot.compile_runtime().unwrap();
    missing.manifest.effects[0].resources[0] = "assets/missing.png".into();
    let definition = &mut missing.manifest.effects[0];
    definition.execution = Some(
        motion_effects::execution_template::ExecutionTemplate::single_pass(
            definition,
            &missing.shader,
        ),
    );
    missing.manifest.validate().unwrap();
    let failure = NodePreview::build_reusing(&context, &snapshot, &missing, Some(&active))
        .err()
        .unwrap();
    assert!(
        failure.to_string().contains("resource is missing"),
        "{failure}"
    );
    assert_eq!(super::tests::pixels(&renderer, &mut active, 0.), original);
    // A canonical pipeline cache retains code, never old image owners/pixels.
    let next = NodeEditor::new(fixture::many_images(2)).snapshot();
    let next_compiled = next.compile_runtime().unwrap();
    let mut replacement =
        NodePreview::build_reusing(&context, &next, &next_compiled, Some(&active)).unwrap();
    assert_solid(
        &super::tests::pixels(&renderer, &mut replacement, 0.),
        [1, 100, 200, 255],
    );
    let budget = context.allocations.budget();
    let allowance = budget.info().unwrap().budget_bytes;
    budget
        .set_limit(budget.info().unwrap().admitted_bytes)
        .unwrap();
    let error = NodePreview::build_reusing(&context, &next, &next_compiled, Some(&active))
        .err()
        .unwrap();
    assert!(error.resource_rejection().is_some());
    budget.set_limit(allowance).unwrap();
    assert_eq!(super::tests::pixels(&renderer, &mut active, 0.), original);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_preview_pipeline_cache_releases_every_resource_after_submission() {
    let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    let context = context(&renderer);
    let budget = context.allocations.budget();
    let baseline = budget.info().unwrap().admitted_bytes;
    let snapshot = NodeEditor::new(fixture::many_images(4)).snapshot();
    let compiled = snapshot.compile_runtime().unwrap();
    let preview = NodePreview::build(&context, &snapshot, &compiled).unwrap();
    let cache = preview.pipeline_cache();
    assert_eq!(preview._images.len(), 4);
    drop(preview);
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        baseline,
        "code cache cannot pin any image texture or pending upload"
    );
    let mut rebuilt =
        NodePreview::build_with_cache(&context, &snapshot, &compiled, Some(&cache)).unwrap();
    assert_eq!(rebuilt.pipeline_builds, 0);
    assert_solid(
        &super::tests::pixels(&renderer, &mut rebuilt, 0.),
        [3, 100, 200, 255],
    );
}
