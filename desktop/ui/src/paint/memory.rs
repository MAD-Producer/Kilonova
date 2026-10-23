//! Optional shared native admission. Standalone Painter keeps its raw resources;
//! the desktop uses the renderer's context and allocated submission contract.
use super::{allocation, GlyphVertex, PaintError, SolidVertex, GLYPH_CAPACITY, SOLID_CAPACITY};
use std::{ops::Deref, rc::Rc};
use wgpu::util::DeviceExt;

pub(super) struct PendingAtlas {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub failure: PaintError,
}

pub(super) enum Buffer {
    Raw(wgpu::Buffer),
    Admitted {
        buffer: Box<dyn allocation::Buffer>,
        context: Rc<dyn allocation::Context>,
    },
}
impl Deref for Buffer {
    type Target = wgpu::Buffer;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(buffer) => buffer,
            Self::Admitted { buffer, .. } => buffer.buffer(),
        }
    }
}
impl Buffer {
    pub fn write(&self, queue: &wgpu::Queue, data: &[u8]) -> Result<(), PaintError> {
        match self {
            Self::Raw(buffer) => {
                queue.write_buffer(buffer, 0, data);
                Ok(())
            }
            Self::Admitted { buffer, .. } => buffer.write(data)
                .map_err(|failure| failure.with_operation("buffer upload")),
        }
    }
    pub fn growth(
        &self,
        device: &wgpu::Device,
        required: u64,
        label: &'static str,
    ) -> Result<Option<Self>, PaintError> {
        if required <= self.size() {
            return Ok(None);
        }
        let size = required
            .checked_next_power_of_two()
            .unwrap_or(required)
            .min(device.limits().max_buffer_size);
        let descriptor = wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        Ok(Some(match self {
            Self::Raw(_) => Self::Raw(device.create_buffer(&descriptor)),
            Self::Admitted { context, .. } => Self::Admitted {
                buffer: context.buffer(&descriptor)
                    .map_err(|failure| failure.with_operation(label))?,
                context: context.clone(),
            },
        }))
    }
}

pub(super) enum Texture {
    Raw(wgpu::Texture),
    Admitted {
        texture: Box<dyn allocation::Texture>,
    },
}
impl Deref for Texture {
    type Target = wgpu::Texture;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(texture) => texture,
            Self::Admitted { texture, .. } => texture.texture(),
        }
    }
}
impl Texture {
    pub fn new(
        device: &wgpu::Device,
        context: Option<&Rc<dyn allocation::Context>>,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<Self, PaintError> {
        Ok(match context {
            None => Self::Raw(device.create_texture(descriptor)),
            Some(context) => Self::Admitted {
                texture: context.texture(descriptor)
                    .map_err(|failure| failure.with_operation("glyph atlas"))?,
            },
        })
    }
    pub fn write(
        &self,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PaintError> {
        let layout = wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        };
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        match self {
            Self::Raw(texture) => {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    layout,
                    size,
                );
                Ok(())
            }
            Self::Admitted { texture } => texture.write(pixels, layout, size)
                .map_err(|failure| failure.with_operation("glyph atlas upload")),
        }
    }
}

pub(super) struct Buffers {
    pub solid_uniform: Buffer,
    pub glyph_uniform: Buffer,
    pub solid_buffer: Buffer,
    pub glyph_buffer: Buffer,
    pub context: Option<Rc<dyn allocation::Context>>,
}
impl Buffers {
    fn uniform<'a>(
        label: &'static str,
        contents: &'a [u8],
    ) -> wgpu::util::BufferInitDescriptor<'a> {
        wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }
    }
    fn vertices(label: &'static str, size: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
    pub fn raw(device: &wgpu::Device) -> Self {
        let bytes = bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]);
        Self {
            solid_uniform: Buffer::Raw(
                device.create_buffer_init(&Self::uniform("solid uniforms", bytes)),
            ),
            glyph_uniform: Buffer::Raw(
                device.create_buffer_init(&Self::uniform("glyph uniforms", bytes)),
            ),
            solid_buffer: Buffer::Raw(device.create_buffer(&Self::vertices(
                "solid vertices",
                (SOLID_CAPACITY * std::mem::size_of::<SolidVertex>()) as u64,
            ))),
            glyph_buffer: Buffer::Raw(device.create_buffer(&Self::vertices(
                "glyph vertices",
                (GLYPH_CAPACITY * std::mem::size_of::<GlyphVertex>()) as u64,
            ))),
            context: None,
        }
    }
    pub fn admitted(context: &Rc<dyn allocation::Context>) -> Result<Self, PaintError> {
        let bytes = bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]);
        let uniform = |label| -> Result<Buffer, PaintError> {
            Ok(Buffer::Admitted {
                buffer: context.buffer_init(&Self::uniform(label, bytes))
                    .map_err(|failure| failure.with_operation(label))?,
                context: context.clone(),
            })
        };
        let vertices = |label| -> Result<Buffer, PaintError> {
            Ok(Buffer::Admitted {
                buffer: context.buffer(&Self::vertices(label, wgpu::COPY_BUFFER_ALIGNMENT))
                    .map_err(|failure| failure.with_operation(label))?,
                context: context.clone(),
            })
        };
        // Admit only minimal empty geometry, then grow from the actual frame.
        // Reserving unused multi-MiB buffers would consume a small preview's
        // entire allowance before any UI geometry had even been supplied.
        Ok(Self {
            solid_uniform: uniform("solid uniforms")?,
            glyph_uniform: uniform("glyph uniforms")?,
            solid_buffer: vertices("solid vertices")?,
            glyph_buffer: vertices("glyph vertices")?,
            context: Some(context.clone()),
        })
    }
}
