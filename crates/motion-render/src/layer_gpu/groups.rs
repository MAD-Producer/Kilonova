use crate::resource_ledger::{DeviceTextureExt, OwnedTexture};
use super::*;
use crate::vector_mesh::{RasterCommand, VectorMesh};
use std::collections::HashSet;

struct Target { multisample: OwnedTexture, resolved: FxTexture }
pub(super) struct StagedTargets(Vec<((u32, u32, usize), Target)>);
struct OpacityBinding {
    group: wgpu::BindGroup,
    // A cached binding owns the same admitted buffer even while it is replaced.
    _allocation: crate::allocation_budget::AllocationLease,
}
pub(super) struct GroupGpu {
    targets: HashMap<(u32,u32,usize),Target>,
    uniform: wgpu::BindGroupLayout,
    composite: wgpu::RenderPipeline,
    encode: wgpu::RenderPipeline,
    opacity_group: Option<OpacityBinding>,
    opacity_buffer: crate::vector_upload::ReusableBuffer,
    opacity_upload: Vec<u8>,
}
impl GroupGpu {
    pub(super) fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        for ((w,h,depth), target) in &self.targets {
            let owner = format!("vectors/group/{w}x{h}/{depth}");
            ledger.retained(&target.multisample, crate::TextureCategory::VectorGroupScratch, format!("{owner}/msaa"));
            ledger.retained(&target.resolved.texture, crate::TextureCategory::VectorGroupScratch, format!("{owner}/resolve"));
        }
    }
    pub fn new(device:&wgpu::Device,image:&wgpu::BindGroupLayout,budget:&crate::AllocationBudget) -> Self {
        let uniform=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:Some("vector group opacity"), entries:&[wgpu::BindGroupLayoutEntry {
                binding:0,visibility:wgpu::ShaderStages::FRAGMENT,
                ty:wgpu::BindingType::Buffer {ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:true,min_binding_size:wgpu::BufferSize::new(16)},count:None,
            }],
        });
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("vector group copy"),source:wgpu::ShaderSource::Wgsl(include_str!("../group_copy.wgsl").into())});
        let layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {label:None,bind_group_layouts:&[Some(image), Some(&uniform)],immediate_size: 0});
        let pipeline=|samples,format,blend| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("vector group composite"),layout:Some(&layout),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[]},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState {format,blend,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:Default::default(),depth_stencil:None,multisample:wgpu::MultisampleState {count:samples,..Default::default()},multiview_mask:None,cache:None,
        });
        Self {targets:HashMap::new(),composite:pipeline(4,wgpu::TextureFormat::Rgba8Unorm,Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)),
            encode:pipeline(1,wgpu::TextureFormat::Rgba8UnormSrgb,None),uniform,
            opacity_buffer:crate::vector_upload::ReusableBuffer::new(budget.clone()),opacity_group:None,opacity_upload:Vec::new()}
    }
    pub fn grouped(mesh:&VectorMesh) -> bool { !mesh.commands.is_empty() || mesh.root_opacity!=1. }
    fn needed(meshes:&[VectorMesh]) -> HashSet<(u32,u32,usize)> {
        meshes.iter().filter(|m|Self::grouped(m)).flat_map(|m|(0..=m.scope_depth()).map(move|d|(m.width,m.height,d))).collect()
    }
    pub fn retain(&mut self,meshes:&[VectorMesh]) { let needed=Self::needed(meshes);self.targets.retain(|k,_|needed.contains(k)); }
    pub fn bytes(&self) -> u64 {self.targets.keys().map(|(w,h,_)|u64::from(*w)*u64::from(*h)*20).sum()}
    pub fn release(&mut self) {self.targets.clear();self.opacity_group=None;self.opacity_buffer.clear();self.opacity_upload=Vec::new();}
    pub fn required_bytes(meshes:&[VectorMesh]) -> u64 {Self::needed(meshes).iter().map(|(w,h,_)|u64::from(*w)*u64::from(*h)*20).sum()}
    pub(super) fn upload_info(&self) -> (u64,u64) { (self.opacity_buffer.bytes(),self.opacity_buffer.created) }
    pub(super) fn stage_targets(&self, device: &wgpu::Device, image: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler, scene: &crate::Scene, meshes: &[&VectorMesh],
        budget: &crate::AllocationBudget) -> Result<StagedTargets, RenderError> {
        let mut staged = Vec::new();
        let mut seen = HashSet::new();
        // Mesh order and ascending depth keep refusal context deterministic.
        for mesh in meshes.iter().filter(|m| Self::grouped(m)) {
            let layer = scene.layers[mesh.layer].id;
            for depth in 0..=mesh.scope_depth() {
                let key = (mesh.width, mesh.height, depth);
                if self.targets.contains_key(&key) || !seen.insert(key) { continue; }
                let format = wgpu::TextureFormat::Rgba8Unorm;
                let multisample = device.create_admitted_texture(&wgpu::TextureDescriptor {
                    label: Some("pooled vector group MSAA"),
                    size: wgpu::Extent3d { width: mesh.width, height: mesh.height, depth_or_array_layers: 1 },
                    mip_level_count: 1, sample_count: 4, dimension: wgpu::TextureDimension::D2,
                    format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
                }, budget).map_err(|error| vector_texture_error(error, layer, scene.frame,
                    (mesh.width, mesh.height), format, &format!("group_msaa_allocate depth={depth}")))?;
                let resolved = crate::effect_gpu::admitted_texture(device, image, sampler,
                    mesh.width, mesh.height, format, budget)
                    .map_err(|error| vector_texture_error(error, layer, scene.frame,
                        (mesh.width, mesh.height), format, &format!("group_resolve_allocate depth={depth}")))?;
                staged.push((key, Target { multisample, resolved }));
            }
        }
        Ok(StagedTargets(staged))
    }
    pub(super) fn commit_targets(&mut self, staged: StagedTargets) {
        self.targets.extend(staged.0);
    }
    fn prepare_opacity(&mut self,device:&wgpu::Device,encoder:&mut wgpu::CommandEncoder,
        mesh:&VectorMesh,uploads:&crate::vector_upload::UploadPool,layer:u64,frame:f64) -> Result<u32,RenderError> {
        let stride=u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16);
        let count=mesh.commands.iter().filter(|c|c.kind==2).count() as u64 + 1;
        let size=(count-1).checked_mul(stride).and_then(|bytes|bytes.checked_add(16))
            .ok_or_else(||RenderError::Invalid("vector opacity upload size overflow".into()))?;
        if size>u64::from(u32::MAX) {return Err(RenderError::Invalid("vector opacity offsets exceed u32".into()));}
        let previous=self.opacity_buffer.created;
        let buffer=self.opacity_buffer.get(device,size,wgpu::BufferUsages::UNIFORM,"reusable vector group opacity")
            .map_err(|error| RenderError::Context {
                message:format!("vector layer={layer} frame={frame} phase=opacity: {error}"),cause:Box::new(error),
            })?;
        if previous!=self.opacity_buffer.created {
            self.opacity_group=Some(OpacityBinding {
                group:device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("reusable vector group opacity"),layout:&self.uniform,
                    entries:&[wgpu::BindGroupEntry {binding:0,resource:wgpu::BindingResource::Buffer(wgpu::BufferBinding {buffer:&buffer,offset:0,size:wgpu::BufferSize::new(16)})}]}),
                _allocation:buffer.allocation(),
            });
        }
        self.opacity_upload.resize(size as usize,0);
        for (i,alpha) in mesh.commands.iter().filter(|c|c.kind==2).map(|c|c.opacity)
            .chain(std::iter::once(mesh.root_opacity)).enumerate() {
            let start=i*stride as usize;
            self.opacity_upload[start..start+16].copy_from_slice(bytemuck::cast_slice(&[alpha,0.,0.,0.]));
        }
        uploads.write(device,encoder,&buffer,&self.opacity_upload,layer,frame)?;
        Ok(stride as u32)
    }
    fn pass<'a>(encoder:&'a mut wgpu::CommandEncoder,target:&'a Target,clear:bool) -> wgpu::RenderPass<'a> {
        // Store samples between paints; resolve at the end of each pass so the
        // enclosing group can sample a completed child at its closing command.
        let view=target.multisample.create_view(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label:Some("ordered vector group paint"),color_attachments:&[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view:&view,resolve_target:Some(&target.resolved.view),
                ops:wgpu::Operations {load:if clear {wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)}else{wgpu::LoadOp::Load},store:wgpu::StoreOp::Store},
            })],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,
        })
    }
    pub fn draw(&mut self,device:&wgpu::Device,encoder:&mut wgpu::CommandEncoder,
        mesh:&VectorMesh,output:&wgpu::TextureView,paint:&wgpu::RenderPipeline,clear:&wgpu::RenderPipeline,clear_vertices:&wgpu::Buffer,
        vertices:&wgpu::Buffer,uploads:&crate::vector_upload::UploadPool,layer:u64,frame:f64) -> Result<(),RenderError> {
        let stride=self.prepare_opacity(device,encoder,mesh,uploads,layer,frame)?;
        let group=&self.opacity_group.as_ref().ok_or_else(||RenderError::Invalid("vector opacity binding missing".into()))?.group;
        let mut opacity_index=0;
        let target=|depth|&self.targets[&(mesh.width,mesh.height,depth)];
        {let mut pass=Self::pass(encoder,target(0),true);pass.set_pipeline(clear);pass.set_vertex_buffer(0,clear_vertices.slice(..));pass.draw(0..3,0..1);}
        let mut depth=0;
        for RasterCommand{kind,start,end,..} in mesh.commands.iter().copied() {
            match kind {
                0=>{let mut pass=Self::pass(encoder,target(depth),false);pass.set_pipeline(paint);pass.set_vertex_buffer(0,vertices.slice(..));pass.draw(start..end,0..1);}
                1=>{depth+=1;let mut pass=Self::pass(encoder,target(depth),true);pass.set_pipeline(clear);pass.set_vertex_buffer(0,clear_vertices.slice(..));pass.draw(0..3,0..1);}
                2=>{if depth==0{return Err(RenderError::Invalid("unbalanced vector group raster commands".into()));}
                    let mut pass=Self::pass(encoder,target(depth-1),false);
                    pass.set_pipeline(&self.composite);pass.set_bind_group(0,&target(depth).resolved.composite,&[]);pass.set_bind_group(1,group,&[opacity_index*stride]);pass.draw(0..3,0..1);depth-=1;opacity_index+=1;}
                _=>return Err(RenderError::Invalid("invalid vector group raster command".into())),
            }
        }
        if depth!=0{return Err(RenderError::Invalid("unclosed vector group raster scope".into()));}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,label:Some("encode vector root once"),
            color_attachments:&[Some(wgpu::RenderPassColorAttachment { depth_slice: None,view:output,resolve_target:None,
                ops:wgpu::Operations {load:wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),store:wgpu::StoreOp::Store}})],
            depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None});
        pass.set_pipeline(&self.encode);pass.set_bind_group(0,&target(0).resolved.composite,&[]);pass.set_bind_group(1,group,&[opacity_index*stride]);pass.draw(0..3,0..1);
        Ok(())
    }
}
