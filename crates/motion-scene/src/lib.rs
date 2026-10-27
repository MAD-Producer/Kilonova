//! Owned scene resources and bounded, CPU-only glTF/GLB and OBJ/MTL ingestion.
//! Importing a scene does not imply that the desktop renderer supports it.
mod accessors;
mod animation;
mod camera;
mod container;
mod decode;
mod frozen;
mod import;
pub mod obj;
mod resources;
mod schema;

pub use animation::{Animation, AnimationSample};
pub use camera::{Camera, CameraKind, CameraView};
pub use frozen::import_frozen;
pub use import::{import, ImportContext, ImportLimits};
pub use resources::{AssetResolver, DirectoryResolver, Resource, ResourceKind};

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
#[error("scene import phase={phase} object={object}: {cause}")]
pub struct Error {
    pub phase: &'static str,
    pub object: String,
    #[source]
    pub cause: Cause,
}

#[derive(Debug, thiserror::Error)]
pub enum Cause {
    #[error("{0}")]
    Invalid(String),
    #[error("unsupported capability: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

fn invalid(phase: &'static str, object: impl ToString, reason: impl Into<String>) -> Error {
    Error {
        phase,
        object: object.to_string(),
        cause: Cause::Invalid(reason.into()),
    }
}

fn unsupported(phase: &'static str, object: impl ToString, reason: impl Into<String>) -> Error {
    Error {
        phase,
        object: object.to_string(),
        cause: Cause::Unsupported(reason.into()),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    /// Missing normals require flat shading. OBJ smoothing groups are resolved on import.
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub texcoords: [Option<Vec<[f32; 2]>>; 2],
    pub colors: Option<Vec<[f32; 4]>>,
    /// Triangle list in canonical right-handed coordinates, retaining authored front faces.
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub primitives: Vec<Primitive>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Column-major local transform, retained as authored; never decomposed to Euler angles.
    pub local: [f32; 16],
    pub mesh: Option<usize>,
    pub camera: Option<usize>,
    pub children: Vec<usize>,
    pub(crate) rest: Option<animation::Transform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureSlot {
    pub texture: usize,
    pub texcoord: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Keeps classic OBJ/MTL distinct from glTF PBR; no implicit Phong-to-PBR conversion.
    pub model: MaterialModel,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub alpha: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// Base color/emissive textures use sRGB; all other texture slots contain linear data.
    pub base_color_texture: Option<TextureSlot>,
    pub metallic_roughness_texture: Option<TextureSlot>,
    pub normal_texture: Option<TextureSlot>,
    pub normal_scale: f32,
    pub occlusion_texture: Option<TextureSlot>,
    pub occlusion_strength: f32,
    pub emissive_texture: Option<TextureSlot>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            model: MaterialModel::MetallicRoughness,
            base_color: [1.; 4],
            metallic: 1.,
            roughness: 1.,
            emissive: [0.; 3],
            alpha: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.,
            occlusion_texture: None,
            occlusion_strength: 1.,
            emissive_texture: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MaterialModel {
    MetallicRoughness,
    Wavefront(Box<obj::WavefrontMaterial>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceFormat {
    Gltf,
    Obj,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sampler {
    /// glTF constants, preserved exactly; absent min/mag select renderer defaults later.
    pub mag_filter: Option<u32>,
    pub min_filter: Option<u32>,
    pub wrap_s: u32,
    pub wrap_t: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Texture {
    pub image: usize,
    pub sampler: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub resource: Resource,
    pub mime_type: String,
}

/// Validated on import. All dependent bytes are owned; rendering never reopens URIs.
/// Fields are read-only through this API. A future package decoder must revalidate them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Scene {
    pub(crate) format: SourceFormat,
    pub(crate) obj_metadata: Option<obj::ObjMetadata>,
    pub(crate) extra_resources: Vec<Resource>,
    pub(crate) source_sha256: [u8; 32],
    pub(crate) buffers: Vec<Resource>,
    pub(crate) images: Vec<Image>,
    pub(crate) textures: Vec<Texture>,
    pub(crate) samplers: Vec<Sampler>,
    pub(crate) materials: Vec<Material>,
    pub(crate) meshes: Vec<Mesh>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) animations: Vec<Animation>,
    pub(crate) cameras: Vec<Camera>,
    pub(crate) scenes: Vec<Vec<usize>>,
    pub(crate) default_scene: Option<usize>,
    pub(crate) ignored_optional_extensions: Vec<String>,
    pub(crate) retained_bytes: usize,
}

impl Scene {
    pub fn source_format(&self) -> SourceFormat {
        self.format
    }
    pub fn obj_metadata(&self) -> Option<&obj::ObjMetadata> {
        self.obj_metadata.as_ref()
    }
    pub fn extra_resources(&self) -> &[Resource] {
        &self.extra_resources
    }
    pub fn source_sha256(&self) -> [u8; 32] {
        self.source_sha256
    }
    pub fn buffers(&self) -> &[Resource] {
        &self.buffers
    }
    pub fn images(&self) -> &[Image] {
        &self.images
    }
    pub fn textures(&self) -> &[Texture] {
        &self.textures
    }
    pub fn samplers(&self) -> &[Sampler] {
        &self.samplers
    }
    pub fn materials(&self) -> &[Material] {
        &self.materials
    }
    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn animations(&self) -> &[Animation] {
        &self.animations
    }
    pub fn cameras(&self) -> &[Camera] {
        &self.cameras
    }
    pub fn scenes(&self) -> &[Vec<usize>] {
        &self.scenes
    }
    pub fn default_scene(&self) -> Option<usize> {
        self.default_scene
    }
    pub fn ignored_optional_extensions(&self) -> &[String] {
        &self.ignored_optional_extensions
    }
    /// Payload bytes, excluding allocator/serde overhead; import limits are not a process RAM cap.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// World matrices in glTF's right-handed, +Y-up metre coordinates.
    /// Traverses only the selected scene; a mesh library is not implicitly instanced.
    pub fn instances(&self, scene: usize, pixels_per_metre: f32) -> Result<Vec<(usize, Mat4)>> {
        self.instances_at(scene, pixels_per_metre, None)
    }

    /// Explicit clip and time; no implicit autoplay, looping, or blending of clips.
    pub fn instances_at(
        &self,
        scene: usize,
        pixels_per_metre: f32,
        sample: Option<AnimationSample>,
    ) -> Result<Vec<(usize, Mat4)>> {
        let mut out = Vec::new();
        self.visit_node_instances_at(scene, pixels_per_metre, sample, |index, world| {
            if let Some(mesh) = self.nodes[index].mesh { out.push((mesh, world)); }
        })?;
        Ok(out)
    }
    fn visit_node_instances_at(
        &self,
        scene: usize,
        pixels_per_metre: f32,
        sample: Option<AnimationSample>,
        mut visit: impl FnMut(usize, Mat4),
    ) -> Result<()> {
        if !pixels_per_metre.is_finite() || pixels_per_metre <= 0. {
            return Err(invalid(
                "instances",
                scene,
                "pixels per metre must be finite and positive",
            ));
        }
        let roots = self
            .scenes
            .get(scene)
            .ok_or_else(|| invalid("instances", scene, "scene index out of range"))?;
        let locals = sample
            .map(|sample| self.sample_animation(sample))
            .transpose()?;
        let scale = Mat4::from_scale(Vec3::splat(pixels_per_metre));
        let mut pending: Vec<_> = roots.iter().rev().map(|&n| (n, scale)).collect();
        while let Some((index, parent)) = pending.pop() {
            let node = &self.nodes[index];
            let local = locals.as_ref().map_or(node.local, |locals| locals[index]);
            let world = parent * Mat4::from_cols_array(&local);
            if !world.is_finite() {
                return Err(invalid("instances", index, "world transform overflow"));
            }
            visit(index, world);
            pending.extend(node.children.iter().rev().map(|&child| (child, world)));
        }
        Ok(())
    }
}
