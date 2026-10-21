#[path = "support/scene_packages.rs"]
mod scene_fixtures;
use motion_effects::{EffectPackage, Registry};
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{Arc, OnceLock},
};

fn package() -> Arc<EffectPackage> {
    static PACKAGE: OnceLock<Arc<EffectPackage>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            let original = scene_fixtures::particle_package().unwrap();
            let mut manifest = original.manifest.clone();
            manifest.id = "test.registry-transaction".into();
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (path, bytes) in &original.files {
                writer
                    .start_file(path, zip::write::SimpleFileOptions::default())
                    .unwrap();
                if path == "manifest.json" {
                    writer
                        .write_all(&serde_json::to_vec(&manifest).unwrap())
                        .unwrap();
                } else {
                    writer.write_all(bytes).unwrap();
                }
            }
            Arc::new(EffectPackage::from_bytes(writer.finish().unwrap().into_inner()).unwrap())
        })
        .clone()
}

fn installed(directory: &Path) -> (Registry, Arc<EffectPackage>) {
    let package = package();
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    fs::write(
        directory.join(format!("{}.msfx", package.hash)),
        &package.bytes,
    )
    .unwrap();
    registry
        .enable(
            directory,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            false,
        )
        .unwrap();
    (registry, package)
}

#[test]
fn metadata_failure_never_deletes_the_package_or_changes_disabled_state() {
    let directory = tempfile::tempdir().unwrap();
    let (mut registry, package) = installed(directory.path());
    let disabled = registry.disabled.clone();
    let metadata = directory.path().join("disabled.json");
    fs::remove_file(&metadata).unwrap();
    fs::create_dir(&metadata).unwrap();
    assert!(registry
        .uninstall(
            directory.path(),
            &package.manifest.id,
            &package.manifest.version,
            &package.hash
        )
        .is_err());
    assert_eq!(registry.disabled, disabled);
    assert_eq!(
        fs::read(directory.path().join(format!("{}.msfx", package.hash))).unwrap(),
        package.bytes
    );
    assert_eq!(registry.packages.len(), 1);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn failed_package_removal_restores_disabled_metadata_and_keeps_registry_entry() {
    let directory = tempfile::tempdir().unwrap();
    let (mut registry, package) = installed(directory.path());
    let before = registry.disabled.clone();
    let path = directory.path().join(format!("{}.msfx", package.hash));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(registry
        .uninstall(
            directory.path(),
            &package.manifest.id,
            &package.manifest.version,
            &package.hash
        )
        .is_err());
    assert_eq!(registry.disabled, before);
    let persisted = serde_json::from_slice::<std::collections::BTreeSet<(String, String, String)>>(
        &fs::read(directory.path().join("disabled.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted, before);
    assert_eq!(registry.packages.len(), 1);
}

#[test]
fn successful_uninstall_removes_payload_and_both_registry_sets() {
    let directory = tempfile::tempdir().unwrap();
    let (mut registry, package) = installed(directory.path());
    registry
        .uninstall(
            directory.path(),
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
        )
        .unwrap();
    assert!(!directory
        .path()
        .join(format!("{}.msfx", package.hash))
        .exists());
    assert!(registry.packages.is_empty());
    assert!(registry.disabled.is_empty());
    assert_eq!(
        fs::read(directory.path().join("disabled.json")).unwrap(),
        b"[]"
    );
}
