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
    pub(crate) fn texture(&self) -> &OwnedTexture {
        &self.texture
    }
    pub(crate) fn check_owner(&self, renderer: &Renderer) -> Result<(), RenderError> {
        if std::sync::Arc::ptr_eq(&renderer.allocation_owner, &self.owner) {
            Ok(())
        } else {
            Err(RenderError::Invalid(
                "image target belongs to another renderer".into(),
            ))
        }
    }
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

    /// Export/validation readback of a native compositor/cache target. Owner,
    /// canonical working storage and shared-budget admission are checked before
    /// copying. This synchronous path is never used by preview cache polling.
    pub fn read_native_render_target(
        &self,
        source: &crate::RenderTarget,
    ) -> Result<CapturedImage, RenderError> {
        source.check_owner(self)?;
        self.check_health()?;
        let precision = crate::image_formats::color_precision(source.texture().format())
            .ok_or_else(|| {
                RenderError::Invalid("native readback requires working color storage".into())
            })?;
        let contract = ImageContract::new(
            motion_effects::image_contract::ImageMeaning::Color {
                space: motion_effects::WorkingSpace::Linear,
                alpha: motion_effects::AlphaMode::Premultiplied,
            },
            precision,
        );
        let storage = crate::image_formats::color_storage(precision);
        if crate::image_formats::native_format(storage) != source.texture().format() {
            return Err(RenderError::Invalid(
                "native readback refuses implicit format conversion".into(),
            ));
        }
        let layout = capture_layout(source.width, source.height, storage, &self.device.limits())?;
        let buffer = crate::allocation_buffer::AllocationBuffer::new(
            &self.device,
            self.allocation_budget(),
            &wgpu::BufferDescriptor {
                label: Some("native cache readback staging"),
                size: layout.buffer_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            },
        )?;
        // Retain the admitted native source; allocate only staging, with no
        // intermediate texture or COPY_DST requirement on render-only targets.
        let output = ImageCaptureTarget {
            texture: source.texture().clone(),
            view: source.view.clone(),
            buffer,
            owner: self.allocation_owner.clone(),
            contract,
            storage,
            width: source.width,
            height: source.height,
            row_bytes: layout.row_bytes,
            row_stride: layout.row_stride,
        };
        self.read_image_target(&output)
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

impl Renderer {
    /// Explicit user pick from a retained SDR preview. Copies one texel only,
    /// excluding UI overlays; never called by hover or presentation polling.
    /// Returns straight sRGB values, matching authored editor color parameters.
    pub fn sample_preview_pixel(&self, source: &crate::RenderTarget, point: [u32; 2])
        -> Result<[f32; 4], RenderError> {
        self.check_health()?;
        source.check_owner(self)?;
        let format = source.texture().format();
        if point[0] >= source.width || point[1] >= source.height
            || !matches!(format, wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb) {
            return Err(RenderError::Invalid("preview color pick requires an in-bounds SDR pixel".into()));
        }
        let observation = self.image_readback.begin();
        let buffer = crate::allocation_buffer::AllocationBuffer::new(&self.device,
            self.allocation_budget(), &wgpu::BufferDescriptor {
                label: Some("Motion Studio single color pick"), size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })?;
        let mut encoder = self.allocation_command_encoder_for(crate::gpu_commands::CommandStage::ReadbackCopy,
            &wgpu::CommandEncoderDescriptor { label: Some("Motion Studio preview color pick") })?;
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo {
            texture: source.texture(), mip_level: 0,
            origin: wgpu::Origin3d { x: point[0], y: point[1], z: 0 }, aspect: wgpu::TextureAspect::All,
        }, wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
            offset: 0, bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT), rows_per_image: Some(1),
        } }, wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
        self.submit_allocated(encoder.finish(), |ledger| source.collect_texture_resources(ledger,
            "preview/color-pick", crate::TextureState::PreviewFront))?;
        observation.submitted(4, 4);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = send.send(result); });
        let _unmap = UnmapOnDrop(&buffer);
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        self.check_health()?;
        receive.recv().map_err(|e| RenderError::Readback(format!("color pick callback: {e}")))?
            .map_err(|e| RenderError::Readback(format!("color pick mapping: {e}")))?;
        let mapped = buffer.slice(..).get_mapped_range()
            .map_err(|e| RenderError::Readback(format!("color pick mapped range: {e}")))?;
        observation.mapped(mapped.len() as u64);
        let alpha = mapped[3] as f32 / 255.;
        let decode = |v: u8| {
            let v = v as f32 / 255.;
            let linear = if format.is_srgb() {
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            } else { v };
            if alpha > 0. {
                crate::renderer::linear_to_srgb((linear / alpha).clamp(0., 1.))
            } else { 0. }
        };
        let value = [decode(mapped[0]), decode(mapped[1]), decode(mapped[2]), alpha];
        observation.completed(4);
        Ok(value)
    }
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
