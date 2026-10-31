//! Logical descriptor capacity and queued-write retention for presentation.
use crate::{
    allocation_buffer::AllocationBuffer, allocation_uploads::AllocationUploads, RenderError,
    Renderer,
};
use std::{ops::Deref, sync::Arc};
use wgpu::util::DeviceExt;

pub(super) enum Buffer {
    Raw(wgpu::Buffer),
    Admitted {
        buffer: AllocationBuffer,
        uploads: Arc<AllocationUploads>,
        queue: wgpu::Queue,
    },
}
impl Deref for Buffer {
    type Target = wgpu::Buffer;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(buffer) => buffer,
            Self::Admitted { buffer, .. } => buffer,
        }
    }
}
impl Buffer {
    pub fn write(&self, queue: &wgpu::Queue, offset: u64, data: &[u8]) -> Result<(), RenderError> {
        match self {
            Self::Raw(buffer) => queue.write_buffer(buffer, offset, data),
            Self::Admitted {
                buffer,
                uploads,
                queue,
            } => {
                // Journal before staging; the originating queue must retire the
                // write, including destruction of the Presenter before submit.
                uploads.track(Some(buffer.allocation()))?;
                queue.write_buffer(buffer, offset, data);
            }
        }
        Ok(())
    }
}
pub(super) struct Buffers {
    pub color: Buffer,
    pub view: Buffer,
}
impl Buffers {
    fn mode(format: wgpu::TextureFormat, canvas: bool) -> [u32; 8] {
        [
            u32::from(format.is_srgb()) | (u32::from(canvas) << 1),
            0,
            0,
            0,
            0,
            0,
            1.0f32.to_bits(),
            1.0f32.to_bits(),
        ]
    }
    fn descriptor<'a>(
        label: &'static str,
        contents: &'a [u8],
        writable: bool,
    ) -> wgpu::util::BufferInitDescriptor<'a> {
        wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents,
            usage: wgpu::BufferUsages::UNIFORM
                | if writable {
                    wgpu::BufferUsages::COPY_DST
                } else {
                    wgpu::BufferUsages::empty()
                },
        }
    }
    pub fn raw(renderer: &Renderer, format: wgpu::TextureFormat) -> Self {
        let mode = Self::mode(format, false);
        let contents = bytemuck::cast_slice(&mode);
        let view_mode = Self::mode(format, true);
        Self {
            color: Buffer::Raw(renderer.device.create_buffer_init(&Self::descriptor(
                "Motion Studio surface color encoding",
                contents,
                false,
            ))),
            view: Buffer::Raw(renderer.device.create_buffer_init(&Self::descriptor(
                "Motion Studio canvas navigation",
                bytemuck::cast_slice(&view_mode),
                true,
            ))),
        }
    }
    pub fn admitted(renderer: &Renderer, format: wgpu::TextureFormat) -> Result<Self, RenderError> {
        let buffer = |label, writable| -> Result<Buffer, RenderError> {
            let mode = Self::mode(format, writable);
            Ok(Buffer::Admitted {
                buffer: AllocationBuffer::new_init(
                    &renderer.device,
                    renderer.allocation_budget(),
                    &Self::descriptor(label, bytemuck::cast_slice(&mode), writable),
                )?,
                uploads: renderer.allocation_uploads(),
                queue: renderer.queue.clone(),
            })
        };
        Ok(Self {
            color: buffer("Motion Studio surface color encoding", false)?,
            view: buffer("Motion Studio canvas navigation", true)?,
        })
    }
}
