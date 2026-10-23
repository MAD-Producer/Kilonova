use motion_render::{AllocationBudget, GpuTimer, Renderer, ResourceBudgetScope, ResourceRejection, FIXED_RENDERER_ALLOCATION_BYTES};

fn profiled(budget: AllocationBudget, timestamps: bool) -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(Renderer::new_profiled_with_allocation_budget(
        &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, timestamps, budget,
    )).unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn timer_buffers_share_renderer_admission_and_retire_after_teardown() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let renderer = profiled(budget.clone(), true);
    // Four 256-byte resolve slots and four 32-byte timestamp readbacks.
    // Query-set/driver metadata is not a buffer descriptor capacity.
    let timer = GpuTimer::new(&renderer).unwrap()
        .expect("the validation GPU must support timestamp queries");
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES + 4 * 256 + 4 * 32);
    assert_eq!(budget.info().unwrap().live_allocations, 11);
    drop(timer);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
    let device = renderer.device.clone();
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn each_timing_buffer_refusal_is_typed_and_partial_construction_retires() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let renderer = profiled(budget.clone(), true);
    for (extra_required, label) in [
        (1024, "Motion Studio timing resolve"),
        (1056, "Motion Studio timing words"),
        (1088, "Motion Studio timing words"),
        (1120, "Motion Studio timing words"),
        (1152, "Motion Studio timing words"),
    ] {
        let required = FIXED_RENDERER_ALLOCATION_BYTES + extra_required;
        budget.set_limit(required - 1).unwrap();
        let error = GpuTimer::new(&renderer).err()
            .expect("a supported timer must not turn capacity refusal into None");
        assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: required, budget_bytes: required - 1,
        }));
        assert!(error.to_string().contains(&format!("phase=allocate label={label}")), "{error}");
        // No submit, retry or poll is needed to roll back a partial timer.
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
        assert_eq!(budget.info().unwrap().live_allocations, 6);
    }
    budget.set_limit(FIXED_RENDERER_ALLOCATION_BYTES + 1152).unwrap();
    assert!(GpuTimer::new(&renderer).unwrap().is_some());
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unsupported_timestamps_do_not_reserve_or_consume_the_budget() {
    let budget = AllocationBudget::new(FIXED_RENDERER_ALLOCATION_BYTES).unwrap();
    let renderer = profiled(budget.clone(), false);
    let before = budget.info().unwrap();
    assert!(GpuTimer::new(&renderer).unwrap().is_none());
    assert_eq!(budget.info().unwrap(), before);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn refused_timer_replacement_preserves_the_old_timer_and_existing_slots() {
    let budget = AllocationBudget::new(FIXED_RENDERER_ALLOCATION_BYTES + 1152).unwrap();
    let renderer = profiled(budget.clone(), true);
    let mut timer = GpuTimer::new(&renderer).unwrap().unwrap();
    let first = timer.begin(7).unwrap();
    let before = budget.info().unwrap();
    let failure = GpuTimer::new(&renderer).err().unwrap();
    assert_eq!(failure.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::RenderAllocations,
        required_bytes: FIXED_RENDERER_ALLOCATION_BYTES + 1152 + 1024,
        budget_bytes: FIXED_RENDERER_ALLOCATION_BYTES + 1152,
    }));
    assert_eq!(budget.info().unwrap(), before);
    budget.set_limit(FIXED_RENDERER_ALLOCATION_BYTES).unwrap();
    for sequence in 8..11 { assert!(timer.begin(sequence).is_some()); }
    assert!(timer.begin(11).is_none());
    timer.cancel_unsubmitted(first);
    assert_eq!(timer.begin(12), Some(first));
    drop(timer);
    assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn timing_capacity_follows_abandoned_and_submitted_command_owners() {
    let budget = AllocationBudget::new(8 << 20).unwrap();
    let renderer = profiled(budget.clone(), true);
    renderer.queue.submit([]);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    for submit in [false, true] {
        let mut timer = GpuTimer::new(&renderer).unwrap().unwrap();
        let slot = timer.begin(4).unwrap();
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        // Resolving unwritten queries is legal and returns zero words. This
        // exercises the actual resolve/copy resources without fake GPU costs.
        timer.resolve(slot, &mut encoder);
        let commands = encoder.finish();
        drop(timer);
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES + 1152);
        if submit {
            renderer.submit_allocated(commands, |_| {}).unwrap();
        } else {
            renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES + 1152);
            drop(commands);
        }
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, FIXED_RENDERER_ALLOCATION_BYTES);
    }
    assert!(renderer.gpu_error().is_none());
}
