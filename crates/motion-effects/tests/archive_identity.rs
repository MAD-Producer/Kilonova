use motion_effects::{builtin, EffectPackage, Registry};
use std::{io::Write, sync::Arc};

#[test]
fn normalized_scene_and_particle_versions_keep_every_published_identity_resolvable() {
    let old_scene = builtin::legacy_scene_editor_package().unwrap();
    let old_particle = builtin::legacy_particle_package().unwrap();
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
    let scene = builtin::scene_package().unwrap();
    let particle = builtin::particle_package().unwrap();
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
    // An upgrade must work even when the old package was already persisted by
    // the previous app version. It retains that identity alongside the new one.
    let directory = tempfile::tempdir().unwrap();
    for old in [&old_scene, &old_particle] {
        std::fs::write(
            directory.path().join(format!("{}.msfx", old.hash)),
            &old.bytes,
        )
        .unwrap();
    }
    let registry = Registry::load(directory.path()).unwrap();
    assert!(registry.diagnostics.is_empty());
    for package in [
        old_scene,
        old_particle,
        scene,
        particle,
        builtin::legacy_scene_package().unwrap(),
    ] {
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
    let old = builtin::legacy_scene_editor_package().unwrap();
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
    let mut registry = Registry::new_with_builtins().unwrap();
    let error = registry.insert(normalized).unwrap_err().to_string();
    assert!(error.contains("increment the version"), "{error}");
}
