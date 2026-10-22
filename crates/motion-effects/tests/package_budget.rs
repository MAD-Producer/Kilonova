use motion_effects::{
    builtin, package_directory, package_directory_with_limits, EffectPackage, PackageReadLimits,
    Registry, DEFAULT_DECODED_IMAGE_BUDGET,
};
use std::{collections::BTreeSet, path::Path};

fn fixture(width: u32, height: u32, editor: bool, shared: bool) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let core = builtin::package().unwrap();
    let mut manifest = core.manifest.clone();
    manifest.id = "test.package-budget".into();
    manifest.version = "1.0.0".into();
    manifest.effects.truncate(1);
    manifest.effects[0].resources = vec!["assets/pixels.png".into()];
    if editor {
        manifest.effects[0].editor = Some(motion_effects::EditorDefinition {
            id: "budget-editor".into(),
            protocol: motion_effects::EDITOR_PROTOCOL,
            entry: "ui/index.html".into(),
            files: vec!["ui/index.html".into(), "ui/pixels.png".into()],
            title: "Generated budget fixture".into(),
        });
        std::fs::create_dir(directory.path().join("ui")).unwrap();
        std::fs::write(
            directory.path().join("ui/index.html"),
            "<!doctype html><title>Budget fixture</title>",
        )
        .unwrap();
    }
    if shared {
        let mut other = manifest.effects[0].clone();
        other.id = "shared-resource".into();
        manifest.effects.push(other);
    }
    let shaders: BTreeSet<_> = manifest
        .effects
        .iter()
        .flat_map(|effect| effect.passes.iter().map(|pass| pass.shader.clone()))
        .collect();
    for shader in shaders {
        let destination = directory.path().join(&shader);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, &core.files[&shader]).unwrap();
    }
    std::fs::create_dir(directory.path().join("assets")).unwrap();
    let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([17, 43, 91, 255]));
    pixels
        .save(directory.path().join("assets/pixels.png"))
        .unwrap();
    if editor {
        std::fs::copy(
            directory.path().join("assets/pixels.png"),
            directory.path().join("ui/pixels.png"),
        )
        .unwrap();
    }
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    directory
}

fn refused(error: motion_effects::Error, name: &str, required: u64, budget: u64) {
    match error {
        motion_effects::Error::DecodedResourceBudget {
            package,
            resource,
            required_bytes,
            budget_bytes,
        } => {
            assert_eq!(package, "test.package-budget");
            assert_eq!(resource, name);
            assert_eq!(required_bytes, required);
            assert_eq!(budget_bytes, budget);
        }
        other => panic!("expected typed package refusal, got {other}"),
    }
}

#[test]
fn editor_and_effect_pngs_share_configured_capacity_and_unique_paths_count_once() {
    let source = fixture(2, 2, true, true);
    refused(
        package_directory_with_limits(source.path(), PackageReadLimits::new(15).unwrap())
            .unwrap_err(),
        "ui/pixels.png",
        16,
        15,
    );
    refused(
        package_directory_with_limits(source.path(), PackageReadLimits::new(31).unwrap())
            .unwrap_err(),
        "assets/pixels.png",
        32,
        31,
    );
    let bytes =
        package_directory_with_limits(source.path(), PackageReadLimits::new(32).unwrap()).unwrap();
    let package =
        EffectPackage::from_bytes_with_limits(bytes, PackageReadLimits::new(32).unwrap()).unwrap();
    assert_eq!(package.manifest.effects.len(), 2);
    assert_eq!(
        package.files["assets/pixels.png"],
        package.files["ui/pixels.png"]
    );
    // Duplicate references from the second effect consume no additional capacity.
    assert_eq!(
        package.manifest.effects[0].resources,
        package.manifest.effects[1].resources
    );
}

