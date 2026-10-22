//! Bridge the toolkit's native allocation contract to this renderer's owner,
//! live allowance, upload retirement and allocated submission identities.
use motion_render::{AdmittedBuffer, AdmittedTexture, AllocationContext, RenderError};
use motion_ui::paint::{allocation, PaintError};
use std::rc::Rc;

pub(crate) fn context(context: AllocationContext) -> Rc<dyn allocation::Context> {
    Rc::new(Context(context))
}

pub(super) fn error(kind: &'static str, cause: RenderError) -> PaintError {
    let refusal = match cause.resource_rejection() {
        Some(motion_render::ResourceRejection::Budget {
            required_bytes,
            budget_bytes,
            ..
        }) => Some(allocation::CapacityRefusal {
            required_bytes: *required_bytes,
            budget_bytes: *budget_bytes,
        }),
        _ => None,
    };
    PaintError::from_source(kind, cause, refusal)
}

struct Context(AllocationContext);
impl allocation::Context for Context {
    fn device(&self) -> &wgpu::Device {
        self.0.device()
    }
    fn available_bytes(&self) -> Result<u64, PaintError> {
        let info = self
            .0
            .budget()
            .info()
            .map_err(|cause| error("glyph atlas admission", cause))?;
        Ok(info.budget_bytes.saturating_sub(info.admitted_bytes))
    }
    fn buffer_init(
        &self,
        descriptor: &wgpu::util::BufferInitDescriptor<'_>,
    ) -> Result<Box<dyn allocation::Buffer>, PaintError> {
        let buffer = self
            .0
            .buffer_init(descriptor)
            .map_err(|cause| error("buffer initialize", cause))?;
        Ok(Box::new(Buffer {
            buffer,
            context: self.0.clone(),
        }))
    }
    fn buffer(
        &self,
        descriptor: &wgpu::BufferDescriptor<'_>,
    ) -> Result<Box<dyn allocation::Buffer>, PaintError> {
        let buffer = self
            .0
            .buffer(descriptor)
            .map_err(|cause| error("buffer allocate", cause))?;
        Ok(Box::new(Buffer {
            buffer,
            context: self.0.clone(),
        }))
    }
    fn texture(
        &self,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<Box<dyn allocation::Texture>, PaintError> {
        let texture = self
            .0
            .texture_in(descriptor, motion_render::TextureCategory::UiAtlas)
            .map_err(|cause| error("glyph atlas", cause))?;
        Ok(Box::new(Texture {
            texture,
            context: self.0.clone(),
        }))
    }
}

struct Buffer {
    buffer: AdmittedBuffer,
    context: AllocationContext,
}
impl allocation::Buffer for Buffer {
    fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
    fn write(&self, data: &[u8]) -> Result<(), PaintError> {
        self.context
            .write_buffer(&self.buffer, 0, data)
            .map_err(|cause| error("buffer upload", cause))
    }
}
struct Texture {
    texture: AdmittedTexture,
    context: AllocationContext,
}
impl allocation::Texture for Texture {
    fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    fn write(
        &self,
        pixels: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) -> Result<(), PaintError> {
        self.context
            .write_texture(&self.texture, pixels, layout, size)
            .map_err(|cause| error("glyph atlas upload", cause))
    }
}
