#[path = "support/scene_packages.rs"]
mod scene_fixtures;
use motion_effects::{builtin, EffectPackage, Registry};
use std::{io::Write, sync::Arc};

#[test]
fn archived_packages_keep_their_exact_identity_after_explicit_sdk_registration() {
    let old_scene = scene_fixtures::legacy_scene_editor_package().unwrap();
    let old_particle = scene_fixtures::legacy_particle_package().unwrap();
    assert_eq!(old_scene.manifest.version, "1.1.0");
    assert_eq!(
        old_scene.hash,
        "c8c312ed6239fe65760e7c41811a9c0a1db2a548b8abe142230377bb57830bc1"
    );
    assert_eq!(old_particle.manifest.version, "1.0.0");
    assert_eq!(
        old_particle.hash,
        "37047f16cb9306b7e15d73c343f4f76fbf26c4ddfee4e1c2d3fe973939760e8b"
    );
    let scene = scene_fixtures::scene_package().unwrap();
    let particle = scene_fixtures::particle_package().unwrap();
    assert_eq!(scene.manifest.version, "1.1.1");
    assert_eq!(particle.manifest.version, "1.0.1");
    for (old, current) in [(&old_scene, &scene), (&old_particle, &particle)] {
        let mut manifest = old.manifest.clone();
        manifest.version = current.manifest.version.clone();
        assert_eq!(
            manifest, current.manifest,
            "archive normalization must not change effect contracts"
        );
        for (name, bytes) in &old.files {
            if name != "manifest.json" {
                assert_eq!(bytes, &current.files[name], "resource changed: {name}");
            }
        }
    }
    // Retired caches no longer reinstall themselves after an app upgrade.
    // The SDK can still register archived packages explicitly for regression
    // tests; the original byte/hash/version assertions continue to apply.
    let directory = tempfile::tempdir().unwrap();
    for old in [&old_scene, &old_particle] {
        std::fs::write(
            directory.path().join(format!("{}.msfx", old.hash)),
            &old.bytes,
        )
        .unwrap();
    }
    let mut registry = Registry::load(directory.path()).unwrap();
    assert!(registry.diagnostics.is_empty());
    for package in [
        old_scene,
        old_particle,
        scene,
        particle,
        scene_fixtures::legacy_scene_package().unwrap(),
    ] {
        assert!(registry
            .resolve(&package.manifest.id, &package.manifest.version, &package.hash)
            .err().expect("retired archives must not reload automatically")
            .to_string().contains("retired bundled plugin"));
        registry.insert(package.clone()).unwrap();
        let resolved = registry
            .resolve(
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
            )
            .unwrap();
        assert_eq!(resolved.bytes, package.bytes);
    }
    assert_eq!(
        builtin::package().unwrap().hash,
        "d492322ae4ee5254b008bc734def9d686313e8e3fece8dd9d9597c8e774dd821"
    );
}

#[test]
fn archive_normalization_cannot_replace_the_same_published_version() {
    let old = scene_fixtures::legacy_scene_editor_package().unwrap();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in &old.files {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    let normalized =
        Arc::new(EffectPackage::from_bytes(writer.finish().unwrap().into_inner()).unwrap());
    assert_ne!(normalized.hash, old.hash);
    let mut registry = scene_fixtures::registry().unwrap();
    let error = registry.insert(normalized).unwrap_err().to_string();
    assert!(error.contains("increment the version"), "{error}");
}
