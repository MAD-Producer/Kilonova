use crate::resource_ledger::{DeviceTextureExt, OwnedTexture};
use crate::{
    effect_plan::{EffectFramePlan, PlanBuilder, ScratchAllocation},
    renderer::GpuImage,
    RenderError,
};
use crate::Scene;
use motion_effects::Registry;
use std::collections::HashMap;

pub(crate) struct FxTexture {
    pub composite: wgpu::BindGroup,
    pub view: wgpu::TextureView,
    pub texture: OwnedTexture,
}
struct Buffer {
    group: wgpu::BindGroup,
    buffer: crate::allocation_buffer::AllocationBuffer,
}
struct Binding {
    key: (
        i32,
        i32,
        i32,
        u32,
        u64,
        Option<crate::renderer::TextureKey>,
        usize,
    ),
    group: wgpu::BindGroup,
    _allocations: [Option<crate::allocation_budget::AllocationLease>; 3],
}
struct InputResource {
    program: u32,
    pointer: usize,
    group: wgpu::BindGroup,
    _allocation: Option<crate::allocation_budget::AllocationLease>,
}
struct InstanceResource {
    program: u32,
    values: std::sync::Arc<motion_effects::shader_data::ShaderDataValues>,
    // Bindings retire before the admitted backing texture.
    group: wgpu::BindGroup,
    texture: FxTexture,
}
struct Lut {
    view: wgpu::TextureView,
    texture: OwnedTexture,
    last: [u8; 1024],
    source: Option<motion_model::SampledColorCurve>,
}
pub(crate) struct EffectGpu {
    pub builder: PlanBuilder,
    pub state: GpuState,
}
pub(crate) struct GpuState {
    allocation_budget: crate::AllocationBudget,
    format_capabilities: std::sync::Arc<crate::image_formats::ImageCapabilities>,
    uniform_layout: wgpu::BindGroupLayout,
    input_layout: wgpu::BindGroupLayout,
    resource_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    curve_sampler: wgpu::Sampler,
    // Cached raw bindings must retire before their admitted backing resources.
    resources: HashMap<u32, wgpu::BindGroup>,
    bindings: Vec<Option<Binding>>,
    input_resources: Vec<Option<InputResource>>,
    instance_resources: Vec<Option<InstanceResource>>,
    white: FxTexture,
    identity: Lut,
    luts: Vec<Lut>,
    pipelines: HashMap<(u32, u32), wgpu::RenderPipeline>,
    resource_textures: HashMap<(String, String), FxTexture>,
    pub resource_bytes: u64,
    pub parameter_resource_upload_bytes: u64,
    pub pipeline_builds: u64,
    pub pipeline_build_nanos: u64,
    pub failed_program: Option<(u32, String)>,
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    pub invalid_pointwise_source_once: bool,
    pub pool: Vec<Option<FxTexture>>,
    pool_width: u32,
    pool_height: u32,
    pool_allocations: Vec<ScratchAllocation>,
    scratch_idle_frames: u32,
    epoch: u64,
    buffers: Vec<Buffer>,
    sprite_buffer: Option<crate::allocation_buffer::AllocationBuffer>,
    allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
}

