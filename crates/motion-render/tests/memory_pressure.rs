use motion_render::{MemoryPressureReading, MemoryPressureSource, Renderer};

fn native_probe(backend: wgpu::Backends, expected: MemoryPressureSource) {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = backend;
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ))
    .unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let policy = renderer.memory_policy();
    let first = renderer.memory_pressure();
    assert_eq!(
        first.backend,
        format!("{:?}", renderer.adapter_info.backend)
    );
    assert!(!first.is_physical_capacity && !first.is_frame_peak);
    match &first.reading {
        MemoryPressureReading::Available { source, pools } => {
            assert_eq!(*source, expected);
            assert!(!pools.is_empty() && pools.len() <= 16);
            assert!(pools.windows(2).all(|p| p[0].index < p[1].index));
        }
        MemoryPressureReading::Unavailable { reason, .. } => {
            // Software Vulkan may not advertise the optional budget extension.
            // Preserve this as unknown, never manufacture a successful budget.
            assert_eq!(expected, MemoryPressureSource::VulkanDeviceLocalHeaps);
            assert!(matches!(
                reason,
                motion_render::MemoryPressureUnavailable::MissingBudgetExtension
                    | motion_render::MemoryPressureUnavailable::MissingPropertiesQuery
                    | motion_render::MemoryPressureUnavailable::NoDeviceLocalHeap
            ));
        }
    }
    for _ in 0..100 {
        let cached = renderer.memory_pressure();
        assert_eq!(cached.backend, first.backend);
        assert!(cached.age_ms < cached.minimum_query_interval_ms);
    }
    assert_eq!(budget.info().unwrap(), before);
    assert_eq!(renderer.memory_policy(), policy);
    println!(
        "native_memory_pressure {}",
        serde_json::to_string(&first).unwrap()
    );
}

#[test]
#[cfg(target_os = "windows")]
fn actual_dxgi_adapter_query_is_bounded_and_does_not_modify_admission() {
    native_probe(wgpu::Backends::DX12, MemoryPressureSource::DxgiLocalSegment);
}

#[test]
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "android"))]
fn actual_vulkan_adapter_query_is_available_or_explicitly_unsupported() {
    native_probe(
        wgpu::Backends::VULKAN,
        MemoryPressureSource::VulkanDeviceLocalHeaps,
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn actual_metal_device_query_is_bounded_and_does_not_modify_admission() {
    native_probe(wgpu::Backends::METAL, MemoryPressureSource::MetalWorkingSet);
}
