//! Public CPU ownership and actual native reuse, with deleted source files.
use super::*;
use motion_render::ModelSourceCatalog;
use std::sync::Arc;

#[test]
fn cpu_catalog_reuses_frozen_scene_after_save_reopen_and_source_removal() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    motion_core::storage::save(root.path(), &project).unwrap();
    let project = motion_core::storage::load(root.path()).unwrap();
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    assert_eq!(catalog.len(), 1);
    assert!(!catalog.is_empty());
    let scene = catalog.scene(7).unwrap().clone();
    assert_eq!(catalog.retained_bytes(), scene.retained_bytes());
    assert!(catalog.retained_bytes() > 0);
    assert!(catalog.scene(8).is_none());
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let cloned = catalog.clone();
    assert!(Arc::ptr_eq(cloned.scene(7).unwrap(), &scene));
    let refreshed = ModelSourceCatalog::prepare(&project, root.path(), &cloned).unwrap();
    assert!(Arc::ptr_eq(refreshed.scene(7).unwrap(), &scene));
    // A different thread consumes the same parsed geometry without a device.
    std::thread::spawn(move || {
        assert_eq!(refreshed.scene(7).unwrap().scenes().len(), 1);
        assert!(Arc::ptr_eq(refreshed.scene(7).unwrap(), &scene));
    }).join().unwrap();
}

#[test]
fn cpu_catalog_candidate_errors_preserve_previous_and_refuse_duplicate_ids() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let scene = catalog.scene(7).unwrap().clone();
    let mut changed = project.clone();
    changed.model_assets[0].sha256 = "0".repeat(64);
    let error = ModelSourceCatalog::prepare(&changed, root.path(), &catalog).err().unwrap();
    assert!(error.to_string().contains("source=7"));
    assert!(error.to_string().contains("SHA-256 mismatch"));
    let mut duplicate = project.clone();
    duplicate.model_assets.push(duplicate.model_assets[0].clone());
    let error = ModelSourceCatalog::prepare(&duplicate, root.path(), &catalog).err().unwrap();
    assert!(error.to_string().contains("source=7 duplicate"));
    assert!(Arc::ptr_eq(catalog.scene(7).unwrap(), &scene));
    assert_eq!(catalog.retained_bytes(), scene.retained_bytes());
}

#[test]
fn cpu_catalog_empty_replacement_releases_its_scope_without_invalidating_readers() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(root.path());
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let reader = catalog.scene(7).unwrap().clone();
    project.layers.clear();
    project.model_assets.clear();
    let empty = ModelSourceCatalog::prepare(&project, root.path(), &catalog).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.retained_bytes(), 0);
    drop(catalog);
    assert_eq!(reader.scenes().len(), 1);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cpu_catalog_is_shared_by_native_upload_and_survives_failed_reconfiguration() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let source = catalog.scene(7).unwrap().clone();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let expected = render(&mut renderer, &project, 0.);
    assert!(expected.chunks_exact(4).any(|p| p[0] > 0 && p[3] > 0));
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    // Discard the old renderer: a new device consumes only the CPU catalog.
    drop(renderer);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.install_model_source_catalog(catalog);
    renderer.configure_assets(&project, root.path()).unwrap();
    assert!(Arc::ptr_eq(renderer.model_source_catalog().scene(7).unwrap(), &source));
    assert_eq!(render(&mut renderer, &project, 0.), expected);
    let mut changed = project.clone();
    changed.model_assets[0].sha256 = "0".repeat(64);
    assert!(renderer.configure_assets(&changed, root.path()).is_err());
    assert!(Arc::ptr_eq(renderer.model_source_catalog().scene(7).unwrap(), &source));
    assert_eq!(render(&mut renderer, &project, 0.), expected);
}
