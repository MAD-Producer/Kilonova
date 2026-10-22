use crate::{
    effect_gpu::FxTexture,
    mask_plan::{self, MaskRaster},
    renderer::{GpuImage, TextureKey},
    RenderError,
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod reuse_tests;

struct Uniform {
    group: wgpu::BindGroup,
    inputs: Option<InputBinding>,
    buffer: crate::allocation_buffer::AllocationBuffer,
}
struct InputBinding {
    input: wgpu::TextureView,
    source: wgpu::TextureView,
    group: wgpu::BindGroup,
    _allocations: [Option<crate::allocation_budget::AllocationLease>; 2],
}
pub(crate) struct MaskGpu {
    raster: wgpu::RenderPipeline,
    filters: Vec<wgpu::RenderPipeline>,
    uniform_layout: wgpu::BindGroupLayout,
    input_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: Vec<Uniform>,
    inputs_cached: bool,
    scratch: Vec<FxTexture>,
    scratch_size: (u32, u32),
    fingerprints: HashMap<u64, u64>,
    meshes: HashMap<(u64,u64),(u64,crate::allocation_buffer::AllocationBuffer)>,
    pub uploads: u64,
    #[cfg(test)]
    input_binding_observations: Vec<wgpu::BindGroup>,
    allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
    allocation_budget: crate::AllocationBudget,
}

fn mask_context(error: RenderError, layer: u64, mask: u64, frame: f64, phase: &str) -> RenderError {
    let message = format!("mask layer={layer} mask={mask} frame={frame} phase={phase}: {error}");
    RenderError::Context { message, cause: Box::new(error) }
}
fn texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    w: u32,
    h: u32,
    budget: &crate::AllocationBudget,
) -> Result<FxTexture, RenderError> {
    crate::effect_gpu::admitted_texture(device, layout, sampler, w, h,
        wgpu::TextureFormat::R8Unorm, budget).map_err(|error| {
            let message = format!("mask texture size={w}x{h} format=R8Unorm phase=allocate: {error}");
            error.with_diagnostic(message)
        })
}
impl MaskGpu {
    pub(crate) fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        for (i,t) in self.scratch.iter().enumerate() {
            ledger.retained(&t.texture, crate::TextureCategory::MaskScratch, format!("masks/scratch/{i}"));
        }
    }
    pub fn new(
        device: &wgpu::Device,
        image_layout: &wgpu::BindGroupLayout,
        allocation_budget: crate::AllocationBudget,
        allocation_uploads: std::sync::Arc<crate::allocation_uploads::AllocationUploads>,
    ) -> Result<Self, RenderError> {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mask coverage"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mask uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(624),
                },
                count: None,
            }],
        });
        let mut entries = vec![];
        for binding in 0..4 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: if binding % 2 == 0 {
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }
                } else {
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
                },
                count: None,
            });
        }
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mask inputs"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mask filters"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&input_layout)],
            immediate_size: 0,
        });
        let filters = mask_plan::shaders()
            .map_err(|error| {
                let message = error.to_string();
                error.with_diagnostic(message)
            })?
            .iter()
            .map(|shader| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("portable mask filter"),
                    source: wgpu::ShaderSource::Wgsl(shader.wgsl.clone().into()),
                });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("mask scalar filter"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("sdk_vertex"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("sdk_fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::R8Unorm,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mask path paint"),
            source: wgpu::ShaderSource::Wgsl(include_str!("vector.wgsl").into()),
        });
        let raster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mask path raster"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let _ = image_layout;
        Ok(Self {
            raster,
            filters,
            uniform_layout,
            input_layout,
            sampler,
            uniforms: vec![],
            inputs_cached: false,
            scratch: vec![],
            scratch_size: (0, 0),
            fingerprints: HashMap::new(),
            meshes: HashMap::new(),
            uploads: 0,
            #[cfg(test)]
            input_binding_observations: Vec::new(),
            allocation_budget,
            allocation_uploads,
        })
    }
    pub fn scratch_bytes(&self) -> u64 {
        u64::from(self.scratch_size.0) * u64::from(self.scratch_size.1) * self.scratch.len() as u64
    }
    pub fn clear(&mut self) {
        self.clear_inputs();
        self.scratch.clear();
        self.scratch_size = (0, 0);
        self.fingerprints.clear();
        self.meshes.clear();
    }
    fn clear_inputs(&mut self) {
        // Cached views refer to the current scratch pool. Release them before
        // retiring that pool, including bindings in currently unused slots.
        if self.inputs_cached {
            for slot in &mut self.uniforms {
                slot.inputs = None;
            }
            self.inputs_cached = false;
        }
    }
    fn filter(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        index: &mut usize,
        pipeline: usize,
        u: crate::effect_plan::EffectUniform,
        input: &wgpu::TextureView,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        if *index >= self.uniforms.len() {
            let buffer = crate::allocation_buffer::AllocationBuffer::new(device, &self.allocation_budget, &wgpu::BufferDescriptor {
                label: Some("mask pass uniform"),
                size: 624,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })?;
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.uniforms.push(Uniform {
                buffer,
                group,
                inputs: None,
            });
        }
        let scratch = &self.scratch;
        let slot = &mut self.uniforms[*index];
        *index += 1;
        self.allocation_uploads.track(Some(slot.buffer.allocation()))?;
        queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(&u));
        // Layout and sampler are immutable here. Match actual view identities,
        // not dimensions: equal-sized replacement textures must rebind.
        if slot
            .inputs
            .as_ref()
            .is_some_and(|cached| cached.input != *input || cached.source != *source)
        {
            slot.inputs = None;
        }
        let group = &slot
            .inputs
            .get_or_insert_with(|| InputBinding {
                input: input.clone(),
                source: source.clone(),
                _allocations: [input, source].map(|view| scratch.iter()
                    .find_map(|target| target.texture.allocation_for_view(view))),
                group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mask pass inputs"),
                    layout: &self.input_layout,
                    entries: &[
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
                    ],
                }),
            })
            .group;
        self.inputs_cached = true;
        #[cfg(test)]
        self.input_binding_observations.push(group.clone());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("mask feather/combine"),
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
        pass.set_viewport(0., 0., u.size[0], u.size[1], 0., 1.);
        pass.set_pipeline(&self.filters[pipeline]);
        pass.set_bind_group(0, &slot.group, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
    // Stage the complete scratch pool and resized outputs before replacing any
    // previous native owner, dimensions or input binding. A partial refusal
    // drops the staged reservations and leaves the previous targets usable.
    fn prepare_textures(
        &mut self,
        device: &wgpu::Device,
        image_layout: &wgpu::BindGroupLayout,
        scene: &crate::Scene,
        masks: &[MaskRaster],
        images: &mut HashMap<TextureKey, GpuImage>,
        bytes: &mut u64,
        other: u64,
        source_budget: u64,
    ) -> Result<bool, RenderError> {
        let (w, h, slots) = mask_plan::scratch_dimensions(masks);
        let scratch = if (w, h) != self.scratch_size || slots != self.scratch.len() {
            let first = masks.first().ok_or_else(|| RenderError::Invalid(
                "mask target preparation requires a nonempty mask list".into()))?;
            let owner = scene.layers[first.layer].id;
            Some((0..slots).map(|slot| {
                texture(device, image_layout, &self.sampler, w, h, &self.allocation_budget)
                    .map_err(|error| mask_context(error, owner, first.id, scene.frame,
                        &format!("scratch_allocate slot={slot}")))
            }).collect::<Result<Vec<_>, _>>()?)
        } else {
            None
        };
        let mut next = Vec::new();
        let mut next_bytes = *bytes;
        for group in masks.chunk_by(|a, b| a.layer == b.layer) {
            let first = &group[0];
            let owner = scene.layers[first.layer].id;
            let key = TextureKey::Mask(owner);
            let signature = mask_plan::group_fingerprint(group);
            if self.fingerprints.get(&owner) == Some(&signature) && images.contains_key(&key) {
                continue;
            }
            let cost = u64::from(first.width) * u64::from(first.height);
            let previous = images.get(&key).map_or(0, |image| image.bytes);
            if next_bytes - previous + cost + other > source_budget {
                return Err(RenderError::budget(crate::ResourceBudgetScope::SourceTextures,
                    next_bytes - previous + cost + other, source_budget,
                    format!("layer {owner}: source and mask textures exceed configured source texture budget")));
            }
            if images.get(&key).is_none_or(|image| image.size != (first.width, first.height)) {
                let target = texture(device, image_layout, &self.sampler, first.width,
                    first.height, &self.allocation_budget).map_err(|error|
                        mask_context(error, owner, first.id, scene.frame, "output_allocate"))?;
                next.push((key, target, cost, (first.width, first.height)));
                next_bytes = next_bytes - previous + cost;
            }
        }
        if let Some(scratch) = scratch {
            self.clear_inputs();
            self.scratch = scratch;
            self.scratch_size = (w, h);
        }
        let changed = !next.is_empty();
        for (key, target, cost, size) in next {
            images.insert(key, GpuImage {
                _texture: target.texture, view: target.view, bind_group: target.composite,
                bytes: cost, size, video_stamp: None,
            });
        }
        *bytes = next_bytes;
        Ok(changed)
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        image_layout: &wgpu::BindGroupLayout,
        scene: &crate::Scene,
        masks: &[MaskRaster],
        images: &mut HashMap<TextureKey, GpuImage>,
        bytes: &mut u64,
        other: u64,
        source_budget: u64,
    ) -> Result<bool, RenderError> {
        #[cfg(test)]
        self.input_binding_observations.clear();
        let active: HashSet<_> = masks.iter().map(|m| scene.layers[m.layer].id).collect();
        let active_meshes:HashSet<_>=masks.iter().map(|m|(scene.layers[m.layer].id,m.id)).collect();
        self.meshes.retain(|key,_|active_meshes.contains(key));
        let mut changed = false;
        let stale: Vec<_> = images
            .keys()
            .copied()
            .filter(|k| matches!(k,TextureKey::Mask(id) if !active.contains(id)))
            .collect();
        for key in stale {
            if let Some(image) = images.remove(&key) {
                *bytes = (*bytes).saturating_sub(image.bytes);
                changed = true;
            }
        }
        self.fingerprints.retain(|id, _| active.contains(id));
        if masks.is_empty() {
            self.clear();
            return Ok(changed);
        }
        changed |= self.prepare_textures(device, image_layout, scene, masks,
            images, bytes, other, source_budget)?;
        let mut uniform_index = 0;
        for group in masks.chunk_by(|a, b| a.layer == b.layer) {
            let owner = scene.layers[group[0].layer].id;
            let key = TextureKey::Mask(owner);
            let signature = mask_plan::group_fingerprint(group);
            if self.fingerprints.get(&owner) == Some(&signature) && images.contains_key(&key) {
                continue;
            }
            let blur = group.iter().any(|m| m.feather.iter().any(|f| *f > 0.));
            let accumulator_start = 1 + usize::from(blur);
            let mut previous_slot = None;
            for (i, m) in group.iter().enumerate() {
                let mesh_key=(owner,m.id);
                if self.meshes.get(&mesh_key).is_none_or(|(fingerprint,_)|*fingerprint!=m.fingerprint) {
                let contents: &[u8] = if m.vertices.is_empty() {
                    &[0; 24]
                } else {
                    bytemuck::cast_slice(&m.vertices)
                };
                let vertices = crate::allocation_buffer::AllocationBuffer::new(device, &self.allocation_budget, &wgpu::BufferDescriptor {
                    label: Some("mask path triangles"),
                    size: contents.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: true,
                }).map_err(|error| mask_context(error, owner, m.id, scene.frame, "vertices"))?;
                {
                    let mut mapped = vertices.get_mapped_range_mut(..).map_err(|error|
                        mask_context(crate::VectorUploadError::Range(std::sync::Arc::new(error)).into(),
                            owner, m.id, scene.frame, "initialize"))?;
                    mapped.copy_from_slice(contents);
                }
                vertices.unmap();
                self.meshes.insert(mesh_key,(m.fingerprint,vertices));
                }
                let vertices=&self.meshes[&mesh_key].1;
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        multiview_mask: None,
                        label: Some("mask source path"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            depth_slice: None,
                            view: &self.scratch[0].view,
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
                    pass.set_viewport(0., 0., m.width as f32, m.height as f32, 0., 1.);
                    pass.set_pipeline(&self.raster);
                    pass.set_vertex_buffer(0, vertices.slice(..));
                    pass.draw(0..m.vertices.len() as u32, 0..1);
                }
                let mut coverage = 0;
                for axis in 0..2 {
                    if m.feather[axis] <= 0. {
                        continue;
                    }
                    let next = 1 - coverage;
                    let input = self.scratch[coverage].view.clone();
                    let target = self.scratch[next].view.clone();
                    self.filter(
                        device,
                        queue,
                        encoder,
                        &mut uniform_index,
                        0,
                        mask_plan::gaussian_uniform(m.width, m.height, axis, m.feather[axis]),
                        &input,
                        &input,
                        &target,
                    ).map_err(|error| mask_context(error, owner, m.id, scene.frame, "feather"))?;
                    coverage = next;
                }
                let source = self.scratch[coverage].view.clone();
                let input = previous_slot.map_or_else(
                    || source.clone(),
                    |index: usize| self.scratch[index].view.clone(),
                );
                let slot = accumulator_start + (i % 2);
                let target = if i + 1 == group.len() {
                    images[&key].view.clone()
                } else {
                    self.scratch[slot].view.clone()
                };
                self.filter(
                    device,
                    queue,
                    encoder,
                    &mut uniform_index,
                    1,
                    mask_plan::combine_uniform(m, i == 0),
                    &input,
                    &source,
                    &target,
                ).map_err(|error| mask_context(error, owner, m.id, scene.frame, "combine"))?;
                previous_slot = Some(slot);
            }
            self.fingerprints.insert(owner, signature);
            self.uploads += 1;
            changed = true;
        }
        Ok(changed)
    }
}
