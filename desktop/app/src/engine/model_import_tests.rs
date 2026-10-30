//! Exercise real worker publication, history, source ownership and output.
use super::*;
use serde_json::json;
use std::{fs, path::Path};

pub(crate) fn triangle(root: &Path, format: &str) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    if format.eq_ignore_ascii_case("obj") {
        fs::write(root.join("paint.mtl"), "newmtl Red\nKd 1 0 0\nillum 0\n").unwrap();
        let path = root.join(format!("triangle.{format}"));
        fs::write(
            &path,
            "mtllib paint.mtl\nv -1 -1 0\nv 1 -1 0\nv 0 1 0\nusemtl Red\nf 1 2 3\n",
        )
        .unwrap();
        return path;
    }
    let mut bin = Vec::new();
    for n in [-1f32, -1., 0., 1., -1., 0., 0., 1., 0.] {
        bin.extend(n.to_le_bytes());
    }
    for n in [0u16, 1, 2] {
        bin.extend(n.to_le_bytes());
    }
    let mut doc = json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
        "buffers":[{"byteLength":bin.len(),"uri":"triangle.bin"}],
        "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-1,-1,0],"max":[1,1,0]},
                     {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}]});
    let path = root.join(format!("triangle.{format}"));
    if format.eq_ignore_ascii_case("gltf") {
        fs::write(root.join("triangle.bin"), bin).unwrap();
        fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
    } else {
        doc["buffers"][0].as_object_mut().unwrap().remove("uri");
        let mut source = serde_json::to_vec(&doc).unwrap();
        while source.len() % 4 != 0 {
            source.push(b' ');
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let mut glb = Vec::new();
        for word in [
            0x46546c67u32,
            2,
            (12 + 8 + source.len() + 8 + bin.len()) as u32,
            source.len() as u32,
            0x4e4f534a,
        ] {
            glb.extend(word.to_le_bytes());
        }
        glb.extend(source);
        for word in [bin.len() as u32, 0x004e4942] {
            glb.extend(word.to_le_bytes());
        }
        glb.extend(bin);
        fs::write(&path, glb).unwrap();
    }
    path
}

fn engine(root: &Path) -> Engine {
    Engine::start(root.into(), motion_desktop_media::platform(), 8 << 30).unwrap()
}
fn import(engine: &Engine, path: PathBuf) -> Value {
    let revision = engine.state().unwrap()["revision"].as_u64().unwrap();
    engine.import_model(path, revision).unwrap()
}