const SCRATCH_IDLE_FRAME_LIMIT: u32 = 120;
#[cfg(test)]
mod fixed_admission_tests;
mod curve_lut;
mod instance_data;
pub(crate) fn admitted_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    budget: &crate::AllocationBudget,
    category: crate::TextureCategory,
) -> Result<FxTexture, RenderError> {
    let texture = device.create_admitted_texture(&texture_descriptor(w, h, format), budget, category)?;
    Ok(bind_texture(device, layout, sampler, texture))
}
fn texture_descriptor(w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("motion-studio effect texture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}
fn bind_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: OwnedTexture,
) -> FxTexture {
    let view = texture.create_view(&Default::default());
    let composite = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("effect composite input"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    FxTexture {
        texture,
        view,
        composite,
    }
}
fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, w: u32, h: u32, bytes: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}
fn bindings(device: &wgpu::Device, name: &str, texture_count: u32) -> wgpu::BindGroupLayout {
    bindings_with_numeric_lut(device, name, texture_count, false)
}
fn bindings_with_numeric_lut(device: &wgpu::Device, name: &str, texture_count: u32, numeric_lut: bool) -> wgpu::BindGroupLayout {
    let mut entries = Vec::new();
    for i in 0..texture_count {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: i * 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: !(numeric_lut && i == 2) },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: i * 2 + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(if numeric_lut && i == 2 {
                wgpu::SamplerBindingType::NonFiltering
            } else { wgpu::SamplerBindingType::Filtering }),
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(name),
        entries: &entries,
    })
}
impl EffectGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composite: &wgpu::BindGroupLayout,
        allocation_budget: crate::AllocationBudget,
        allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
        format_capabilities: std::sync::Arc<crate::image_formats::ImageCapabilities>,
    ) -> Result<Self, RenderError> {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effect SDK uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        motion_effects::shader::UNIFORM_BYTES as u64,
                    ),
                },
                count: None,
            }],
        });
        let input_layout = bindings_with_numeric_lut(device, "effect input/source/LUT", 3, true);
        let resource_layout = bindings(device, "effect resource textures", 4);
        let curve_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("unfiltered numeric curve taps"), ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effect sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut builder = PlanBuilder::new(
            Registry::new_with_builtins().map_err(RenderError::from)?,
        )?;
        builder.set_alpha(0, 1, 1, &[255; 4])?;
        let white = admitted_texture(
            device,
            composite,
            &sampler,
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            &allocation_budget,
            crate::TextureCategory::SystemTextures,
        ).map_err(|error| {
            let message = format!("effect fallback size=1x1 phase=allocate: {error}");
            error.with_diagnostic(message)
        })?;
        let identity_texture = admitted_texture(
            device,
            composite,
            &sampler,
            256,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            &allocation_budget,
            crate::TextureCategory::CurveLuts,
        ).map_err(|error| {
            let message = format!("effect identity LUT size=256x1 phase=allocate: {error}");
            error.with_diagnostic(message)
        })?;
        let identity_bytes: [u8; 1024] = std::array::from_fn(|i| (i / 4) as u8);
        allocation_uploads.track(white.texture.allocation())?;
        allocation_uploads.track(identity_texture.texture.allocation())?;
        upload(queue, &white.texture, 1, 1, &[255; 4]);
        upload(queue, &identity_texture.texture, 256, 1, &identity_bytes);
        let identity = Lut {
            texture: identity_texture.texture,
            view: identity_texture.view,
            last: identity_bytes,
            source: None,
        };
        Ok(Self {
            builder,
            state: GpuState {
                allocation_budget,
                format_capabilities,
                allocation_uploads,
                uniform_layout,
                input_layout,
                resource_layout,
                sampler,
                curve_sampler,
                white,
                identity,
                luts: Vec::new(),
                pipelines: HashMap::new(),
                resources: HashMap::new(),
                resource_textures: HashMap::new(),
                resource_bytes: 0,
                parameter_resource_upload_bytes: 0,
                pipeline_builds: 0,
                pipeline_build_nanos: 0,
                failed_program: None,
                #[cfg(all(debug_assertions, feature = "gpu-tests"))]
                invalid_pointwise_source_once: false,
                pool: (0..8).map(|_| None).collect(),
                pool_width: 0,
                pool_height: 0,
                pool_allocations: Vec::new(),
                scratch_idle_frames: 0,
                epoch: 0,
                buffers: Vec::new(),
                sprite_buffer: None,
                bindings: Vec::new(),
                input_resources: Vec::new(),
                instance_resources: Vec::new(),
            },
        })
    }
    pub fn set_registry(&mut self, r: Registry) {
        if crate::effect_plan_cache::same_registry_packages(&self.builder.registry, &r) {
            // Keep preflighted pipelines, resources, program IDs and preview plan.
            // Refresh non-rendering diagnostics without pretending packages changed.
            self.builder.registry = r;
            return;
        }
        self.builder.set_registry(r);
        self.state.pipelines.clear();
        self.state.resources.clear();
        self.state.resource_textures.clear();
        self.state.resource_bytes = 0;
        self.state.invalidate();
    }
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
}
impl GpuState {
    fn upload_instances(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        sprites: &[crate::scene_generator::Sprite]) -> Result<(), RenderError> {
        if sprites.len() > motion_effects::MAX_SPRITES {
            return Err(RenderError::Invalid(format!("effect instances count={} phase=upload exceeds capacity", sprites.len())));
        }
        if sprites.is_empty() { return Ok(()); }
        let bytes = std::mem::size_of_val(sprites) as u64;
        if self.sprite_buffer.as_ref().is_none_or(|buffer| buffer.size() < bytes) {
            let next = crate::allocation_buffer::AllocationBuffer::new(device, &self.allocation_budget, &wgpu::BufferDescriptor {
                label: Some("bounded scene instances"), size: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }).map_err(|error| {
                let message = format!("effect instances count={} phase=allocate: {error}", sprites.len());
                error.with_diagnostic(message)
            })?;
            self.allocation_uploads.track(Some(next.allocation()))?;
            self.sprite_buffer = Some(next);
        }
        let buffer = self.sprite_buffer.as_ref().ok_or_else(||
            RenderError::Invalid("effect instance buffer phase=upload is missing".into()))?;
        self.allocation_uploads.track(Some(buffer.allocation()))?;
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(sprites));
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.epoch += 1;
        self.bindings.clear();
        self.input_resources.clear();
        self.instance_resources.clear();
    }
    pub fn release_scratch(&mut self) {
        self.sprite_buffer = None;
        let has_resources = !self.pool_allocations.is_empty()
            || !self.instance_resources.is_empty()
            || self.pool.iter().any(Option::is_some)
            || !self.luts.is_empty()
            || !self.buffers.is_empty();
        if has_resources {
            self.pool.iter_mut().for_each(|v| *v = None);
            self.pool_width = 0;
            self.pool_height = 0;
            self.pool_allocations.clear();
            self.scratch_idle_frames = 0;
            self.luts.clear();
            self.buffers.clear();
            self.invalidate();
        }
    }
    pub(crate) fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        use crate::TextureCategory::*;
        ledger.retained(&self.white.texture, SystemTextures, "effects/white".into());
        ledger.retained(&self.identity.texture, SystemTextures, "effects/identity".into());
        for (i, lut) in self.luts.iter().enumerate() {
            ledger.retained(&lut.texture, CurveLuts, format!("effects/lut/{i}"));
        }
        for ((package, resource), texture) in &self.resource_textures {
            ledger.retained(&texture.texture, PluginResources, format!("effects/resource/{package}/{resource}"));
        }
        for (index, resource) in self.instance_resources.iter().enumerate() {
            if let Some(resource) = resource {
                ledger.retained(&resource.texture.texture, PluginResources, format!("effects/instance-data/{index}"));
            }
        }
        for (i, texture) in self.pool.iter().enumerate() {
            if let Some(texture) = texture {
                ledger.retained(&texture.texture, EffectScratch, format!("effects/pool/{i}"));
            }
        }
    }
    pub fn bytes(&self) -> u64 {
        self.scratch_bytes()
            + self.resource_bytes()
            + self.luts.len() as u64 * 1024
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.pool_allocations.iter().map(|allocation| allocation.bytes()).sum()
    }
    pub fn output(&self, frame: &EffectFramePlan, draw: usize) -> Result<&FxTexture, RenderError> {
        let id = frame.scratch_output(draw)?;
        self.pool.get(id).and_then(Option::as_ref)
            .ok_or_else(|| RenderError::Invalid(format!("effect draw {draw} output allocation {id} is unavailable")))
    }
    pub fn curve_lut_bytes(&self) -> u64 {
        self.luts.iter().fold(0, |bytes, lut|
            bytes.saturating_add(crate::resource_ledger::logical_bytes(&lut.texture).unwrap_or(u64::MAX)))
    }
    /// Permanent white fallback and identity curve, omitted by the legacy sum.
    pub fn system_texture_bytes(&self) -> u64 { 4 + 1024 }
    /// Reserve validated package PNGs before uploading, so idle preview images
    /// cannot turn an otherwise valid active effect into a budget failure.
    pub fn pending_resource_bytes(&self, builder: &PlanBuilder) -> u64 {
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0;
        for pass in &builder.frame.passes {
            let program = &builder.programs[pass.program as usize];
            if self.resources.contains_key(&pass.program) { continue; }
            if let Some(package) = &program.package {
                for path in &program.resources {
                    let key = (package.hash.clone(), path.clone());
                    if self.resource_textures.contains_key(&key) || !seen.insert(key) { continue; }
                    if let Some(data) = package.files.get(path) {
                        if let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format() {
                            if let Ok((w, h)) = reader.into_dimensions() { bytes += u64::from(w) * u64::from(h) * 4; }
                        }
                    }
                }
            }
        }
        bytes + self.pending_instance_bytes(&builder.frame)
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composite: &wgpu::BindGroupLayout,
        builder: &PlanBuilder,
        scene: &Scene,
        asset_bytes: u64,
        source_budget: u64,
    ) -> Result<(), RenderError> {
        self.failed_program = None;
        let frame = &builder.frame;
        if frame.passes.is_empty() {
            self.scratch_idle_frames = self.scratch_idle_frames.saturating_add(1);
            if self.scratch_idle_frames >= SCRATCH_IDLE_FRAME_LIMIT {
                self.release_scratch();
            } else {
                // Empty effect plans are common while switching layers. Keep the
                // validated pool warm for a short idle window instead of forcing
                // the next effect frame to recreate every texture and buffer.
                self.invalidate();
            }
            return Ok(());
        }
        self.scratch_idle_frames = 0;
        if frame.width > device.limits().max_texture_dimension_2d
            || frame.height > device.limits().max_texture_dimension_2d
        {
            return Err(RenderError::Resource {
                rejection: crate::ResourceRejection::TextureDimensions { width: frame.width, height: frame.height, max_dimension: device.limits().max_texture_dimension_2d },
                message: "effect texture exceeds device dimensions".into(),
            });
        }
        if self.pool_allocations != frame.scratch_allocations || self.pool.len() != frame.scratch_allocations.len() {
            // Validate every replacement before admitting any allocation. An
            // unsupported format must preserve the previous pool and bindings,
            // just like a budget refusal. Capabilities are cached per device.
            for (index, allocation) in frame.scratch_allocations.iter().enumerate() {
                self.format_capabilities.require(
                    allocation.class.format.storage(),
                    motion_effects::image_contract::ImageUsage::EFFECT_TARGET,
                ).map_err(|cause| {
                    let message = format!(
                        "effect scratch slot={index} extent={}x{} phase=format-capabilities: {cause}",
                        allocation.width, allocation.height,
                    );
                    RenderError::from(cause).with_diagnostic(message)
                })?;
            }
            // Allocate replacements before moving any live slot. A refusal in a
            // later slot must leave the previous pool and bindings intact.
            let mut next_pool = Vec::with_capacity(frame.scratch_allocations.len());
            for (i, allocation) in frame.scratch_allocations.iter().enumerate() {
                let reusable = self.pool_allocations.get(i).is_some_and(|old| old.same_storage(*allocation))
                    && self.pool.get(i).is_some_and(|slot| slot.is_some());
                if reusable {
                    next_pool.push(None);
                } else {
                    next_pool.push(Some(admitted_texture(
                        device,
                        composite,
                        &self.sampler,
                        allocation.width,
                        allocation.height,
                        allocation.class.format.native(),
                        &self.allocation_budget,
                        crate::TextureCategory::EffectScratch,
                    ).map_err(|error| {
                        let message = format!("effect scratch slot={i} extent={}x{} phase=allocate: {error}",
                            allocation.width, allocation.height);
                        error.with_diagnostic(message)
                    })?));
                }
            }
            self.invalidate();
            for (i, slot) in next_pool.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = self.pool[i].take();
                }
            }
            self.pool = next_pool;
            self.pool_allocations.clone_from(&frame.scratch_allocations);
        }
        // Width and height are frame metadata; scratch texture dimensions come
        // from physical descriptors. Do not rebuild for a one-pixel frame
        // change when every slot can still be reused.
        self.pool_width = frame.width;
        self.pool_height = frame.height;
        self.prepare_curves(device, queue, composite, frame.precision, scene)?;
        while self.buffers.len() < frame.passes.len() {
            let index = self.buffers.len();
            let buffer = crate::allocation_buffer::AllocationBuffer::new(device, &self.allocation_budget, &wgpu::BufferDescriptor {
                label: Some("reusable effect parameters"),
                size: motion_effects::shader::UNIFORM_BYTES as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }).map_err(|error| {
                let message = format!("effect parameters pass={index} phase=allocate: {error}");
                error.with_diagnostic(message)
            })?;
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.buffers.push(Buffer { buffer, group });
        }
        self.bindings.resize_with(frame.passes.len(), || None);
        self.input_resources.resize_with(frame.passes.len(), || None);
        self.instance_resources.resize_with(frame.passes.len(), || None);
        for (index, pass) in frame.passes.iter().enumerate() {
            if pass.instance_data == 0 { self.instance_resources[index] = None; }
        }
        if !frame.sprites.is_empty() {
            self.upload_instances(device, queue, &frame.sprites)?;
        }
        for (index, p) in frame.passes.iter().enumerate() {
            let binding = frame.scratch_bindings.get(index).ok_or("missing effect scratch binding")?;
            let target = frame.scratch_allocations.get(binding.output as usize)
                .ok_or("missing effect scratch descriptor")?.class.format;
            let target_kind = target as u32;
            let key = (p.program, target_kind);
            if !self.pipelines.contains_key(&key) {
                let pipeline_began = std::time::Instant::now();
                let program = &builder.programs[p.program as usize];
                let pointwise = program.key.starts_with("pointwise-v");
                let group = builder.pointwise_fusion_report().groups.iter().find(|g| g.program == p.program);
                let observation = pointwise.then(|| crate::pointwise_observation::CompilePhase::start(
                    "pointwise_native_pipeline", &program.key, group.map(|g| g.object),
                    group.into_iter().flat_map(|g| g.effects.iter().copied()), Some(p.program), Some(target_kind)));
                let source = program.shader.wgsl.clone();
                #[cfg(all(debug_assertions, feature = "gpu-tests"))]
                let source = if pointwise && std::mem::take(&mut self.invalid_pointwise_source_once) {
                    source + "\nfn pointwise_diagnostic_invalid( {"
                } else { source };
                let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("effect WGSL"),
                    source: wgpu::ShaderSource::Wgsl(
                        source.into(),
                    ),
                });
                let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&self.uniform_layout), Some(&self.input_layout), Some(&self.resource_layout)],
                    immediate_size: 0,
                });
                let sprite = builder.programs[p.program as usize].shader.sprite;
                let additive = builder.programs[p.program as usize].shader.additive;
                const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
                    wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4];
                let vertex_layout = Some(wgpu::VertexBufferLayout {
                    array_stride: 48,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ATTRIBUTES,
                });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("effect fullscreen pass"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some("sdk_vertex"),
                        compilation_options: Default::default(),
                        buffers: if sprite {
                            std::slice::from_ref(&vertex_layout)
                        } else {
                            &[]
                        },
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &module,
                        entry_point: Some("sdk_fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target.native(),
                            blend: if sprite {
                                Some(if additive {
                                    wgpu::BlendState {
                                        color: wgpu::BlendComponent {
                                            src_factor: wgpu::BlendFactor::One,
                                            dst_factor: wgpu::BlendFactor::One,
                                            operation: wgpu::BlendOperation::Add,
                                        },
                                        alpha: wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING.alpha,
                                    }
                                } else {
                                    wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
                                })
                            } else {
                                None
                            },
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let validation = error_scope.pop();
                let polled = device.poll(wgpu::PollType::wait_indefinitely());
                if let Some(e) = pollster::block_on(validation) {
                    let message = format!(
                        "effect program {}: {e}",
                        builder.programs[p.program as usize].key
                    );
                    let message = match &polled {
                        Err(error) => format!("{message}; GPU wait also failed: {error}"),
                        Ok(_) => message,
                    };
                    self.failed_program = Some((p.program, message.clone()));
                    if let Some(observation) = observation { observation.failed(&message); }
                    return Err(RenderError::Invalid(message));
                }
                if let Err(error) = polled {
                    let error = RenderError::from(error);
                    if let Some(observation) = observation { observation.failed(&error.to_string()); }
                    return Err(error);
                }
                if let Some(observation) = observation { observation.done(program.shader.wgsl.len()); }
                self.pipelines.insert(key, pipeline);
                self.pipeline_builds += 1;
                self.pipeline_build_nanos = self.pipeline_build_nanos.saturating_add(
                    pipeline_began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
            }
            if !self.resources.contains_key(&p.program) {
                let program = &builder.programs[p.program as usize];
                if let Some(package) = &program.package {
                    for path in &program.resources {
                        let key = (package.hash.clone(), path.clone());
                        if !self.resource_textures.contains_key(&key) {
                            let rgba = crate::image_resources::decode_png_with_budget(
                                &package.files[path], builder.memory_policy().decoded_image_budget_bytes,
                            ).map_err(|error| {
                                let error = RenderError::from(error);
                                let message = format!("effect resource package={} path={} phase=decode: {error}",
                                    package.hash, path);
                                error.with_diagnostic(message)
                            })?;
                            let bytes = u64::from(rgba.width()) * u64::from(rgba.height()) * 4;
                            if rgba.width() > device.limits().max_texture_dimension_2d
                                || rgba.height() > device.limits().max_texture_dimension_2d
                            {
                                let message = format!(
                                    "effect resource {path} exceeds device texture dimensions"
                                );
                                return Err(RenderError::Resource {
                                    rejection: crate::ResourceRejection::TextureDimensions { width: rgba.width(), height: rgba.height(), max_dimension: device.limits().max_texture_dimension_2d },
                                    message,
                                });
                            }
                            if asset_bytes + self.resource_bytes() + bytes > source_budget {
                                return Err(RenderError::budget(crate::ResourceBudgetScope::SourceTextures,
                                    asset_bytes + self.resource_bytes() + bytes, source_budget,
                                    "images and effect resources exceed configured source texture budget"));
                            }
                            let t = admitted_texture(
                                device,
                                composite,
                                &self.sampler,
                                rgba.width(),
                                rgba.height(),
                                wgpu::TextureFormat::Rgba8Unorm,
                                &self.allocation_budget,
                                crate::TextureCategory::PluginResources,
                            ).map_err(|error| {
                                let message = format!("effect resource package={} path={} phase=allocate: {error}",
                                    package.hash, path.chars().take(192).collect::<String>());
                                error.with_diagnostic(message)
                            })?;
                            self.allocation_uploads.track(t.texture.allocation())?;
                            upload(
                                queue,
                                &t.texture,
                                rgba.width(),
                                rgba.height(),
                                rgba.as_raw(),
                            );
                            self.resource_textures.insert(key, t);
                            self.resource_bytes += bytes;
                        }
                    }
                }
                let mut entries = Vec::new();
                for i in 0..4 {
                    let view = if let (Some(package), Some(path)) =
                        (&program.package, program.resources.get(i))
                    {
                        &self.resource_textures[&(package.hash.clone(), path.clone())].view
                    } else {
                        &self.white.view
                    };
                    entries.push(wgpu::BindGroupEntry {
                        binding: i as u32 * 2,
                        resource: wgpu::BindingResource::TextureView(view),
                    });
                    entries.push(wgpu::BindGroupEntry {
                        binding: i as u32 * 2 + 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    });
                }
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.resource_layout,
                    entries: &entries,
                });
                self.resources.insert(p.program, group);
            }
            self.prepare_instance_data(index, builder, device, queue, composite, asset_bytes, source_budget)?;
        }
        Ok(())
    }
    pub fn encode_pass(
        &mut self,
        index: usize,
        frame: &EffectFramePlan,
        assets: &[u64],
        source_key: Option<crate::renderer::TextureKey>,
        external: Option<&wgpu::TextureView>,
        images: &HashMap<crate::renderer::TextureKey, GpuImage>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), RenderError> {
        let p = frame
            .passes
            .get(index)
            .ok_or_else(|| RenderError::Invalid(format!("effect pass {index} is out of range")))?;
        let source_key = if p.sprite {None} else {source_key};
        if p.instance_data != 0 {
            let resource = self.instance_resources.get(index).and_then(Option::as_ref)
                .ok_or("instance shader data binding is missing")?;
            // A later composition may replace this cache before submission.
            // Keep every encoded use admitted until its GPU work completes.
            self.allocation_uploads.track(resource.texture.texture.allocation())?;
        }
        let physical = frame.scratch_bindings.get(index)
            .ok_or_else(|| RenderError::Invalid(format!("effect pass {index} scratch binding is missing")))?;
        let external = if p.sprite {None} else {external};
        let buffer = self
            .buffers
            .get(index)
            .ok_or_else(|| RenderError::Invalid(format!("effect buffer {index} is missing")))?;
        self.allocation_uploads.track(Some(buffer.buffer.allocation()))?;
        queue.write_buffer(
            &buffer.buffer,
            0,
            bytemuck::bytes_of(&p.uniform),
        );
        let external_id = external.map_or(0, |v| v as *const _ as usize);
        let key = (
            physical.input,
            physical.source,
            p.lut,
            p.program,
            self.epoch,
            source_key,
            external_id,
        );
        if self
            .bindings
            .get(index)
            .and_then(|binding| binding.as_ref())
            .is_none_or(|v| v.key != key)
        {
            let view = |id: i32| -> Result<&wgpu::TextureView, RenderError> {
                if id <= crate::mask_plan::SOURCE_TOKEN {
                    let owner = i64::from(crate::mask_plan::SOURCE_TOKEN) - i64::from(id);
                    let owner = usize::try_from(owner).map_err(|_| {
                        RenderError::Invalid(format!("effect mask owner token {id} is invalid"))
                    })?;
                    let draw = frame.draws.get(owner).ok_or_else(|| {
                        RenderError::Invalid(format!("effect mask owner {owner} is out of range"))
                    })?;
                    return images
                        .get(&crate::renderer::TextureKey::Mask(draw.layer))
                        .map(|image| &image.view)
                        .ok_or_else(|| {
                            RenderError::Invalid(format!("effect mask texture for layer {} is missing", draw.layer))
                        });
                }
                if id < 0 {
                    let asset_index = usize::try_from(-i64::from(id) - 1).map_err(|_| {
                        RenderError::Invalid(format!("effect asset token {id} is invalid"))
                    })?;
                    if p.sprite {
                        let asset = *assets.get(asset_index).ok_or_else(|| {
                            RenderError::Invalid(format!("effect asset slot {asset_index} is missing"))
                        })?;
                        return images
                            .get(&crate::renderer::TextureKey::Static(asset))
                            .map(|image| &image.view)
                            .ok_or_else(|| {
                                RenderError::Invalid(format!("effect asset {asset} texture is missing"))
                            });
                    }
                    if let Some(view) = external {
                        return Ok(view);
                    }
                    // External pass inputs refer to the owning layer's source.
                    // Videos use a live instance texture rather than the white asset.
                    let image = if let Some(source_key) = source_key {
                        source_key
                    } else {
                        let asset = *assets.get(asset_index).ok_or_else(|| {
                            RenderError::Invalid(format!("effect input asset slot {asset_index} is missing"))
                        })?;
                        crate::renderer::TextureKey::Static(asset)
                    };
                    return images
                        .get(&image)
                        .map(|image| &image.view)
                        .ok_or_else(|| RenderError::Invalid(format!("effect input texture {image:?} is missing")));
                } else {
                    let slot = usize::try_from(id).map_err(|_| {
                        RenderError::Invalid(format!("effect scratch slot {id} is invalid"))
                    })?;
                    return self
                        .pool
                        .get(slot)
                        .and_then(|texture| texture.as_ref())
                        .map(|texture| &texture.view)
                        .ok_or_else(|| {
                            RenderError::Invalid(format!("effect scratch slot {slot} is unavailable"))
                        });
                }
            };
            let lut = if p.lut >= 0 {
                let index = usize::try_from(p.lut).map_err(|_| {
                    RenderError::Invalid(format!("effect LUT index {} is invalid", p.lut))
                })?;
                &self
                    .luts
                    .get(index)
                    .ok_or_else(|| RenderError::Invalid(format!("effect LUT {index} is missing")))?
                    .view
            } else {
                &self.identity.view
            };
            let input = view(physical.input)?;
            let source = view(physical.source)?;
            let allocations = [input, source, lut].map(|view| images.values()
                .find_map(|image| image._texture.allocation_for_view(view))
                .or_else(|| self.pool.iter().flatten().find_map(|texture| texture.texture.allocation_for_view(view)))
                .or_else(|| self.luts.iter().find_map(|lut| lut.texture.allocation_for_view(view))));
            let entries = [
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(lut),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.curve_sampler),
                },
            ];
            let binding_slot = self
                .bindings
                .get_mut(index)
                .ok_or_else(|| RenderError::Invalid(format!("effect binding slot {index} is out of range")))?;
            *binding_slot = Some(Binding {
                key,
                _allocations: allocations,
                group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.input_layout,
                    entries: &entries,
                }),
            });
        }
        let target = self
            .pool
            .get(physical.output as usize)
            .and_then(|texture| texture.as_ref())
            .map(|texture| &texture.view)
            .ok_or_else(|| RenderError::Invalid(format!("effect output allocation {} is unavailable", physical.output)))?;
        if p.resource_input != 0 {
            use crate::renderer::TextureKey;
            let image = if p.resource_input < 0 {
                let asset_index = usize::try_from(-i64::from(p.resource_input) - 1).map_err(|_| {
                    RenderError::Invalid(format!("effect resource asset token {} is invalid", p.resource_input))
                })?;
                TextureKey::Static(*assets.get(asset_index).ok_or_else(|| {
                    RenderError::Invalid(format!("effect resource asset slot {asset_index} is missing"))
                })?)
            } else {
                let draw_index = usize::try_from(p.resource_input - 1).map_err(|_| {
                    RenderError::Invalid(format!("effect resource draw {} is invalid", p.resource_input))
                })?;
                let draw = frame.draws.get(draw_index).ok_or_else(|| {
                    RenderError::Invalid(format!("effect resource draw {draw_index} is missing"))
                })?;
                if p.uniform.params[30][1]>0.5 {TextureKey::EffectInput(draw.layer)}
                else if draw.words[31]==1. {TextureKey::Vector(draw.layer)}
                else if draw.words[24]<0. {TextureKey::Video(draw.layer)}
                else {
                    let asset_index = usize::try_from(draw.words[24] as i64).map_err(|_| {
                        RenderError::Invalid(format!("effect resource asset index {} is invalid", draw.words[24]))
                    })?;
                    TextureKey::Static(*assets.get(asset_index).ok_or_else(|| {
                        RenderError::Invalid(format!("effect resource asset slot {asset_index} is missing"))
                    })?)
                }
            };
            let image=images.get(&image).ok_or_else(||RenderError::Invalid("effect image input is not available".into()))?;
            let view=&image.view;
            let pointer=view as *const _ as usize;
            let input_resource = self
                .input_resources
                .get(index)
                .ok_or_else(|| RenderError::Invalid(format!("effect resource slot {index} is out of range")))?;
            if input_resource
                .as_ref()
                .is_none_or(|resource| resource.program != p.program || resource.pointer != pointer)
            {
                let mut entries=Vec::new();
                for slot in 0..4 {
                    entries.push(wgpu::BindGroupEntry {binding:slot*2,resource:wgpu::BindingResource::TextureView(if slot==0 {view}else{&self.white.view})});
                    entries.push(wgpu::BindGroupEntry {binding:slot*2+1,resource:wgpu::BindingResource::Sampler(&self.sampler)});
                }
                let input_resource = self
                    .input_resources
                    .get_mut(index)
                    .ok_or_else(|| RenderError::Invalid(format!("effect resource slot {index} is out of range")))?;
                *input_resource = Some(InputResource { program: p.program, pointer,
                    _allocation: image._texture.allocation(),
                    group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label:Some("effect image input"),layout:&self.resource_layout,entries:&entries
                }) });
            }
        }
        let pipeline_format = frame.scratch_allocations.get(physical.output as usize)
            .ok_or("effect output descriptor is missing")?.class.format as u32;
        let pipeline = self
            .pipelines
            .get(&(p.program, pipeline_format))
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "effect pipeline ({}, {}) is missing",
                    p.program, pipeline_format
                ))
            })?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("motion-studio effect"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
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
        pass.set_viewport(0.0, 0.0, p.width as f32, p.height as f32, 0.0, 1.0);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.buffers[index].group, &[]);
        let binding = self
            .bindings
            .get(index)
            .and_then(|binding| binding.as_ref())
            .ok_or_else(|| RenderError::Invalid(format!("effect binding {index} is missing")))?;
        pass.set_bind_group(1, &binding.group, &[]);
        let resources = if p.instance_data != 0 {
            &self.instance_resources.get(index).and_then(Option::as_ref)
                .ok_or("instance shader data binding is missing")?.group
        } else if p.resource_input != 0 {
            &self
                .input_resources
                .get(index)
                .and_then(|resource| resource.as_ref())
                .ok_or_else(|| RenderError::Invalid(format!("effect resource binding {index} is missing")))?
                .group
        } else {
            self.resources
                .get(&p.program)
                .ok_or_else(|| RenderError::Invalid(format!("effect resource program {} is missing", p.program)))?
        };
        pass.set_bind_group(2, resources, &[]);
        if p.sprite {
            // The pass still clears its target when every instance is culled.
            // Zero-instance work needs no vertex buffer or allocation.
            if p.sprite_count != 0 {
                let instances = self.sprite_buffer.as_ref().ok_or_else(||
                    RenderError::Invalid(format!("effect instance buffer program={} phase=draw is missing", p.program)))?;
                pass.set_vertex_buffer(0, instances.slice(..));
                pass.draw(0..6, p.sprite_start..p.sprite_start + p.sprite_count);
            }
        } else {
            pass.draw(0..3, 0..1);
        }
        Ok(())
    }
}
