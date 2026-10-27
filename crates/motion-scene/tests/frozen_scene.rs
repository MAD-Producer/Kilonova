use motion_scene::{
    import, import_frozen, AssetResolver, ImportContext, ImportLimits, ResourceKind,
};
use serde_json::json;
use std::io;

struct Bytes(Vec<u8>);
impl AssetResolver for Bytes {
    fn read(&mut self, path: &str, _: ResourceKind, max: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "mesh.bin");
        assert!(max >= self.0.len());
        Ok(self.0.clone())
    }
}
fn fixture() -> (Vec<u8>, motion_scene::Scene) {
    let bytes: Vec<_> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let source=serde_json::to_vec(&json!({"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":36}],
        "bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"type":"VEC3","count":3,"min":[0,0,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0})).unwrap();
    let scene = import(
        ImportContext {
            request: 1,
            source: 87,
        },
        &source,
        &mut Bytes(bytes),
        &ImportLimits::default(),
    )
    .unwrap();
    (source, scene)
}

#[test]
fn self_contained_bundle_roundtrips_without_original_files_and_is_deterministic() {
    let (source, scene) = fixture();
    let bundle = scene.freeze(&source, 1 << 20).unwrap();
    assert_eq!(scene.freeze(&source, 1 << 20).unwrap(), bundle);
    let restored = import_frozen(
        ImportContext {
            request: 2,
            source: 87,
        },
        &bundle,
        1 << 20,
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(restored, scene);
    assert_eq!(restored.freeze(&source, 1 << 20).unwrap(), bundle);
    assert!(scene.freeze(b"wrong document", 1 << 20).is_err());
}

#[test]
fn snapshot_checks_source_and_resource_digests_lengths_and_exact_version() {
    let (source, scene) = fixture();
    let bundle = scene.freeze(&source, 1 << 20).unwrap();
    let read = |b: &[u8]| {
        import_frozen(
            ImportContext {
                request: 3,
                source: 87,
            },
            b,
            1 << 20,
            &ImportLimits::default(),
        )
    };
    for length in [0, 7, 8, 16, 47, 48, bundle.len() - 1] {
        assert!(read(&bundle[..length]).is_err());
    }
    let mut bad = bundle.clone();
    bad[7] = 2;
    assert!(read(&bad).unwrap_err().to_string().contains("version"));
    let mut bad = bundle.clone();
    bad[48] ^= 1;
    assert!(read(&bad)
        .unwrap_err()
        .to_string()
        .contains("source digest"));
    let mut bad = bundle.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(read(&bad)
        .unwrap_err()
        .to_string()
        .contains("resource digest"));
    let mut bad = bundle.clone();
    bad[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(read(&bad).is_err());
    let mut bad = bundle;
    bad.push(0);
    assert!(read(&bad).unwrap_err().to_string().contains("trailing"));
}

#[test]
fn bundle_and_import_limits_are_enforced_on_both_sides_of_snapshot() {
    let (source, scene) = fixture();
    let bundle = scene.freeze(&source, 1 << 20).unwrap();
    assert!(scene.freeze(&source, bundle.len() - 1).is_err());
    assert!(import_frozen(
        ImportContext {
            request: 4,
            source: 87
        },
        &bundle,
        bundle.len() - 1,
        &ImportLimits::default()
    )
    .is_err());
    assert!(import_frozen(
        ImportContext {
            request: 5,
            source: 87
        },
        &bundle,
        bundle.len(),
        &ImportLimits {
            max_resource_bytes: 35,
            ..ImportLimits::default()
        }
    )
    .is_err());
    assert!(import_frozen(
        ImportContext {
            request: 6,
            source: 87
        },
        &bundle,
        bundle.len(),
        &ImportLimits {
            max_vertices: 2,
            ..ImportLimits::default()
        }
    )
    .is_err());
}
