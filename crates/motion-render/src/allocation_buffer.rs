//! Descriptor-capacity admission for owned GPU buffers, not backend residency.
use crate::{
    allocation_budget::{AllocationLease, AllocationRetention},
    AllocationBudget, RenderError,
};
use std::ops::Deref;

#[derive(Clone)]
pub(crate) struct AllocationBuffer {
    buffer: wgpu::Buffer,
    // Drop the raw handle before releasing its logical reservation.
    allocation: AllocationLease,
}
impl AllocationBuffer {
    pub(crate) fn new_init(
        device: &wgpu::Device,
        budget: &AllocationBudget,
        descriptor: &wgpu::util::BufferInitDescriptor<'_>,
    ) -> Result<Self, RenderError> {
        use wgpu::util::DeviceExt;
        // DeviceExt pads mapped-at-creation contents to COPY_BUFFER_ALIGNMENT.
        // Reserve that native descriptor capacity, before native creation.
        let size = u64::try_from(descriptor.contents.len())
            .ok()
            .and_then(|size| size.checked_add(wgpu::COPY_BUFFER_ALIGNMENT - 1))
            .map(|size| size / wgpu::COPY_BUFFER_ALIGNMENT * wgpu::COPY_BUFFER_ALIGNMENT)
            .filter(|size| *size > 0)
            .ok_or_else(|| {
                RenderError::Invalid("empty or overflowing initialized buffer capacity".into())
            })?;
        let allocation = budget
            .reserve_in(
                size,
                crate::allocation_budget::AllocationCategory::buffer(descriptor.usage),
            )
            .map_err(|error| {
                let label: String = descriptor
                    .label
                    .unwrap_or("unlabelled")
                    .chars()
                    .take(96)
                    .collect();
                let message = format!("buffer size={size} phase=initialize label={label}: {error}");
                error.with_diagnostic(message)
            })?;
        Ok(Self {
            buffer: device.create_buffer_init(descriptor),
            allocation,
        })
    }
    pub(crate) fn new(
        device: &wgpu::Device,
        budget: &AllocationBudget,
        descriptor: &wgpu::BufferDescriptor<'_>,
    ) -> Result<Self, RenderError> {
        let allocation = budget
            .reserve_in(
                descriptor.size,
                crate::allocation_budget::AllocationCategory::buffer(descriptor.usage),
            )
            .map_err(|error| {
                let label: String = descriptor
                    .label
                    .unwrap_or("unlabelled")
                    .chars()
                    .take(96)
                    .collect();
                let message = format!(
                    "buffer size={} phase=allocate label={label}: {error}",
                    descriptor.size
                );
                error.with_diagnostic(message)
            })?;
        Ok(Self {
            buffer: device.create_buffer(descriptor),
            allocation,
        })
    }
    pub(crate) fn allocation(&self) -> AllocationLease {
        self.allocation.clone()
    }
    pub(crate) fn retention(&self) -> AllocationRetention {
        self.allocation.retention()
    }
}
impl Deref for AllocationBuffer {
    type Target = wgpu::Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}
