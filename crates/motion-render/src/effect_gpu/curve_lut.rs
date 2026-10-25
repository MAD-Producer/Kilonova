//! Admitted numeric LUTs. F32 tables need sampling/copy, never filtering/blending.
use super::{admitted_texture, upload, GpuState, Lut};
use crate::{resource_ledger::DeviceTextureExt, RenderError, Scene};
use motion_effects::image_contract::{ChannelPrecision, ImageStorage, ImageUsage};

impl GpuState {
    pub(super) fn prepare_curves(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composite: &wgpu::BindGroupLayout,
        precision: ChannelPrecision,
        scene: &Scene,
    ) -> Result<(), RenderError> {
        if precision != ChannelPrecision::Unorm8
            && scene.curve_sources.len() != scene.curve_luts.len()
        {
            return Err(RenderError::Invalid(format!(
                "effect high-precision curve sources={} do not match LUTs={}",
                scene.curve_sources.len(),
                scene.curve_luts.len()
            )));
        }
        if self.luts.len() != scene.curve_luts.len() {
            self.invalidate();
        }
        self.luts.truncate(scene.curve_luts.len());
        for (index, lut) in scene.curve_luts.iter().enumerate() {
            if precision == ChannelPrecision::Unorm8 {
                let bytes: [u8; 1024] =
                    std::array::from_fn(|i| (lut[i / 4][i % 4].clamp(0., 1.) * 255.).round() as u8);
                let replace = self
                    .luts
                    .get(index)
                    .is_none_or(|old| old.texture.format() != wgpu::TextureFormat::Rgba8Unorm);
                if replace {
                    let target = admitted_texture(
                        device,
                        composite,
                        &self.sampler,
                        256,
                        1,
                        wgpu::TextureFormat::Rgba8Unorm,
                        &self.allocation_budget,
                        crate::TextureCategory::CurveLuts,
                    )
                    .map_err(|error| {
                        let message = format!("effect curve index={index} phase=allocate: {error}");
                        error.with_diagnostic(message)
                    })?;
                    self.allocation_uploads.track(target.texture.allocation())?;
                    upload(queue, &target.texture, 256, 1, &bytes);
                    self.parameter_resource_upload_bytes =
                        self.parameter_resource_upload_bytes.saturating_add(1024);
                    let next = Lut {
                        texture: target.texture,
                        view: target.view,
                        last: bytes,
                        source: None,
                    };
                    self.invalidate();
                    if index == self.luts.len() {
                        self.luts.push(next);
                    } else {
                        self.luts[index] = next;
                    }
                } else if self.luts[index].last != bytes {
                    self.allocation_uploads
                        .track(self.luts[index].texture.allocation())?;
                    upload(queue, &self.luts[index].texture, 256, 1, &bytes);
                    self.parameter_resource_upload_bytes =
                        self.parameter_resource_upload_bytes.saturating_add(1024);
                    self.luts[index].last = bytes;
                }
                continue;
            }
            let source = scene.curve_sources.get(index).ok_or_else(|| RenderError::Invalid(
                format!("effect curve index={index} phase=authored-source: high-precision source is missing")))?;
            let replace = self
                .luts
                .get(index)
                .is_none_or(|old| old.texture.format() != wgpu::TextureFormat::Rgba32Float);
            if !replace && self.luts[index].source.as_ref() == Some(source) {
                continue;
            }
            self.format_capabilities.require(
                ImageStorage::Rgba32Float,
                ImageUsage {
                    sampled: true,
                    copy_dst: true,
                    ..Default::default()
                },
            )?;
            let width = motion_model::HIGH_PRECISION_CURVE_SAMPLES as u32;
            let max_dimension = device.limits().max_texture_dimension_2d;
            if width > max_dimension {
                return Err(RenderError::Resource {
                    rejection: crate::ResourceRejection::TextureDimensions { width, height: 1, max_dimension },
                    message: format!("effect curve index={index} phase=dimensions: LUT width={width} exceeds {max_dimension}"),
                });
            }
            // Admit the replacement before materializing a dense CPU table.
            // Existing bindings/backing stay valid if allocation or tracking fails.
            let next = if replace {
                Some(
                    device
                        .create_admitted_texture(
                            &wgpu::TextureDescriptor {
                                label: Some("numeric F32 authored curve LUT"),
                                size: wgpu::Extent3d {
                                    width,
                                    height: 1,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Rgba32Float,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING
                                    | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            },
                            &self.allocation_budget,
                            crate::TextureCategory::CurveLuts,
                        )
                        .map_err(|error| {
                            let message =
                                format!("effect curve index={index} phase=dense-allocate: {error}");
                            error.with_diagnostic(message)
                        })?,
                )
            } else {
                None
            };
            let target = if let Some(texture) = &next {
                texture
            } else {
                &self
                    .luts
                    .get(index)
                    .ok_or_else(|| {
                        RenderError::Invalid(format!(
                            "effect curve index={index} phase=upload: retained LUT is missing"
                        ))
                    })?
                    .texture
            };
            self.allocation_uploads.track(target.allocation())?;
            let rows = source.high_precision_lut();
            let bytes: &[u8] = bytemuck::cast_slice(&rows);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 16),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            self.parameter_resource_upload_bytes = self
                .parameter_resource_upload_bytes
                .saturating_add(bytes.len() as u64);
            if let Some(texture) = next {
                let view = texture.create_view(&Default::default());
                let next = Lut {
                    texture,
                    view,
                    last: [0; 1024],
                    source: Some(source.clone()),
                };
                self.invalidate();
                if index == self.luts.len() {
                    self.luts.push(next);
                } else {
                    self.luts[index] = next;
                }
            } else {
                self.luts[index].source = Some(source.clone());
            }
        }
        Ok(())
    }
}
