use motion_effects::{builtin, EffectPackage, Registry};
use std::{io::Write, path::Path};

fn archives() -> [&'static [u8]; 6] {
    [
        include_bytes!("../library/legacy/core-effects-1.0.0.msfx"),
        include_bytes!("../library/legacy/core-effects-1.1.0.msfx"),
        include_bytes!("../library/legacy/core-effects-1.2.0.msfx"),
        include_bytes!("../library/legacy/core-effects-1.3.0.msfx"),
        include_bytes!("../library/legacy/core-effects-1.4.0.msfx"),
        include_bytes!("../library/core-effects.msfx"),
    ]
}

fn assert_current(registry: &Registry) {
    let package = builtin::package().unwrap();
    assert!(
        registry.diagnostics.is_empty(),
        "{:?}",
        registry.diagnostics
    );
    assert_eq!(registry.packages.len(), 1);
    assert_eq!(
        registry
            .resolve(
                &package.manifest.id,
                &package.manifest.version,
                &package.hash
            )
            .unwrap()
            .bytes,
        package.bytes,
    );
    assert_eq!(
        registry
            .packages
            .values()
            .next()
            .unwrap()
            .manifest
            .effects
            .len(),
        95
    );
}

#[test]
fn runtime_installs_only_the_current_builtin_and_restores_disabled_settings() {
    let directory = tempfile::tempdir().unwrap();
    let mut registry = Registry::load_current(directory.path()).unwrap();
    assert_current(&registry);
    let package = builtin::package().unwrap();
    let installed: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        installed,
        vec![std::ffi::OsString::from(format!("{}.msfx", package.hash))]
    );
    registry
        .enable(
            directory.path(),
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            false,
        )
        .unwrap();
    let restored = Registry::load_current(directory.path()).unwrap();
    assert_eq!(restored.packages.len(), 1);
    assert!(restored
        .resolve(
            &package.manifest.id,
            &package.manifest.version,
            &package.hash
        )
        .err()
        .expect("disabled current builtin resolved")
        .to_string()
        .contains("disabled"));
}

#[test]
fn runtime_skips_all_exact_archives_even_when_renamed_and_keeps_their_bytes() {
    let directory = tempfile::tempdir().unwrap();
    for (i, bytes) in archives().iter().enumerate() {
        std::fs::write(directory.path().join(format!("renamed-{i}.msfx")), bytes).unwrap();
    }
    for _ in 0..2 {
        assert_current(&Registry::load_current(directory.path()).unwrap());
        for (i, bytes) in archives().iter().enumerate() {
            assert_eq!(
                std::fs::read(directory.path().join(format!("renamed-{i}.msfx"))).unwrap(),
                *bytes
            );
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 7);
    }
}

#[test]
fn runtime_reports_damaged_archives_and_current_cache_without_erasing_them() {
    let directory = tempfile::tempdir().unwrap();
    let current = builtin::package().unwrap();
    use sha2::{Digest, Sha256};
    let archived_hash = format!("{:x}", Sha256::digest(archives()[0]));
    let damaged = b"not a package";
    for name in [
        format!("{archived_hash}.msfx"),
        format!("{}.msfx", current.hash),
    ] {
        std::fs::write(directory.path().join(name), damaged).unwrap();
    }
    std::fs::write(directory.path().join("disabled.json"), b"invalid settings").unwrap();
    let registry = Registry::load_current(directory.path()).unwrap();
    assert_eq!(registry.packages.len(), 1);
    assert_eq!(registry.diagnostics.len(), 3, "{:?}", registry.diagnostics);
    assert!(registry
        .diagnostics
        .iter()
        .any(|cause| cause.contains("bundled package cache is corrupt")));
    assert!(registry
        .diagnostics
        .iter()
        .any(|cause| cause.contains(&archived_hash)));
    assert!(registry
        .diagnostics
        .iter()
        .any(|cause| cause.contains(&current.hash)));
    assert!(registry
        .diagnostics
        .iter()
        .any(|cause| cause.contains("disabled.json")));
    assert!(registry
        .diagnostics
        .iter()
        .any(|cause| cause.contains("zip") || cause.contains("Zip")));
    for name in [
        format!("{archived_hash}.msfx"),
        format!("{}.msfx", current.hash),
    ] {
        assert_eq!(std::fs::read(directory.path().join(name)).unwrap(), damaged);
    }
}

