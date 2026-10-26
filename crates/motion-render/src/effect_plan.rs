//! The same frame plan drives wgpu and the MediaCodec/GLES adapter.
use crate::{RenderError, ResourceBudgetScope, ResourceRejection, Scene};
use motion_effects::image_contract::ChannelPrecision;
use motion_effects::{
    shader, AlphaMode, EdgeMode, EffectDefinition, EffectPackage, Registry, WorkingSpace,
};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

#[cfg(test)]
mod error_tests;
mod scratch_graph;
mod scratch_lifetimes;
mod scratch;
mod pointwise;
pub use pointwise::{FusionFrameReport, FusionGroupReport, FusionRetainReason};
mod execution;
pub use execution::{native_execution_capabilities, native_snapshot_limits, SnapshotEncode, SnapshotExecution};
mod effect_execution;
#[cfg(all(debug_assertions, feature = "gpu-tests"))]
mod execution_fault;
#[cfg(all(debug_assertions, feature = "gpu-tests"))]
#[doc(hidden)]
pub use execution_fault::ExecutionDiagnosticFault;
mod runtime_template;
mod cpu_prepare;
pub use cpu_prepare::CpuPreparationInfo;
mod preparation;
pub use effect_execution::{EffectExecution, EffectOperationBinding, EffectNativeTask, ExecutionCodeCacheInfo, native_effect_limits};
pub use scratch::{ScratchAllocation, ScratchBinding, ScratchClass, ScratchFormat, MAX_SCRATCH_PASSES, SCRATCH_RECORD_BYTES};
#[cfg(test)]
mod scratch_proposal_tests;

pub const PLAN_MAGIC: u32 = 0x46584d53;
pub const PLAN_VERSION: u32 = 12;
pub const HEADER_BYTES: usize = 160;
pub const DRAW_WORDS: usize = 32;
pub const PASS_WORDS: usize = 10;
/// Counterfactual equal-sized semantic slots, retained for regression comparisons.
/// Actual native capacity is EffectFramePlan::scratch_bytes().
pub fn scratch_bytes(width: u32, height: u32, slots: u32) -> u64 {
    u64::from(width)
        * u64::from(height)
        * 4
        * u64::from(slots.count_ones() + u32::from(slots & 128 != 0))
}

