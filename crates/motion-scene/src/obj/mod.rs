//! Static Wavefront OBJ/MTL ingestion into owned scene resources.
mod material;
mod parse;
mod polygon;
mod text;

use crate::{AssetResolver, ImportContext, ImportLimits, Result, Scene};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpAxis {
    Y,
    Z,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Handedness {
    Right,
    Left,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialColorSpace {
    Linear,
    Srgb,
}

/// These are explicit import choices, not guarantees made by the OBJ format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjOptions {
    pub metres_per_unit: f32,
    pub up_axis: UpAxis,
    pub handedness: Handedness,
    /// Convert bottom-origin OBJ UVs into top-origin glTF texture coordinates.
    pub flip_v: bool,
    pub material_color_space: MaterialColorSpace,
}
impl Default for ObjOptions {
    fn default() -> Self {
        Self {
            metres_per_unit: 1.,
            up_axis: UpAxis::Y,
            handedness: Handedness::Right,
            flip_v: true,
            material_color_space: MaterialColorSpace::Linear,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MapKind {
    Ambient,
    Diffuse,
    Specular,
    Emissive,
    Opacity,
    Exponent,
    Bump,
    Normal,
    Displacement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WavefrontMap {
    pub texture: usize,
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub clamp: bool,
    pub bump_multiplier: f32,
    pub channel: Option<char>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WavefrontMaterial {
    pub name: String,
    /// Original authored RGB values; `color_space` declares the import interpretation.
    pub color_space: MaterialColorSpace,
    pub ambient: Option<[f32; 3]>,
    pub diffuse: Option<[f32; 3]>,
    pub specular: Option<[f32; 3]>,
    pub emissive: Option<[f32; 3]>,
    pub transmission: Option<[f32; 3]>,
    pub specular_exponent: Option<f32>,
    pub optical_density: Option<f32>,
    pub dissolve: Option<f32>,
    pub dissolve_halo: bool,
    pub illumination: Option<u8>,
    pub maps: BTreeMap<MapKind, WavefrontMap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveInfo {
    pub mesh: usize,
    pub primitive: usize,
    pub object: String,
    pub groups: Vec<String>,
    pub smoothing_group: Option<u32>,
    pub first_line: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjMetadata {
    pub options: ObjOptions,
    pub primitives: Vec<PrimitiveInfo>,
    /// Nonplanar faces use dominant-axis triangulation while retaining 3D positions.
    pub projected_nonplanar_faces: usize,
}

/// Import a static polygon scene. No commands/scripts, network reads or freeform evaluation.
pub fn import_obj(
    context: ImportContext,
    source: &[u8],
    resolver: &mut impl AssetResolver,
    options: &ObjOptions,
    limits: &ImportLimits,
) -> Result<Scene> {
    let began = std::time::Instant::now();
    log::info!(
        "start #{} obj_import source={}",
        context.request,
        context.source
    );
    let result = parse::load(source, resolver, options, limits);
    match &result {
        Ok(scene) => {
            if let Some(metadata) = scene.obj_metadata() {
                if metadata.projected_nonplanar_faces > 0 {
                    log::warn!(
                        "obj_import #{} source={} projected_nonplanar_faces={}",
                        context.request,
                        context.source,
                        metadata.projected_nonplanar_faces
                    );
                }
            }
            log::debug!(
                "done #{} obj_import source={} elapsed_ms={} retained_bytes={}",
                context.request,
                context.source,
                began.elapsed().as_millis(),
                scene.retained_bytes()
            );
        }
        Err(error) => {
            let cause: String = error.to_string().chars().take(1024).collect();
            log::error!(
                "obj_import #{} source={} elapsed_ms={} cause={}",
                context.request,
                context.source,
                began.elapsed().as_millis(),
                cause
            );
        }
    }
    result
}