#[test]
fn configured_package_png_can_cross_the_previous_128_mib_cap() {
    let source = fixture(8192, 4097, false, false);
    let required = 8192_u64 * 4097 * 4;
    assert!(required > DEFAULT_DECODED_IMAGE_BUDGET);
    refused(
        package_directory(source.path()).unwrap_err(),
        "assets/pixels.png",
        required,
        DEFAULT_DECODED_IMAGE_BUDGET,
    );
    let limits = PackageReadLimits::new(required).unwrap();
    let bytes = package_directory_with_limits(source.path(), limits).unwrap();
    let package = EffectPackage::from_bytes_with_limits(bytes.clone(), limits).unwrap();
    assert_eq!(package.bytes, bytes);
    let pixels = image::load_from_memory(&package.files["assets/pixels.png"])
        .unwrap()
        .into_rgba8();
    assert_eq!(pixels.dimensions(), (8192, 4097));
    assert!(pixels.pixels().all(|pixel| pixel.0 == [17, 43, 91, 255]));
}

#[test]
fn package_refusal_is_atomic_and_registry_reload_uses_the_same_limit() {
    let source = fixture(2, 2, true, false);
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.msfx");
    let original = package_directory(source.path()).unwrap();
    std::fs::write(&input, &original).unwrap();
    let store = directory.path().join("plugins");
    let mut registry = Registry::new_with_builtins().unwrap();
    let before: Vec<_> = registry.packages.keys().cloned().collect();
    refused(
        registry
            .install_with_limits(&store, &input, PackageReadLimits::new(31).unwrap())
            .unwrap_err(),
        "assets/pixels.png",
        32,
        31,
    );
    assert!(!store.exists());
    assert_eq!(
        registry.packages.keys().cloned().collect::<Vec<_>>(),
        before
    );
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let installed = registry
        .install_with_limits(&store, &input, PackageReadLimits::new(32).unwrap())
        .unwrap();
    let loaded = Registry::load_with_limits(&store, PackageReadLimits::new(32).unwrap()).unwrap();
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(
        loaded
            .resolve(&installed.id, &installed.version, &installed.hash)
            .unwrap()
            .bytes,
        original
    );
    assert_eq!(
        std::fs::read(store.join(format!("{}.msfx", installed.hash))).unwrap(),
        original
    );
}

#[test]
fn invalid_limits_and_corrupt_pngs_are_not_typed_capacity_refusals() {
    for budget in [0, 3, motion_effects::MAX_DECODED_IMAGE_BUDGET + 1, u64::MAX] {
        assert!(matches!(
            PackageReadLimits::new(budget),
            Err(motion_effects::Error::Invalid(_))
        ));
    }
    let source = fixture(2, 2, false, false);
    std::fs::write(
        source.path().join("assets/pixels.png"),
        b"invalid PNG pixels",
    )
    .unwrap();
    let error = package_directory_with_limits(source.path(), PackageReadLimits::new(16).unwrap())
        .unwrap_err();
    assert!(!matches!(
        error,
        motion_effects::Error::DecodedResourceBudget { .. }
    ));
    assert!(error.to_string().contains("resource assets/pixels.png"));
}

fn tool(arguments: &[&str], directory: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_effect_tool"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .unwrap()
}

#[test]
fn pack_and_check_cli_take_explicit_limits_and_refusal_preserves_existing_output() {
    let source = fixture(2, 2, true, false);
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("effect.msfx");
    std::fs::write(&path, b"existing publication").unwrap();
    let source_path = source.path().to_str().unwrap();
    let destination = path.to_str().unwrap();
    let rejected = tool(
        &[
            "pack",
            source_path,
            destination,
            "--decoded-image-budget-bytes",
            "31",
        ],
        output.path(),
    );
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("required_bytes=32 budget_bytes=31"));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing publication");
    let accepted = tool(
        &[
            "pack",
            source_path,
            destination,
            "--decoded-image-budget-bytes",
            "32",
        ],
        output.path(),
    );
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let before = std::fs::read(&path).unwrap();
    assert!(!tool(
        &["check", destination, "--decoded-image-budget-bytes", "31"],
        output.path()
    )
    .status
    .success());
    assert!(tool(
        &["check", destination, "--decoded-image-budget-bytes", "32"],
        output.path()
    )
    .status
    .success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
