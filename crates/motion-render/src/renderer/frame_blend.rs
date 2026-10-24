//! Two decoded source images, retained separately from the composited video image.
use super::*;
use std::collections::HashSet;

pub(super) struct FrameBlendGpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: crate::allocation_buffer::AllocationBuffer,
    outputs: HashMap<u64, (u64, u64, u64, u32)>,
}

impl FrameBlendGpu {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        budget: &crate::AllocationBudget,
    ) -> Result<Self> {
        let uniform = crate::allocation_buffer::AllocationBuffer::new(
            device,
            budget,
            &wgpu::BufferDescriptor {
                label: Some("video frame blend weight"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        )?;
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video frame blend layout"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear premultiplied video frame blend"),
            source: wgpu::ShaderSource::Wgsl(include_str!("frame_blend.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video frame blend pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video frame blend pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if format == wgpu::TextureFormat::Rgba8Unorm {
                    "fragment_main"
                } else {
                    "fragment_srgb"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            layout,
            pipeline,
            uniform,
            outputs: HashMap::new(),
        })
    }
}

impl Renderer {
    pub(super) fn invalidate_video_blend(&mut self, object: u64) {
        if let Some(gpu) = &mut self.video_blend {
            gpu.outputs.remove(&object);
        }
    }

    /// A normal miss is not a fault. Slots are bounded to the left/right sources.
    pub fn video_blend_source_ready(&self, object: u64, source: u64, pts: u64, slot: u8) -> bool {
        slot < 2
            && self
                .images
                .get(&TextureKey::VideoBlendSource(object, slot))
                .is_some_and(|image| image.video_stamp == Some((source, pts)))
    }
    pub fn video_blend_source_size(&self, object: u64, slot: u8) -> Option<(u32, u32)> {
        self.images
            .get(&TextureKey::VideoBlendSource(object, slot))
            .map(|i| i.size)
    }

    /// Promote the previous right frame to the new left (or the reverse on
    /// backward seeks) without copying pixels, uploading or allocating textures.
    pub fn reuse_video_blend_source(
        &mut self,
        object: u64,
        source: u64,
        pts: u64,
        slot: u8,
        size: (u32, u32),
    ) -> bool {
        if slot > 1 {
            return false;
        }
        let ready = |renderer: &Self, slot| {
            renderer.video_blend_source_ready(object, source, pts, slot)
                && renderer.video_blend_source_size(object, slot) == Some(size)
        };
        if ready(self, slot) {
            return true;
        }
        let other = 1 - slot;
        if !ready(self, other) {
            return false;
        }
        let from = TextureKey::VideoBlendSource(object, other);
        let to = TextureKey::VideoBlendSource(object, slot);
        let image = self
            .images
            .remove(&from)
            .expect("checked video blend source");
        if let Some(previous) = self.images.insert(to, image) {
            self.images.insert(from, previous);
        }
        self.invalidate_video_blend(object);
        true
    }

    /// Preserve a completed upload/conversion before replacing the live video image.
    /// Do not call this with an encoded, unsubmitted YUV conversion.
    pub fn cache_video_blend_source(
        &mut self,
        object: u64,
        source: u64,
        pts: u64,
        slot: u8,
    ) -> Result<()> {
        self.check_health()?;
        self.check_video_pending(object)?;
        if slot > 1 {
            return Err(RenderError::Invalid(format!(
                "video object={object} phase=blend_cache invalid slot={slot}"
            )));
        }
        let input = TextureKey::Video(object);
        let size = self.images.get(&input).filter(|i| i.video_stamp == Some((source, pts))
            && self.video_gpu.as_ref().is_none_or(|gpu| gpu.ready(object)))
            .map(|i| i.size).ok_or_else(|| RenderError::Invalid(format!(
                "video object={object} source={source} pts_us={pts} phase=blend_cache completed source missing")))?;
        if self.video_blend_source_ready(object, source, pts, slot)
            && self.video_blend_source_size(object, slot) == Some(size)
        {
            return Ok(());
        }
        let output = TextureKey::VideoBlendSource(object, slot);
        self.submit_video_blend(input, input, output, 0.0, source).map_err(|error| {
            let context = format!("video object={object} source={source} pts_us={pts} slot={slot} size={}x{} phase=blend_cache: {error}", size.0, size.1);
            error.with_diagnostic(context)
        })?;
        self.images
            .get_mut(&output)
            .expect("blend target allocated")
            .video_stamp = Some((source, pts));
        // A newly cached source invalidates any mixture referencing the previous slot.
        self.invalidate_video_blend(object);
        Ok(())
    }

