//! Independently authored glTF geometry and camera; no external assets.
use motion_core::{Content, ImportedCamera, Layer, ModelAnimation, ModelAsset, ModelClip, Project};
use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io, path::Path};

struct Buffer(Vec<u8>);
impl AssetResolver for Buffer {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "mesh.bin");
        assert!(self.0.len() <= limit);
        Ok(self.0.clone())
    }
}
pub fn project(root: &Path, fixed_aspect: bool) -> Project {
    let bytes: Vec<u8> = [
        -0.8f32, -0.6, 0., 0.8, -0.6, 0., -0.4, 0.8, 0., 0., 2., 0., 0., 3., 1., 0., 3.,
    ]
    .into_iter()
    .flat_map(f32::to_le_bytes)
    .collect();
    let mut projection = json!({"yfov":1.,"znear":0.1,"zfar":10.});
    if fixed_aspect {
        projection["aspectRatio"] = json!(1.);
    }
    let source = serde_json::to_vec(&json!({
        "asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":bytes.len()}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":8},
            {"buffer":0,"byteOffset":44,"byteLength":24}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-0.8,-0.6,0],"max":[0.8,0.8,0]},
            {"bufferView":1,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[2]},
            {"bufferView":2,"componentType":5126,"count":2,"type":"VEC3"}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0,0,0,1]},"emissiveFactor":[1,0,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
        "cameras":[{"type":"perspective","perspective":projection}],
        "nodes":[{"mesh":0},{"camera":0,"translation":[0,0,3]}],
        "scenes":[{"nodes":[0,1]}],"scene":0,
        "animations":[{"samplers":[{"input":1,"output":2}],
            "channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}]
    })).unwrap();
    let scene = motion_scene::import(
        ImportContext {
            request: 8710,
            source: 7,
        },
        &source,
        &mut Buffer(bytes),
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = scene
        .freeze(
            &source,
            motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
        )
        .unwrap();
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::write(root.join("assets/models/camera.mscn"), &frozen).unwrap();
    let mut project = Project::new(96, 64, 30, 61).unwrap();
    project.background = [0.; 4];
    project.model_assets.push(ModelAsset {
        id: 7,
        path: "assets/models/camera.mscn".into(),
        bytes: frozen.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&frozen)),
    });
    project.camera.created = true;
    project.camera.imported = Some(ImportedCamera {
        asset: 7,
        scene: 0,
        node: 1,
        pixels_per_metre: 20.,
        pivot: [0.; 3],
        animation: Some(ModelAnimation {
            clip: 0,
            offset_seconds: 0.,
            speed: 1.,
        }),
    });
    let mut model = Layer::solid(
        1,
        "Camera fixture triangle",
        [40.; 2],
        [48., 32., 0.],
        [1.; 4],
    );
    model.three_d = true;
    model.content = Content::Model {
        model: ModelClip {
            asset: 7,
            scene: 0,
            pixels_per_metre: 20.,
            pivot: [0.; 3],
            samples: 1,
            animation: None,
        },
    };
    project.layers.push(model);
    project.validate().unwrap();
    project
}