#[cfg(test)]
mod vector_frame_budget_tests {
    use super::*;
    fn scene(count:u64)->Scene {
        let mut p=motion_model::Project::new(64,64,30,60).unwrap();
        for id in 1..=count {
            let mut l=motion_model::Layer::solid(id,"vector",[20.;2],[32.,32.,0.],[1.;4]);
            l.content=motion_model::Content::Vector {vector:motion_model::vector::VectorContent::shape(motion_model::vector::ShapeKind::Rectangle)};
            p.layers.push(l);
        }
        let mut s=Scene::new(&p).unwrap();s.sample(&p,0.,None,&motion_core::ExpressionEvaluator).unwrap();s
    }
    #[test]
    fn cached_vectors_spend_the_frame_limit_before_new_or_cached_meshes_are_appended() {
        for existing_third in [false,true] {
            let mut builder=PlanBuilder::new(Registry::new_with_builtins().unwrap()).unwrap();
            builder.build(&scene(if existing_third{3}else{2}),&[0],64,64,false).unwrap();
            // Repeat the opaque cached rectangle's actual triangles. This
            // exercises cached buffer capacity without another large Lyon run.
            for id in [1,2] {
                let mesh=&mut builder.vector_cache.get_mut(&id).unwrap().3;
                assert_eq!(mesh.vertices.len(),6);
                mesh.vertices=std::sync::Arc::new(mesh.vertices.repeat(21845));
            }
            assert!(builder.build(&scene(3),&[0],64,64,false).is_err());
            assert!(builder.frame.vectors.iter().map(|m|m.vertices.len()).sum::<usize>()<=crate::vector_mesh::MAX_TRIANGLE_VERTICES);
            assert_eq!(builder.vector_cache.contains_key(&3),existing_third,"new rejected geometry must not enter the cache");
            assert_eq!(builder.vector_cache[&1].3.vertices.len(),131070);
        }
    }
}
/// Exact capacities of the independently sized, host-owned scratch textures.
/// Semantic-slot maxima for construction diagnostics and baseline comparisons.
/// Physical admission and allocation use EffectFramePlan::scratch_bytes().
pub fn scratch_capacity_bytes(sizes: &[[u32; 2]; 8]) -> u64 {
    sizes
        .iter()
        .enumerate()
        .map(|(i, size)| u64::from(size[0]) * u64::from(size[1]) * if i == 7 { 8 } else { 4 })
        .sum()
}
fn reserve_scratch(sizes: &mut [[u32; 2]; 8], slot: usize, width: u32, height: u32) {
    sizes[slot][0] = sizes[slot][0].max(width);
    sizes[slot][1] = sizes[slot][1].max(height);
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EffectUniform {
    pub size: [f32; 4],
    pub region: [f32; 4],
    pub input_region: [f32; 4],
    pub source_region: [f32; 4],
    pub clock: [f32; 4],
    pub mode: [f32; 4],
    pub output_mode: [f32; 4],
    pub params: [[f32; 4]; 32],
}
#[derive(Clone)]
pub struct EffectProgram {
    pub key: String,
    pub shader: Arc<shader::CompiledShader>,
    pub package: Option<Arc<EffectPackage>>,
    pub resources: Vec<String>,
}
#[derive(Clone, Copy, Debug)]
pub struct EffectPass {
    /// Native per-instance resource override, index + 1 into the frame sidecar.
    /// Zero uses immutable package defaults. Never serialized as packed v12.
    pub instance_data: u32,
    /// v7 reuses non-sprite word 8: 0=package, >0=draw+1, <0=asset slot.
    pub resource_input: i32,
    pub program: u32,
    pub input: i32,
    pub source: i32,
    pub output: u32,
    pub width: u32,
    pub height: u32,
    pub lut: i32,
    pub uniform: EffectUniform,
    pub sprite: bool,
    pub sprite_start: u32,
    pub sprite_count: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct PlannedDraw {
    pub layer: u64,
    pub words: [f32; DRAW_WORDS],
    pub pass_start: usize,
    pub pass_end: usize,
}
#[derive(Default, Debug)]
pub struct EffectFramePlan {
    pub instance_data: Vec<Arc<motion_effects::shader_data::ShaderDataValues>>,
    pub precision: ChannelPrecision,
    /// Canonical UV-to-clip homographies for native drawing. CPU-only metadata;
    /// deliberately excluded from the portable packed-plan ABI.
    pub uv_maps: Vec<glam::DMat3>,
    pub scratch_budget: u64,
    pub draws: Vec<PlannedDraw>,
    pub passes: Vec<EffectPass>,
    pub width: u32,
    pub height: u32,
    pub slots: u32,
    pub scratch_sizes: [[u32; 2]; 8],
    /// Actual physical descriptors; semantic maxima above are counterfactual
    /// construction diagnostics, not native capacity or budget accounting.
    pub scratch_allocations: Vec<ScratchAllocation>,
    pub scratch_bindings: Vec<ScratchBinding>,
    pub scratch_outputs: Vec<Option<u32>>,
    pub diagnostics: Vec<String>,
    pub vertices: Vec<crate::PlaneVertex>,
    pub batches: Vec<crate::PlaneBatch>,
    pub sprites: Vec<crate::scene_generator::Sprite>,
    pub generator_stats: crate::scene_generator::GeneratorStats,
    pub vectors: Vec<crate::vector_mesh::VectorMesh>,
    pub masks: Vec<crate::mask_plan::MaskRaster>,
    pub compositing:Vec<LayerComposite>,
}
#[derive(Clone,Copy,Debug)]
pub struct LayerComposite {pub blend:u32,pub space:u32,pub matte:i32,pub mode:u32,pub visible:bool}
/// GPU snapshots are evaluated once in dependency order, before compositing.
pub fn image_input_order(frame: &EffectFramePlan) -> Result<Vec<usize>, RenderError> {
    image_input_order_for(frame, &[])
}
pub(crate) fn image_input_order_for(frame: &EffectFramePlan, targets: &[u64]) -> Result<Vec<usize>, RenderError> {
    fn visit(
        frame: &EffectFramePlan,
        node: usize,
        marks: &mut Vec<u8>,
        order: &mut Vec<usize>,
    ) -> Result<(), RenderError> {
        if node >= frame.draws.len() {
            return Err("effect image input index is invalid".into());
        }
        // Ordinary frames have no snapshot dependencies. Allocate traversal
        // storage only when an effected input, matte or receiver needs it.
        if marks.is_empty() {
            marks.resize(frame.draws.len(), 0);
        }
        if marks[node] == 1 {
            return Err("effect image input cycle".into());
        }
        if marks[node] == 2 {
            return Ok(());
        }
        marks[node] = 1;
        let draw = &frame.draws[node];
        let passes = frame.passes.get(draw.pass_start..draw.pass_end).ok_or_else(||
            RenderError::Invalid(format!("effect image input draw {node} has invalid pass range {}..{} ({} passes)",
                draw.pass_start, draw.pass_end, frame.passes.len())))?;
        for p in passes {
            if p.resource_input > 0 && p.uniform.params[30][1] > 0.5 {
                visit(frame, (p.resource_input - 1) as usize, marks, order)?;
            }
        }
        marks[node] = 2;
        order.push(node);
        Ok(())
    }
    let mut order = Vec::new();
    let mut marks = Vec::new();
    for pass in &frame.passes {
        if pass.resource_input > 0 && pass.uniform.params[30][1] > 0.5 {
            visit(
                frame,
                (pass.resource_input - 1) as usize,
                &mut marks,
                &mut order,
            )?;
        }
    }
    for composite in &frame.compositing {
        if composite.matte >= 0 {
            visit(frame, composite.matte as usize, &mut marks, &mut order)?;
        }
    }
    for target in targets {
        let index = frame.draws.iter().position(|draw| draw.layer == *target)
            .ok_or_else(|| format!("materialized receiver {target} is absent"))?;
        visit(frame, index, &mut marks, &mut order)?;
    }
    Ok(order)
}

impl EffectFramePlan {
    /// Wire word 156 selects this numeric table encoding. Sampling is manual.
    pub fn curve_lut_layout(&self) -> (usize, usize) {
        if self.precision == ChannelPrecision::Unorm8 { (256, 4) }
        else { (motion_model::HIGH_PRECISION_CURVE_SAMPLES, 16) }
    }
    fn curve_lut_stride(&self) -> usize {
        let (samples, pixel_bytes) = self.curve_lut_layout();
        samples * pixel_bytes
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.scratch_allocations.iter().map(|allocation| allocation.bytes()).sum()
    }
    pub(crate) fn scratch_output(&self, draw: usize) -> Result<usize, RenderError> {
        self.scratch_outputs.get(draw).copied().flatten().map(|id| id as usize)
            .ok_or_else(|| RenderError::Invalid(format!("effect draw {draw} has no scratch output")))
    }
    pub fn buffer_bytes(&self, scene: &Scene) -> usize {
        HEADER_BYTES
            + self.draws.len() * 128
            + self.passes.len() * 40
            + self.passes.len() * shader::UNIFORM_BYTES
            + scene.curve_luts.len() * self.curve_lut_stride()
            + self.batches.len() * 12
            + self.vertices.len() * 20
            + self.sprites.len() * 48
            + self.vectors.len() * 40
            + self.vectors.iter().map(|v|v.commands.len()*16).sum::<usize>()
            + self
                .vectors
                .iter()
                .map(|v| v.vertices.len() * 24)
                .sum::<usize>()
            + self.masks.len() * crate::mask_plan::RECORD_BYTES
            + self.masks.iter().map(|m|m.vertices.len()*24).sum::<usize>()
            + self.compositing.len()*20
            + self.scratch_allocations.len()*SCRATCH_RECORD_BYTES
    }
    pub fn write(&self, scene: &Scene, out: &mut [u8]) -> Result<usize, RenderError> {
        if self.passes.iter().any(|pass| pass.instance_data != 0) {
            return Err("packed plan v12 cannot transport instance_shader_data textures; use a native consumer with instance_shader_data_v1".into());
        }
        if self.precision != ChannelPrecision::Unorm8 && scene.curve_sources.len() != scene.curve_luts.len() {
            return Err(RenderError::Invalid(format!(
                "packed high-precision curve sources={} do not match LUTs={}",
                scene.curve_sources.len(), scene.curve_luts.len())));
        }
        let size = self.buffer_bytes(scene);
        if out.len() < size {
            return Err(format!("render plan buffer requires {size} bytes").into());
        }
        let draw_offset = HEADER_BYTES;
        let pass_offset = draw_offset + self.draws.len() * 128;
        let uniform_offset = pass_offset + self.passes.len() * 40;
        let lut_offset = uniform_offset + self.passes.len() * shader::UNIFORM_BYTES;
        let lut_stride = self.curve_lut_stride();
        let sprite_offset = lut_offset + scene.curve_luts.len() * lut_stride;
        let batch_offset = sprite_offset + self.sprites.len() * 48;
        let vertex_offset = batch_offset + self.batches.len() * 12;
        let vector_offset = vertex_offset + self.vertices.len() * 20;
        let vector_data_offset = vector_offset + self.vectors.len() * 40;
        let vector_command_offset = vector_data_offset + self.vectors.iter().map(|v|v.vertices.len()*24).sum::<usize>();
        let mask_offset = vector_command_offset + self.vectors.iter().map(|v|v.commands.len()*16).sum::<usize>();
        let mask_data_offset = mask_offset + self.masks.len()*crate::mask_plan::RECORD_BYTES;
        let compositing_offset=mask_data_offset+self.masks.iter().map(|m|m.vertices.len()*24).sum::<usize>();
        let scratch_offset = compositing_offset + self.compositing.len()*20;
        let header = [
            PLAN_MAGIC,
            PLAN_VERSION,
            self.draws.len() as u32,
            self.passes.len() as u32,
            draw_offset as u32,
            pass_offset as u32,
            uniform_offset as u32,
            size as u32,
            self.width,
            self.height,
            self.slots,
            lut_offset as u32,
            scene.curve_luts.len() as u32,
            batch_offset as u32,
            self.batches.len() as u32,
            vertex_offset as u32,
            sprite_offset as u32,
            self.sprites.len() as u32,
            48,
            self.scratch_budget as u32,
            self.vertices.len() as u32,
            vector_offset as u32,
            self.vectors.len() as u32,
            vector_data_offset as u32,
            24,
            scene.width,
            scene.height,
            40,
            mask_offset as u32,
            self.masks.len() as u32,
            mask_data_offset as u32,
            crate::mask_plan::RECORD_BYTES as u32,
            compositing_offset as u32,self.compositing.len() as u32,20,
            scratch_offset as u32, self.scratch_allocations.len() as u32,
            SCRATCH_RECORD_BYTES as u32, self.precision as u32,
            u32::from(self.precision != ChannelPrecision::Unorm8),
        ];
        out[..HEADER_BYTES].copy_from_slice(bytemuck::cast_slice(&header));
        for (i, d) in self.draws.iter().enumerate() {
            out[draw_offset + i * 128..draw_offset + (i + 1) * 128]
                .copy_from_slice(bytemuck::cast_slice(&d.words));
        }
        for (i, p) in self.passes.iter().enumerate() {
            let binding = self.scratch_bindings.get(i).ok_or("packed effect scratch binding is missing")?;
            let data = [
                p.program,
                binding.input as u32,
                binding.source as u32,
                binding.output,
                p.width,
                p.height,
                (uniform_offset + i * shader::UNIFORM_BYTES) as u32,
                if p.lut < 0 {
                    u32::MAX
                } else {
                    (lut_offset + p.lut as usize * lut_stride) as u32
                },
                if p.sprite {p.sprite_start} else {p.resource_input as u32},
                p.sprite_count,
            ];
            out[pass_offset + i * 40..pass_offset + (i + 1) * 40]
                .copy_from_slice(bytemuck::cast_slice(&data));
            out[uniform_offset + i * shader::UNIFORM_BYTES
                ..uniform_offset + (i + 1) * shader::UNIFORM_BYTES]
                .copy_from_slice(bytemuck::bytes_of(&p.uniform));
        }
        for (i, lut) in scene.curve_luts.iter().enumerate() {
            if self.precision != ChannelPrecision::Unorm8 {
                let source = scene.curve_sources.get(i).ok_or_else(|| RenderError::Invalid(
                    format!("packed effect curve index={i}: high-precision authored source is missing")))?;
                let rows = source.high_precision_lut();
                out[lut_offset + i * lut_stride..lut_offset + (i + 1) * lut_stride]
                    .copy_from_slice(bytemuck::cast_slice(&rows));
                continue;
            }
            for (j, pixel) in lut.iter().enumerate() {
                for c in 0..4 {
                    out[lut_offset + i * 1024 + j * 4 + c] =
                        (pixel[c].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
        out[sprite_offset..batch_offset].copy_from_slice(bytemuck::cast_slice(&self.sprites));
        for (i, batch) in self.batches.iter().enumerate() {
            let data = [
                batch.layer as u32,
                batch.vertices.start,
                batch.vertices.end - batch.vertices.start,
            ];
            out[batch_offset + i * 12..batch_offset + (i + 1) * 12]
                .copy_from_slice(bytemuck::cast_slice(&data));
        }
        for (i, v) in self.vertices.iter().enumerate() {
            let data = [
                v.position[0],
                v.position[1],
                v.position[2],
                v.uv[0],
                v.uv[1],
            ];
            out[vertex_offset + i * 20..vertex_offset + (i + 1) * 20]
                .copy_from_slice(bytemuck::cast_slice(&data));
        }
        let mut offset = vector_data_offset;
        let mut command_offset = vector_command_offset;
        for (i, v) in self.vectors.iter().enumerate() {
            let record = [
                v.layer as u32,
                v.width,
                v.height,
                offset as u32,
                v.vertices.len() as u32,
                v.fingerprint as u32,
                (v.fingerprint >> 32) as u32,
                command_offset as u32,
                v.commands.len() as u32,
                v.root_opacity.to_bits(),
            ];
            out[vector_offset + i * 40..vector_offset + (i + 1) * 40]
                .copy_from_slice(bytemuck::cast_slice(&record));
            let end = offset + v.vertices.len() * 24;
            out[offset..end].copy_from_slice(bytemuck::cast_slice(&v.vertices));
            offset = end;
            let end = command_offset + v.commands.len()*16;
            out[command_offset..end].copy_from_slice(bytemuck::cast_slice(&v.commands));
            command_offset = end;
        }
        crate::mask_plan::write(&self.masks,out,mask_offset,mask_data_offset);
        for (i,c) in self.compositing.iter().enumerate(){let words=[c.blend,c.space,c.matte as u32,c.mode,u32::from(c.visible)];out[compositing_offset+i*20..compositing_offset+(i+1)*20].copy_from_slice(bytemuck::cast_slice(&words));}
        for (i, allocation) in self.scratch_allocations.iter().enumerate() {
            out[scratch_offset+i*SCRATCH_RECORD_BYTES..scratch_offset+(i+1)*SCRATCH_RECORD_BYTES]
                .copy_from_slice(bytemuck::cast_slice(&allocation.words()));
        }
        Ok(size)
    }
}
struct Resolved {
    signature: (String, String, String, String),
    definition: EffectDefinition,
    programs: Vec<u32>,
    mapping: Vec<usize>,
    error: Option<Arc<RenderError>>,
    pointwise: motion_effects::pointwise::Eligibility,
    semantics: Option<Arc<preparation::DefinitionSemantics>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchRejection {
    DimensionLimit,
    CapacityBudget,
}
/// Last checked candidate, including one rejected before frame capacities commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchRequest {
    /// Semantic maxima retained for counterfactual diagnostics only.
    pub sizes: [[u32; 2]; 8],
    pub required_bytes: u64,
    pub allocations: u32,
    pub budget_bytes: u64,
    pub rejection: Option<ScratchRejection>,
}
pub struct PlanBuilder {
    precision: ChannelPrecision,
    scratch_budget: u64,
    pub(crate) source_texture_budget: u64,
    pub(crate) decoded_image_budget: u64,
    preview_target_budget: u64,
    session_allocation_budget: u64,
    pub registry: Registry,
    pub device_dimension: u32,
    pub program_errors: std::collections::BTreeMap<u32, String>,
    pub programs: Vec<EffectProgram>,
    resolved: Vec<Option<Resolved>>,
    pub frame: EffectFramePlan,
    /// Last actual pool check; cache hits do not perform a new check.
    pub last_scratch_request: Option<ScratchRequest>,
    /// Refusal responsible for the current failed build; never a stale diagnostic.
    pub last_resource_rejection: Option<ResourceRejection>,
    geometry: crate::PlaneCompositor,
    plane_coverage: bool,
    sizes: Vec<[f32; 2]>,
    origins: Vec<[f32; 2]>,
    overlays: Vec<bool>,
    pub generator_scratch: crate::scene_generator::GeneratorScratch,
    pub alpha_images: std::collections::HashMap<u64, crate::scene_generator::AlphaImage>,
    vector_cache: std::collections::HashMap<
        u64,
        (
            Arc<motion_model::vector::SampledVector>,
            [f32; 2],
            f32,
            crate::vector_mesh::VectorMesh,
        ),
    >,
    mask_cache: crate::mask_plan::MaskCache,
    cpu_preparation: CpuPreparationInfo,
    preview_inputs: Option<crate::effect_plan_cache::PreviewInputs>,
    pass_clocks: Vec<Option<usize>>,
    pub preview_cache_hits: u64,
    pub preview_plan_builds: u64,
    fusion: pointwise::FusionState,
    effect_execution: Option<effect_execution::EffectExecution>,
    code_identities: effect_execution::CodeIdentities,
    #[cfg(all(debug_assertions, feature = "gpu-tests"))]
    execution_fault: Option<ExecutionDiagnosticFault>,
}

impl PlanBuilder {
    /// Explicit package-owned CPU capacity. Shared registry/program references
    /// count once; independent program shader copies are outside this scope.
    pub fn package_memory_info(&self) -> motion_effects::memory::PackageMemoryInfo {
        motion_effects::memory::package_memory(self.package_memory_owners())
    }

    /// Borrowed allocation owners for one combined query across renderers.
    pub fn compiled_shader_owners(&self) -> impl Iterator<Item = (&str, &Arc<shader::CompiledShader>)> {
        self.programs.iter().map(|p| (p.key.as_str(), &p.shader))
    }

    /// Borrowed package allocation owners for one combined query across renderers.
    pub fn package_memory_owners(&self) -> impl Iterator<Item = (
        motion_effects::memory::PackageOwner, &Arc<EffectPackage>,
    )> {
        use motion_effects::memory::PackageOwner;
        self.registry.packages.values().map(|p| (PackageOwner::Registry, p))
            .chain(self.programs.iter().filter_map(|p| p.package.as_ref())
                .map(|p| (PackageOwner::PlanProgram, p)))
    }
}

#[cfg(test)]
mod package_memory_tests;
fn utility(code: &str) -> Result<Arc<shader::CompiledShader>, RenderError> {
    shader::compile(code, "main_fx")
        .map(Arc::new)
        .map_err(RenderError::from)
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
impl PlanBuilder {
    fn check_scratch(
        sizes: &[[u32; 2]; 8], device_dimension: u32, budget: u64,
        request: &mut Option<ScratchRequest>,
        rejection: &mut Option<ResourceRejection>,
    ) -> Result<(), RenderError> {
        *rejection = None;
        *request = Some(ScratchRequest { sizes: *sizes, required_bytes: 0,
            allocations: 0, budget_bytes: budget, rejection: None });
        for (slot, size) in sizes.iter().enumerate() {
            if size.iter().any(|&n| n > device_dimension) {
                request.as_mut().unwrap().rejection = Some(ScratchRejection::DimensionLimit);
                let refused = ResourceRejection::TextureDimensions { width: size[0], height: size[1], max_dimension: device_dimension };
                *rejection = Some(refused);
                return Err(RenderError::Resource { rejection: refused, message: format!(
                    "effect scratch texture {slot} requires {}x{}; device dimension limit is {}",
                    size[0], size[1], device_dimension
                ) });
            }
        }
        Ok(())
    }
    fn check_placed_scratch(
        frame: &EffectFramePlan, current: Option<std::ops::Range<usize>>,
        sizes: [[u32; 2]; 8], budget: u64,
        request: &mut Option<ScratchRequest>, rejection: &mut Option<ResourceRejection>,
    ) -> Result<scratch::Layout, RenderError> {
        let layout = scratch::layout(frame, current)?;
        let denied = (layout.bytes > budget).then_some(ScratchRejection::CapacityBudget);
        *request = Some(ScratchRequest { sizes, required_bytes: layout.bytes,
            allocations: layout.allocations.len() as u32, budget_bytes: budget, rejection: denied });
        if denied.is_some() {
            let refused = ResourceRejection::Budget { scope: ResourceBudgetScope::EffectScratch,
                required_bytes: layout.bytes, budget_bytes: budget };
            *rejection = Some(refused);
            return Err(RenderError::Resource { rejection: refused, message: format!(
                "effect scratch textures require {:.2} MiB; budget is {} MiB (full layer bounds, including effect padding)",
                layout.bytes as f64 / 1048576.0, budget / 1048576) });
        }
        *rejection = None;
        Ok(layout)
    }
    pub fn set_alpha(
        &mut self,
        id: u64,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), RenderError> {
        let size = u64::from(width) * u64::from(height);
        let previous = self
            .alpha_images
            .get(&id)
            .map_or(0, |a| a.pixels.len() as u64);
        let total = self
            .alpha_images
            .values()
            .map(|a| a.pixels.len() as u64)
            .sum::<u64>();
        if width == 0
            || height == 0
            || rgba.len() as u64 != size * 4
            || total - previous + size > 32 * 1024 * 1024
        {
            return Err("occlusion alpha cache exceeds 32 MiB or has invalid dimensions".into());
        }
        self.alpha_images.insert(
            id,
            crate::scene_generator::AlphaImage {
                width,
                height,
                pixels: rgba.chunks_exact(4).map(|p| p[3]).collect(),
            },
        );
        Ok(())
    }
    pub fn synchronize_alpha(
        &mut self,
        project: &motion_model::Project,
        root: &std::path::Path,
    ) -> Result<(), RenderError> {
        if !project.layers.iter().any(|l| {
            l.effects
                .iter()
                .any(|e| e.enabled && e.scene.as_ref().is_some_and(|s| s.occlusion))
        }) {
            self.alpha_images.clear();
            return Ok(());
        }
        for asset in &project.assets {
            if !self.alpha_images.contains_key(&asset.id) {
                let image = image::open(root.join(&asset.path))
                    .map_err(RenderError::from)?
                    .into_rgba8();
                if image.width() != asset.width || image.height() != asset.height {
                    return Err("occlusion asset metadata mismatch".into());
                }
                self.set_alpha(asset.id, asset.width, asset.height, image.as_raw())?;
            }
        }
        self.alpha_images
            .retain(|id, _| project.assets.iter().any(|a| a.id == *id));
        Ok(())
    }
    /// Alpha dependencies use the same sampled working set as image textures,
    /// including nested compositions. Unused library images cannot exhaust it.
    pub fn synchronize_scene_alpha(
        &mut self, scene: &Scene, project: &motion_model::Project, root: &std::path::Path,
    ) -> Result<(), RenderError> {
        fn collect(scene: &Scene, wanted: &mut std::collections::BTreeSet<u64>) {
            if scene.effects.iter().any(|e| e.enabled && scene.layers.iter().any(|l|l.id==e.layer)
                && e.scene.as_ref().is_some_and(|s| s.occlusion)) {
                wanted.extend(scene.layers.iter().filter_map(|l| l.asset));
            }
            for child in &scene.nested { collect(&child.scene, wanted); }
        }
        let mut wanted = Default::default();
        collect(scene, &mut wanted);
        self.alpha_images.retain(|id, _| wanted.contains(id));
        let pixels: u64 = project.assets.iter().filter(|a| wanted.contains(&a.id))
            .map(|a| u64::from(a.width)*u64::from(a.height)).sum();
        if pixels > 32*1024*1024 { return Err("active occlusion alpha exceeds 32 MiB".into()); }
        for id in wanted {
            if self.alpha_images.contains_key(&id) { continue; }
            let asset = project.assets.iter().find(|a| a.id == id).ok_or("occlusion asset missing")?;
            let source = crate::image_resources::Source::new(root, asset)?;
            let image = crate::image_resources::decode(&source, crate::image_resources::Resolution::Full)?;
            self.set_alpha(id, image.width, image.height, &image.rgba)?;
        }
        Ok(())
    }
    pub fn preflight_project(&self, project: &motion_model::Project) -> Result<(), RenderError> {
        for layer in &project.layers {
            for (chain_index, e) in layer.effects.iter().filter(|e| e.enabled).enumerate() {
                let location = format!("layer {}, effect {} ({})", layer.id, e.id, e.effect);
                let package = self
                    .registry
                    .resolve(&e.plugin, &e.version, &e.hash)
                    .map_err(|err| RenderError::Context {
                        message: format!("{location}: {err}"), cause: Box::new(RenderError::Effects(err)),
                    })?;
                let definition = package
                    .manifest
                    .effects
                    .iter()
                    .find(|d| d.id == e.effect)
                    .ok_or_else(|| format!("{location}: effect definition missing"))?;
                if matches!(layer.content, motion_model::Content::Adjustment)
                    && definition.renderer != motion_effects::RendererKind::Image
                {
                    return Err(format!(
                        "{location}: adjustment layers support image effects only"
                    ).into());
                }
                if definition.renderer != motion_effects::RendererKind::Image && chain_index != 0 {
                    return Err(format!(
                        "{location}: scene generator must be first enabled effect"
                    ).into());
                }
                if (definition.renderer != motion_effects::RendererKind::Image) != e.scene.is_some() {
                    return Err(format!(
                        "{location}: saved scene contract differs from plugin"
                    ).into());
                }
                if let Some(source) = e.scene.as_ref().and_then(|s| s.source_layer) {
                    if !project.layers.iter().any(|l| l.id == source) {
                        return Err(format!("{location}: source layer {source} missing").into());
                    }
                }
                if definition.compatibility == motion_effects::Compatibility::Unsupported {
                    return Err(format!("{location}: effect is unsupported").into());
                }
                if definition.params.len() != e.params.len() {
                    return Err(format!(
                        "{location}: saved parameter layout differs from the plugin"
                    ).into());
                }
                for p in &definition.params {
                    let saved = e
                        .params
                        .get(&p.id)
                        .ok_or_else(|| format!("{location}: parameter {} missing", p.id))?;
                    if saved.kind != p.kind
                        || saved.animatable != p.animatable
                        || saved.min != p.min
                        || saved.max != p.max
                        || saved.implemented != p.implemented
                    {
                        return Err(format!(
                            "{location}: parameter {} contract differs from the plugin",
                            p.id
                        ).into());
                    }
                    if !p.implemented
                        && (saved.track.value != p.default
                            || saved.track.keys.iter().any(|k| k.value != p.default))
                    {
                        return Err(format!("{location}: parameter {} is not implemented", p.id).into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn new(registry: Registry) -> Result<Self, RenderError> {
        let raster=utility("fn main_fx(p:vec2<f32>)->vec4<f32>{let c=sample_input(p);return vec4(c.rgb*fx.params[0].rgb*fx.params[0].a,c.a*fx.params[0].a);}")?;
        let convert=utility("fn main_fx(p:vec2<f32>)->vec4<f32>{return convert_pixel(sample_input(p),fx.mode.yz,fx.output_mode.xy);}")?;
        let mask_source=utility(crate::mask_plan::APPLY)?;
        Ok(Self {
            precision: ChannelPrecision::Unorm8,
            registry,
            device_dimension: 8192,
            program_errors: Default::default(),
            programs: vec![
                EffectProgram {
                    key: "sdk-raster".into(),
                    shader: raster,
                    package: None,
                    resources: vec![],
                },
                EffectProgram {
                    key: "sdk-convert".into(),
                    shader: convert,
                    package: None,
                    resources: vec![],
                },
                EffectProgram {key:"sdk-mask-source".into(),shader:mask_source,package:None,resources:vec![]},
            ],
            resolved: Vec::new(),
            frame: EffectFramePlan::default(),
            last_scratch_request: None,
            last_resource_rejection: None,
            geometry: crate::PlaneCompositor::new(),
            plane_coverage: false,
            origins: Vec::with_capacity(motion_model::MAX_LAYERS),
            sizes: Vec::with_capacity(motion_model::MAX_LAYERS),
            overlays: Vec::with_capacity(motion_model::MAX_LAYERS),
            alpha_images: Default::default(),
            generator_scratch: Default::default(),
            vector_cache: Default::default(),
            mask_cache: Default::default(),
            cpu_preparation: CpuPreparationInfo { worker_limit: cpu_prepare::default_worker_limit(), ..Default::default() },
            scratch_budget: motion_effects::SCRATCH_BUDGET_FLOOR,
            source_texture_budget: crate::resource_policy::DEFAULT_SOURCE_TEXTURE_BUDGET,
            decoded_image_budget: crate::resource_policy::DEFAULT_DECODED_IMAGE_BUDGET,
            preview_target_budget: crate::resource_policy::MAX_PREVIEW_TARGET_BUDGET,
            session_allocation_budget: crate::allocation_budget::DEFAULT_RENDER_ALLOCATION_BUDGET,
            preview_inputs: None,
            pass_clocks: Vec::new(),
            preview_cache_hits: 0,
            preview_plan_builds: 0,
            fusion: pointwise::FusionState::default(),
            effect_execution: None,
            code_identities: Default::default(),
            #[cfg(all(debug_assertions, feature = "gpu-tests"))]
            execution_fault: None,
        })
    }
    pub(crate) fn enable_plane_coverage(&mut self) {
        if !self.plane_coverage {
            self.plane_coverage = true;
            self.preview_inputs = None;
        }
    }
    pub fn set_registry(&mut self, registry: Registry) {
        self.registry = registry;
        self.resolved.clear();
        self.program_errors.clear();
        self.programs.truncate(3);
        self.fusion.reset_programs();
        self.effect_execution = None;
        self.code_identities.clear();
        self.frame = EffectFramePlan::default();
        self.last_scratch_request = None;
        self.last_resource_rejection = None;
        self.preview_inputs = None;
    }
    pub fn set_precision(&mut self, precision: ChannelPrecision) {
        if self.precision != precision {
            self.precision = precision;
            self.preview_inputs = None;
        }
    }
    pub fn precision(&self) -> ChannelPrecision { self.precision }
    pub fn scratch_budget(&self) -> u64 { self.scratch_budget }
    pub fn memory_policy(&self) -> crate::resource_policy::MemoryPolicy {
        crate::resource_policy::MemoryPolicy {
            session_allocation_budget_bytes: self.session_allocation_budget,
            source_texture_budget_bytes: self.source_texture_budget,
            decoded_image_budget_bytes: self.decoded_image_budget,
            scratch_budget_bytes: self.scratch_budget,
            preview_target_budget_bytes: self.preview_target_budget,
        }
    }
    pub fn set_memory_policy(
        &mut self,
        policy: crate::resource_policy::MemoryPolicy,
    ) -> Result<(), RenderError> {
        policy.validate()?;
        if self.memory_policy() != policy {
            self.preview_inputs = None;
        }
        self.source_texture_budget = policy.source_texture_budget_bytes;
        self.decoded_image_budget = policy.decoded_image_budget_bytes;
        self.scratch_budget = policy.scratch_budget_bytes;
        self.preview_target_budget = policy.preview_target_budget_bytes;
        self.session_allocation_budget = policy.session_allocation_budget_bytes;
        Ok(())
    }
    pub fn set_scratch_budget(&mut self, budget: u64) -> Result<(), RenderError> {
        if !(motion_effects::SCRATCH_BUDGET_FLOOR..=crate::resource_policy::MAX_SCRATCH_BUDGET).contains(&budget) {
            return Err("invalid host scratch memory policy".into());
        }
        if self.scratch_budget != budget { self.preview_inputs = None; }
        self.scratch_budget = budget;
        Ok(())
    }
    pub fn synchronize(&mut self, scene: &Scene) -> Result<(), RenderError> {
        self.resolved.resize_with(scene.effects.len(), || None);
        for (index, e) in scene.effects.iter().enumerate() {
            if self.resolved[index].as_ref().is_some_and(|r| {
                r.signature.0 == e.plugin
                    && r.signature.1 == e.version
                    && r.signature.2 == e.hash
                    && r.signature.3 == e.effect
            }) {
                continue;
            }
            let signature = (
                e.plugin.clone(),
                e.version.clone(),
                e.hash.clone(),
                e.effect.clone(),
            );
            let result = (|| -> Result<Resolved, RenderError> {
                let package = self
                    .registry
                    .resolve(&e.plugin, &e.version, &e.hash)
                    .map_err(RenderError::from)?;
                let definition = package
                    .manifest
                    .effects
                    .iter()
                    .find(|d| d.id == e.effect)
                    .cloned()
                    .ok_or_else(|| format!("effect definition is missing ({})",
                        e.effect.chars().take(96).collect::<String>()))?;
                if (definition.renderer != motion_effects::RendererKind::Image) != e.scene.is_some() {
                    return Err("saved scene contract differs from plugin".into());
                }
                if definition.compatibility == motion_effects::Compatibility::Unsupported {
                    return Err("effect is declared unsupported".into());
                }
                if definition.params.len() != e.param_ids.len() {
                    return Err("saved parameters do not match plugin definition".into());
                }
                let mapping = definition
                    .params
                    .iter()
                    .map(|p| {
                        e.param_ids
                            .iter()
                            .position(|id| id == &p.id)
                            .ok_or("saved parameter ID is missing".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut programs = Vec::new();
                for i in 0..definition.passes.len() {
                    let key = format!("{}:{}:{i}", package.hash, definition.id);
                    let id = if let Some(j) = self.programs.iter().position(|p| p.key == key) {
                        j
                    } else {
                        let shader = Arc::new(package.shaders[&(definition.id.clone(), i)].clone());
                        self.programs.push(EffectProgram {
                            key,
                            shader,
                            package: Some(package.clone()),
                            resources: definition.resources.clone(),
                        });
                        self.programs.len() - 1
                    };
                    programs.push(id as u32);
                }
                let pointwise = motion_effects::pointwise::prove_builtin(&package, &definition)?;
                Ok(Resolved {
                    signature: signature.clone(),
                    semantics: Some(Arc::new(preparation::DefinitionSemantics::new(&definition)?)),
                    definition,
                    programs,
                    mapping,
                    error: None,
                    pointwise,
                })
            })();
            self.resolved[index] = Some(match result {
                Ok(v) => v,
                Err(err) => Resolved {
                    signature,
                    definition: motion_effects::builtin::manifest().effects.remove(0),
                    programs: vec![],
                    mapping: vec![],
                    error: Some(Arc::new(err)),
                    semantics: None,
                    pointwise: motion_effects::pointwise::Eligibility::Retain(
                        motion_effects::pointwise::RetainReason::UnsupportedBoundary),
                },
            });
        }
        Ok(())
    }
    pub fn build(
        &mut self,
        scene: &Scene,
        assets: &[u64],
        width: u32,
        height: u32,
        strict: bool,
    ) -> Result<&EffectFramePlan, RenderError> {
        self.preview_inputs = None;
        self.build_internal(scene, assets, width, height, strict, false, false, &[])
    }


    /// Retained local images must not silently bypass failed effects or reuse
    /// an ordinary preview plan that did not materialize the receiver.
    pub(crate) fn build_materialized_checked(&mut self, scene: &Scene, assets: &[u64],
        width: u32, height: u32, receiver: u64, preview: bool) -> Result<&EffectFramePlan, RenderError> {
        self.preview_inputs = None;
        self.build_internal(scene, assets, width, height, !preview, preview, true, &[receiver])
    }
    /// Strict receiver-center dependencies used by phase-time adjustments.
    pub(crate) fn build_exposure_context_checked(&mut self, scene: &Scene, assets: &[u64],
        width: u32, height: u32, preview: bool) -> Result<&EffectFramePlan, RenderError> {
        self.preview_inputs = None;
        self.build_internal(scene, assets, width, height, !preview, preview, true, &[])
    }
    pub(crate) fn materialized_plane(&self, scene: &Scene, index: usize) -> crate::DrawLayer {
        let mut layer = scene.layers[index].clone();
        let draw = &self.frame.draws[index];
        let size = [draw.words[20], draw.words[21]];
        let overlay = self.overlays[index];
        let origin = self.origins[index];
        layer.model = crate::scene::plane_model(&layer, overlay) * glam::Mat4::from_translation(
            if overlay { glam::Vec3::ZERO } else { glam::Vec3::new(
                origin[0] + (size[0] - layer.size[0]) * 0.5,
                -origin[1] - (size[1] - layer.size[1]) * 0.5, 0.) });
        layer.size = size;
        layer.source_size = size;
        layer.source_rect = [0., 0., size[0], size[1]];
        layer.screen_overlay = overlay;
        layer.view_projection = glam::Mat4::from_cols_array(&std::array::from_fn(|i| draw.words[i]));
        layer.color = [1.; 4];
        layer.asset = None;
        layer.video = None;
        layer.vector = None;
        layer.masks = Default::default();
        layer.track_matte = None;
        layer.composition = false;
        layer.sampling = Default::default();
        layer
    }

    fn reject_budget(&mut self, scope: ResourceBudgetScope, required_bytes: u64, budget_bytes: u64, message: impl Into<String>) -> RenderError {
        let rejection = ResourceRejection::Budget { scope, required_bytes, budget_bytes };
        self.last_resource_rejection = Some(rejection);
        RenderError::Resource { rejection, message: message.into() }
    }
    /// Drop retained preview geometry after an external resource-policy change.
    pub fn invalidate_preview_plan(&mut self) {
        self.preview_inputs = None;
    }
    /// Preview may reduce minified image-effect density. Preview and formal
    /// plans both increase density for magnified image effects; shader packages,
    /// logical coordinates and effect parameter values remain unchanged.
    pub fn build_preview(
        &mut self,
        scene: &Scene,
        assets: &[u64],
        width: u32,
        height: u32,
    ) -> Result<&EffectFramePlan, RenderError> {
        self.build_preview_internal(scene, assets, width, height, false)
    }
    /// Guarded preview refuses visible effect failures without removing effects.
    pub fn build_preview_guarded(&mut self, scene: &Scene, assets: &[u64], width: u32, height: u32, projected: bool) -> Result<&EffectFramePlan, RenderError> {
        if projected {
            self.build_preview_internal(scene, assets, width, height, true)
        } else {
            self.preview_inputs = None;
            self.build_internal(scene, assets, width, height, false, false, true, &[])
        }
    }
    fn build_preview_internal(&mut self, scene: &Scene, assets: &[u64], width: u32, height: u32, guarded: bool) -> Result<&EffectFramePlan, RenderError> {
        self.last_scratch_request = None;
        self.last_resource_rejection = None;
        // Scene generators sample births, camera/light sources and occlusion
        // every frame. Image shader time stays live by updating pass clocks.
        let eligible = scene.effects.iter().all(|e| e.scene.is_none());
        if eligible && self.preview_inputs.as_ref().is_some_and(|inputs|
            inputs.matches(scene, assets, width, height, self.device_dimension, &self.registry, &self.program_errors)) {
            for (pass, &effect) in self.frame.passes.iter_mut().zip(&self.pass_clocks) {
                let Some(effect) = effect else { continue; };
                let local = scene.effects[effect].local_frame;
                pass.uniform.clock[0] = (local / f64::from(scene.fps)) as f32;
                pass.uniform.clock[1] = local as f32;
            }
            self.preview_cache_hits += 1;
            return Ok(&self.frame);
        }
        if self.preview_inputs.as_ref().is_some_and(|inputs| !inputs.registry_matches(&self.registry)) {
            self.resolved.clear();
        }
        self.preview_inputs = None;
        self.preview_plan_builds += 1;
        self.build_internal(scene, assets, width, height, false, true, guarded, &[])?;
        if eligible && self.frame.diagnostics.is_empty() {
            self.preview_inputs = Some(crate::effect_plan_cache::PreviewInputs::capture(
                scene, assets, width, height, self.device_dimension, &self.registry, &self.program_errors));
        }
        Ok(&self.frame)
    }
    fn build_internal(
        &mut self,
        scene: &Scene,
        assets: &[u64],
        width: u32,
        height: u32,
        strict: bool,
        preview: bool,
        guarded: bool,
        materialize: &[u64],
    ) -> Result<&EffectFramePlan, RenderError> {
        self.last_scratch_request = None;
        self.last_resource_rejection = None;
        self.effect_execution = None;
        self.cpu_preparation = CpuPreparationInfo { worker_limit: self.cpu_preparation.worker_limit, ..Default::default() };
        preparation::validate_scene(scene)?;
        self.synchronize(scene)?;
        self.frame.precision = self.precision;
        self.frame.scratch_budget = self.scratch_budget;
        self.frame.sprites.clear();
        self.frame.generator_stats = Default::default();
        self.generator_scratch.retain(scene);
        self.frame.width = 0;
        self.frame.height = 0;
        self.frame.slots = 0;
        self.frame.scratch_sizes = [[0; 2]; 8];
        self.frame.scratch_allocations.clear();
        self.frame.scratch_bindings.clear();
        self.frame.scratch_outputs.clear();
        self.frame.draws.clear();
        self.frame.vectors.clear();
        self.vector_cache.retain(|id, _| {
            scene
                .layers
                .iter()
                .any(|l| l.id == *id && l.vector.is_some())
        });
        self.overlays.clear();
        self.frame.passes.clear();
        self.frame.instance_data.clear();
        self.pass_clocks.clear();
        self.frame.diagnostics.clear();
        if !scene.effects.iter().any(|e| e.enabled) {
            self.frame.width = 0;
            self.frame.height = 0;
            self.frame.slots = 0;
        }
        // Dependencies on hidden/transparent layers are still required by formal output.
        if strict {
            let mut preceding = std::collections::BTreeSet::new();
            for (index, e) in scene.effects.iter().enumerate().filter(|(_, e)| e.enabled) {
                let resolved = self.resolved[index].as_ref().unwrap();
                if resolved.definition.renderer != motion_effects::RendererKind::Image
                    && preceding.contains(&e.layer)
                {
                    return Err(format!(
                        "layer {}, effect {}: scene generator must be first enabled effect",
                        e.layer, e.instance
                    ).into());
                }
                preceding.insert(e.layer);
                if let Some(err) = &resolved.error {
                    return Err(RenderError::Shared(err.clone()).with_diagnostic(format!(
                        "layer {}, effect {}: {}", e.layer, e.instance, err.planning_message())));
                }
                if resolved.definition.renderer != motion_effects::RendererKind::Image {
                    crate::scene_generator::validate_settings(e, resolved.definition.renderer)
                        .map_err(|cause| {
                            let message = format!("layer {}, effect {}: {}", e.layer, e.instance, cause.planning_message());
                            cause.with_diagnostic(message)
                        })?;
                    if let Some(source) = e.scene.as_ref().and_then(|s| s.source_layer) {
                        if scene.world_matrix(source).is_none() {
                            return Err(format!(
                                "layer {}, effect {}: source layer {source} missing",
                                e.layer, e.instance
                            ).into());
                        }
                    }
                }
                for id in &resolved.programs {
                    if let Some(error) = self.program_errors.get(id) {
                        return Err(format!("layer {}, effect {}: {error}", e.layer, e.instance).into());
                    }
                }
                for (i, p) in resolved.definition.params.iter().enumerate() {
                    let value = e.values[resolved.mapping[i]];
                    if !p.kind.valid_value(&value, p.min, p.max)
                        || (!p.implemented && value != p.default)
                    {
                        return Err(format!(
                            "layer {}, effect {}, parameter {} is invalid or unsupported",
                            e.layer, e.instance, p.id
                        ).into());
                    }
                }
            }
        }
        let scale = (width as f32 / scene.width as f32)
            .min(height as f32 / scene.height as f32)
            .min(1.0)
            .max(0.001);
        let layer_scales = self.layer_scales(scene, [width, height], scale, preview);
        let mut preparation = self.prepare_effect_inputs(scene, assets)?;
        let [_, _, viewport_width, viewport_height] = crate::fit_viewport(
            [0., 0., width as f32, height as f32], [scene.width, scene.height]);
        self.frame.masks=self.mask_cache.build(scene,scale,preview)?;
        self.frame.compositing=scene.layers.iter().map(|l|LayerComposite {blend:l.blend.mode.code(),space:u32::from(l.blend.space==motion_model::compositing::BlendSpace::Srgb),matte:l.track_matte.and_then(|m|scene.layers.iter().position(|s|s.id==m.source)).map_or(-1,|i|i as i32),mode:l.track_matte.map_or(0,|m|m.mode.code()),visible:l.composite_visible}).collect();
        for mask in &self.frame.masks {
            if mask.width>self.device_dimension || mask.height>self.device_dimension {
                let rejection = ResourceRejection::TextureDimensions { width: mask.width, height: mask.height, max_dimension: self.device_dimension };
                self.last_resource_rejection = Some(rejection);
                return Err(RenderError::Resource { rejection, message: format!("layer {}, mask {}: raster exceeds device dimensions",scene.layers[mask.layer].id,mask.id) });
            }
        }
        let post_sources:std::collections::HashSet<_>=scene.effects.iter().filter(|e|e.enabled&&scene.layers.iter().any(|l|l.id==e.layer))
            .filter_map(|e| match e.image_input {Some(motion_model::EffectImageInput::Layer{layer,stage:motion_model::EffectImageStage::Effects})=>Some(layer),_=>None})
            .chain(scene.layers.iter().filter_map(|l|l.track_matte.map(|m|m.source)))
            .chain(materialize.iter().copied()).collect();
        self.prepare_vectors(scene, [width, height], &layer_scales)?;
        for (layer_index, layer) in scene.layers.iter().enumerate() {
            let scale = layer_scales[layer_index];
            let asset = assets
                .iter()
                .position(|id| *id == layer.asset.unwrap_or(0))
                .ok_or("image asset is not loaded")?;
            let pass_start = self.frame.passes.len();
            let source_size = if layer.adjustment {
                [scene.width as f32, scene.height as f32]
            } else {
                layer.source_size
            };
            let mut region = if layer.adjustment {
                [0.0, 0.0, source_size[0], source_size[1]]
            } else {
                layer.source_rect
            };
            let mut materialized = false;
            let mut overlay = false;
            let mut additive = false;
            let has_image_effect=scene.effects.iter().enumerate().any(|(i,e)|e.layer==layer.id&&e.enabled&&self.resolved[i].as_ref().is_some_and(|r|r.definition.renderer==motion_effects::RendererKind::Image));
            let starts_with_generator=scene.effects.iter().enumerate().find(|(_,e)|e.layer==layer.id&&e.enabled)
                .is_some_and(|(i,_)|self.resolved[i].as_ref().is_some_and(|r|r.definition.renderer!=motion_effects::RendererKind::Image));
            if !layer.masks.is_empty() && has_image_effect && !starts_with_generator {
                let w=(region[2]*scale).ceil().max(1.) as u32;let h=(region[3]*scale).ceil().max(1.) as u32;
                reserve_scratch(&mut self.frame.scratch_sizes,0,w,h);
                let mut u=crate::mask_plan::uniform(w,h);
                u.size=[region[2],region[3],w as f32,h as f32];u.region=region;u.input_region=region;u.source_region=region;
                u.mode=[0.,0.,0.,1.];u.output_mode=[0.,0.,1.,scale];
                u.params[0]=[linear(layer.color[0]),linear(layer.color[1]),linear(layer.color[2]),layer.color[3]];
                self.frame.passes.push(EffectPass {instance_data:0,resource_input:0,program:2,input:-(asset as i32)-1,source:crate::mask_plan::SOURCE_TOKEN-layer_index as i32,
                    output:0,width:w,height:h,lut:-1,uniform:u,sprite:false,sprite_start:0,sprite_count:0});
                self.pass_clocks.push(None);
                self.frame.slots|=1;self.frame.width=self.frame.width.max(w);self.frame.height=self.frame.height.max(h);materialized=true;
            }
            for &index in &preparation.effects_by_layer[layer_index] {
                let e = &scene.effects[index];
                let clock_start = self.frame.passes.len();
                let data_start = self.frame.instance_data.len();
                let sprite_start = self.frame.sprites.len();
                let generator_stats = self.frame.generator_stats;
                let original_region = region;
                self.last_resource_rejection = None;
                let resolved = self.resolved[index].as_ref().unwrap();
                let result = (|| -> Result<(), RenderError> {
                    let prepared = preparation.inputs[index].take()
                        .ok_or("effect preparation input is absent")??;
                    let (params, resource_input) = prepared.native_uniforms();
                    let def = &resolved.definition;
                    // The SDK wrapper mixes the final result with the original input.
                    // Zero opacity is an exact identity: avoid all conversions, padding,
                    // and shader passes while retaining dependency/parameter validation.
                    if !crate::scene::effect_has_output(e) {
                        return Ok(());
                    }
                    if def.renderer != motion_effects::RendererKind::Image {
                        if !crate::scene::effect_is_screen_generator(e) {
                            return Err("scene generator state missing".into());
                        }
                        if materialized {
                            return Err(
                                "a scene generator must be the first enabled effect on its layer"
                                    .into(),
                            );
                        }
                        let w = ((scene.width as f32 * scale).ceil() as u32).max(self.frame.width);
                        let h =
                            ((scene.height as f32 * scale).ceil() as u32).max(self.frame.height);
                        let slots = self.frame.slots | 129;
                        let mut sizes = self.frame.scratch_sizes;
                        for slot in [0, 7] {
                            reserve_scratch(
                                &mut sizes,
                                slot,
                                (scene.width as f32 * scale).ceil() as u32,
                                (scene.height as f32 * scale).ceil() as u32,
                            );
                        }
                        Self::check_scratch(&sizes, self.device_dimension, self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                        let start = self.frame.sprites.len();
                        let stats = crate::scene_generator::generate(
                            scene,
                            layer,
                            e,
                            def.renderer,
                            &self.alpha_images,
                            &mut self.frame.sprites,
                            &mut self.generator_scratch,
                        )?;
                        self.frame.generator_stats.alive += stats.alive;
                        self.frame.generator_stats.visible += stats.visible;
                        self.frame.generator_stats.culled += stats.culled;
                        self.frame.generator_stats.births_sampled += stats.births_sampled;
                        region = [0., 0., scene.width as f32, scene.height as f32];
                        let sprite_asset = if let Some(image) = e.scene.as_ref().and_then(|s| s.sprite_asset) {
                            assets.iter().position(|id| *id == image).ok_or_else(||format!("particle sprite image {image} is not loaded"))?
                        } else {asset};
                        self.frame.passes.push(EffectPass {
                            instance_data: 0,
                            resource_input: 0,
                            program: resolved.programs[0],
                            input: -(sprite_asset as i32) - 1,
                            source: -(sprite_asset as i32) - 1,
                            output: 7,
                            width: (region[2] * scale).ceil() as u32,
                            height: (region[3] * scale).ceil() as u32,
                            lut: e.lut.map_or(-1, |index| index as i32),
                            uniform: EffectUniform {
                                size: [
                                    region[2],
                                    region[3],
                                    (region[2] * scale).ceil(),
                                    (region[3] * scale).ceil(),
                                ],
                                region,
                                input_region: region,
                                source_region: region,
                                clock: [
                                    (e.local_frame / scene.fps as f64) as f32,
                                    e.local_frame as f32,
                                    0.,
                                    e.seed as f32,
                                ],
                                mode: [0.; 4],
                                output_mode: [0., 0., 1., scale],
                                params,
                            },
                            sprite: true,
                            sprite_start: start as u32,
                            sprite_count: (self.frame.sprites.len() - start) as u32,
                        });
                        let mut conversion = self.frame.passes.last().unwrap().uniform;
                        conversion.output_mode = [0., 0., 1., scale];
                        self.frame.passes.push(EffectPass {
                            instance_data: 0,
                            resource_input: 0,
                            program: 1,
                            input: 7,
                            source: 7,
                            output: 0,
                            width: (region[2] * scale).ceil() as u32,
                            height: (region[3] * scale).ceil() as u32,
                            lut: -1,
                            uniform: conversion,
                            sprite: false,
                            sprite_start: 0,
                            sprite_count: 0,
                        });
                        Self::check_placed_scratch(&self.frame, Some(pass_start..self.frame.passes.len()),
                            sizes, self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                        self.frame.width = w;
                        self.frame.height = h;
                        self.frame.slots = slots;
                        self.frame.scratch_sizes = sizes;
                        materialized = true;
                        overlay = true;
                        additive = def.blend == motion_effects::SpriteBlend::Additive;
                        return Ok(());
                    }
                    let next = prepared.output_region(region)?;
                    let work = prepared.working_slot();
                    let direct_output = prepared.direct_output();
                    let runtime = runtime_template::schedule(def, &resolved.programs, &self.programs,
                        &mut self.code_identities, &scene.composition_id, layer.id, e.instance, self.precision,
                        [(region[2] * scale).ceil() as u32, (region[3] * scale).ceil() as u32],
                        [(next[2] * scale).ceil() as u32, (next[3] * scale).ceil() as u32], work, direct_output)?;
                    let intermediate_passes = resolved.programs.len() - usize::from(direct_output);
                    let work_slots = match intermediate_passes {
                        0 => 1,
                        1 => 3,
                        _ => 7,
                    };
                    let slots = self.frame.slots | 1 | runtime.as_ref().map_or(work_slots << work, |r| r.slots);
                    let w = ((next[2] * scale).ceil() as u32).max(self.frame.width);
                    let h = ((next[3] * scale).ceil() as u32).max(self.frame.height);
                    let mut sizes = self.frame.scratch_sizes;
                    reserve_scratch(
                        &mut sizes,
                        0,
                        (next[2].max(region[2]) * scale).ceil() as u32,
                        (next[3].max(region[3]) * scale).ceil() as u32,
                    );
                    reserve_scratch(
                        &mut sizes,
                        work as usize,
                        (region[2] * scale).ceil() as u32,
                        (region[3] * scale).ceil() as u32,
                    );
                    if let Some(runtime) = &runtime {
                        for pass in &runtime.passes {
                            reserve_scratch(&mut sizes, pass.output as usize,
                                (next[2] * scale).ceil() as u32, (next[3] * scale).ceil() as u32);
                        }
                    } else {
                    for i in 0..intermediate_passes {
                        reserve_scratch(
                            &mut sizes,
                            (work + 1 + i as u32 % 2) as usize,
                            (next[2] * scale).ceil() as u32,
                            (next[3] * scale).ceil() as u32,
                        );
                    }
                    }
                    Self::check_scratch(&sizes, self.device_dimension, self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                    let working = prepared.working();
                    let edge = prepared.edge();
                    let uniform = |out: [f32; 4], input: [f32; 4], source: [f32; 4], pass: f32| {
                        EffectUniform {
                            size: [
                                source_size[0],
                                source_size[1],
                                (out[2] * scale).ceil(),
                                (out[3] * scale).ceil(),
                            ],
                            region: out,
                            input_region: input,
                            source_region: source,
                            clock: [
                                (e.local_frame / f64::from(scene.fps)) as f32,
                                e.local_frame as f32,
                                pass,
                                e.seed as f32,
                            ],
                            mode: [edge, working[0], working[1], 0.0],
                            output_mode: [working[0], working[1], 1.0, scale],
                            params,
                        }
                    };
                    let push = |frame: &mut EffectFramePlan,
                                program: u32,
                                input: i32,
                                source: i32,
                                output: u32,
                                u: EffectUniform,
                                lut: i32| {
                        frame.passes.push(EffectPass {
                            instance_data: 0,
                            resource_input: 0,
                            program,
                            input,
                            source,
                            output,
                            width: u.size[2] as u32,
                            height: u.size[3] as u32,
                            lut,
                            uniform: u,
                            sprite: false,
                            sprite_start: 0,
                            sprite_count: 0,
                        });
                    };
                    if !materialized {
                        let mut u = uniform(region, region, region, 0.0);
                        u.mode = [0.0, 0.0, 0.0, 1.0];
                        u.params[0] = [
                            linear(layer.color[0]),
                            linear(layer.color[1]),
                            linear(layer.color[2]),
                            layer.color[3],
                        ];
                        push(
                            &mut self.frame,
                            0,
                            -(asset as i32) - 1,
                            -(asset as i32) - 1,
                            0,
                            u,
                            -1,
                        );
                    }
                    let mut u = uniform(region, region, region, 0.0);
                    u.mode = [0.0, 0.0, 0.0, 0.0];
                    push(&mut self.frame, 1, 0, 0, work, u, -1);
                    let source_region = region;
                    let mut input = work as i32;
                    let mut input_region = region;
                    let instance_data = if let Some(values) = e.shader_data.as_ref().filter(|v| !v.texture.is_empty()) {
                        self.frame.instance_data.push(values.clone());
                        self.frame.instance_data.len() as u32
                    } else { 0 };
                    if let Some(runtime) = &runtime {
                        for (i, pass) in runtime.passes.iter().enumerate() {
                            let mut u = uniform(next,
                                if pass.input_import { region } else { next },
                                if pass.source_import { region } else { next }, pass.pass as f32);
                            if pass.output == 0 { u.output_mode[0] = 0.; u.output_mode[1] = 0.; }
                            if i + 1 == runtime.passes.len() {
                                u.output_mode[2] = prepared.opacity();
                            }
                            push(&mut self.frame, pass.program, pass.input, pass.source, pass.output,
                                u, e.lut.map_or(-1, |v| v as i32));
                            let native = self.frame.passes.last_mut().unwrap();
                            native.resource_input = resource_input;
                            native.instance_data = instance_data;
                            input = pass.output as i32;
                        }
                    } else {
                    for (i, program) in resolved.programs.iter().enumerate() {
                        let final_direct = direct_output && i + 1 == resolved.programs.len();
                        let output = if final_direct {
                            0
                        } else {
                            work + 1 + (i as u32 % 2)
                        };
                        let mut u = uniform(next, input_region, source_region, i as f32);
                        if final_direct {
                            u.output_mode[0] = 0.;
                            u.output_mode[1] = 0.;
                        }
                        if i + 1 == resolved.programs.len() {
                            u.output_mode[2] = prepared.opacity();
                        }
                        push(
                            &mut self.frame,
                            *program,
                            input,
                            work as i32,
                            output,
                            u,
                            e.lut.map_or(-1, |v| v as i32),
                        );
                        let pass = self.frame.passes.last_mut().unwrap();
                        pass.resource_input = resource_input;
                        pass.instance_data = instance_data;
                        input = output as i32;
                        input_region = next;
                    }
                    }
                    if !direct_output {
                        let mut u = uniform(next, next, source_region, 0.0);
                        u.output_mode = [0.0, 0.0, 1.0, scale];
                        push(&mut self.frame, 1, input, input, 0, u, -1);
                    }
                    Self::check_placed_scratch(&self.frame, Some(pass_start..self.frame.passes.len()),
                        sizes, self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                    self.frame.width = w;
                    self.frame.height = h;
                    self.frame.slots = slots;
                    self.frame.scratch_sizes = sizes;
                    region = next;
                    materialized = true;
                    Ok(())
                })();
                if let Err(err) = result {
                    self.frame.passes.truncate(clock_start);
                    self.frame.instance_data.truncate(data_start);
                    self.frame.sprites.truncate(sprite_start);
                    self.frame.generator_stats = generator_stats;
                    region = original_region;
                    let message = format!(
                        "layer {}, effect {} ({}): {}",
                        layer.id, e.instance, e.effect, err.planning_message()
                    );
                    if strict || guarded || runtime_template::requires_refusal(&err) {
                        return Err(err.with_diagnostic(message));
                    }
                    self.frame.diagnostics.push(message);
                    // A bypassed effect is a diagnostic, not the cause of a later error.
                    self.last_resource_rejection = None;
                } else {
                    self.pass_clocks.extend(std::iter::repeat_n(Some(index), self.frame.passes.len() - clock_start));
                }
            }
            if post_sources.contains(&layer.id)&&!materialized {
                // A retained continuous vector must preserve its projected
                // raster density instead of downsampling through a native utility pass.
                let local_scale = if materialize.contains(&layer.id) && layer.vector.is_some() {
                    crate::raster_density::vector_scale(layer, width, height, scale)?
                } else { scale };
                let w=(region[2]*local_scale).ceil().max(1.) as u32;let h=(region[3]*local_scale).ceil().max(1.) as u32;
                reserve_scratch(&mut self.frame.scratch_sizes,0,w,h);
                Self::check_scratch(&self.frame.scratch_sizes,self.device_dimension,self.scratch_budget,&mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                let mut u=crate::mask_plan::uniform(w,h);u.size=[source_size[0],source_size[1],w as f32,h as f32];
                u.region=region;u.input_region=region;u.source_region=region;u.mode=[0.,0.,0.,1.];u.output_mode=[0.,0.,1.,local_scale];
                u.params[0]=[linear(layer.color[0]),linear(layer.color[1]),linear(layer.color[2]),layer.color[3]];
                let masked=!layer.masks.is_empty();
                self.frame.passes.push(EffectPass {instance_data:0,resource_input:0,program:if masked{2}else{0},input:-(asset as i32)-1,
                    source:if masked{crate::mask_plan::SOURCE_TOKEN-layer_index as i32}else{-(asset as i32)-1},output:0,width:w,height:h,
                    lut:-1,uniform:u,sprite:false,sprite_start:0,sprite_count:0});
                Self::check_placed_scratch(&self.frame, Some(pass_start..self.frame.passes.len()),
                    self.frame.scratch_sizes, self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)?;
                self.pass_clocks.push(None);self.frame.slots|=1;self.frame.width=self.frame.width.max(w);self.frame.height=self.frame.height.max(h);materialized=true;
            }
            let mut words = [0.0; 32];
            self.overlays.push(overlay);
            let mvp = crate::scene::plane_projection(scene, layer, overlay);
            words[..16].copy_from_slice(&mvp.to_cols_array());
            words[16..20].copy_from_slice(&if materialized {
                [1.0; 4]
            } else {
                [
                    linear(layer.color[0]),
                    linear(layer.color[1]),
                    linear(layer.color[2]),
                    layer.color[3],
                ]
            });
            words[20] = region[2];
            words[21] = region[3];
            words[22] = layer.opacity;
            words[23] = if additive { 1. } else { 0. };
            words[24] = if layer.video.is_some() || layer.composition {
                -(layer.order as f32 + 1.0)
            } else {
                asset as f32
            };
            words[25] = 1.0;
            words[26] = 1.0;
            words[27] = if materialized { 0.0 } else { -1.0 };
            words[28] = pass_start as f32;
            words[29] = self.frame.passes.len() as f32;
            words[30] = if layer.composition { 2. } else if layer.video.is_some() { 1. } else { 0. };
            words[31] = if layer.adjustment {
                2.
            } else if layer.vector.is_some() {
                1.
            } else {
                0.
            };
            if layer.adjustment {
                let inverse = if layer.model.determinant().abs() > 1e-8 {
                    layer.model.inverse()
                } else {
                    words[22] = 0.;
                    glam::Mat4::IDENTITY
                };
                words[..16].copy_from_slice(&inverse.to_cols_array());
                words[16..20].copy_from_slice(&region);
                words[20] = layer.size[0];
                words[21] = layer.size[1];
                words[30] = scene.width as f32;
            }
            self.frame.draws.push(PlannedDraw {
                layer: layer.id,
                words,
                pass_start,
                pass_end: self.frame.passes.len(),
            });
        }
        self.lower_effect_execution(scene, materialize)?;
        self.fuse_pointwise(scene)?;
        let layout = if self.frame.passes.is_empty() { scratch::layout(&self.frame, None)? }
            else { Self::check_placed_scratch(&self.frame, None, self.frame.scratch_sizes,
                self.scratch_budget, &mut self.last_scratch_request, &mut self.last_resource_rejection)? };
        self.frame.scratch_allocations = layout.allocations;
        self.frame.scratch_bindings = layout.bindings;
        self.frame.scratch_outputs = layout.outputs;
        if self.frame.width > 0 {
            for (index, d) in self.frame.draws.iter_mut().enumerate() {
                if d.words[27] >= 0.0 {
                    let output = &self.frame.passes[d.pass_end - 1];
                    let id = self.frame.scratch_outputs[index].ok_or("missing final scratch output")?;
                    let allocation = self.frame.scratch_allocations[id as usize];
                    d.words[27] = id as f32;
                    d.words[25] = output.width as f32 / allocation.width as f32;
                    d.words[26] = output.height as f32 / allocation.height as f32;
                }
            }
        }
        self.sizes.clear();
        self.sizes
            .extend(self.frame.draws.iter().map(|d| [d.words[20], d.words[21]]));
        self.origins.clear();
        self.origins.extend(self.frame.draws.iter().zip(&scene.layers).map(|(draw, layer)| {
            self.frame.passes.get(draw.pass_end.wrapping_sub(1))
                .filter(|_| draw.pass_end > draw.pass_start)
                // Vector geometry is already centered on its expanded source
                // rectangle; only the displacement relative to that source
                // belongs in the final plane. Do not apply its origin twice.
                .map_or([0.; 2], |pass| [
                    pass.uniform.region[0] - layer.source_rect[0],
                    pass.uniform.region[1] - layer.source_rect[1],
                ])
        }));
        // Transfer/adjustment layers first composite into the working canvas.
        // Its pixels, rather than a potentially magnified final target, define
        // the fringe needed by the plane fragment shader.
        let accumulated = self.frame.draws.iter().any(|d|d.words[31]==2.&&d.pass_start<d.pass_end)
            || self.frame.compositing.iter().any(|c|c.visible&&(c.blend!=0||c.space!=0));
        let coverage_viewport = if accumulated {
            [(scene.width as f32*scale).ceil(),(scene.height as f32*scale).ceil()]
        } else { [viewport_width, viewport_height] };
        self.geometry.set_coverage_viewport(self.plane_coverage.then_some(coverage_viewport));
        self.geometry
            .prepare_with_bounds_and_overlays(scene, &self.sizes, &self.origins, &self.overlays)
            .map_err(RenderError::from)?;
        self.frame.vertices.clone_from(&self.geometry.vertices);
        self.frame.uv_maps.clone_from(&self.geometry.uv_maps);
        self.frame.batches.clone_from(&self.geometry.batches);
        let vector_bytes = self
            .frame
            .vectors
            .iter()
            .map(|v| u64::from(v.width) * u64::from(v.height)
                * crate::image_formats::color_storage(self.precision).bytes_per_pixel())
            .sum::<u64>();
        if self
            .frame
            .vectors
            .iter()
            .map(|v| v.vertices.len())
            .sum::<usize>()
            > 262144
        {
            return Err("frame vector vertex limit exceeded (262144)".into());
        }
        if vector_bytes > self.source_texture_budget {
            return Err(self.reject_budget(ResourceBudgetScope::VectorSources, vector_bytes, self.source_texture_budget, "vector sources exceed configured source texture budget"));
        }
        if self
            .frame
            .draws
            .iter()
            .any(|d| d.words[31] == 2. && d.pass_start < d.pass_end)
        {
            let accumulator = 2
                * u64::from((scene.width as f32 * scale).ceil() as u32)
                * u64::from((scene.height as f32 * scale).ceil() as u32)
                * crate::image_formats::color_storage(self.precision).bytes_per_pixel();
            if accumulator > self.source_texture_budget {
                return Err(self.reject_budget(ResourceBudgetScope::LayerCompositing, accumulator, self.source_texture_budget, format!(
                    "adjustment accumulators require {:.2} MiB; composite source budget exceeded",
                    accumulator as f64 / 1048576.0
                )));
            }
        }
        let (mw,mh,slots)=crate::mask_plan::scratch_dimensions(&self.frame.masks);
        let coverage_bytes=motion_effects::image_contract::coverage_storage(self.precision).bytes_per_pixel();
        let mask_scratch=u64::from(mw)*u64::from(mh)*slots as u64*coverage_bytes;
        let mask_outputs=self.frame.masks.chunk_by(|a,b|a.layer==b.layer).map(|g|u64::from(g[0].width)*u64::from(g[0].height)*coverage_bytes).sum::<u64>();
        if mask_outputs>self.source_texture_budget {return Err(self.reject_budget(ResourceBudgetScope::MaskOutputs, mask_outputs, self.source_texture_budget, "layer mask outputs exceed configured source texture budget"));}
        let scratch = mask_scratch+self.frame.scratch_bytes();
        if scratch>self.scratch_budget {
            return Err(self.reject_budget(ResourceBudgetScope::MaskAndEffectScratch, scratch, self.scratch_budget, format!("layer mask and effect scratch textures require {:.2} MiB; budget is {} MiB", scratch as f64/1048576., self.scratch_budget/1048576)));
        }
        Ok(&self.frame)
    }
}
