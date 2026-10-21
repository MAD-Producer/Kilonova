#[path = "support/scene_packages.rs"]
mod scene_fixtures;
use motion_effects::{builtin, EffectPackage, Registry};
use std::collections::BTreeSet;

#[test]
fn default_catalogue_contains_no_lens_or_particle_effects() {
    let registry = Registry::new_with_builtins().unwrap();
    let mut effects = BTreeSet::new();
    for package in registry.packages.values() {
        assert_eq!(package.manifest.id, builtin::PLUGIN_ID);
        effects.extend(package.manifest.effects.iter().map(|e| e.id.as_str()));
    }
    assert_eq!(effects.len(), 95);
    for id in [
        "lens_flare",
        "particle_emitter",
        "starfield",
        "sparks",
        "dust",
        "snow",
        "energy",
    ] {
        assert!(!effects.contains(id), "retired effect rediscovered: {id}");
    }
}

#[test]
fn upgrading_preserves_old_files_without_reinstalling_removed_builtins() {
    let directory = tempfile::tempdir().unwrap();
    let packages = [
        scene_fixtures::legacy_scene_package().unwrap(),
        scene_fixtures::legacy_scene_editor_package().unwrap(),
        scene_fixtures::scene_package().unwrap(),
        scene_fixtures::legacy_particle_package().unwrap(),
        scene_fixtures::particle_package().unwrap(),
    ];
    for package in &packages {
        std::fs::write(
            directory.path().join(format!("{}.msfx", package.hash)),
            &package.bytes,
        )
        .unwrap();
    }
    for _ in 0..2 {
        let registry = Registry::load(directory.path()).unwrap();
        assert!(
            registry.diagnostics.is_empty(),
            "intentional retirement is not a load failure"
        );
        for package in &packages {
            let error = registry
                .resolve(
                    &package.manifest.id,
                    &package.manifest.version,
                    &package.hash,
                )
                .err()
                .expect("cached retired library must stay unavailable")
                .to_string();
            assert!(error.contains("retired bundled plugin"), "{error}");
            assert!(
                error.contains(&package.manifest.id) && error.contains(&package.hash),
                "{error}"
            );
            assert_eq!(
                std::fs::read(directory.path().join(format!("{}.msfx", package.hash))).unwrap(),
                package.bytes
            );
        }
    }
}

#[test]
fn retired_import_is_rejected_before_mutating_registry_or_disk() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("retired.msfx");
    let destination = directory.path().join("plugins");
    let package = scene_fixtures::particle_package().unwrap();
    std::fs::write(&source, &package.bytes).unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    let before = registry.packages.keys().cloned().collect::<Vec<_>>();
    let error = registry
        .install(&destination, &source)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("retired bundled plugin") && error.contains(&package.manifest.id),
        "{error}"
    );
    assert_eq!(
        registry.packages.keys().cloned().collect::<Vec<_>>(),
        before
    );
    assert!(!destination.exists());
    assert_eq!(std::fs::read(source).unwrap(), package.bytes);
}

#[test]
fn independent_sdk_scene_packages_still_install_reload_and_resolve() {
    let directory = tempfile::tempdir().unwrap();
    let original = scene_fixtures::particle_package().unwrap();
    for (name, bytes) in &original.files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut manifest = original.manifest.clone();
    manifest.id = "example.user-emitter".into();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = motion_effects::package_directory(directory.path()).unwrap();
    let package = EffectPackage::from_bytes(bytes.clone()).unwrap();
    let source = directory.path().join("user.msfx");
    std::fs::write(&source, bytes).unwrap();
    let destination = directory.path().join("plugins");
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.install(&destination, &source).unwrap();
    let reloaded = Registry::load(&destination).unwrap();
    let resolved = reloaded
        .resolve(
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
        )
        .unwrap();
    assert_eq!(resolved.bytes, package.bytes);
    assert_eq!(
        resolved.manifest.effects[0].renderer,
        motion_effects::RendererKind::ParticleEmitter
    );
    assert!(resolved.manifest.effects[0].native_editor.is_some());
}

#[test]
fn corrupted_retired_cache_remains_an_observed_load_failure() {
    let directory = tempfile::tempdir().unwrap();
    let package = scene_fixtures::scene_package().unwrap();
    let path = directory.path().join(format!("{}.msfx", package.hash));
    std::fs::write(&path, b"corrupt cache").unwrap();
    let registry = Registry::load(directory.path()).unwrap();
    assert_eq!(registry.diagnostics.len(), 1);
    assert!(!registry.diagnostics[0].is_empty());
    assert_eq!(std::fs::read(path).unwrap(), b"corrupt cache");
}