fn external_package(root: &Path, invalid_shader: bool) -> Vec<u8> {
    external_package_named(root, invalid_shader, "test.runtime-external")
}

fn external_package_named(root: &Path, invalid_shader: bool, id: &str) -> Vec<u8> {
    let current = builtin::package().unwrap();
    let mut manifest = current.manifest.clone();
    manifest.id = id.into();
    manifest.version = "1.0.0".into();
    manifest.effects.truncate(1);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    let shaders: std::collections::BTreeSet<_> = manifest
        .effects
        .iter()
        .flat_map(|effect| effect.passes.iter().map(|pass| &pass.shader))
        .collect();
    for path in shaders {
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(if invalid_shader {
                b"invalid WGSL"
            } else {
                &current.files[path]
            })
            .unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    std::fs::write(root.join("external.msfx"), &bytes).unwrap();
    bytes
}

#[test]
fn runtime_validates_external_shaders_and_preserves_package_identity() {
    let directory = tempfile::tempdir().unwrap();
    let valid = external_package(directory.path(), false);
    let package = EffectPackage::from_bytes(valid).unwrap();
    let registry = Registry::load_current(directory.path()).unwrap();
    assert!(registry.diagnostics.is_empty());
    assert_eq!(registry.packages.len(), 2);
    assert_eq!(
        registry
            .resolve(
                &package.manifest.id,
                &package.manifest.version,
                &package.hash
            )
            .unwrap()
            .bytes,
        package.bytes
    );
    let invalid = external_package(directory.path(), true);
    let registry = Registry::load_current(directory.path()).unwrap();
    assert_eq!(registry.packages.len(), 1);
    assert_eq!(registry.diagnostics.len(), 1);
    let cause = &registry.diagnostics[0];
    assert!(
        cause.contains(&format!("effect {}", package.manifest.effects[0].id))
            && cause.contains("pass 0"),
        "{cause}"
    );
    assert_eq!(
        std::fs::read(directory.path().join("external.msfx")).unwrap(),
        invalid
    );
}

#[test]
fn runtime_install_refresh_does_not_restore_archives_and_keeps_other_sessions_plugins() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let bytes = external_package(source.path(), false);
    let package = EffectPackage::from_bytes(bytes).unwrap();
    let mut first = Registry::load_current(directory.path()).unwrap();
    let mut other = Registry::load_current(directory.path()).unwrap();
    std::fs::write(directory.path().join("archived.msfx"), archives()[0]).unwrap();
    other
        .install_current_with_limits(
            directory.path(),
            &source.path().join("external.msfx"),
            motion_effects::PackageReadLimits::default(),
        )
        .unwrap();
    let second = EffectPackage::from_bytes(external_package_named(
        source.path(),
        false,
        "test.runtime-second",
    ))
    .unwrap();
    // The first registry predates the other session's installation. Refresh
    // must discover it without installing any of the six archived builtins.
    first
        .install_current_with_limits(
            directory.path(),
            &source.path().join("external.msfx"),
            motion_effects::PackageReadLimits::default(),
        )
        .unwrap();
    assert_eq!(first.packages.len(), 3);
    assert_eq!(
        first
            .resolve(
                &package.manifest.id,
                &package.manifest.version,
                &package.hash
            )
            .unwrap()
            .bytes,
        package.bytes
    );
    assert_eq!(
        first
            .resolve(&second.manifest.id, &second.manifest.version, &second.hash)
            .unwrap()
            .bytes,
        second.bytes
    );
    let restored = Registry::load_current(directory.path()).unwrap();
    assert_eq!(restored.packages.len(), 3);
    assert!(restored.diagnostics.is_empty());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 4);
    assert_eq!(
        std::fs::read(directory.path().join("archived.msfx")).unwrap(),
        archives()[0]
    );
}

#[test]
fn runtime_archive_install_is_refused_before_registry_or_disk_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let mut registry = Registry::load_current(directory.path()).unwrap();
    for bytes in archives() {
        let path = source.path().join("archived.msfx");
        std::fs::write(&path, bytes).unwrap();
        let error = registry
            .install_current_with_limits(
                directory.path(),
                &path,
                motion_effects::PackageReadLimits::default(),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("archived builtin is not installable"),
            "{error}"
        );
        assert_current(&registry);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
