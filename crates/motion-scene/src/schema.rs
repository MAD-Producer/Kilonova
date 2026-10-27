//! glTF defaults here are format-defined optional semantics, not project migrations.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Root {
    pub asset: Asset,
    #[serde(default)]
    pub buffers: Vec<Buffer>,
    #[serde(default)]
    pub buffer_views: Vec<View>,
    #[serde(default)]
    pub accessors: Vec<Accessor>,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub textures: Vec<Texture>,
    #[serde(default)]
    pub samplers: Vec<Sampler>,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub meshes: Vec<Mesh>,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    pub scene: Option<usize>,
    #[serde(default)]
    pub extensions_used: Vec<String>,
    #[serde(default)]
    pub extensions_required: Vec<String>,
    #[serde(default)]
    pub animations: Vec<Animation>,
    #[serde(default)]
    pub skins: Vec<Skin>,
    #[serde(default)]
    pub cameras: Vec<Camera>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Option<usize>,
    pub skeleton: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct Camera {
    #[serde(rename = "type")]
    pub kind: String,
    pub perspective: Option<Perspective>,
    pub orthographic: Option<Orthographic>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Perspective {
    pub aspect_ratio: Option<f32>,
    pub yfov: f32,
    pub znear: f32,
    pub zfar: Option<f32>,
}
#[derive(Deserialize)]
pub(crate) struct Orthographic {
    pub xmag: f32,
    pub ymag: f32,
    pub znear: f32,
    pub zfar: f32,
}

#[derive(Deserialize)]
pub(crate) struct Animation {
    pub name: Option<String>,
    pub channels: Vec<AnimationChannel>,
    pub samplers: Vec<AnimationSampler>,
}
#[derive(Deserialize)]
pub(crate) struct AnimationChannel {
    pub sampler: usize,
    pub target: AnimationTarget,
}
#[derive(Deserialize)]
pub(crate) struct AnimationTarget {
    pub node: Option<usize>,
    pub path: String,
}
#[derive(Deserialize)]
pub(crate) struct AnimationSampler {
    pub input: usize,
    pub output: usize,
    #[serde(default = "linear")]
    pub interpolation: String,
}
fn linear() -> String {
    "LINEAR".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Asset {
    pub version: String,
    pub min_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Buffer {
    pub byte_length: usize,
    pub uri: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub buffer: usize,
    #[serde(default)]
    pub byte_offset: usize,
    pub byte_length: usize,
    pub byte_stride: Option<usize>,
    pub target: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Accessor {
    pub buffer_view: Option<usize>,
    #[serde(default)]
    pub byte_offset: usize,
    pub component_type: u32,
    pub count: usize,
    #[serde(rename = "type")]
    pub shape: String,
    #[serde(default)]
    pub normalized: bool,
    pub sparse: Option<Sparse>,
    pub min: Option<Vec<f32>>,
    pub max: Option<Vec<f32>>,
}

#[derive(Deserialize)]
pub(crate) struct Sparse {
    pub count: usize,
    pub indices: SparseIndices,
    pub values: SparseValues,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SparseIndices {
    pub buffer_view: usize,
    #[serde(default)]
    pub byte_offset: usize,
    pub component_type: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SparseValues {
    pub buffer_view: usize,
    #[serde(default)]
    pub byte_offset: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Image {
    pub uri: Option<String>,
    pub buffer_view: Option<usize>,
    pub mime_type: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Texture {
    pub source: Option<usize>,
    pub sampler: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sampler {
    pub mag_filter: Option<u32>,
    pub min_filter: Option<u32>,
    #[serde(default = "repeat")]
    pub wrap_s: u32,
    #[serde(default = "repeat")]
    pub wrap_t: u32,
}
fn repeat() -> u32 {
    10497
}
fn one() -> f32 {
    1.
}
fn half() -> f32 {
    0.5
}
fn white() -> [f32; 4] {
    [1.; 4]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Slot {
    pub index: usize,
    #[serde(default)]
    pub tex_coord: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NormalSlot {
    #[serde(flatten)]
    pub slot: Slot,
    #[serde(default = "one")]
    pub scale: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OcclusionSlot {
    #[serde(flatten)]
    pub slot: Slot,
    #[serde(default = "one")]
    pub strength: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pbr {
    #[serde(default = "white")]
    pub base_color_factor: [f32; 4],
    #[serde(default = "one")]
    pub metallic_factor: f32,
    #[serde(default = "one")]
    pub roughness_factor: f32,
    pub base_color_texture: Option<Slot>,
    pub metallic_roughness_texture: Option<Slot>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Material {
    pub pbr_metallic_roughness: Option<Pbr>,
    pub normal_texture: Option<NormalSlot>,
    pub occlusion_texture: Option<OcclusionSlot>,
    pub emissive_texture: Option<Slot>,
    #[serde(default)]
    pub emissive_factor: [f32; 3],
    pub alpha_mode: Option<String>,
    #[serde(default = "half")]
    pub alpha_cutoff: f32,
    #[serde(default)]
    pub double_sided: bool,
}

#[derive(Deserialize)]
pub(crate) struct Primitive {
    pub attributes: BTreeMap<String, usize>,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    pub mode: Option<u32>,
    #[serde(default)]
    pub targets: Vec<BTreeMap<String, usize>>,
}

#[derive(Deserialize)]
pub(crate) struct Mesh {
    pub primitives: Vec<Primitive>,
    pub weights: Option<Vec<f32>>,
}

#[derive(Deserialize)]
pub(crate) struct Node {
    pub matrix: Option<[f32; 16]>,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub camera: Option<usize>,
    pub weights: Option<Vec<f32>>,
    #[serde(default)]
    pub children: Vec<usize>,
}

#[derive(Deserialize)]
pub(crate) struct Scene {
    #[serde(default)]
    pub nodes: Vec<usize>,
}
