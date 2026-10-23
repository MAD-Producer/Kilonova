//! Host-supplied native allocation. The widget toolkit does not own a renderer,
//! a policy, or a second budget. Implementations retain native reservations
//! through their queue writes and the host's allocated submissions.
use super::PaintError;

/// An allocation refused by the host's logical capacity policy. Other native
/// faults retain their original error source and must not trigger atlas retries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityRefusal {
    pub required_bytes: u64,
    pub budget_bytes: u64,
}

pub trait Buffer {
    /// This handle must not outlive the allocation wrapper.
    fn buffer(&self) -> &wgpu::Buffer;
    fn write(&self, data: &[u8]) -> Result<(), PaintError>;
}

pub trait Texture {
    /// Views and bind groups remain owned by the Painter alongside this wrapper.
    fn texture(&self) -> &wgpu::Texture;
    fn write(
        &self,
        pixels: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), PaintError>;
}

/// Injected on the native owner's thread. Use the host's allocated encoder and
/// submission contract for every draw referencing resources created here.
pub trait Context {
    fn device(&self) -> &wgpu::Device;
    fn available_bytes(&self) -> Result<u64, PaintError>;
    fn buffer_init(
        &self,
        descriptor: &wgpu::util::BufferInitDescriptor<'_>,
    ) -> Result<Box<dyn Buffer>, PaintError>;
    fn buffer(
        &self,
        descriptor: &wgpu::BufferDescriptor<'_>,
    ) -> Result<Box<dyn Buffer>, PaintError>;
    fn texture(
        &self,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<Box<dyn Texture>, PaintError>;
}
