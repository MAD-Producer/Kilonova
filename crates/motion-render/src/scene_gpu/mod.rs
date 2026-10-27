//! Native static scene rendering. Owned inputs never reopen source paths.
mod images;
mod materials;
mod meshes;
mod render;

use crate::{AdmittedBuffer, RenderError, Renderer};
use bytemuck::{Pod, Zeroable};
pub use render::{SceneFrame, SceneLight, ScenePass, ScenePassFrame, ScenePassInstance, SceneRenderStats};
use std::{collections::HashMap, sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug)]
pub struct SceneRequest {
    pub request: u64,
    pub source: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneGpuLimits {
    /// Per-image transient CPU pixel allowance; not a process or codec RAM cap.
    pub decoded_image_bytes: u64,
    /// Logical descriptors for meshes, image mips and material constants.
    /// Frame uniforms, color/depth targets also join the renderer's shared cap.
    pub resident_bytes: u64,
    /// Physical material-image storage, including mips and fallback variants.
    /// Frame color/depth targets use the shared native allocation budget.
    pub source_texture_bytes: u64,
    pub max_draws: usize,
    pub max_image_edge: u32,
}
impl Default for SceneGpuLimits {
    fn default() -> Self {
        Self {
            decoded_image_bytes: 128 << 20,
            resident_bytes: 256 << 20,
            source_texture_bytes: 128 << 20,
            max_draws: 16_384,
            max_image_edge: 8192,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SceneGpuError {
    #[error("scene pass instance={instance} request=#{request} source={source_id}: {cause}")]
    PassInstance {
        instance: usize,
        request: u64,
        source_id: u64,
        #[source]
        cause: Box<SceneGpuError>,
    },
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("scene image={image} phase=decode: {cause}")]
    Image {
        image: usize,
        #[source]
        cause: image::ImageError,
    },
    #[error("scene GPU phase={phase} object={object}: {reason}")]
    Invalid {
        phase: &'static str,
        object: usize,
        reason: &'static str,
    },
    #[error("scene GPU unsupported object={object}: {capability}")]
    Unsupported {
        object: usize,
        capability: &'static str,
    },
    #[error(transparent)]
    Source(#[from] motion_scene::Error),
    #[error("scene native GPU: {0}")]
    Native(#[from] wgpu::Error),
    #[error("scene resident capacity required_bytes={required_bytes} budget_bytes={budget_bytes}")]
    ResidentBudget {
        required_bytes: u64,
        budget_bytes: u64,
    },
}
impl SceneGpuError {
    pub fn resource_rejection(&self) -> Option<&crate::ResourceRejection> {
        match self {
            Self::Render(cause) => cause.resource_rejection(),
            Self::PassInstance { cause, .. } => cause.resource_rejection(),
            _ => None,
        }
    }
}
pub type Result<T> = std::result::Result<T, SceneGpuError>;
fn invalid(phase: &'static str, object: usize, reason: &'static str) -> SceneGpuError {
    SceneGpuError::Invalid {
        phase,
        object,
        reason,
    }
}
fn unsupported(object: usize, capability: &'static str) -> SceneGpuError {
    SceneGpuError::Unsupported { object, capability }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    uv0: [f32; 2],
    uv1: [f32; 2],
    color: [f32; 4],
}
struct MeshBuffers {
    vertices: AdmittedBuffer,
    indices: AdmittedBuffer,
    index_count: u32,
    material: usize,
    normal: bool,
    tangent: bool,
    center: glam::Vec3,
}

/// Opaque admission-owned resources tied to one native renderer queue.
/// Use allocated encoders/submissions to retain their reservations through GPU completion.
pub struct GpuScene {
    owner: Arc<()>,
    thread: std::thread::ThreadId,
    source: Arc<motion_scene::Scene>,
    meshes: Vec<Vec<MeshBuffers>>,
    materials: Vec<materials::GpuMaterial>,
    material_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    images: images::Images,
    pipelines: HashMap<render::PipelineKey, wgpu::RenderPipeline>,
    targets: Option<render::Targets>,
    limits: SceneGpuLimits,
    resident_bytes: u64,
    render_failures: u8,
}
impl GpuScene {
    pub fn source(&self) -> &motion_scene::Scene {
        &self.source
    }
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }
    /// Native image allocations, including fallback pixels and color/data variants.
    pub fn texture_count(&self) -> usize {
        self.images.len()
    }
    pub fn source_texture_bytes(&self) -> u64 {
        self.images.bytes()
    }
    /// Logical native texture capacity; does not allocate observation metadata.
    pub fn texture_bytes(&self) -> u64 {
        self.images.bytes() + self.targets.as_ref().map_or(0, render::Targets::bytes)
    }
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        self.images.collect(&mut ledger);
        if let Some(targets) = &self.targets {
            targets.collect(&mut ledger);
        }
        ledger
    }
    /// Start is emitted before decoder/device entry. Host workers must publish
    /// their own in-flight state before calling this synchronous preparation API.
    pub fn upload(
        renderer: &Renderer,
        request: SceneRequest,
        source: Arc<motion_scene::Scene>,
        limits: SceneGpuLimits,
    ) -> Result<Self> {
        let began = Instant::now();
        log::info!(
            "start #{} scene_gpu_upload source={}",
            request.request,
            request.source
        );
        let result = Self::prepare(renderer, source, limits);
        match &result {
            Ok(scene) => log::debug!(
                "done #{} scene_gpu_upload source={} elapsed_ms={} resident_bytes={}",
                request.request,
                request.source,
                began.elapsed().as_millis(),
                scene.resident_bytes
            ),
            Err(cause) => log_failure(request, "scene_gpu_upload", began, cause),
        }
        result
    }
    fn prepare(
        renderer: &Renderer,
        source: Arc<motion_scene::Scene>,
        limits: SceneGpuLimits,
    ) -> Result<Self> {
        renderer.check_health()?;
        if limits.resident_bytes == 0
            || limits.decoded_image_bytes == 0
            || limits.max_draws == 0
            || limits.max_image_edge == 0
        {
            return Err(invalid("limits", 0, "scene GPU limits must be positive"));
        }
        if source.materials().len() > limits.max_draws
            || source
                .meshes()
                .iter()
                .map(|m| m.primitives.len())
                .sum::<usize>()
                > limits.max_draws
        {
            return Err(invalid("limits", 0, "scene descriptor limit exceeded"));
        }
        let context = renderer.allocation_context();
        let mut bytes = 0;
        let material_layout = materials::layout(context.device());
        let draw_layout = render::layout(context.device());
        let mut images = images::Images::new();
        let materials = materials::prepare(
            &context,
            &source,
            &mut images,
            &material_layout,
            limits,
            &mut bytes,
        )?;
        let meshes = meshes::prepare(&context, &source, limits, &mut bytes)?;
        Ok(Self {
            owner: renderer.allocation_owner.clone(),
            thread: std::thread::current().id(),
            source,
            meshes,
            materials,
            material_layout,
            draw_layout,
            images,
            pipelines: HashMap::new(),
            targets: None,
            limits,
            resident_bytes: bytes,
            render_failures: 0,
        })
    }
}
fn charge(total: &mut u64, bytes: u64, limits: SceneGpuLimits) -> Result<()> {
    let required = total
        .checked_add(bytes)
        .ok_or_else(|| invalid("admission", 0, "scene capacity overflow"))?;
    if required > limits.resident_bytes {
        return Err(SceneGpuError::ResidentBudget {
            required_bytes: required,
            budget_bytes: limits.resident_bytes,
        });
    }
    *total = required;
    Ok(())
}
fn log_failure(request: SceneRequest, operation: &str, began: Instant, cause: &SceneGpuError) {
    let bounded: String = cause.to_string().chars().take(1024).collect();
    log::error!(
        "{} #{} source={} elapsed_ms={} cause={}",
        operation,
        request.request,
        request.source,
        began.elapsed().as_millis(),
        bounded
    );
}
