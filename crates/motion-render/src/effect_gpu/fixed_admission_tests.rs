use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn sdk_instances_are_lazy_bounded_and_refused_growth_preserves_the_old_allocation() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let budget = crate::AllocationBudget::new(1028).unwrap();
    let uploads = std::sync::Arc::new(crate::allocation_uploads::AllocationUploads::new(renderer.queue.clone()));
    let layout = bindings(&renderer.device, "instance admission fixture", 1);
    let mut effect = EffectGpu::new(&renderer.device, &renderer.queue, &layout, budget.clone(), uploads.clone()).unwrap();
    assert!(effect.state.sprite_buffer.is_none());
    assert_eq!(budget.info().unwrap().admitted_bytes, 1028);
    effect.state.upload_instances(&renderer.device, &renderer.queue, &[]).unwrap();
    assert!(effect.state.sprite_buffer.is_none());
    let sprite = crate::scene_generator::Sprite { rect: [0.; 4], color: [1.; 4], style: [0.; 4] };
    let failure = effect.state.upload_instances(&renderer.device, &renderer.queue, &[sprite]).unwrap_err();
    assert_eq!(failure.resource_rejection(), Some(&crate::ResourceRejection::Budget {
        scope: crate::ResourceBudgetScope::RenderAllocations, required_bytes: 1076, budget_bytes: 1028,
    }));
    assert!(failure.to_string().contains("effect instances count=1 phase=allocate"));
    assert!(effect.state.sprite_buffer.is_none());
    budget.set_limit(1076).unwrap();
    effect.state.upload_instances(&renderer.device, &renderer.queue, &[sprite]).unwrap();
    let old = effect.state.sprite_buffer.as_ref().unwrap().allocation().id();
    assert_eq!(effect.state.sprite_buffer.as_ref().unwrap().size(), 48);
    let failure = effect.state.upload_instances(&renderer.device, &renderer.queue, &[sprite; 2]).unwrap_err();
    assert_eq!(failure.resource_rejection(), Some(&crate::ResourceRejection::Budget {
        scope: crate::ResourceBudgetScope::RenderAllocations, required_bytes: 1172, budget_bytes: 1076,
    }));
    assert_eq!(effect.state.sprite_buffer.as_ref().unwrap().allocation().id(), old);
    budget.set_limit(1028).unwrap();
    effect.state.upload_instances(&renderer.device, &renderer.queue, &[sprite]).unwrap();
    assert_eq!(effect.state.sprite_buffer.as_ref().unwrap().allocation().id(), old);
    let too_many = vec![sprite; motion_effects::MAX_SPRITES + 1];
    assert!(effect.state.upload_instances(&renderer.device, &renderer.queue, &too_many).unwrap_err().resource_rejection().is_none());
    effect.state.release_scratch();
    assert_eq!(budget.info().unwrap().admitted_bytes, 1076, "queued instance writes still own the destination");
    renderer.queue.submit([]);
    uploads.submitted();
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 1028);
    budget.set_limit(1172).unwrap();
    effect.state.upload_instances(&renderer.device, &renderer.queue, &[sprite; 3]).unwrap();
    assert_eq!(effect.state.sprite_buffer.as_ref().unwrap().size(), 144);
    let device = renderer.device.clone();
    let queue = renderer.queue.clone();
    drop(effect);
    drop(uploads);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    drop(queue);
    renderer.check_health().unwrap();
}
