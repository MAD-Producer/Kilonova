//! Caller-owned, cancelable temporal accumulation; no CPU pixel path in preview.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Unorm;

pub(super) struct ExposureGpu {
    layout: wgpu::BindGroupLayout,
    accumulate: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
}
impl ExposureGpu {
    fn new(device: &wgpu::Device, output: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exposure layout"),
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
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear exposure accumulation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("exposure.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exposure pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let mean = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Constant,
            dst_factor: wgpu::BlendFactor::OneMinusConstant,
            operation: wgpu::BlendOperation::Add,
        };
        let accumulate = pipeline(
            "accumulate",
            ACCUMULATION_FORMAT,
            Some(wgpu::BlendState {
                color: mean,
                alpha: mean,
            }),
        );
        let resolve = pipeline("resolve", output, None);
        Self {
            layout,
            accumulate,
            resolve,
        }
    }
}

pub(super) struct Reservation {
    pub(super) counter: Arc<AtomicU64>,
    pub(super) bytes: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

/// One reusable sRGB sample and one UNORM16 linear weighted mean: twelve bytes per pixel.
/// Drop cancels the exposure; no partial result can be resolved. Queued work may
/// outlive ownership, as represented by the renderer's submission ledger.
pub struct ExposureTarget {
    sample: RenderTarget,
    accumulation: OwnedTexture,
    view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    sample_group: wgpu::BindGroup,
    resolve_group: wgpu::BindGroup,
    weights: Vec<f64>,
    total_weight: f64,
    phase: usize,
    poisoned: bool,
    reservation: Reservation,
}
impl ExposureTarget {
    pub fn size(&self) -> [u32; 2] {
        [self.sample.width, self.sample.height]
    }
    pub fn phase_index(&self) -> usize {
        self.phase
    }
    pub fn phase_count(&self) -> usize {
        self.weights.len()
    }
    pub fn complete(&self) -> bool {
        !self.poisoned && self.phase == self.weights.len()
    }
    pub fn texture_bytes(&self) -> u64 {
        self.reservation.bytes
    }
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        ledger.retained(
            &self.sample.texture,
            crate::TextureCategory::ExposureTargets,
            "exposure/sample".into(),
        );
        ledger.retained(
            &self.accumulation,
            crate::TextureCategory::ExposureTargets,
            "exposure/linear-mean".into(),
        );
        ledger
    }
}

