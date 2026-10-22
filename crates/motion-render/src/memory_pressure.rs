//! Read-only budgets for the renderer's actual adapter, not machine-wide VRAM.
use serde::Serialize;
use std::time::{Duration, Instant};

const QUERY_INTERVAL: Duration = Duration::from_secs(1);
const MAX_CAUSE_CHARS: usize = 512;
mod observation;
pub use observation::MemoryPressureLogScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressureSource {
    DxgiLocalSegment,
    VulkanDeviceLocalHeaps,
    MetalWorkingSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPressurePool {
    pub index: u32,
    pub budget_bytes: u64,
    pub usage_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressureUnavailable {
    NoActiveRenderer,
    UnsupportedBackend,
    MissingNativeAdapter,
    MissingBudgetExtension,
    MissingPropertiesQuery,
    NoDeviceLocalHeap,
    NativeQueryFailed,
    InvalidNativeResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MemoryPressureReading {
    Available {
        source: MemoryPressureSource,
        pools: Vec<MemoryPressurePool>,
    },
    Unavailable {
        reason: MemoryPressureUnavailable,
        cause: Option<String>,
    },
}

impl MemoryPressureReading {
    fn unavailable(reason: MemoryPressureUnavailable) -> Self {
        Self::Unavailable {
            reason,
            cause: None,
        }
    }
    fn failed(cause: impl std::fmt::Display) -> Self {
        Self::Unavailable {
            reason: MemoryPressureUnavailable::NativeQueryFailed,
            cause: Some(cause.to_string().chars().take(MAX_CAUSE_CHARS).collect()),
        }
    }
}

/// Native budgets/usage concern this process or Metal device. They are advisory,
/// may change independently of our allocations, and are not free system VRAM.
/// Unknown readings contain no fabricated zero budget or successful fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPressureSnapshot {
    pub version: u32,
    pub backend: String,
    pub age_ms: u64,
    pub minimum_query_interval_ms: u64,
    pub is_physical_capacity: bool,
    pub is_frame_peak: bool,
    pub reading: MemoryPressureReading,
}

impl MemoryPressureSnapshot {
    /// A metadata request must not construct a GPU just to obtain a budget.
    pub fn without_renderer() -> Self {
        Self {
            version: 1,
            backend: "unattached".into(),
            age_ms: 0,
            minimum_query_interval_ms: QUERY_INTERVAL.as_millis() as u64,
            is_physical_capacity: false,
            is_frame_peak: false,
            reading: MemoryPressureReading::unavailable(
                MemoryPressureUnavailable::NoActiveRenderer,
            ),
        }
    }
}

#[derive(Default)]
pub(crate) struct MemoryPressureProbe {
    cached: Option<(Instant, MemoryPressureSnapshot)>,
    consecutive_query_failures: u8,
}

impl MemoryPressureProbe {
    pub(crate) fn query(&mut self, adapter: &wgpu::Adapter) -> MemoryPressureSnapshot {
        let backend = format!("{:?}", adapter.get_info().backend);
        self.sample(Instant::now(), &backend, || native_reading(adapter))
    }

    fn sample(
        &mut self,
        now: Instant,
        backend: &str,
        query: impl FnOnce() -> MemoryPressureReading,
    ) -> MemoryPressureSnapshot {
        if let Some((sampled, value)) = &self.cached {
            let elapsed = now.saturating_duration_since(*sampled);
            if elapsed < QUERY_INTERVAL {
                let mut value = value.clone();
                value.age_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
                return value;
            }
        }
        let began = Instant::now();
        let reading = query();
        // This instance counts actual consecutive native query failures, not
        // callers served from cache. Unsupported probes are normal boundaries.
        if let MemoryPressureReading::Unavailable {
            reason:
                MemoryPressureUnavailable::NativeQueryFailed
                | MemoryPressureUnavailable::InvalidNativeResult,
            cause,
        } = &reading
        {
            let prior = self.consecutive_query_failures;
            self.consecutive_query_failures = prior.saturating_add(1).min(60);
            if prior == 0 || prior == 59 {
                observation::Event::record(
                    backend,
                    cause.as_deref().unwrap_or("unavailable"),
                    self.consecutive_query_failures,
                    began.elapsed(),
                );
            }
        } else {
            self.consecutive_query_failures = 0;
        }
        let value = MemoryPressureSnapshot {
            version: 1,
            backend: backend.to_owned(),
            age_ms: 0,
            minimum_query_interval_ms: QUERY_INTERVAL.as_millis() as u64,
            is_physical_capacity: false,
            is_frame_peak: false,
            reading,
        };
        self.cached = Some((now, value.clone()));
        value
    }
}

fn native_reading(adapter: &wgpu::Adapter) -> MemoryPressureReading {
    match adapter.get_info().backend {
        #[cfg(target_os = "windows")]
        wgpu::Backend::Dx12 => dx12(adapter),
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "android"))]
        wgpu::Backend::Vulkan => vulkan(adapter),
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        wgpu::Backend::Metal => metal(adapter),
        _ => MemoryPressureReading::unavailable(MemoryPressureUnavailable::UnsupportedBackend),
    }
}