    /// Mix actual adjacent PTS sources, in linear premultiplied color space.
    /// Cached inputs survive weight-only changes; no decoder or CPU pixel loop runs here.
    pub fn blend_video_sources(
        &mut self,
        object: u64,
        source: u64,
        left_pts: u64,
        right_pts: u64,
        weight: f32,
    ) -> Result<()> {
        self.check_health()?;
        self.check_video_pending(object)?;
        if !weight.is_finite() || !(0.0..=1.0).contains(&weight) || left_pts >= right_pts {
            return Err(RenderError::Invalid(format!(
                "video object={object} source={source} phase=blend invalid PTS/weight"
            )));
        }
        if !self.video_blend_source_ready(object, source, left_pts, 0)
            || !self.video_blend_source_ready(object, source, right_pts, 1)
        {
            return Err(RenderError::Invalid(format!(
                "video object={object} source={source} phase=blend adjacent source missing"
            )));
        }
        let stamp = (source, left_pts, right_pts, weight.to_bits());
        if self
            .video_blend
            .as_ref()
            .is_some_and(|gpu| gpu.outputs.get(&object) == Some(&stamp))
        {
            return Ok(());
        }
        let left = TextureKey::VideoBlendSource(object, 0);
        let right = TextureKey::VideoBlendSource(object, 1);
        let size = self.images[&left].size;
        if self.images[&right].size != size {
            return Err(RenderError::Invalid(format!(
                "video object={object} source={source} phase=blend adjacent dimensions differ"
            )));
        }
        let output = TextureKey::Video(object);
        self.submit_video_blend(left, right, output, weight, source).map_err(|error| {
            let context = format!("video object={object} source={source} left_pts_us={left_pts} right_pts_us={right_pts} size={}x{} phase=blend: {error}", size.0, size.1);
            error.with_diagnostic(context)
        })?;
        // The result is not a decoded frame: a subsequent Hold must upload it anew.
        self.images
            .get_mut(&output)
            .expect("blend target allocated")
            .video_stamp = None;
        self.video_blend
            .as_mut()
            .expect("blend pipeline initialized")
            .outputs
            .insert(object, stamp);
        Ok(())
    }

    fn ensure_video_blend_image(
        &mut self,
        key: TextureKey,
        size: (u32, u32),
        source: u64,
    ) -> Result<()> {
        // Write explicitly encoded bytes, then sample through the usual sRGB
        // view. This is the same format contract as GPU video conversion.
        let reinterpret = self
            .adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VIEW_FORMATS);
        let format = if reinterpret {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            TARGET_FORMAT
        };
        let bytes = u64::from(size.0) * u64::from(size.1) * 4;
        let previous = self.images.get(&key).map_or(0, |i| i.bytes);
        self.trim_image_cache(bytes.saturating_sub(previous));
        let required = self.texture_bytes - previous
            + bytes
            + self.video_plane_bytes()
            + self.effect_gpu.state.resource_bytes;
        let budget = self.effect_gpu.builder.source_texture_budget;
        if required > budget {
            return Err(RenderError::budget(ResourceBudgetScope::SourceTextures, required, budget,
                format!("video target={key:?} source={source} phase=blend_allocate: adjacent video textures exceed source budget")));
        }
        if self.images.get(&key).is_some_and(|i| {
            i.size == size
                && i._texture.format() == format
                && i._texture
                    .usage()
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        }) {
            return Ok(());
        }
        let texture = self.device.create_admitted_texture(&wgpu::TextureDescriptor {
            label: Some("video frame blend image"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            view_formats: if reinterpret { &[TARGET_FORMAT] } else { &[] },
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
        }, &self.allocation_budget, crate::TextureCategory::VideoRgba)?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(TARGET_FORMAT),
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video frame blend image"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.images.insert(
            key,
            GpuImage {
                _texture: texture,
                view,
                bind_group,
                bytes,
                size,
                video_stamp: None,
            },
        );
        self.texture_bytes = self.texture_bytes - previous + bytes;
        self.effect_gpu.invalidate();
        Ok(())
    }

    fn submit_video_blend(
        &mut self,
        left: TextureKey,
        right: TextureKey,
        output: TextureKey,
        weight: f32,
        source: u64,
    ) -> Result<()> {
        // Refuse scope exhaustion before replacing a completed output image,
        // allocating a cache target or queuing a uniform write.
        let mut encoder = self.allocation_command_encoder_for(crate::gpu_commands::CommandStage::VideoBlend, &wgpu::CommandEncoderDescriptor {
                label: Some("video frame blend"),
            })?;
        let size = self.images[&left].size;
        self.ensure_video_blend_image(output, size, source)?;
        let output_image = &self.images[&output];
        let format = output_image._texture.format();
        let output_view = output_image._texture.create_view(&Default::default());
        if self.video_blend.is_none() {
            self.video_blend = Some(FrameBlendGpu::new(&self.device, format, &self.allocation_budget)?);
        }
        let gpu = self.video_blend.as_ref().expect("blend pipeline initialized");
        self.allocation_uploads.track(Some(gpu.uniform.allocation()))?;
        self.queue.write_buffer(
            &gpu.uniform,
            0,
            bytemuck::cast_slice(&[weight, 0.0, 0.0, 0.0]),
        );
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video frame blend sources"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.images[&left].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.images[&right].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: gpu.uniform.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("video frame blend"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Each uniform write precedes its own submission, including cached-source copies.
        self.submit_allocated(encoder.finish(), |_| {})?;
        self.video_blend_submissions = self.video_blend_submissions.saturating_add(1);
        self.check_health()
    }

    pub(super) fn retain_video_blend_sources(&mut self, scene: &Scene) {
        fn collect(scene: &Scene, live: &mut HashSet<u64>) {
            for layer in &scene.layers {
                if layer.video.is_some()
                    && layer.sampling.frame_blend
                        == motion_model::render_sampling::FrameBlend::Linear
                {
                    live.insert(layer.id);
                }
            }
            for child in &scene.nested {
                collect(&child.scene, live);
            }
        }
        let mut live = HashSet::new();
        collect(scene, &mut live);
        let stale: Vec<_> = self.images.keys().copied().filter(|key|
            matches!(key, TextureKey::VideoBlendSource(object, _) if !live.contains(object))).collect();
        for key in stale {
            if let Some(image) = self.images.remove(&key) {
                self.texture_bytes = self.texture_bytes.saturating_sub(image.bytes);
            }
        }
        if let Some(gpu) = &mut self.video_blend {
            gpu.outputs.retain(|id, _| live.contains(id));
        }
    }
}
