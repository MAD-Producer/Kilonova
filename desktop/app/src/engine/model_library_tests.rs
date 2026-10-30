//! Real library-only model ownership and separate placement transactions.
use super::*;
use crate::layer_clipboard::KeyframeSource;
use crate::workflow::library_media::Request;
use motion_core::{LibraryAction, ProjectItem};
use serde_json::json;

fn run(engine: &Engine, operation: Request) -> Result<Value, String> {
    let source = KeyframeSource::of(&engine.state().unwrap());
    engine.call(|reply| Command::ProjectAsset {
        source,
        operation,
        reply,
    })
}

#[test]
fn model_library_registration_and_repeated_placement_have_independent_undo_boundaries() {
    for format in ["gltf", "GLB", "OBJ"] {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("originals");
        let path = super::model_import_tests::triangle(&original, format);
        let root = dir.path().join("project");
        let worker =
            Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
        worker
            .edit(
                serde_json::to_string(&motion_core::Command::ProjectLibrary {
                    action: LibraryAction::CreateFolder {
                        id: 7,
                        parent: None,
                        name: "Models".into(),
                    },
                })
                .unwrap(),
            )
            .unwrap();
        let before = worker.state().unwrap();
        let registered = run(
            &worker,
            Request::Model {
                file: path,
                folder: Some(7),
            },
        )
        .unwrap();
        assert_eq!(registered["project"]["layers"], before["project"]["layers"]);
        assert!(registered["modelImport"]["object"].is_null());
        let asset = registered["assetRegistration"]["asset"].as_u64().unwrap();
        let project: motion_core::Project =
            serde_json::from_value(registered["project"].clone()).unwrap();
        assert_eq!(
            project.library.folder_of(&ProjectItem::Model { id: asset }),
            Some(7)
        );
        assert_eq!(project.model_assets.len(), 1);
        let owned = root.join(&project.model_assets[0].path);
        assert!(owned.is_file());
        std::fs::remove_dir_all(&original).unwrap();
        // Registration is one undo, including folder placement, and owns bytes for redo.
        assert_eq!(worker.history(0).unwrap()["project"], before["project"]);
        assert!(owned.is_file());
        assert_eq!(worker.history(1).unwrap()["project"], registered["project"]);
        let first = run(
            &worker,
            Request::PlaceModel {
                asset,
                at_frame: 10,
            },
        )
        .unwrap();
        let second = run(
            &worker,
            Request::PlaceModel {
                asset,
                at_frame: 20,
            },
        )
        .unwrap();
        let first_id = first["assetPlacement"]["object"].as_u64().unwrap();
        let second_id = second["assetPlacement"]["object"].as_u64().unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(
            second["project"]["model_assets"],
            registered["project"]["model_assets"]
        );
        for (state, object, at) in [(&first, first_id, 10), (&second, second_id, 20)] {
            let layer = state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == object)
                .unwrap();
            assert_eq!(layer["three_d"], true);
            assert_eq!(layer["content"]["model"]["asset"], asset);
            assert_eq!(layer["content"]["model"]["pixels_per_metre"], 100.);
            assert_eq!(layer["timeline"]["in_frame"], at);
            assert_eq!(layer["timeline"]["offset_frame"], at);
        }
        assert_eq!(worker.history(0).unwrap()["project"], first["project"]);
        assert_eq!(worker.history(0).unwrap()["project"], registered["project"]);
        assert_eq!(worker.history(1).unwrap()["project"], first["project"]);
        worker.save().unwrap();
        drop(worker);
        let reopened = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(reopened.state().unwrap()["project"], first["project"]);
        let output = reopened.render_preview(Some(10.)).unwrap();
        let image = image::open(output["path"].as_str().unwrap())
            .unwrap()
            .into_rgba8();
        assert!(
            image
                .pixels()
                .filter(|p| p[0] > p[1].saturating_add(20) && p[0] > p[2].saturating_add(20))
                .count()
                > 500
        );
    }
}

#[test]
fn model_library_refusals_and_superseded_requests_preserve_document_and_owned_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let worker = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let before = worker.state().unwrap();
    // Invalid destination is rejected before even opening the missing source.
    let error = run(
        &worker,
        Request::Model {
            file: dir.path().join("missing.obj"),
            folder: Some(999),
        },
    )
    .unwrap_err();
    assert!(error.contains("folder"), "{error}");
    let mut stale = before.clone();
    stale["document_generation"] = json!(before["document_generation"].as_u64().unwrap() + 1);
    let cancelled = worker
        .call(|reply| Command::ProjectAsset {
            source: KeyframeSource::of(&stale),
            operation: Request::Model {
                file: dir.path().join("missing.obj"),
                folder: None,
            },
            reply,
        })
        .unwrap();
    assert_eq!(cancelled["assetOperation"]["cancelled"], true);
    assert_eq!(worker.state().unwrap()["project"], before["project"]);
    assert!(!root.join("assets/models").exists());
    let path = super::model_import_tests::triangle(&dir.path().join("originals"), "obj");
    let registered = run(
        &worker,
        Request::Model {
            file: path,
            folder: None,
        },
    )
    .unwrap();
    let asset = registered["assetRegistration"]["asset"].as_u64().unwrap();
    assert!(run(
        &worker,
        Request::PlaceModel {
            asset: asset + 1,
            at_frame: 0
        }
    )
    .unwrap_err()
    .contains("source"));
    let end = registered["project"]["frames"].as_u64().unwrap() as u32;
    assert!(run(
        &worker,
        Request::PlaceModel {
            asset,
            at_frame: end
        }
    )
    .unwrap_err()
    .contains("outside"));
    assert_eq!(worker.state().unwrap()["project"], registered["project"]);
    assert_eq!(worker.state().unwrap()["canUndo"], registered["canUndo"]);
    assert_eq!(
        std::fs::read_dir(root.join("assets/models"))
            .unwrap()
            .count(),
        1
    );
    assert!(run(&worker, Request::PlaceModel { asset, at_frame: 0 }).is_ok());
}
