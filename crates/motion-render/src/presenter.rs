use crate::Renderer;
mod memory;
#[cfg(test)]
mod admission_tests;

/// Inspect a composition channel before presentation encoding as opaque grayscale.
/// RGB is sampled in working linear light; Alpha is the original coverage value.
/// This is presentation state; it does not change the source or exported color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ViewChannel {
    #[default]
    Color = 0,
    Red = 1,
    Green = 2,
    Blue = 3,
    Alpha = 4,
}

/// GPU-only final presentation keeps the compositor in a predictable sRGB target
/// even when an Android swapchain exposes a linear UNORM pixel format.
pub struct Presenter {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    view_group: wgpu::BindGroup,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    buffer: memory::Buffer,
    view_buffer: memory::Buffer,
    view_channel: ViewChannel,
    // Retire GPU bindings before releasing their source reservation.
    source_allocation: Option<crate::allocation_budget::AllocationLease>,
}
impl Presenter {
    pub fn new_admitted(renderer: &Renderer, source: &crate::AdmittedTextureView,
        format: wgpu::TextureFormat) -> Result<Self, crate::RenderError> {
        let buffers = memory::Buffers::admitted(renderer, format)?;
        let mut presenter = Self::with_buffers(renderer, source, format, buffers);
        presenter.source_allocation = Some(source.lease.clone());
        Ok(presenter)
    }
    pub fn set_admitted_source(&mut self, device: &wgpu::Device, source: &crate::AdmittedTextureView) {
        self.set_source(device, source);
        self.source_allocation = Some(source.lease.clone());
    }
    pub fn new(
        renderer: &Renderer,
        source: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::with_buffers(renderer, source, format, memory::Buffers::raw(renderer, format))
    }
    fn with_buffers(renderer: &Renderer, source: &wgpu::TextureView,
        format: wgpu::TextureFormat, buffers: memory::Buffers) -> Self {
        let device = &renderer.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Motion Studio surface presentation"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let memory::Buffers { color: buffer, view: view_buffer } = buffers;
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        let view_group = Self::source_group(device, &layout, &sampler, &view_buffer, source);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Motion Studio surface encoding shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("present.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Motion Studio GPU presentation"),
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            source_allocation: None,
            pipeline,
            group,
            format,
            layout,
            sampler,
            buffer,
            view_buffer,
            view_group,
            view_channel: ViewChannel::Color,
        }
    }
    /// Rebind a preview buffer without rebuilding the presentation pipeline.
    pub fn set_source(&mut self, device: &wgpu::Device, source: &wgpu::TextureView) {
        self.group = Self::source_group(device, &self.layout, &self.sampler, &self.buffer, source);
        self.view_group = Self::source_group(device, &self.layout, &self.sampler, &self.view_buffer, source);
        self.source_allocation = None;
    }
    /// Select a raw stored channel for `encode_view`. The ordinary `encode` /
    /// `draw` paths keep their color output, including their original alpha.
    /// Single-channel intensity is a display value, so alpha is not gamma-encoded.
    pub fn set_view_channel(&mut self, queue: &wgpu::Queue, channel: ViewChannel) -> Result<(), crate::RenderError> {
        if self.view_channel != channel {
            self.view_buffer.write(queue, 4, bytemuck::bytes_of(&(channel as u32)))?;
            self.view_channel = channel;
        }
        Ok(())
    }
    fn source_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler, buffer: &wgpu::Buffer, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: buffer.as_entire_binding() },
            ],
        })
    }
    /// Crop a virtual canvas inside a valid surface viewport. Pan/zoom never sets
    /// an out-of-surface GPU viewport or changes the compositor/export camera.
    pub fn encode_view(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView, timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        viewport: [f32; 4], image: Option<[f32; 4]>, background: [f32; 4]) -> Result<(), crate::RenderError> {
        if let Some([ix, iy, iw, ih]) = image {
            let [x, y, w, h] = viewport;
            let uv = [(x-ix)/iw, (y-iy)/ih, w/iw, h/ih];
            self.view_buffer.write(queue, 16, bytemuck::cast_slice(&uv))?;
            self.encode_group(encoder, output, timestamps, Some(viewport), background, &self.view_group);
        } else if self.view_channel != ViewChannel::Color {
            self.view_buffer.write(queue, 16, bytemuck::cast_slice(&[0.0f32, 0., 1., 1.]))?;
            self.encode_group(encoder, output, timestamps, Some(viewport), background, &self.view_group);
        } else {
            self.encode(encoder, output, timestamps, Some(viewport), background);
        }
        Ok(())
    }
    /// Present a live image, optionally comparing a captured image to its right.
    /// `region` and `split` use normalized composition coordinates; the entire
    /// output is cleared on every call, including outside the selected region.
    pub fn encode_inspection_view(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView, viewport: [f32; 4], image: Option<[f32; 4]>,
        background: [f32; 4], region: Option<[f32; 4]>,
        comparison: Option<(&Presenter, f32)>) -> Result<(), crate::RenderError> {
        let [x, y, w, h] = viewport;
        let [ix, iy, iw, ih] = image.unwrap_or(viewport);
        let [rx, ry, rw, rh] = region.unwrap_or([0., 0., 1., 1.]);
        if !viewport.iter().chain([ix, iy, iw, ih, rx, ry, rw, rh].iter()).all(|v| v.is_finite())
            || x < 0. || y < 0. || w <= 0. || h <= 0. || iw <= 0. || ih <= 0.
            || rx < 0. || ry < 0. || rw <= 0. || rh <= 0. || rx + rw > 1. || ry + rh > 1.
            || x + w > u32::MAX as f32 || y + h > u32::MAX as f32 {
            return Err(crate::RenderError::Invalid("invalid viewer inspection viewport, image or region".into()));
        }
        if let Some((other, split)) = comparison {
            if !split.is_finite() || !(0. ..=1.).contains(&split)
                || other.format != self.format || other.view_channel != self.view_channel {
                return Err(crate::RenderError::Invalid("invalid viewer comparison split, format or channel".into()));
            }
        }
        let uv = [(x-ix)/iw, (y-iy)/ih, w/iw, h/ih];
        self.view_buffer.write(queue, 16, bytemuck::cast_slice(&uv))?;
        if let Some((other, _)) = comparison { other.view_buffer.write(queue, 16, bytemuck::cast_slice(&uv))?; }
        let left = x.max(ix + rx*iw);
        let top = y.max(iy + ry*ih);
        let right = (x+w).min(ix+(rx+rw)*iw);
        let bottom = (y+h).min(iy+(ry+rh)*ih);
        let scissor = |l: f32, r: f32| -> Option<[u32; 4]> {
            let l = l.ceil().max(0.) as u32;
            let t = top.ceil().max(0.) as u32;
            let r = r.floor().max(0.) as u32;
            let b = bottom.floor().max(0.) as u32;
            (r > l && b > t).then(|| [l, t, r-l, b-t])
        };
        let cut = comparison.map_or(right, |(_, split)| (ix+split*iw).clamp(left, right.max(left)));
        self.encode_group_region(encoder, output, Some(viewport), background, &self.view_group,
            scissor(left, cut), false);
        if let Some((other, _)) = comparison {
            other.encode_group_region(encoder, output, Some(viewport), background, &other.view_group,
                scissor(cut, right), true);
        }
        Ok(())
    }
    pub fn draw(&self, renderer: &Renderer, output: &wgpu::TextureView) -> Result<(), crate::RenderError> {
        let mut encoder = renderer
            .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio present"),
            })?;
        self.encode(&mut encoder, output, None, None, [0.0, 0.0, 0.0, 1.0]);
        renderer.submit_allocated(encoder.finish(), |_| {})?;
        Ok(())
    }
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        viewport: Option<[f32; 4]>,
        background: [f32; 4],
    ) {
        self.encode_group(encoder, output, timestamps, viewport, background, &self.group);
    }
    fn encode_group(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>, viewport: Option<[f32; 4]>,
        background: [f32; 4], group: &wgpu::BindGroup) {
        self.encode_group_impl(encoder, output, timestamps, viewport, background, group, None, false, false)
    }
    fn encode_group_region(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView,
        viewport: Option<[f32; 4]>, background: [f32; 4], group: &wgpu::BindGroup,
        scissor: Option<[u32; 4]>, load: bool) {
        self.encode_group_impl(encoder, output, None, viewport, background, group, scissor, load, true)
    }
    fn encode_group_impl(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>, viewport: Option<[f32; 4]>,
        background: [f32; 4], group: &wgpu::BindGroup, scissor: Option<[u32; 4]>, load: bool, clipped: bool) {
        let mut clear = [0.0; 4];
        clear[3] = background[3];
        for i in 0..3 {
            let linear = crate::renderer::srgb_to_linear(background[i]) * background[3];
            clear[i] = if self.format.is_srgb() {
                linear
            } else {
                crate::renderer::linear_to_srgb(linear)
            };
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                multiview_mask: None,
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if load { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0] as f64,
                            g: clear[1] as f64,
                            b: clear[2] as f64,
                            a: clear[3] as f64,
                        }) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timestamps,
                occlusion_query_set: None,
            });
            if let Some([x, y, w, h]) = viewport {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
            }
            if !clipped || scissor.is_some() {
                if let Some([x, y, w, h]) = scissor { pass.set_scissor_rect(x, y, w, h); }
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }
}
