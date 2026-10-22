use super::*;
use motion_render::{AllocationBudget, Renderer, ResourceBudgetScope, ResourceRejection};

fn drain(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
const SOURCE: u64 = LOGICAL_SIZE as u64 * LOGICAL_SIZE as u64 * 4;
const LUT: u64 = 256 * 4;
const UNIFORM: u64 = std::mem::size_of::<EffectUniform>() as u64;
const OUTPUT: u64 = RASTER_SIZE as u64 * RASTER_SIZE as u64 * 8;
const TOTAL: u64 = SOURCE + LUT + UNIFORM + OUTPUT + 16;

#[test]
fn every_preview_allocation_refuses_before_creation_and_pending_uploads_retire() {
    let budget = AllocationBudget::default();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&renderer);
    let context = GpuContext {
        allocations: renderer.allocation_context(),
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
    };
    let snapshot = crate::node_editor::NodeEditor::default().snapshot();
    let compiled = snapshot.compile().unwrap();
    let fixed = budget.info().unwrap().admitted_bytes;
    for (allowance, required, queued) in [
        (1, SOURCE, 0),
        (SOURCE, SOURCE + LUT, SOURCE),
        (SOURCE + LUT, SOURCE + LUT + UNIFORM, SOURCE + LUT),
        (SOURCE + LUT + UNIFORM, TOTAL - 16, SOURCE + LUT),
        (TOTAL - 16, TOTAL, SOURCE + LUT),
    ] {
        budget.set_limit(fixed + allowance).unwrap();
        let validation = renderer
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let error = NodePreview::build(&context, &snapshot, &compiled)
            .err()
            .unwrap();
        assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: fixed + required,
                budget_bytes: fixed + allowance,
            }),
            "allowance={allowance}: {error}"
        );
        assert!(
            pollster::block_on(validation.pop()).is_none(),
            "budget refusal precedes native creation"
        );
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            fixed + queued,
            "retain unsubmitted source writes"
        );
        drain(&renderer);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            fixed,
            "failed construction retires completely"
        );
    }
    budget.set_limit(fixed + TOTAL).unwrap();
    let canonical = NodePreview::build(&context, &snapshot, &compiled).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, fixed + TOTAL);
    drop(canonical);
    assert_eq!(budget.info().unwrap().admitted_bytes, fixed + SOURCE + LUT);
    assert!(
        NodePreview::build(&context, &snapshot, &compiled).is_err(),
        "canonical validation uploads still occupy the shared allowance"
    );
    drain(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, fixed);
    let recovered = NodePreview::build(&context, &snapshot, &compiled).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, fixed + TOTAL);
    drop(recovered);
    drain(&renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, fixed);
}

fn render(
    renderer: &Renderer,
    preview: &mut NodePreview,
    target: &motion_render::CaptureTarget,
    seconds: f64,
) -> Vec<u8> {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    preview
        .encode(
            &mut encoder,
            &target.view,
            Viewport {
                rect: [0., 0., 64., 64.],
                seconds,
            },
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
        .unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })
        .unwrap();
    renderer.read_target(target).unwrap()
}

#[test]
fn refused_replacement_preserves_live_pixels_and_recovery_keeps_full_raster_capacity() {
    let budget = AllocationBudget::default();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    drain(&renderer);
    let context = GpuContext {
        allocations: renderer.allocation_context(),
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut editor = crate::node_editor::NodeEditor::default();
    let snapshot = editor.snapshot();
    let mut preview =
        NodePreview::build(&context, &snapshot, &snapshot.compile().unwrap()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    // Initial presentation format differs from construction, but uses the
    // same admitted color-contract buffer rather than allocating another one.
    let occupied = budget.info().unwrap().admitted_bytes;
    budget.set_limit(occupied).unwrap();
    let first = render(&renderer, &mut preview, &target, 0.);
    assert_eq!(budget.info().unwrap().admitted_bytes, occupied);
    editor.set_parameter_default("color", 0, 0.9).unwrap();
    let changed = editor.snapshot();
    let error = NodePreview::build(&context, &changed, &changed.compile().unwrap())
        .err()
        .unwrap();
    assert!(error.resource_rejection().is_some());
    assert_eq!(preview.stamp, snapshot.preview_stamp());
    assert_eq!(render(&renderer, &mut preview, &target, 0.), first);
    assert_ne!(
        render(&renderer, &mut preview, &target, 0.65),
        first,
        "retained graph still animates at the live limit"
    );
    budget.set_limit(occupied + TOTAL).unwrap();
    let mut recovered =
        NodePreview::build(&context, &changed, &changed.compile().unwrap()).unwrap();
    assert_eq!(recovered._texture.width(), RASTER_SIZE);
    assert_eq!(recovered._texture.height(), RASTER_SIZE);
    assert_eq!(
        recovered._texture.format(),
        wgpu::TextureFormat::Rgba16Float
    );
    assert_ne!(render(&renderer, &mut recovered, &target, 0.), first);
    drop((preview, recovered, target));
    drain(&renderer);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        motion_render::FIXED_RENDERER_ALLOCATION_BYTES
    );
}
