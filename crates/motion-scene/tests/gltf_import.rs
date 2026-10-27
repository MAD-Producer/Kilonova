use base64::{engine::general_purpose::STANDARD, Engine};
use glam::{Mat4, Vec3};
use motion_scene::{
    import, AlphaMode, AssetResolver, Cause, DirectoryResolver, ImportContext, ImportLimits,
    ResourceKind,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, Once,
    },
};

struct Assets(BTreeMap<String, Vec<u8>>);
impl AssetResolver for Assets {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "fixture asset missing"))?;
        if bytes.len() > limit {
            return Err(io::Error::other("fixture byte limit"));
        }
        Ok(bytes.clone())
    }
}

fn triangle() -> (Value, Vec<u8>) {
    let bytes: Vec<_> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    (
        json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":bytes.len(),"uri":"mesh.bin"}],
        "bufferViews":[{"buffer":0,"byteLength":bytes.len()}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}),
        bytes,
    )
}

fn context() -> ImportContext {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    ImportContext {
        request: NEXT.fetch_add(1, Ordering::Relaxed),
        source: 87,
    }
}
fn assets(bytes: Vec<u8>) -> Assets {
    Assets(BTreeMap::from([("mesh.bin".into(), bytes)]))
}
fn load(root: &Value, bytes: Vec<u8>) -> motion_scene::Result<motion_scene::Scene> {
    import(
        context(),
        &serde_json::to_vec(root).unwrap(),
        &mut assets(bytes),
        &ImportLimits::default(),
    )
}
fn rejected(root: &Value, bytes: Vec<u8>, phase: &str) {
    let error = load(root, bytes).unwrap_err();
    assert_eq!(error.phase, phase, "{error}");
}

fn glb(root: &Value, bytes: &[u8]) -> Vec<u8> {
    let mut root = root.clone();
    root["buffers"][0].as_object_mut().unwrap().remove("uri");
    let mut json = serde_json::to_vec(&root).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bin = bytes.to_vec();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut result = b"glTF".to_vec();
    result.extend(2u32.to_le_bytes());
    result.extend((12u32 + 8 + json.len() as u32 + 8 + bin.len() as u32).to_le_bytes());
    result.extend((json.len() as u32).to_le_bytes());
    result.extend(0x4e4f534au32.to_le_bytes());
    result.extend(json);
    result.extend((bin.len() as u32).to_le_bytes());
    result.extend(0x004e4942u32.to_le_bytes());
    result.extend(bin);
    result
}

#[test]
fn json_glb_and_data_uri_produce_identical_owned_meshes() {
    let (mut root, bytes) = triangle();
    let external = load(&root, bytes.clone()).unwrap();
    let binary = import(
        context(),
        &glb(&root, &bytes),
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default(),
    )
    .unwrap();
    root["buffers"][0]["uri"] = json!(format!(
        "data:application/octet-stream;base64,{}",
        STANDARD.encode(&bytes)
    ));
    let embedded = load(&root, Vec::new()).unwrap();
    assert_eq!(external.meshes(), binary.meshes());
    assert_eq!(external.meshes(), embedded.meshes());
    assert_eq!(external.buffers()[0].bytes, bytes);
    assert_eq!(binary.buffers()[0].sha256, external.buffers()[0].sha256);
    assert_eq!(external.meshes()[0].primitives[0].indices, [0, 1, 2]);
    assert!(external.meshes()[0].primitives[0].normals.is_none());
    assert_eq!(external.retained_bytes(), 36 + 36 + 12);
    assert!(binary.buffers()[0].locator.is_none());
}

