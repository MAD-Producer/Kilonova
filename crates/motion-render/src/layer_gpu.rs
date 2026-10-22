use crate::resource_ledger::{DeviceTextureExt, OwnedTexture};
use crate::{
    effect_gpu::FxTexture,
    renderer::{GpuImage, TextureKey},
    RenderError,
};
use std::collections::HashMap;
mod groups;
const CLEAR_VERTICES: [[f32; 6]; 3] = [
    [-1., -1., 0., 0., 0., 0.],
    [3., -1., 0., 0., 0., 0.],
    [-1., 3., 0., 0., 0., 0.],
];
pub(crate) const CLEAR_VERTEX_BYTES: u64 = std::mem::size_of_val(&CLEAR_VERTICES) as u64;
pub(crate) struct LayerGpu {
    vector: wgpu::RenderPipeline,
    vector_clear: wgpu::RenderPipeline,
    vector_resolve: wgpu::RenderPipeline,
    clear_vertices: crate::allocation_buffer::AllocationBuffer,
    pub adjustment: wgpu::RenderPipeline,
    pub present: wgpu::RenderPipeline,
    pub accumulators: Vec<FxTexture>,
    size: [u32; 2],
    fingerprints: HashMap<u64, u64>,
    raster_scratch: HashMap<(u32, u32), OwnedTexture>,
    raster_resolve: HashMap<(u32, u32), FxTexture>,
    groups: groups::GroupGpu,
    uploads: crate::vector_upload::UploadPool,
    vertex_upload: crate::vector_upload::ReusableBuffer,
    allocation_budget: crate::AllocationBudget,
}
impl LayerGpu {
    pub fn upload_info(&self) -> Result<crate::VectorUploadInfo, RenderError> {
        let mut info = self.uploads.info()?;
        let (opacity_bytes, opacity_created) = self.groups.upload_info();
        info.vertex_capacity_bytes = self.vertex_upload.bytes();
        info.opacity_capacity_bytes = opacity_bytes;
        info.destination_buffers_created_total = self.vertex_upload.created.saturating_add(opacity_created);
        Ok(info)
    }
    pub fn release(&mut self) {
        self.accumulators.clear();self.size=[0;2];self.fingerprints.clear();
        self.raster_scratch.clear();self.raster_resolve.clear();self.groups.release();
        self.uploads.release(); self.vertex_upload.clear();
    }
    pub fn new(
        device: &wgpu::Device,
        uniform: &wgpu::BindGroupLayout,
        image: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        budget: &crate::AllocationBudget,
    ) -> Result<Self, RenderError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vector paints"),
            source: wgpu::ShaderSource::Wgsl(include_str!("vector.wgsl").into()),
        });
        let make_vector = |blend| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vector MSAA raster"),
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let vector=make_vector(Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING));
        let vector_clear=make_vector(None);
        let copy_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear vector resolve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("linear_resolve.wgsl").into()),
        });
        let vector_resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("linear vector resolve to sRGB"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None, bind_group_layouts: &[Some(image)], immediate_size: 0,
            })),
            vertex: wgpu::VertexState { module: &copy_shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &copy_shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb, blend: None, write_mask: wgpu::ColorWrites::ALL,
            })] }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let clear_vertices = crate::allocation_buffer::AllocationBuffer::new(device, budget, &wgpu::BufferDescriptor {
            label:Some("transparent vector scratch clear"),
            size: CLEAR_VERTEX_BYTES,
            usage:wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        })?;
        {
            let mut mapped = clear_vertices.get_mapped_range_mut(..).map_err(|error|
                RenderError::Invalid(format!("transparent vector scratch clear phase=initialize: {error}")))?;
            mapped.copy_from_slice(bytemuck::cast_slice(&CLEAR_VERTICES));
        }
        clear_vertices.unmap();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("adjustment and present"),
            source: wgpu::ShaderSource::Wgsl(include_str!("adjustment.wgsl").into()),
        });
        let make = |entry, blend, layouts: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: layouts,
                        immediate_size: 0,
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("fullscreen"),
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
        Ok(Self {
            vector,
            vector_clear,
            vector_resolve,
            clear_vertices,
            adjustment: make("adjust", None, &[Some(uniform), Some(image), Some(image)]),
            present: make(
                "present",
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                &[Some(uniform), Some(image)],
            ),
            accumulators: Vec::new(),
            size: [0; 2],
            fingerprints: HashMap::new(),
            raster_scratch: HashMap::new(),
            raster_resolve: HashMap::new(),
            groups: groups::GroupGpu::new(device,image,budget),
            uploads: crate::vector_upload::UploadPool::new(budget.clone()),
            vertex_upload: crate::vector_upload::ReusableBuffer::new(budget.clone()),
            allocation_budget: budget.clone(),
        })
    }
    pub fn prepare_accumulators(
        &mut self,
        device: &wgpu::Device,
        image: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        format: wgpu::TextureFormat,
        size: Option<[u32; 2]>,
    ) -> Result<bool, RenderError> {
        let changed = self.size != size.unwrap_or([0; 2]);
        match size {
            None => {
                self.accumulators.clear();
                self.size = [0; 2];
            }
            Some(size) if self.size != size => {
                // Reserve both replacement targets before retiring the old pair.
                let next = (0..2)
                    .map(|slot| {
                        crate::effect_gpu::admitted_texture(device, image, sampler,
                            size[0], size[1], format, &self.allocation_budget)
                            .map_err(|error| RenderError::Context {
                                message: format!("layer accumulator slot={slot} size={}x{} format={format:?} phase=allocate: {error}",
                                    size[0], size[1]),
                                cause: Box::new(error),
                            })
                    })
                    .collect::<Result<Vec<_>, RenderError>>()?;
                self.accumulators = next;
                self.size = size;
            }
            _ => {}
        }
        Ok(changed)
    }
    pub(crate) fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        use crate::TextureCategory::*;
        for (i,t) in self.accumulators.iter().enumerate() {
            ledger.retained(&t.texture, LayerCompositingScratch, format!("layers/accumulator/{i}"));
        }
        for ((w,h), t) in &self.raster_scratch {
            ledger.retained(t, VectorRasterScratch, format!("vectors/raster/{w}x{h}/msaa"));
        }
        for ((w,h), t) in &self.raster_resolve {
            ledger.retained(&t.texture, VectorRasterScratch, format!("vectors/raster/{w}x{h}/resolve"));
        }
        self.groups.collect_textures(ledger);
    }
    pub fn bytes(&self) -> u64 {
        self.accumulators.len() as u64 * u64::from(self.size[0]) * u64::from(self.size[1]) * 4 + self.groups.bytes()
            + self
                .raster_scratch
                .iter()
                .map(|((w, h), _)| u64::from(*w) * u64::from(*h) * 20)
                .sum::<u64>()
    }
    pub fn accumulator_bytes(&self) -> u64 {
        self.accumulators.len() as u64 * u64::from(self.size[0]) * u64::from(self.size[1]) * 4
    }
    pub fn raster_scratch_bytes(&self) -> u64 {
        // One shared size owns 16-byte MSAA plus a 4-byte resolve. The separate
        // 4-byte vector source belongs to images/VectorSources, not this pool.
        self.raster_scratch.keys().map(|(w,h)| u64::from(*w)*u64::from(*h)*20).sum()
    }
    pub fn group_scratch_bytes(&self) -> u64 { self.groups.bytes() }
    pub fn pending_vector_bytes(&self, scene: &crate::Scene, meshes: &[crate::vector_mesh::VectorMesh], images: &HashMap<TextureKey, GpuImage>) -> u64 {
        let mut scratch: std::collections::HashSet<_> = self.raster_scratch.keys().copied()
            .filter(|size| meshes.iter().any(|m| !groups::GroupGpu::grouped(m) && *size == (m.width, m.height))).collect();
        let mut bytes = scratch.iter().map(|(w, h)| u64::from(*w) * u64::from(*h) * 20).sum::<u64>() + groups::GroupGpu::required_bytes(meshes);
        for mesh in meshes {
            let id = scene.layers[mesh.layer].id;
            let old = images.get(&TextureKey::Vector(id));
            if self.fingerprints.get(&id) == Some(&mesh.fingerprint) && old.is_some_and(|i| i.size == (mesh.width, mesh.height)) { continue; }
            let cost = u64::from(mesh.width) * u64::from(mesh.height) * 4;
            if old.is_none_or(|i| i.size != (mesh.width, mesh.height)) { bytes += cost; }
            if !groups::GroupGpu::grouped(mesh) && scratch.insert((mesh.width, mesh.height)) { bytes += cost * 5; }
        }
        bytes
    }
    pub fn prepare_vectors(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        image: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        scene: &crate::Scene,
        meshes: &[crate::vector_mesh::VectorMesh],
        images: &mut HashMap<TextureKey, GpuImage>,
        bytes: &mut u64,
        resource_bytes: u64,
        source_budget: u64,
    ) -> Result<bool, RenderError> {
        let stale = images
            .keys()
            .filter_map(|key| match key {
                TextureKey::Vector(id)
                    if !scene
                        .layers
                        .iter()
                        .any(|l| l.id == *id && l.vector.is_some()) =>
                {
                    Some(*key)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for key in stale {
            if let Some(image) = images.remove(&key) {
                *bytes = (*bytes).saturating_sub(image.bytes);
                changed = true;
            }
        }
        self.fingerprints
            .retain(|id, _| images.contains_key(&TextureKey::Vector(*id)));
        self.raster_scratch
            .retain(|size, _| meshes.iter().any(|m| !groups::GroupGpu::grouped(m) && *size == (m.width, m.height)));
        self.raster_resolve
            .retain(|size, _| meshes.iter().any(|m| !groups::GroupGpu::grouped(m) && *size == (m.width, m.height)));
        self.groups.retain(meshes);
        if meshes.is_empty() {
            self.uploads.release(); self.vertex_upload.clear(); self.groups.release();
        }
        // Include the complete working set even when all fingerprints hit.
        // Policy reductions and later meshes must refuse before any new upload.
        let required = *bytes + self.pending_vector_bytes(scene, meshes, images) + resource_bytes;
        if !meshes.is_empty() && required > source_budget {
            return Err(RenderError::budget(crate::ResourceBudgetScope::VectorWorkingSet,
                required, source_budget, "vector sources and MSAA raster resources exceed configured source texture budget"));
        }
        for mesh in meshes {
            let id = scene.layers[mesh.layer].id;
            let key = TextureKey::Vector(id);
            if self.fingerprints.get(&id) == Some(&mesh.fingerprint)
                && images
                    .get(&key)
                    .is_some_and(|i| i.size == (mesh.width, mesh.height))
            {
                continue;
            }
            let cost = u64::from(mesh.width) * u64::from(mesh.height) * 4;
            let previous = images.get(&key).map_or(0, |i| i.bytes);
            let resize = images
                .get(&key)
                .is_none_or(|i| i.size != (mesh.width, mesh.height));
            let new_source = if resize { cost } else { 0 };
            let msaa_bytes = self
                .raster_scratch
                .iter()
                .map(|((w, h), _)| u64::from(*w) * u64::from(*h) * 20)
                .sum::<u64>() + groups::GroupGpu::required_bytes(meshes);
            let new_scratch = if groups::GroupGpu::grouped(mesh) || self.raster_scratch.contains_key(&(mesh.width, mesh.height)) {
                0
            } else {
                cost * 5
            };
            if *bytes + new_source + msaa_bytes + new_scratch + resource_bytes > source_budget {
                return Err(RenderError::budget(crate::ResourceBudgetScope::VectorWorkingSet,
                    *bytes + new_source + msaa_bytes + new_scratch + resource_bytes,
                    source_budget, format!(
                    "layer {id}: vector sources and MSAA raster resources exceed configured source texture budget"
                )));
            }
            if resize {
                let target = crate::effect_gpu::texture(
                    device,
                    image,
                    sampler,
                    mesh.width,
                    mesh.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                );
                images.insert(
                    key,
                    GpuImage {
                        _texture: target.texture,
                        view: target.view,
                        bind_group: target.composite,
                        bytes: cost,
                        size: (mesh.width, mesh.height),
                        video_stamp: None,
                    },
                );
                *bytes = *bytes - previous + cost;
                changed = true;
            }
            let contents = if mesh.vertices.is_empty() { &[0u8;24][..] } else { bytemuck::cast_slice(&mesh.vertices) };
            let vertices = self.vertex_upload.get(device, contents.len() as u64,
                wgpu::BufferUsages::VERTEX, "reusable vector raster triangles")
                .map_err(|error| RenderError::Context {
                    message: format!("vector layer={id} frame={} phase=vertices: {error}", scene.frame),
                    cause: Box::new(error),
                })?;
            self.uploads.write(device, encoder, &vertices, contents, id, scene.frame)?;
            if groups::GroupGpu::grouped(mesh) {
                self.groups.draw(device,encoder,image,sampler,mesh,&images[&key].view,&self.vector,&self.vector_clear,&self.clear_vertices,
                    &vertices,&self.uploads,id,scene.frame)?;
                self.fingerprints.insert(id,mesh.fingerprint);
                continue;
            }
            let multisample = self
                .raster_scratch
                .entry((mesh.width, mesh.height))
                .or_insert_with(|| {
                    device.create_owned_texture(&wgpu::TextureDescriptor {
                        label: Some("shared vector MSAA scratch"),
                        size: wgpu::Extent3d {
                            width: mesh.width,
                            height: mesh.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 4,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                });
            let view = multisample.create_view(&Default::default());
            let resolved = self.raster_resolve.entry((mesh.width, mesh.height)).or_insert_with(|| {
                crate::effect_gpu::texture(device, image, sampler, mesh.width, mesh.height, wgpu::TextureFormat::Rgba8Unorm)
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    multiview_mask: None,
                    label: Some("materialize vector source"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        depth_slice: None,
                        view: &view,
                        resolve_target: Some(&resolved.view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Discard,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                // Some GLES drivers retain resolved MSAA alpha after a load clear.
                // Overwrite all samples before reusing the shared raster target.
                pass.set_pipeline(&self.vector_clear);
                pass.set_vertex_buffer(0,self.clear_vertices.slice(..));
                pass.draw(0..3,0..1);
                pass.set_pipeline(&self.vector);
                pass.set_vertex_buffer(0, vertices.slice(..));
                if !mesh.vertices.is_empty() {
                    pass.draw(0..mesh.vertices.len() as u32, 0..1);
                }
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    multiview_mask: None,
                    label: Some("encode resolved vector color"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        depth_slice: None,
                        view: &images[&key].view, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                });
                pass.set_pipeline(&self.vector_resolve);
                pass.set_bind_group(0, &resolved.composite, &[]);
                pass.draw(0..3, 0..1);
            }
            self.fingerprints.insert(id, mesh.fingerprint);
        }
        Ok(changed)
    }
}
