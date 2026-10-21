//! Descriptor-capacity admission for owned GPU buffers, not backend residency.
use crate::{allocation_budget::AllocationLease, AllocationBudget, RenderError};
use std::ops::Deref;

pub(crate) struct AllocationBuffer {
    buffer: wgpu::Buffer,
    // Drop the raw handle before releasing its logical reservation.
    allocation: AllocationLease,
}
impl AllocationBuffer {
    pub(crate) fn new(
        device: &wgpu::Device,
        budget: &AllocationBudget,
        descriptor: &wgpu::BufferDescriptor<'_>,
    ) -> Result<Self, RenderError> {
        let allocation = budget.reserve(descriptor.size).map_err(|error| {
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
}
impl Deref for AllocationBuffer {
    type Target = wgpu::Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}
