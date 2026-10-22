//! Native resources created outside Renderer with its existing admission and
//! upload retirement identity. Logical capacity, not driver residency.
use crate::{
    allocation_buffer::AllocationBuffer,
    allocation_uploads::AllocationUploads,
    resource_ledger::{DeviceTextureExt, OwnedTexture},
    AdmittedTextureView, AllocationBudget, RenderError,
};
use std::{marker::PhantomData, ops::Deref, rc::Rc, sync::Arc};

/// Clone on the renderer's owning thread before constructing external resources.
/// Queue writes retain destination reservations until that renderer's next
/// allocated submission completes. Raw wgpu aliases must not outlive their
/// admitted wrapper/view; use allocated encoders for commands referencing them.
#[derive(Clone)]
pub struct AllocationContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    budget: AllocationBudget,
    uploads: Arc<AllocationUploads>,
    owner: Arc<()>,
    _thread: PhantomData<Rc<()>>,
}

#[derive(Clone)]
pub struct AdmittedTexture {
    texture: OwnedTexture,
    owner: Arc<()>,
}
impl AdmittedTexture {
    pub fn view(&self) -> Result<AdmittedTextureView, RenderError> {
        self.texture.admitted_view()
    }
}
impl Deref for AdmittedTexture {
    type Target = wgpu::Texture;
    fn deref(&self) -> &Self::Target {
        &self.texture
    }
}

#[derive(Clone)]
pub struct AdmittedBuffer {
    buffer: AllocationBuffer,
    owner: Arc<()>,
}
impl Deref for AdmittedBuffer {
    type Target = wgpu::Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl AllocationContext {
    pub(crate) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        budget: AllocationBudget,
        uploads: Arc<AllocationUploads>,
        owner: Arc<()>,
    ) -> Self {
        Self {
            device,
            queue,
            budget,
            uploads,
            owner,
            _thread: PhantomData,
        }
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn budget(&self) -> &AllocationBudget {
        &self.budget
    }
    pub fn texture(
        &self,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<AdmittedTexture, RenderError> {
        Ok(AdmittedTexture {
            texture: self
                .device
                .create_admitted_texture(descriptor, &self.budget, crate::TextureCategory::PluginResources)
                .map_err(|error| {
                    let label: String = descriptor
                        .label
                        .unwrap_or("unlabelled")
                        .chars()
                        .take(96)
                        .collect();
                    let message = format!(
                        "texture size={:?} format={:?} phase=allocate label={label}: {error}",
                        descriptor.size, descriptor.format
                    );
                    error.with_diagnostic(message)
                })?,
            owner: self.owner.clone(),
        })
    }
    pub fn buffer_init(
        &self,
        descriptor: &wgpu::util::BufferInitDescriptor<'_>,
    ) -> Result<AdmittedBuffer, RenderError> {
        Ok(AdmittedBuffer {
            buffer: AllocationBuffer::new_init(&self.device, &self.budget, descriptor)?,
            owner: self.owner.clone(),
        })
    }
    pub fn write_texture(
        &self,
        texture: &AdmittedTexture,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), RenderError> {
        self.check_owner(&texture.owner)?;
        self.uploads.track(texture.texture.allocation())?;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            layout,
            size,
        );
        Ok(())
    }
    pub fn write_buffer(
        &self,
        buffer: &AdmittedBuffer,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RenderError> {
        self.check_owner(&buffer.owner)?;
        self.uploads.track(Some(buffer.buffer.allocation()))?;
        self.queue.write_buffer(buffer, offset, data);
        Ok(())
    }
    fn check_owner(&self, owner: &Arc<()>) -> Result<(), RenderError> {
        if Arc::ptr_eq(&self.owner, owner) {
            Ok(())
        } else {
            Err(RenderError::Invalid(
                "native allocation belongs to another renderer".into(),
            ))
        }
    }
}