#[test]
fn gltf_glb_obj_import_is_owned_atomic_undoable_and_reopens_without_sources() {
    for format in ["gltf", "GLB", "OBJ"] {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("originals");
        let root = dir.path().join("project");
        let path = triangle(&original, format);
        let worker = engine(&root);
        let before = worker.state().unwrap();
        let after = import(&worker, path);
        let asset = &after["project"]["model_assets"][0];
        let object = after["modelImport"]["object"].as_u64().unwrap();
        let layer = after["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == object)
            .unwrap();
        assert_eq!(layer["three_d"], true);
        assert_eq!(layer["content"]["kind"], "model");
        assert_eq!(layer["content"]["model"]["pixels_per_metre"], 100.);
        assert!(layer["content"]["model"]["animation"].is_null());
        assert!(root.join(asset["path"].as_str().unwrap()).is_file());
        for file in fs::read_dir(&original).unwrap() {
            fs::remove_file(file.unwrap().path()).unwrap();
        }
        worker.history(0).unwrap();
        assert_eq!(worker.state().unwrap()["project"], before["project"]);
        worker.history(1).unwrap();
        assert_eq!(worker.state().unwrap()["project"], after["project"]);
        worker.save().unwrap();
        drop(worker);
        let reopened = engine(&root);
        assert_eq!(reopened.state().unwrap()["project"], after["project"]);
        // Uses the real model renderer after the input and all dependencies are gone.
        let output = reopened.render_preview(Some(0.)).unwrap();
        let pixels = image::open(output["path"].as_str().expect("native preview path"))
            .unwrap()
            .into_rgba8();
        let red = pixels
            .pixels()
            .filter(|p| p[0] > p[1].saturating_add(20) && p[0] > p[2].saturating_add(20))
            .count();
        assert!(
            red > 500,
            "{format}: imported red triangle has only {red} visible red pixels"
        );
    }
}

#[test]
fn missing_dependency_and_stale_revision_preserve_project_history_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let original = dir.path().join("originals");
    let path = triangle(&original, "gltf");
    let worker = engine(&root);
    let before = worker.state().unwrap();
    fs::remove_file(original.join("triangle.bin")).unwrap();
    let error = worker
        .import_model(path.clone(), before["revision"].as_u64().unwrap())
        .unwrap_err();
    assert!(
        error.contains("triangle.bin") && error.contains("model_import"),
        "{error}"
    );
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    assert_eq!(worker.state().unwrap()["canUndo"], before["canUndo"]);
    assert!(!root.join("assets/models").exists());
    // No source open may occur when the caller's revision is stale.
    let error = worker
        .import_model(original.join("not-present.obj"), u64::MAX)
        .unwrap_err();
    assert!(error.contains("revision"), "{error}");
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    let path = triangle(&original, "obj");
    assert_eq!(
        import(&worker, path)["project"]["model_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn imported_model_ids_share_the_existing_asset_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let original = dir.path().join("originals");
    let path = triangle(&original, "obj");
    let image = original.join("picture.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]))
        .save(&image)
        .unwrap();
    let worker = engine(&root);
    let before = worker.import_image(image).unwrap();
    let after = import(&worker, path);
    assert_ne!(
        after["modelImport"]["asset"],
        before["project"]["assets"][0]["id"]
    );
    assert_eq!(after["project"]["assets"], before["project"]["assets"]);
}

#[test]
fn refused_publication_removes_only_its_frozen_file_and_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let path = triangle(&dir.path().join("originals"), "obj");
    let mut project = motion_core::Project::new(64, 64, 30, 90).unwrap();
    project.layers = (1..=motion_core::MAX_LAYERS)
        .map(|id| {
            motion_core::Layer::solid(id as u64, "Existing", [4.; 2], [32., 32., 0.], [1.; 4])
        })
        .collect();
    motion_core::storage::save(&root, &project).unwrap();
    let folder = root.join("assets/models");
    fs::create_dir_all(&folder).unwrap();
    let foreign = folder.join("existing-resource.bin");
    fs::write(&foreign, b"unrelated owned data").unwrap();
    let worker = engine(&root);
    let before = worker.state().unwrap();
    let error = worker
        .import_model(path.clone(), before["revision"].as_u64().unwrap())
        .unwrap_err();
    assert!(
        error.contains("layer") && error.contains("model_import"),
        "{error}"
    );
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    assert_eq!(worker.state().unwrap()["canUndo"], before["canUndo"]);
    assert_eq!(fs::read(&foreign).unwrap(), b"unrelated owned data");
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
    worker
        .edit(
            serde_json::to_string(&motion_core::Command::Remove {
                object: motion_core::MAX_LAYERS as u64,
                frame: 0,
            })
            .unwrap(),
        )
        .unwrap();
    let after = import(&worker, path);
    assert_eq!(
        after["project"]["layers"].as_array().unwrap().len(),
        motion_core::MAX_LAYERS
    );
    assert_eq!(
        after["project"]["model_assets"].as_array().unwrap().len(),
        1
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
}

#[test]
fn mcp_model_import_checks_write_permission_schema_and_revision_before_publication() {
    use crate::mcp::{Access, ToolRouter};
    let dir = tempfile::tempdir().unwrap();
    let path = triangle(&dir.path().join("originals"), "obj");
    let worker = engine(&dir.path().join("project"));
    let before = worker.state().unwrap();
    assert_eq!(before["desktop"]["modelImport"]["supported"], true);
    let request = json!({"path":path,"expectedRevision":before["revision"]});
    let read = ToolRouter {
        engine: &worker,
        access: Access::ReadOnly,
    };
    let edit = ToolRouter {
        engine: &worker,
        access: Access::Edit,
    };
    assert!(read
        .call("import_model", request.clone())
        .unwrap_err()
        .contains("disabled"));
    assert!(edit.call("import_model", json!({"path":path})).is_err());
    assert!(edit
        .call(
            "import_model",
            json!({"path":path,"expectedRevision":before["revision"],"unknown":true})
        )
        .is_err());
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    let after = edit.call("import_model", request.clone()).unwrap();
    assert_eq!(after["modelImport"]["owned"], true);
    assert!(edit
        .call("import_model", request)
        .unwrap_err()
        .contains("revision"));
    assert_eq!(worker.state().unwrap()["project"], after["project"]);
    worker.history(0).unwrap();
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    assert!(!crate::workflow::model::supported(Path::new("picture.png")));
    assert!(crate::workflow::model::supported(Path::new("scene.GLTf")));
}

#[test]
fn superseded_ui_import_cancels_before_opening_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let worker = engine(&root);
    let before = worker.state().unwrap();
    let mut stale = before.clone();
    stale["document_generation"] = json!(before["document_generation"]
        .as_u64()
        .unwrap()
        .checked_add(1)
        .unwrap());
    let source = crate::layer_clipboard::KeyframeSource::of(&stale);
    let cancelled = worker
        .call(|reply| Command::ImportModel {
            file: dir.path().join("missing.obj"),
            expected_revision: None,
            source: Some(source),
            reply,
        })
        .unwrap();
    assert_eq!(cancelled["modelImport"]["cancelled"], true);
    assert_eq!(cancelled["project"], before["project"]);
    assert!(!root.join("assets/models").exists());
}