#[test]
fn interleaved_normalized_uvs_preserve_vertex_layout() {
    let (mut root, _) = triangle();
    let mut bytes = Vec::new();
    for (position, uv) in [
        ([0f32, 0., 0.], [0u8, 0]),
        ([1., 0., 0.], [255, 0]),
        ([0., 1., 0.], [0, 255]),
    ] {
        bytes.extend(position.into_iter().flat_map(f32::to_le_bytes));
        bytes.extend(uv);
        bytes.extend([17, 19]);
    }
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    root["bufferViews"][0] =
        json!({"buffer":0,"byteLength":bytes.len(),"byteStride":16,"target":34962});
    root["accessors"].as_array_mut().unwrap().push(json!({"bufferView":0,"byteOffset":12,"componentType":5121,"normalized":true,"count":3,"type":"VEC2"}));
    root["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
    let scene = load(&root, bytes).unwrap();
    assert_eq!(
        scene.meshes()[0].primitives[0].texcoords[0]
            .as_ref()
            .unwrap(),
        &[[0., 0.], [1., 0.], [0., 1.]]
    );
}

#[test]
fn sparse_zero_base_and_strict_index_order_are_validated() {
    let (mut root, _) = triangle();
    let mut bytes = vec![1u8, 2, 0, 0];
    bytes.extend(
        [1f32, 0., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    root["bufferViews"] =
        json!([{"buffer":0,"byteLength":2},{"buffer":0,"byteOffset":4,"byteLength":24}]);
    root["accessors"][0] = json!({"componentType":5126,"type":"VEC3","count":3,"min":[0,0,0],"max":[1,1,0],
        "sparse":{"count":2,"indices":{"bufferView":0,"componentType":5121},"values":{"bufferView":1}}});
    let scene = load(&root, bytes.clone()).unwrap();
    assert_eq!(
        scene.meshes()[0].primitives[0].positions,
        [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
    );
    bytes[1] = 1;
    rejected(&root, bytes.clone(), "sparse");
    bytes[1] = 3;
    rejected(&root, bytes, "sparse");
}

#[test]
fn external_bytes_are_owned_and_relinking_has_content_identity() {
    let directory = tempfile::tempdir().unwrap();
    let (mut root, bytes) = triangle();
    let name = "三角形 mesh.bin";
    root["buffers"][0]["uri"] = json!("%E4%B8%89%E8%A7%92%E5%BD%A2%20mesh.bin");
    std::fs::write(directory.path().join(name), &bytes).unwrap();
    let mut resolver = DirectoryResolver::new(directory.path()).unwrap();
    let scene = import(
        context(),
        &serde_json::to_vec(&root).unwrap(),
        &mut resolver,
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = serde_json::to_vec(&scene).unwrap();
    std::fs::write(directory.path().join(name), [0u8; 36]).unwrap();
    assert!(import(
        context(),
        &serde_json::to_vec(&root).unwrap(),
        &mut resolver,
        &ImportLimits::default()
    )
    .is_err());
    assert_eq!(scene.buffers()[0].bytes, bytes);
    assert_eq!(scene.buffers()[0].locator.as_deref(), Some(name));
    assert_eq!(serde_json::to_vec(&scene).unwrap(), frozen);
}

#[test]
fn traversal_and_network_uris_are_rejected_before_resolver_entry() {
    struct RejectRead;
    impl AssetResolver for RejectRead {
        fn read(&mut self, _: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
            panic!("invalid URI reached resolver")
        }
    }
    let (mut root, _) = triangle();
    for uri in [
        "../mesh.bin",
        "%2e%2e/mesh.bin",
        "/mesh.bin",
        "C:/mesh.bin",
        "https://host/mesh.bin",
        "file:///mesh.bin",
        "a\\mesh.bin",
        "a/%5cmesh.bin",
        "a/%00.bin",
        "a/%ZZ.bin",
        "a//mesh.bin",
        "mesh.bin?q=1",
        "mesh.bin#part",
    ] {
        root["buffers"][0]["uri"] = json!(uri);
        let error = import(
            context(),
            &serde_json::to_vec(&root).unwrap(),
            &mut RejectRead,
            &ImportLimits::default(),
        )
        .unwrap_err();
        assert_eq!(error.phase, "resource", "{uri}: {error}");
    }
}

#[test]
fn directory_resolver_rejects_absolute_paths_and_directories() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    let mut resolver = DirectoryResolver::new(directory.path()).unwrap();
    for path in ["../outside.bin", "/outside.bin", "C:/outside.bin", "nested"] {
        assert!(resolver.read(path, ResourceKind::Buffer, 100).is_err());
    }
}

#[test]
fn retained_resource_and_decoded_payload_budgets_are_distinct() {
    let (root, bytes) = triangle();
    let source = serde_json::to_vec(&root).unwrap();
    let limits = ImportLimits {
        max_resource_bytes: 35,
        ..ImportLimits::default()
    };
    assert!(import(context(), &source, &mut assets(bytes.clone()), &limits).is_err());
    let limits = ImportLimits {
        max_retained_bytes: 83,
        ..ImportLimits::default()
    };
    let error = import(context(), &source, &mut assets(bytes.clone()), &limits).unwrap_err();
    assert_eq!(error.phase, "budget");
    let limits = ImportLimits {
        max_retained_bytes: 84,
        ..ImportLimits::default()
    };
    assert_eq!(
        import(context(), &source, &mut assets(bytes), &limits)
            .unwrap()
            .retained_bytes(),
        84
    );
}

#[test]
fn aggregate_vertex_and_index_limits_count_repeated_primitives() {
    let (mut root, bytes) = triangle();
    let p = root["meshes"][0]["primitives"][0].clone();
    root["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(p);
    for limits in [
        ImportLimits {
            max_vertices: 5,
            ..ImportLimits::default()
        },
        ImportLimits {
            max_indices: 5,
            ..ImportLimits::default()
        },
    ] {
        let error = import(
            context(),
            &serde_json::to_vec(&root).unwrap(),
            &mut assets(bytes.clone()),
            &limits,
        )
        .unwrap_err();
        assert_eq!(error.phase, "budget");
    }
}

#[test]
fn glb_lengths_order_duplicates_and_padding_are_checked() {
    let (root, bytes) = triangle();
    let original = glb(&root, &bytes);
    for size in [4, 8, 11, 12, 17, original.len() - 1] {
        assert!(import(
            context(),
            &original[..size],
            &mut Assets(BTreeMap::new()),
            &ImportLimits::default()
        )
        .is_err());
    }
    let mut wrong = original.clone();
    wrong[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert!(import(
        context(),
        &wrong,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .is_err());
    let mut wrong = original.clone();
    wrong[16..20].copy_from_slice(&0x004e4942u32.to_le_bytes());
    assert!(import(
        context(),
        &wrong,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .is_err());
    let mut wrong = original;
    wrong.extend(0u32.to_le_bytes());
    wrong.extend(0x4e4f534au32.to_le_bytes());
    let length = wrong.len() as u32;
    wrong[8..12].copy_from_slice(&length.to_le_bytes());
    assert!(import(
        context(),
        &wrong,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .is_err());
}

#[test]
fn accessors_reject_overflow_stride_alignment_and_nonfinite_positions() {
    let (root, bytes) = triangle();
    for (field, value) in [
        ("byteOffset", json!(usize::MAX)),
        ("byteStride", json!(8)),
        ("byteStride", json!(253)),
    ] {
        let mut bad = root.clone();
        bad["bufferViews"][0][field] = value;
        assert!(load(&bad, bytes.clone()).is_err());
    }
    let mut bad = root.clone();
    bad["accessors"][0]["byteOffset"] = json!(1);
    rejected(&bad, bytes.clone(), "accessor");
    let mut bad = root.clone();
    bad["accessors"][0]["count"] = json!(usize::MAX);
    rejected(&bad, bytes.clone(), "accessor");
    let mut data = bytes;
    data[0..4].copy_from_slice(&f32::NAN.to_le_bytes());
    rejected(&root, data, "accessor");
}

#[test]
fn malformed_triangle_indices_are_not_silently_truncated() {
    let (mut root, mut bytes) = triangle();
    bytes.extend([0u8, 1, 3, 0]);
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    root["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":36,"byteLength":3}));
    root["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}));
    root["meshes"][0]["primitives"][0]["indices"] = json!(1);
    rejected(&root, bytes.clone(), "indices");
    bytes[38] = 2;
    assert!(load(&root, bytes.clone()).is_ok());
    bytes[38] = 255;
    rejected(&root, bytes, "indices");
}

#[test]
fn node_instances_preserve_trs_order_metres_handedness_and_authored_matrix() {
    let (mut root, bytes) = triangle();
    root["nodes"] = json!([{"translation":[1,2,3],"children":[1]},
        {"mesh":0,"scale":[-2,3,1],"rotation":[0,0,0,1]}]);
    let scene = load(&root, bytes.clone()).unwrap();
    let instances = scene.instances(0, 100.).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(
        instances[0].1.transform_point3(Vec3::X),
        Vec3::new(-100., 200., 300.)
    );
    assert!(instances[0].1.determinant() < 0.); // Negative authored scale changes winding; import does not reverse it.
    assert!(scene.instances(0, 0.).is_err());
    assert!(scene.instances(1, 100.).is_err());
    let local = Mat4::from_scale(Vec3::new(-1., 2., 0.)).to_cols_array();
    root["nodes"] = json!([{"mesh":0,"matrix":local}]);
    assert_eq!(load(&root, bytes).unwrap().nodes()[0].local, local); // Singular TRS is allowed, never decomposed.
}

#[test]
fn disconnected_cycles_multi_parent_and_nonroot_scene_entries_are_rejected() {
    let (mut root, bytes) = triangle();
    for nodes in [
        json!([{"mesh":0},{"children":[2]},{"children":[1]}]),
        json!([{"children":[2]},{"children":[2]},{"mesh":0}]),
        json!([{"mesh":0,"children":[0]}]),
    ] {
        root["nodes"] = nodes;
        rejected(&root, bytes.clone(), "node");
    }
    root["nodes"] = json!([{"mesh":0,"children":[1]},{"mesh":0}]);
    root["scenes"][0]["nodes"] = json!([1]);
    rejected(&root, bytes, "scene");
}

#[test]
fn transforms_reject_shear_mixed_matrix_trs_and_bad_quaternions() {
    let (mut root, bytes) = triangle();
    let mut matrix = Mat4::IDENTITY.to_cols_array();
    matrix[4] = 0.25;
    for node in [
        json!({"mesh":0,"matrix":matrix}),
        json!({"mesh":0,"matrix":Mat4::IDENTITY.to_cols_array(),"scale":[1,1,1]}),
        json!({"mesh":0,"rotation":[0,0,0,2]}),
    ] {
        root["nodes"] = json!([node]);
        rejected(&root, bytes.clone(), "node");
    }
}

#[test]
fn materials_preserve_core_defaults_alpha_and_linear_factors() {
    let (mut root, bytes) = triangle();
    root["materials"] = json!([{}, {"pbrMetallicRoughness":{"baseColorFactor":[0.5,0.25,0.125,0.4],"metallicFactor":0.7,"roughnessFactor":0.2},
        "alphaMode":"BLEND","doubleSided":true},{"alphaMode":"MASK","alphaCutoff":0.3}]);
    root["meshes"][0]["primitives"][0]["material"] = json!(1);
    let scene = load(&root, bytes.clone()).unwrap();
    assert_eq!(scene.materials()[0], motion_scene::Material::default());
    assert_eq!(scene.materials()[1].base_color, [0.5, 0.25, 0.125, 0.4]);
    assert_eq!(scene.materials()[1].alpha, AlphaMode::Blend);
    assert!(scene.materials()[1].double_sided);
    assert_eq!(scene.materials()[2].alpha, AlphaMode::Mask);
    root["materials"][1]["pbrMetallicRoughness"]["roughnessFactor"] = json!(2);
    rejected(&root, bytes, "material");
}

#[test]
fn optional_extensions_are_reported_and_required_features_are_explicit_errors() {
    let (mut root, bytes) = triangle();
    root["extensionsUsed"] = json!(["VENDOR_material_hint"]);
    assert_eq!(
        load(&root, bytes.clone())
            .unwrap()
            .ignored_optional_extensions(),
        ["VENDOR_material_hint"]
    );
    root["extensionsRequired"] = json!(["VENDOR_material_hint"]);
    let error = load(&root, bytes.clone()).unwrap_err();
    assert!(matches!(error.cause, Cause::Unsupported(_)));
    root.as_object_mut().unwrap().remove("extensionsRequired");
    // Transform clips and cameras are supported; skinning requires another profile.
    for feature in ["skins"] {
        let mut bad = root.clone();
        bad[feature] = json!([{}]);
        let error = load(&bad, bytes.clone()).unwrap_err();
        assert!(matches!(error.cause, Cause::Unsupported(_)));
    }
}

#[test]
fn mesh_library_is_not_implicitly_instanced() {
    let (mut root, bytes) = triangle();
    for key in ["scenes", "scene", "nodes"] {
        root.as_object_mut().unwrap().remove(key);
    }
    let scene = load(&root, bytes).unwrap();
    assert!(scene.scenes().is_empty());
    assert!(scene.default_scene().is_none());
    assert_eq!(scene.meshes().len(), 1);
    assert!(scene.instances(0, 100.).is_err());
}

#[test]
fn view_cannot_read_trailing_file_bytes_beyond_declared_buffer_length() {
    let (mut root, mut bytes) = triangle();
    bytes.extend([0u8; 12]);
    root["bufferViews"][0]["byteLength"] = json!(48);
    rejected(&root, bytes, "buffer_view");
}

#[test]
fn glb_zero_padding_and_unknown_chunks_follow_container_rules() {
    let (mut root, mut bytes) = triangle();
    bytes.push(7);
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    let good = glb(&root, &bytes);
    assert!(import(
        context(),
        &good,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .is_ok());
    let mut bad = good.clone();
    *bad.last_mut().unwrap() = 1;
    assert!(import(
        context(),
        &bad,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("padding"));
    let mut extension = good;
    extension.extend(4u32.to_le_bytes());
    extension.extend(0x12345678u32.to_le_bytes());
    extension.extend([1, 2, 3, 4]);
    let len = extension.len() as u32;
    extension[8..12].copy_from_slice(&len.to_le_bytes());
    assert!(import(
        context(),
        &extension,
        &mut Assets(BTreeMap::new()),
        &ImportLimits::default()
    )
    .is_ok());
}

#[test]
fn color_conversion_preserves_linear_values_and_adds_spec_alpha() {
    let (mut root, mut bytes) = triangle();
    bytes.extend(
        [-1f32, 0.25, 2., 1., 0., 0., 0., 0.5, 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    root["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":36,"byteLength":36}));
    root["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}));
    root["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(1);
    let scene = load(&root, bytes).unwrap();
    assert_eq!(
        scene.meshes()[0].primitives[0].colors.as_deref().unwrap(),
        &[[0., 0.25, 1., 1.], [1., 0., 0., 1.], [0., 0.5, 1., 1.]]
    );
}

#[test]
fn image_signatures_slots_sampler_and_missing_coordinates_are_checked() {
    let (mut root, mut bytes) = triangle();
    bytes.extend(
        [0f32, 0., 1., 0., 0., 1.]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    root["buffers"][0]["byteLength"] = json!(bytes.len());
    root["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffer":0,"byteOffset":36,"byteLength":24}));
    root["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
    root["images"] =
        json!([{"uri":format!("data:image/png;base64,{}",STANDARD.encode(b"\x89PNG\r\n\x1a\n"))}]);
    root["textures"] = json!([{"source":0,"sampler":0}]);
    root["samplers"] = json!([{"magFilter":9729,"minFilter":9987,"wrapS":33648,"wrapT":33071}]);
    root["materials"] = json!([{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}},"normalTexture":{"index":0,"scale":0.4},"occlusionTexture":{"index":0,"strength":0.7}}]);
    root["meshes"][0]["primitives"][0]["material"] = json!(0);
    rejected(&root, bytes.clone(), "primitive");
    root["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(1);
    let scene = load(&root, bytes.clone()).unwrap();
    assert_eq!(scene.images()[0].mime_type, "image/png");
    assert_eq!(scene.materials()[0].normal_scale, 0.4);
    assert_eq!(scene.materials()[0].occlusion_strength, 0.7);
    assert_eq!(scene.samplers()[0].wrap_s, 33648);
    root["images"][0]["mimeType"] = json!("image/jpeg");
    rejected(&root, bytes.clone(), "image");
    root["images"][0]
        .as_object_mut()
        .unwrap()
        .remove("mimeType");
    root["samplers"][0]["magFilter"] = json!(9987);
    rejected(&root, bytes, "sampler");
}

struct Records(Mutex<Vec<String>>);
impl log::Log for Records {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} {}", record.level(), record.args()));
        }
    }
    fn flush(&self) {}
}
static RECORDS: Records = Records(Mutex::new(Vec::new()));

#[test]
fn default_info_start_precedes_a_controlled_blocking_resolver_and_errors_keep_ids() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&RECORDS).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
    struct Blocking {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl AssetResolver for Blocking {
        fn read(&mut self, _: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled fixture denial",
            ))
        }
    }
    let ctx = context();
    let (root, _) = triangle();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        import(
            ctx,
            &serde_json::to_vec(&root).unwrap(),
            &mut Blocking {
                entered: entered_tx,
                release: release_rx,
            },
            &ImportLimits::default(),
        )
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let expected = format!("INFO start #{} gltf_import source=87", ctx.request);
    let found = RECORDS
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|line| line == &expected);
    release_tx.send(()).unwrap();
    let error = worker.join().unwrap().unwrap_err();
    assert!(found, "start was absent while the resolver was blocked");
    assert!(matches!(error.cause,Cause::Io(ref e) if e.kind()==io::ErrorKind::PermissionDenied));
    let records = RECORDS.0.lock().unwrap();
    let errors: Vec<_> = records
        .iter()
        .filter(|l| l.starts_with(&format!("ERROR gltf_import #{} ", ctx.request)))
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("source=87"));
    assert!(errors[0].contains("elapsed_ms="));
    assert!(errors[0].contains("phase=resource"));
    assert!(errors[0].contains("mesh.bin"));
    assert!(errors[0].contains("controlled fixture denial"));
    assert!(!records
        .iter()
        .any(|l| l.starts_with(&format!("DEBUG done #{} ", ctx.request))));
}
