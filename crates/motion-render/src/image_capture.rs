//! Format-preserving image readback. No display transfer, alpha conversion or
//! implicit precision reduction. Producers must honor the declared meaning.
use crate::resource_ledger::{DeviceTextureExt, OwnedTexture};
use crate::{image_formats, RenderError, Renderer, TextureResourceLedger};
use motion_effects::image_contract::{ImageContract, ImageStorage, ImageUsage};

pub struct ImageCaptureTarget {
    texture: OwnedTexture,
    view: crate::AdmittedTextureView,
    buffer: crate::allocation_buffer::AllocationBuffer,
    owner: std::sync::Arc<()>,
    contract: ImageContract,
    storage: ImageStorage,
    width: u32,
    height: u32,
    row_bytes: u32,
    row_stride: u32,
}

/// Tightly packed native storage bytes. For sRGB storage these bytes are encoded;
/// the contract describes values seen after sampling, not raw byte values.
#[derive(Debug)]
pub struct CapturedImage {
    pub contract: ImageContract,
    pub storage: ImageStorage,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

struct UnmapOnDrop<'a>(&'a wgpu::Buffer);
impl Drop for UnmapOnDrop<'_> {
    fn drop(&mut self) {
        self.0.unmap();
    }
}

impl ImageCaptureTarget {
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
    pub fn contract(&self) -> ImageContract {
        self.contract
    }
    pub fn storage(&self) -> ImageStorage {
        self.storage
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn texture_bytes(&self) -> u64 {
        u64::from(self.row_bytes) * u64::from(self.height)
    }
    /// Staging buffer capacity is separate from the texture ledger.
    pub fn readback_bytes(&self) -> u64 {
        u64::from(self.row_stride) * u64::from(self.height)
    }
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = TextureResourceLedger::default();
        ledger.retained(
            &self.texture,
            crate::TextureCategory::CaptureTargets,
            "capture/image".into(),
        );
        ledger
    }
}

impl Renderer {
    /// Allocate a format-preserving capture target after capability/size checks.
    /// The producer pipeline must target `image_formats::native_format(storage)`.
    /// This does not change the renderer's scene-working precision or pipelines.
    pub fn image_capture_target(
        &self,
        width: u32,
        height: u32,
        contract: ImageContract,
    ) -> Result<ImageCaptureTarget, RenderError> {
        self.check_health()?;
        let required = ImageUsage {
            sampled: true,
            render: true,
            copy_src: true,
            ..Default::default()
        };
        let storage = self
            .image_capabilities
            .negotiate(contract, required)
            .map_err(|cause| {
                let message =
                    format!("image capture {width}x{height} phase=format-capabilities: {cause}");
                RenderError::from(cause).with_diagnostic(message)
            })?;
        let layout = capture_layout(width, height, storage, &self.device.limits())?;
        let texture = self.device.create_admitted_texture(
            &wgpu::TextureDescriptor {
                label: Some("Motion Studio format-preserving image capture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: image_formats::native_format(storage),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            self.allocation_budget(),
            crate::TextureCategory::CaptureTargets,
        )?;
        let view = texture.admitted_view()?;
        let buffer = crate::allocation_buffer::AllocationBuffer::new(
            &self.device,
            self.allocation_budget(),
            &wgpu::BufferDescriptor {
                label: Some("Motion Studio format-preserving image staging"),
                size: layout.buffer_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            },
        )?;
        Ok(ImageCaptureTarget {
            texture,
            view,
            buffer,
            owner: self.allocation_owner.clone(),
            contract,
            storage,
            width,
            height,
            row_bytes: layout.row_bytes,
            row_stride: layout.row_stride,
        })
    }

    /// Read storage bytes without unpremultiplying, gamma conversion or clamping.
    /// This synchronous export/validation path is never used by preview polling.
    pub fn read_image_target(
        &self,
        target: &ImageCaptureTarget,
    ) -> Result<CapturedImage, RenderError> {
        let observation = self.image_readback.begin();
        self.check_health()?;
        if !std::sync::Arc::ptr_eq(&self.allocation_owner, &target.owner) {
            return Err(RenderError::Invalid(
                "image capture phase=readback: target belongs to another renderer".into(),
            ));
        }
        let mut encoder = self.allocation_command_encoder_for(
            crate::gpu_commands::CommandStage::ReadbackCopy,
            &wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio raw image copy"),
            },
        )?;
        encoder.copy_texture_to_buffer(
            target.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &target.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.row_stride),
                    rows_per_image: Some(target.height),
                },
            },
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );
        self.submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })?;
        let row_bytes = u64::from(target.row_bytes);
        observation.submitted(
            target.texture_bytes(),
            u64::from(target.row_stride) * u64::from(target.height.saturating_sub(1)) + row_bytes,
        );
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        target
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        // Covers poll, device-health, callback and mapped-range failures too.
        let _unmap = UnmapOnDrop(&target.buffer);
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        self.check_health()?;
        receive
            .recv()
            .map_err(|cause| RenderError::Readback(format!("image capture callback: {cause}")))?
            .map_err(|cause| RenderError::Readback(format!("image capture mapping: {cause}")))?;
        let pixels = (|| {
            let mapped = target
                .buffer
                .slice(..)
                .get_mapped_range()
                .map_err(|cause| {
                    RenderError::Readback(format!("image capture mapped range: {cause}"))
                })?;
            observation.mapped(mapped.len() as u64);
            let mut pixels = Vec::with_capacity(target.texture_bytes() as usize);
            for row in mapped.chunks_exact(target.row_stride as usize) {
                pixels.extend_from_slice(&row[..target.row_bytes as usize]);
            }
            Ok::<_, RenderError>(pixels)
        })();
        let pixels = pixels?;
        self.check_health()?;
        observation.completed(pixels.len() as u64);
        Ok(CapturedImage {
            contract: target.contract,
            storage: target.storage,
            width: target.width,
            height: target.height,
            pixels,
        })
    }
}