impl Renderer {
    pub(super) fn reserve_exposure_bytes(&self, bytes: u64, context: &str) -> Result<Reservation> {
        let budget = self.memory_policy().scratch_budget_bytes;
        if let Err(active) = self.exposure_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed,
            |active| active.checked_add(bytes).filter(|required| *required <= budget)) {
            return Err(RenderError::budget(ResourceBudgetScope::ExposureTargets, active.saturating_add(bytes), budget,
                format!("{context} live={active} additional={bytes} exceeds exposure target budget")));
        }
        Ok(Reservation { counter: self.exposure_bytes.clone(), bytes })
    }
    pub(super) fn check_exposure_media(&self, scene: &Scene, context: &str) -> Result<()> {
        for layer in &scene.layers {
            if let Some(video) = &layer.video {
                let pending = self.video_gpu.as_ref().is_some_and(|gpu| gpu.pending(layer.id));
                let completed = self.images.contains_key(&TextureKey::Video(layer.id))
                    && self.video_gpu.as_ref().is_none_or(|gpu| gpu.ready(layer.id));
                if pending || !completed {
                    return Err(RenderError::Invalid(format!(
                        "{context} object={} source={} time_us={} phase=video_ready: {}",
                        layer.id, video.asset, video.source_time_us,
                        if pending { "unsubmitted conversion in another encoder" } else { "completed video source missing or canceled" })));
                }
            }
        }
        for child in &scene.nested { self.check_exposure_media(&child.scene, context)?; }
        Ok(())
    }
    /// Live caller-owned exposure capacity, separate from source and effect
    /// scratch budgets. Includes every active target and retained local raster
    /// created by this renderer. This is logical capacity, not total GPU VRAM.
    pub fn exposure_texture_bytes(&self) -> u64 {
        self.exposure_bytes.load(Ordering::Relaxed)
    }

    pub fn exposure_target(
        &mut self,
        width: u32,
        height: u32,
        weights: &[f64],
    ) -> Result<ExposureTarget> {
        self.check_health()?;
        let invalid = |cause| {
            RenderError::Invalid(format!("exposure {width}x{height} phase=allocate: {cause}"))
        };
        if weights.is_empty()
            || weights.len() > motion_model::render_sampling::MAX_EXPOSURE_PHASES
            || weights
                .iter()
                .any(|w| !w.is_finite() || *w <= 0. || *w > 1. || *w as f32 <= 0.)
            || (weights.iter().sum::<f64>() - 1.).abs() > 1e-7
        {
            return Err(invalid("invalid complete exposure weights"));
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 {
            return Err(invalid("empty extent"));
        }
        if width > limit || height > limit {
            return Err(RenderError::Resource {
                rejection: ResourceRejection::TextureDimensions {
                    width,
                    height,
                    max_dimension: limit,
                },
                message: format!(
                    "exposure {width}x{height} phase=allocate exceeds device dimensions"
                ),
            });
        }
        if u64::from(width) * u64::from(height) > CAPTURE_PIXEL_LIMIT {
            return Err(invalid("extent exceeds offscreen pixel limit"));
        }
        if !matches!(
            self.target_format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(invalid("requires an sRGB render target"));
        }
        if !self.device.features().contains(
            wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
                | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        ) {
            return Err(invalid("adapter lacks 16-bit normalized exposure accumulation"));
        }
        let features = self
            .adapter
            .get_texture_format_features(ACCUMULATION_FORMAT);
        if !features
            .allowed_usages
            .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        {
            return Err(invalid("adapter lacks linear 16-bit normalized blending"));
        }
        let bytes = u64::from(width) * u64::from(height) * 12;
        let reservation = self.reserve_exposure_bytes(bytes, &format!("exposure {width}x{height} phase=allocate"))?;
        let sample = self.render_target(width, height)?;
        let accumulation = self.device.create_owned_texture(&wgpu::TextureDescriptor {
            label: Some("linear normalized exposure mean"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = accumulation.create_view(&Default::default());
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure phase weight"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let gpu = self
            .exposure_gpu
            .get_or_insert_with(|| ExposureGpu::new(&self.device, self.target_format));
        let group = |view: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("exposure source"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        let sample_group = group(&sample.view);
        let resolve_group = group(&view);
        self.check_health()?;
        Ok(ExposureTarget {
            sample,
            accumulation,
            view,
            uniform,
            sample_group,
            resolve_group,
            weights: weights.to_vec(),
            total_weight: 0.,
            phase: 0,
            poisoned: false,
            reservation,
        })
    }

    pub(super) fn check_exposure(&self, target: &ExposureTarget, operation: &str) -> Result<()> {
        self.check_health()?;
        let context = || {
            format!(
                "exposure {}x{} phase={operation} sample={}/{}",
                target.sample.width,
                target.sample.height,
                target.phase,
                target.weights.len()
            )
        };
        if !Arc::ptr_eq(&target.reservation.counter, &self.exposure_bytes) {
            return Err(RenderError::Invalid(format!(
                "{}: belongs to another renderer",
                context()
            )));
        }
        if target.poisoned {
            return Err(RenderError::Invalid(format!(
                "{}: canceled after failed submission",
                context()
            )));
        }
        let active = self.exposure_texture_bytes();
        let budget = self.memory_policy().scratch_budget_bytes;
        if active > budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::ExposureTargets,
                active,
                budget,
                format!("{}: live exposure targets exceed current budget", context()),
            ));
        }
        Ok(())
    }

    /// Advance exactly one phase after all media for that scene is ready. None
    /// contributes transparent weight, including at composition boundaries. A
    /// pending decoder must not call this method or advance the phase index.
    pub fn accumulate_exposure(
        &mut self,
        target: &mut ExposureTarget,
        scene: Option<&Scene>,
        preview: bool,
    ) -> Result<Option<RenderStats>> {
        self.check_exposure(target, "accumulate")?;
        let Some(_) = target.weights.get(target.phase).copied() else {
            return Err(RenderError::Invalid(format!(
                "exposure phase=accumulate sample={}: already complete",
                target.phase
            )));
        };
        // This method owns its encoder. A conversion encoded elsewhere cannot
        // be completed by our submission, even if a texture/stamp already exists.
        if let Some(scene) = scene {
            self.check_exposure_media(scene, &format!("exposure sample={}", target.phase))?;
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("exposure phase"),
            });
        let stats = if let Some(scene) = scene {
            Some(
                self.encode_internal(
                    scene,
                    &target.sample.view,
                    target.sample.width,
                    target.sample.height,
                    &mut encoder,
                    None,
                    preview,
                )
                .map_err(|error| {
                    let message = format!(
                        "exposure sample={}/{} phase=render: {error}",
                        target.phase,
                        target.weights.len()
                    );
                    error.with_diagnostic(message)
                })?,
            )
        } else {
            None
        };
        self.accumulate_exposure_encoded(target, encoder, stats, scene.is_some())
    }

    /// Accumulate a phase assembled from independently timed local images.
    /// The phase compositor and sum are ordered GPU submissions; rejection
    /// leaves the phase index unchanged and exposes no partial resolved frame.
    pub fn accumulate_layer_exposure(&mut self, target: &mut ExposureTarget, context: &Scene,
        rasters: &[&super::LayerRaster]) -> Result<RenderStats> {
        self.accumulate_exposure_phase_inner(target, context, rasters, None)
    }
    /// Apply center-time adjustments to each independently timed phase before
    /// adding its weight. Exact center-time dependency media must be prepared.
    pub fn accumulate_exposure_phase(&mut self, target: &mut ExposureTarget, context: &Scene,
        rasters: &[&super::LayerRaster], preview: bool) -> Result<RenderStats> {
        self.accumulate_exposure_phase_inner(target, context, rasters, Some(preview))
    }
    fn accumulate_exposure_phase_inner(&mut self, target: &mut ExposureTarget, context: &Scene,
        rasters: &[&super::LayerRaster], adjustments: Option<bool>) -> Result<RenderStats> {
        self.check_exposure(target, "local_accumulate")?;
        if target.complete() {
            return Err(RenderError::Invalid("exposure phase=local_accumulate: already complete".into()));
        }
        let stats = self.draw_exposure_phase_tracked(context, rasters, &target.sample.view,
            target.sample.width, target.sample.height, adjustments, |ledger| ledger.merge(target.texture_resource_ledger()))?;
        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained phase exposure sum"),
        });
        self.accumulate_exposure_encoded(target, encoder, Some(stats), true)?
            .ok_or_else(|| RenderError::Invalid("exposure phase=local_accumulate: missing phase statistics".into()))
    }

    fn accumulate_exposure_encoded(&mut self, target: &mut ExposureTarget, mut encoder: wgpu::CommandEncoder,
        mut stats: Option<RenderStats>, has_sample: bool) -> Result<Option<RenderStats>> {
        let weight = target.weights[target.phase];
        let total_weight = target.total_weight + weight;
        // A normalized running mean is algebraically the same weighted linear
        // exposure, but avoids repeatedly adding small values to an FP16 sum.
        // In particular, an opaque constant must remain opaque at 80 phases.
        let fraction = weight / total_weight;
        {
            self.queue.write_buffer(
                &target.uniform,
                0,
                bytemuck::cast_slice(&[if has_sample { 1.0f32 } else { 0.0 }, 0., 0., 0.]),
            );
            let started = Instant::now();
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("linear exposure accumulation"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if target.phase == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                {
                    pass.set_pipeline(
                        &self
                            .exposure_gpu
                            .as_ref()
                            .expect("exposure pipeline created with target")
                            .accumulate,
                    );
                    pass.set_bind_group(0, &target.sample_group, &[]);
                    pass.set_blend_constant(wgpu::Color {r:fraction,g:fraction,b:fraction,a:fraction});
                    pass.draw(0..3, 0..1);
                }
            }
            self.submit_tracked(encoder.finish(), |ledger| {
                ledger.merge(target.texture_resource_ledger())
            });
            if let Err(error) = self.check_health() {
                target.poisoned = true;
                let message = format!(
                    "exposure sample={}/{} phase=submit: {error}",
                    target.phase,
                    target.weights.len()
                );
                return Err(error.with_diagnostic(message));
            }
            if let Some(stats) = &mut stats {
                stats.cpu_prepare_us += started.elapsed().as_micros() as u64;
            }
        }
        target.total_weight = total_weight;
        target.phase += 1;
        Ok(stats)
    }

    pub(super) fn resolve_exposure_view(
        &self,
        target: &ExposureTarget,
        view: &wgpu::TextureView,
        output_resources: impl FnOnce(&mut crate::TextureResourceLedger),
    ) -> Result<()> {
        self.check_exposure(target, "resolve")?;
        if !target.complete() {
            return Err(RenderError::Invalid(format!(
                "exposure phase=resolve sample={}/{}: incomplete",
                target.phase,
                target.weights.len()
            )));
        }
        let output = view.texture();
        if output.width() != target.sample.width
            || output.height() != target.sample.height
            || output.format() != self.target_format
        {
            return Err(RenderError::Invalid(
                "exposure phase=resolve: output extent or format mismatch".into(),
            ));
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("complete exposure resolve"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("complete exposure resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            pass.set_pipeline(
                &self
                    .exposure_gpu
                    .as_ref()
                    .expect("exposure pipeline created with target")
                    .resolve,
            );
            pass.set_bind_group(0, &target.resolve_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.submit_tracked(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger());
            output_resources(ledger);
        });
        self.check_health()
    }

    pub fn resolve_exposure(&self, target: &ExposureTarget, output: &RenderTarget) -> Result<()> {
        self.resolve_exposure_view(target, &output.view, |ledger| {
            output.collect_texture_resources(
                ledger,
                "exposure/output",
                crate::TextureState::Retained,
            )
        })
    }
    /// Explicit readback for tests and frozen output; not called during preview.
    pub fn capture_exposure(&self, target: &ExposureTarget) -> Result<Vec<u8>> {
        self.check_exposure(target, "capture")?;
        if !target.complete() {
            return Err(RenderError::Invalid(
                "exposure phase=capture: incomplete".into(),
            ));
        }
        let output = self.capture_target(target.sample.width, target.sample.height)?;
        self.resolve_exposure_view(target, &output.view, |ledger| {
            ledger.merge(output.texture_resource_ledger())
        })?;
        self.read_target(&output)
    }
}
