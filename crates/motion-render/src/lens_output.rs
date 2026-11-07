//! Nonlinear spatial-raster output primitive. This module does not warp an
//! entire composition or reinterpret flat authored layers as camera geometry.
use crate::{
    allocation_buffer::AllocationBuffer, resource_ledger::OwnedTexture, AllocationEncoder,
    ImageCaptureTarget, RenderError, RenderTarget, Renderer,
};
use motion_effects::{
    image_contract::{ImageMeaning, ImageStorage, ImageUsage},
    AlphaMode, WorkingSpace,
};
use motion_model::tracking::lens::{LensRaster, LensWarp};
use std::sync::Arc;

/// One pipeline per output storage/device; changing the sampled camera creates
/// only a small immutable per-encode uniform and binding, not another pipeline.
pub struct LensOutputPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    owner: Arc<()>,
}
impl LensOutputPipeline {
    pub fn new(renderer: &Renderer, format: wgpu::TextureFormat) -> Result<Self, RenderError> {
        renderer.check_health()?;
        let storage = match format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm => {
                ImageStorage::Rgba8Unorm
            }
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb => {
                ImageStorage::Rgba8UnormSrgb
            }
            wgpu::TextureFormat::Rgba16Unorm => ImageStorage::Rgba16Unorm,
            wgpu::TextureFormat::Rgba16Float => ImageStorage::Rgba16Float,
            wgpu::TextureFormat::Rgba32Float => ImageStorage::Rgba32Float,
            _ => {
                return Err(RenderError::Invalid(
                    "lens output requires supported RGBA color storage".into(),
                ))
            }
        };
        // The pass uses loads rather than a sampler and never blends. Do not
        // demand optional Float32 filtering/blending, or create an unsupported
        // Norm16 pipeline before the enabled-device capability check.
        renderer.image_capabilities.require(
            storage,
            ImageUsage {
                render: true,
                ..Default::default()
            },
        )?;
        let device = &renderer.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("calibrated lens inputs"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(128),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("calibrated lens inverse"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lens_output.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("calibrated lens layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("calibrated lens spatial output"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
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
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        renderer.check_health()?;
        Ok(Self {
            pipeline,
            layout,
            format,
            owner: renderer.allocation_owner.clone(),
        })
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// Input and output are linear premultiplied color. Interpolation acts on
    /// those values, with transparent samples outside the producer allocation.
    /// There is no tone mapping, clamping, transfer or precision reduction.
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        lens: LensWarp,
        raster: LensRaster,
        source: &RenderTarget,
        destination: &RenderTarget,
    ) -> Result<(), RenderError> {
        source.check_owner(renderer)?;
        destination.check_owner(renderer)?;
        encoder.check_owner(&self.owner)?;
        self.encode_checked(
            renderer,
            encoder,
            lens,
            raster,
            source.texture(),
            &destination.view,
        )
    }
    pub fn encode_images(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        lens: LensWarp,
        raster: LensRaster,
        source: &ImageCaptureTarget,
        destination: &ImageCaptureTarget,
    ) -> Result<(), RenderError> {
        source.check_owner(renderer)?;
        destination.check_owner(renderer)?;
        encoder.check_owner(&self.owner)?;
        let meaning = ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        };
        if source.contract().meaning != meaning || destination.contract().meaning != meaning {
            return Err(RenderError::Invalid(
                "lens image input/output must be linear premultiplied color".into(),
            ));
        }
        self.encode_checked(
            renderer,
            encoder,
            lens,
            raster,
            source.texture(),
            destination.view(),
        )
    }
    /// Internal composition passes already receive the renderer's admitted
    /// parent encoder. Public entry points retain the encoder ownership guard.
    pub(crate) fn encode_spatial(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        lens: LensWarp,
        raster: LensRaster,
        source: &RenderTarget,
        destination: &RenderTarget,
    ) -> Result<(), RenderError> {
        source.check_owner(renderer)?;
        destination.check_owner(renderer)?;
        self.encode_checked(
            renderer,
            encoder,
            lens,
            raster,
            source.texture(),
            &destination.view,
        )
    }
    fn encode_checked(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        lens: LensWarp,
        raster: LensRaster,
        source: &OwnedTexture,
        destination: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        renderer.check_health()?;
        if !Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(RenderError::Invalid(
                "lens pipeline belongs to another renderer".into(),
            ));
        }
        lens.validate_raster(raster)?;
        if &**source == destination.texture() {
            return Err(RenderError::Invalid(
                "lens output cannot sample its destination".into(),
            ));
        }
        if destination.texture().format() != self.format
            || crate::image_formats::color_precision(source.format())
                != crate::image_formats::color_precision(self.format)
        {
            return Err(RenderError::Invalid(format!(
                "lens output storage mismatch source={:?} destination={:?} pipeline={:?}",
                source.format(),
                destination.texture().format(),
                self.format
            )));
        }
        if raster.size() != [source.width(), source.height()] {
            return Err(RenderError::Invalid(format!(
                "lens source dimensions {:?} do not match certified raster {:?}",
                [source.width(), source.height()],
                raster.size()
            )));
        }
        let words = uniform(
            lens,
            raster,
            [
                destination.texture().width(),
                destination.texture().height(),
            ],
        )?;
        // This allocation belongs to this encoder. Two queued lens frames may
        // use different cameras without overwriting uniforms before submission.
        let buffer = AllocationBuffer::new_init(
            &renderer.device,
            renderer.allocation_budget(),
            &wgpu::util::BufferInitDescriptor {
                label: Some("immutable calibrated lens parameters"),
                contents: bytemuck::cast_slice(&words),
                usage: wgpu::BufferUsages::UNIFORM,
            },
        )?;
        let view = source.create_view(&Default::default());
        let group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("calibrated lens frame"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer.as_entire_binding(),
                    },
                ],
            });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("calibrated lens output"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

fn uniform(
    lens: LensWarp,
    raster: LensRaster,
    target: [u32; 2],
) -> Result<[[f32; 4]; 8], RenderError> {
    let c = lens.calibration();
    let f = c.focal().map(|v| v * lens.zoom());
    let p = c.principal();
    let plate = lens.plate();
    let inverse = |point: [f64; 2]| -> Result<[f64; 2], RenderError> {
        let v = plate
            .inverse(point)
            .map_err(RenderError::from)?;
        Ok([(v[0] - p[0]) / f[0], (v[1] - p[1]) / f[1]])
    };
    let ideal = |q: [f64; 2]| -> Result<[f64; 2], RenderError> {
        plate
            .apply([q[0] * f[0] + p[0], q[1] * f[1] + p[1]])
            .map_err(RenderError::from)
    };
    let o = inverse([0., 0.])?;
    let x = inverse([1., 0.])?;
    let y = inverse([0., 1.])?;
    let a = ideal([0., 0.])?;
    let b = ideal([1., 0.])?;
    let d = ideal([0., 1.])?;
    let k = c.radial();
    let t = c.tangential();
    let [w, h] = lens.output_size();
    let words = [
        [x[0] - o[0], y[0] - o[0], o[0], 0.],
        [x[1] - o[1], y[1] - o[1], o[1], 0.],
        [b[0] - a[0], d[0] - a[0], a[0], 0.],
        [b[1] - a[1], d[1] - a[1], a[1], 0.],
        [k[0], k[1], k[2], lens.derivative_bound()],
        [
            t[0],
            t[1],
            f64::from(u8::from(
                lens.is_identity()
                    && raster.rect() == [0., 0., f64::from(w), f64::from(h)]
                    && raster.size() == target,
            )),
            lens.inverse_step(),
        ],
        raster.rect(),
        [
            f64::from(w) / f64::from(target[0]),
            f64::from(h) / f64::from(target[1]),
            0.,
            0.,
        ],
    ]
    .map(|r| r.map(|v| v as f32));
    if words.iter().flatten().any(|v| !v.is_finite()) {
        return Err(RenderError::Invalid(
            "lens parameters exceed finite shader range".into(),
        ));
    }
    Ok(words)
}