#[cfg(target_os = "windows")]
fn dx12(adapter: &wgpu::Adapter) -> MemoryPressureReading {
    // SAFETY: borrow the HAL adapter only within its wgpu guard. We perform a
    // read-only query through HAL's safe DXGI wrapper and retain only numbers.
    let Some(native) = (unsafe { adapter.as_hal::<wgpu::hal::api::Dx12>() }) else {
        return MemoryPressureReading::unavailable(MemoryPressureUnavailable::MissingNativeAdapter);
    };
    // DXGI_MEMORY_SEGMENT_GROUP's default value is LOCAL (0). This avoids
    // introducing another direct Windows binding dependency for one constant.
    match native
        .raw_adapter()
        .query_video_memory_info(Default::default())
    {
        Ok(info) => MemoryPressureReading::Available {
            source: MemoryPressureSource::DxgiLocalSegment,
            pools: vec![MemoryPressurePool {
                index: 0,
                budget_bytes: info.Budget,
                usage_bytes: info.CurrentUsage,
            }],
        },
        Err(error) => MemoryPressureReading::failed(error),
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "android"))]
fn vulkan(adapter: &wgpu::Adapter) -> MemoryPressureReading {
    use ash::vk;
    // SAFETY: all Vulkan handles remain borrowed from the guarded wgpu adapter;
    // the queries do not create, destroy, mutate or retain native GPU objects.
    let Some(native) = (unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return MemoryPressureReading::unavailable(MemoryPressureUnavailable::MissingNativeAdapter);
    };
    if !native
        .physical_device_capabilities()
        .supports_extension(ash::ext::memory_budget::NAME)
    {
        return MemoryPressureReading::unavailable(
            MemoryPressureUnavailable::MissingBudgetExtension,
        );
    }
    let shared = native.shared_instance();
    let instance = shared.raw_instance();
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let (count, heaps) = {
        let mut properties = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
        // SAFETY: the extension was checked above; the stack pNext chain and output
        // pointers live through this call and are not retained by Vulkan. Select an
        // entry point supported by the actual instance, including Vulkan 1.0 + KHR.
        unsafe {
            if shared.instance_api_version() >= vk::API_VERSION_1_1 {
                instance.get_physical_device_memory_properties2(
                    native.raw_physical_device(),
                    &mut properties,
                );
            } else if shared
                .extensions()
                .contains(&ash::khr::get_physical_device_properties2::NAME)
            {
                let query = ash::khr::get_physical_device_properties2::Instance::new(
                    shared.entry(),
                    instance,
                );
                query.get_physical_device_memory_properties2(
                    native.raw_physical_device(),
                    &mut properties,
                );
            } else {
                return MemoryPressureReading::unavailable(
                    MemoryPressureUnavailable::MissingPropertiesQuery,
                );
            }
        }
        let count = properties.memory_properties.memory_heap_count as usize;
        if count > properties.memory_properties.memory_heaps.len() {
            return MemoryPressureReading::Unavailable {
                reason: MemoryPressureUnavailable::InvalidNativeResult,
                cause: Some(format!(
                    "Vulkan memory_heap_count={count} exceeds the native heap-array bound"
                )),
            };
        }
        (count, properties.memory_properties.memory_heaps)
    };
    let pools = heaps[..count]
        .iter()
        .enumerate()
        .filter(|(_, heap)| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|(index, _)| MemoryPressurePool {
            index: index as u32,
            budget_bytes: budget.heap_budget[index],
            usage_bytes: budget.heap_usage[index],
        })
        .collect::<Vec<_>>();
    if pools.is_empty() {
        return MemoryPressureReading::unavailable(MemoryPressureUnavailable::NoDeviceLocalHeap);
    }
    MemoryPressureReading::Available {
        source: MemoryPressureSource::VulkanDeviceLocalHeaps,
        pools,
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn metal(adapter: &wgpu::Adapter) -> MemoryPressureReading {
    use objc2_metal::MTLDevice;
    // SAFETY: use the guarded, existing MTLDevice only for read-only properties;
    // no native handles escape the borrow and no resource lifetime is changed.
    let Some(native) = (unsafe { adapter.as_hal::<wgpu::hal::api::Metal>() }) else {
        return MemoryPressureReading::unavailable(MemoryPressureUnavailable::MissingNativeAdapter);
    };
    let device = native.raw_device();
    MemoryPressureReading::Available {
        source: MemoryPressureSource::MetalWorkingSet,
        pools: vec![MemoryPressurePool {
            index: 0,
            budget_bytes: device.recommendedMaxWorkingSetSize(),
            usage_bytes: device.currentAllocatedSize() as u64,
        }],
    }
}

#[cfg(test)]
mod tests;
