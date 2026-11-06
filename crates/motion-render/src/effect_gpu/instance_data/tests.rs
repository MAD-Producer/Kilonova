use super::*;

fn fixture() -> (
    crate::Renderer,
    wgpu::BindGroupLayout,
    wgpu::TextureView,
    wgpu::Sampler,
) {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let device = &renderer.device;
    let layout = crate::effect_gpu::bindings(device, "instance validation fixture", 4);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("instance validation fixture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let sampler = device.create_sampler(&Default::default());
    let view = texture.create_view(&Default::default());
    (renderer, layout, view, sampler)
}

fn entries<'a>(
    view: &'a wgpu::TextureView,
    sampler: &'a wgpu::Sampler,
) -> [wgpu::BindGroupEntry<'a>; 8] {
    std::array::from_fn(|index| wgpu::BindGroupEntry {
        binding: index as u32,
        resource: if index % 2 == 0 {
            wgpu::BindingResource::TextureView(view)
        } else {
            wgpu::BindingResource::Sampler(sampler)
        },
    })
}

fn poll(device: &wgpu::Device, kind: wgpu::PollType) -> Result<wgpu::PollStatus, wgpu::PollError> {
    // Observe the actual argument passed by production, then forward it to the
    // real device. This test does not substitute a successful mock GPU result.
    assert!(
        !kind.is_wait(),
        "instance validation may not wait for submitted work"
    );
    device.poll(kind)
}

fn invalid_message(error: RenderError) -> String {
    let display = error.to_string();
    let RenderError::Invalid(message) = error else {
        panic!("instance validation must retain its Invalid error variant: {display}")
    };
    assert_eq!(display, format!("GPU validation: {message}"));
    eprintln!("{display}");
    message
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn healthy_instance_binding_uses_nonwaiting_native_validation() {
    let (renderer, layout, view, sampler) = fixture();
    let group = create_instance_group(
        &renderer.device,
        &layout,
        &entries(&view, &sampler),
        31,
        7,
        |kind| poll(&renderer.device, kind),
    )
    .unwrap();
    drop(group);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_invalid_binding_keeps_native_cause_and_same_device_recovers() {
    let (renderer, layout, view, sampler) = fixture();
    let values = entries(&view, &sampler);
    let error = invalid_message(
        create_instance_group(&renderer.device, &layout, &values[..7], 31, 7, |kind| {
            poll(&renderer.device, kind)
        })
        .unwrap_err(),
    );
    assert!(
        error.starts_with("instance shader data program=31 pass=7 phase=bind:"),
        "{error}"
    );
    assert!(
        error.contains("create_bind_group"),
        "original wgpu cause missing: {error}"
    );
    let group = create_instance_group(&renderer.device, &layout, &values, 31, 7, |kind| {
        poll(&renderer.device, kind)
    })
    .unwrap();
    drop(group);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_binding_failure_remains_primary_with_synthetic_secondary_poll_failure() {
    let (renderer, layout, view, sampler) = fixture();
    let values = entries(&view, &sampler);
    let error = invalid_message(
        create_instance_group(&renderer.device, &layout, &values[..7], 31, 7, |kind| {
            poll(&renderer.device, kind)?;
            // Exercise the combined-failure branch without claiming a concurrent
            // hardware fault. The primary binding error above is genuinely native.
            Err(wgpu::PollError::Timeout)
        })
        .unwrap_err(),
    );
    assert!(
        error.starts_with("instance shader data program=31 pass=7 phase=bind:"),
        "{error}"
    );
    assert!(error.contains("create_bind_group"), "{error}");
    assert!(
        error.contains("; GPU poll also failed:"),
        "secondary cause missing: {error}"
    );
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn synthetic_poll_failure_after_real_valid_binding_is_not_discarded() {
    let (renderer, layout, view, sampler) = fixture();
    let error = invalid_message(
        create_instance_group(
            &renderer.device,
            &layout,
            &entries(&view, &sampler),
            31,
            7,
            |kind| {
                poll(&renderer.device, kind)?;
                Err(wgpu::PollError::Timeout)
            },
        )
        .unwrap_err(),
    );
    assert!(
        error.starts_with("instance shader data program=31 pass=7 phase=poll:"),
        "{error}"
    );
    assert!(
        error.ends_with(&wgpu::PollError::Timeout.to_string()),
        "{error}"
    );
    renderer.check_health().unwrap();
}