struct CaptureLayout {
    row_bytes: u32,
    row_stride: u32,
    buffer_bytes: u64,
}

fn capture_layout(
    width: u32,
    height: u32,
    storage: ImageStorage,
    limits: &wgpu::Limits,
) -> Result<CaptureLayout, RenderError> {
    let invalid = |reason: &str| {
        RenderError::Invalid(format!(
            "image capture {width}x{height} {storage:?}: {reason}"
        ))
    };
    if width == 0
        || height == 0
        || width > limits.max_texture_dimension_2d
        || height > limits.max_texture_dimension_2d
        || u64::from(width) * u64::from(height) > crate::renderer::CAPTURE_PIXEL_LIMIT
    {
        return Err(invalid("extent exceeds capture limits"));
    }
    let row_bytes = u64::from(width) * storage.bytes_per_pixel();
    let row_stride = row_bytes.div_ceil(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT))
        * u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer_bytes = row_stride
        .checked_mul(u64::from(height))
        .ok_or_else(|| invalid("staging capacity overflow"))?;
    if row_stride > u64::from(u32::MAX)
        || buffer_bytes > limits.max_buffer_size
        || buffer_bytes > usize::MAX as u64
    {
        return Err(invalid(
            "staging capacity exceeds device or address-space limit",
        ));
    }
    Ok(CaptureLayout {
        row_bytes: row_bytes as u32,
        row_stride: row_stride as u32,
        buffer_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_capacity_checks_include_alignment_precision_and_device_buffer_limit() {
        let mut limits = wgpu::Limits::default();
        limits.max_buffer_size = 1024;
        assert!(capture_layout(0, 1, ImageStorage::R8Unorm, &limits).is_err());
        assert!(capture_layout(
            limits.max_texture_dimension_2d + 1,
            1,
            ImageStorage::R8Unorm,
            &limits
        )
        .is_err());
        let layout = capture_layout(17, 2, ImageStorage::Rgba32Float, &limits).unwrap();
        assert_eq!(
            (layout.row_bytes, layout.row_stride, layout.buffer_bytes),
            (272, 512, 1024)
        );
        assert!(capture_layout(17, 3, ImageStorage::Rgba32Float, &limits).is_err());
        // The same extent fits at lower precision; using four bytes for Float32
        // would incorrectly accept the rejected request above.
        assert!(capture_layout(17, 3, ImageStorage::Rgba8Unorm, &limits).is_ok());
    }
}
