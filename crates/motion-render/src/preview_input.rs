//! Bounded, independent source textures for authoring previews.
//! Every exposed view samples linear premultiplied RGBA. No CPU GPU-readback.
use crate::resource_ledger::{DeviceTextureExt, OwnedTexture};
use crate::{
    allocation_buffer::AllocationBuffer, allocation_uploads::AllocationUploads,
    image_resources::Resolution, video_gpu::VideoGpu, AdmittedTextureView, AllocationBudget,
    RenderError, ResourceBudgetScope, Yuv420Frame,
};
use std::sync::Arc;

pub const DEFAULT_INPUT_BUDGET: u64 = 64 * 1024 * 1024;
pub struct PreviewInputTexture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    budget: u64,
    copy_group: Option<wgpu::BindGroup>,
    view: Option<AdmittedTextureView>,
    size: (u32, u32),
    rgba_size: (u32, u32),
    video: Option<VideoGpu>,
    copy_layout: wgpu::BindGroupLayout,
    copy_pipeline: wgpu::RenderPipeline,
    copy_sampler: wgpu::Sampler,
    copy_uniform: AllocationBuffer,
    output: Option<OwnedTexture>,
    rgba: Option<OwnedTexture>,
    allocation_budget: AllocationBudget,
    allocation_owner: Arc<()>,
    // Submit pending writes only after views/bindings and owned caches drop.
    allocation_uploads: AllocationUploads,
}
impl PreviewInputTexture {
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        if let Some(texture) = &self.output {
            ledger.retained(
                texture,
                crate::TextureCategory::AuthoringInputs,
                "authoring/input/output".into(),
            );
        }
        if let Some(texture) = &self.rgba {
            ledger.retained(
                texture,
                crate::TextureCategory::AuthoringInputs,
                "authoring/input/rgba".into(),
            );
        }
        if let Some(video) = &self.video {
            video.collect_textures(&mut ledger);
        }
        ledger
    }
    /// Source category allowance plus the same combined budget as the renderer.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        budget: u64,
        allocation_budget: AllocationBudget,
    ) -> Result<Self, RenderError> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("authoring source copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authoring source color contract"),
            source: wgpu::ShaderSource::Wgsl(include_str!("preview_input.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("authoring source resize"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let copy_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let copy_uniform = AllocationBuffer::new(
            &device,
            &allocation_budget,
            &wgpu::BufferDescriptor {
                label: Some("source alpha convention"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        )?;
        let allocation_uploads = AllocationUploads::new(queue.clone());
        Ok(Self {
            device,
            queue,
            budget,
            output: None,
            view: None,
            size: (0, 0),
            rgba: None,
            rgba_size: (0, 0),
            video: None,
            copy_layout: layout,
            copy_pipeline: pipeline,
            copy_sampler,
            copy_uniform,
            copy_group: None,
            allocation_budget,
            allocation_owner: Arc::new(()),
            allocation_uploads,
        })
    }
    pub fn view(&self) -> Option<&AdmittedTextureView> {
        self.view.as_ref()
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.size
    }
    pub fn memory_bytes(&self) -> u64 {
        bytes(self.size) + bytes(self.rgba_size) + self.video.as_ref().map_or(0, VideoGpu::bytes)
    }
    fn validate_size(&self, width: u32, height: u32) -> Result<(), RenderError> {
        let limit = self.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err(RenderError::Invalid(
                "preview input dimensions exceed device limits".into(),
            ));
        }
        Ok(())
    }
    fn output_size(
        &self,
        width: u32,
        height: u32,
        max_edge: u32,
    ) -> Result<(u32, u32), RenderError> {
        self.validate_size(width, height)?;
        if !(1..=crate::image_resources::MAX_PREVIEW_EDGE).contains(&max_edge) {
            return Err(RenderError::Invalid(
                "invalid preview input raster edge".into(),
            ));
        }
        Ok(Resolution::Preview(max_edge).dimensions(width, height))
    }
    fn budget(&self, output: (u32, u32), source_bytes: u64) -> Result<(), RenderError> {
        let required = bytes(output)
            .checked_add(source_bytes)
            .ok_or_else(|| RenderError::Invalid("preview input capacity overflow".into()))?;
        if required > self.budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::SourceTextures,
                required,
                self.budget,
                format!(
                    "preview input exceeds GPU budget required_bytes={required} budget_bytes={}",
                    self.budget
                ),
            ));
        }
        Ok(())
    }
    fn texture(&self, size: (u32, u32)) -> Result<OwnedTexture, RenderError> {
        self.device.create_admitted_texture(
            &wgpu::TextureDescriptor {
                label: Some("independent authoring source"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            &self.allocation_budget,
            crate::TextureCategory::AuthoringInputs,
        )
    }
    fn next_output(&self, size: (u32, u32)) -> Result<OwnedTexture, RenderError> {
        if self.size == size {
            self.output
                .clone()
                .ok_or_else(|| RenderError::Invalid("authoring output is missing".into()))
        } else {
            self.texture(size)
        }
    }
    fn commit_output(
        &mut self,
        texture: OwnedTexture,
        view: AdmittedTextureView,
        size: (u32, u32),
    ) {
        self.view = Some(view);
        self.output = Some(texture);
        self.size = size;
    }
    /// Image-resource Pixels already use linear-premultiplied sRGB storage.
    /// Raw codec RGBA is straight sRGB; pass false to premultiply on the GPU.
    pub fn upload_rgba(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
        premultiplied: bool,
        max_edge: u32,
    ) -> Result<(), RenderError> {
        self.upload_rgba_inner(width, height, pixels, premultiplied, max_edge)
            .map_err(|error| {
                let message = format!("authoring input size={width}x{height} edge={max_edge} phase=rgba_upload: {error}");
                error.with_diagnostic(message)
            })
    }
    fn upload_rgba_inner(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
        premultiplied: bool,
        max_edge: u32,
    ) -> Result<(), RenderError> {
        let output = self.output_size(width, height, max_edge)?;
        if pixels.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return Err(RenderError::Invalid(
                "preview RGBA byte count mismatch".into(),
            ));
        }
        let direct = premultiplied && output == (width, height);
        self.budget(output, if direct { 0 } else { bytes((width, height)) })?;
        if direct {
            let texture = self.next_output(output)?;
            let view = texture.admitted_view()?;
            self.allocation_uploads.track(texture.allocation())?;
            write(&self.queue, &texture, width, height, pixels);
            self.copy_group = None;
            self.rgba = None;
            self.rgba_size = (0, 0);
            self.video = None;
            self.commit_output(texture, view, output);
            return Ok(());
        }
        // Hold the prior input through all allocation, scope and journal checks.
        let mut encoder = self.allocation_budget.encoder(
            &self.device,
            self.allocation_owner.clone(),
            &Default::default(),
        )?;
        let texture = self.next_output(output)?;
        let output_view = texture.admitted_view()?;
        let source = if self.rgba_size == (width, height) {
            self.rgba
                .clone()
                .ok_or_else(|| RenderError::Invalid("authoring RGBA source is missing".into()))?
        } else {
            self.texture((width, height))?
        };
        let group = if self.rgba_size == (width, height) {
            self.copy_group.clone()
        } else {
            None
        };
        let group = group.unwrap_or_else(|| {
            let view = source.create_view(&Default::default());
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("authoring source copy"),
                layout: &self.copy_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.copy_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.copy_uniform.as_entire_binding(),
                    },
                ],
            })
        });
        let source_lease = source
            .allocation()
            .ok_or_else(|| RenderError::Invalid("authoring source has no reservation".into()))?;
        self.allocation_uploads
            .track_many([source_lease, self.copy_uniform.allocation()])?;
        write(&self.queue, &source, width, height, pixels);
        let flags = [u32::from(!premultiplied), 0, 0, 0];
        self.queue
            .write_buffer(&self.copy_uniform, 0, bytemuck::cast_slice(&flags));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: Some("authoring source GPU resize"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.copy_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.finish().submit(
            &self.allocation_budget,
            &self.allocation_owner,
            |buffer| self.queue.submit(Some(buffer)),
            &self.queue,
            &self.allocation_uploads,
        )?;
        self.copy_group = Some(group);
        self.rgba = Some(source);
        self.rgba_size = (width, height);
        self.video = None;
        self.commit_output(texture, output_view, output);
        Ok(())
    }
    pub fn upload_yuv(&mut self, frame: &Yuv420Frame, max_edge: u32) -> Result<(), RenderError> {
        self.upload_yuv_inner(frame, max_edge).map_err(|error| {
            let message = format!(
                "authoring input size={}x{} edge={max_edge} phase=yuv_upload: {error}",
                frame.width, frame.height
            );
            error.with_diagnostic(message)
        })
    }
    fn upload_yuv_inner(&mut self, frame: &Yuv420Frame, max_edge: u32) -> Result<(), RenderError> {
        frame.validate()?;
        self.validate_size(frame.width, frame.height)?;
        let (width, height) = frame.display_size();
        let output = self.output_size(width, height, max_edge)?;
        self.budget(output, frame.bytes() as u64)?;
        let mut encoder = self.allocation_budget.encoder(
            &self.device,
            self.allocation_owner.clone(),
            &Default::default(),
        )?;
        let texture = self.next_output(output)?;
        let output_view = texture.admitted_view()?;
        let new_converter = self
            .video
            .is_none()
            .then(|| VideoGpu::new(&self.device, wgpu::TextureFormat::Rgba8UnormSrgb));
        let converter = self
            .video
            .as_ref()
            .or(new_converter.as_ref())
            .ok_or_else(|| RenderError::Invalid("authoring video converter is missing".into()))?;
        let prepared = converter
            .prepare_input(&self.device, &self.allocation_budget, 0, frame)?
            .track(&self.allocation_uploads, &texture)?;
        if let Some(converter) = new_converter {
            self.video = Some(converter);
        }
        let converter = self.video.as_mut().ok_or_else(|| {
            RenderError::Invalid("prepared authoring converter is missing".into())
        })?;
        let upload = converter.encode_prepared(
            &self.queue,
            prepared,
            frame,
            &output_view,
            output,
            &mut encoder,
        );
        encoder.finish().submit(
            &self.allocation_budget,
            &self.allocation_owner,
            |buffer| self.queue.submit(Some(buffer)),
            &self.queue,
            &self.allocation_uploads,
        )?;
        upload.mark_submitted();
        self.copy_group = None;
        self.rgba = None;
        self.rgba_size = (0, 0);
        self.commit_output(texture, output_view, output);
        Ok(())
    }
    pub fn transparent(&mut self) -> Result<(), RenderError> {
        self.upload_rgba(1, 1, &[0; 4], true, 1)
    }
}
fn bytes(size: (u32, u32)) -> u64 {
    u64::from(size.0) * u64::from(size.1) * 4
}
fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, width: u32, height: u32, pixels: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}
