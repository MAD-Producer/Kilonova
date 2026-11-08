use motion_effects::{
    color_config::{ColorConfigManifest, ColorSelection, ContentIdentity, VerifiedColorConfig},
    color_processor::{ColorProcessorPhase, ColorProcessorSource},
    native_color::{Error, NativeColorRuntime},
    AlphaMode,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Deserialize)]
struct Config {
    manifest: ColorConfigManifest,
    config: String,
    selection: ColorSelection,
}
#[derive(Deserialize)]
struct Case {
    config: usize,
    processor: ColorProcessorSource,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
#[derive(Deserialize)]
struct Reference {
    configs: Vec<Config>,
    cases: Vec<Case>,
    absolute_tolerance: f32,
    relative_tolerance: f32,
}
fn reference() -> Reference {
    serde_json::from_slice(include_bytes!("color_processor/reference.json")).unwrap()
}
fn runtime() -> NativeColorRuntime {
    let manifest = PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit pinned runtime manifest"),
    );
    NativeColorRuntime::load(&manifest).unwrap()
}
fn config(item: &Config) -> VerifiedColorConfig {
    VerifiedColorConfig::new(
        item.manifest.clone(),
        Arc::from(item.config.as_bytes()),
        BTreeMap::new(),
    )
    .unwrap()
}

#[test]
fn missing_runtime_preserves_the_original_io_cause() {
    let directory = tempfile::tempdir().unwrap();
    let error = NativeColorRuntime::load(&directory.path().join("missing.json"))
        .err()
        .unwrap();
    assert!(
        matches!(error, Error::Io { phase: "manifest", source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    );
}

#[test]
#[ignore = "requires explicit checksum-verified native OCIO 2.6.0 prefix"]
fn ambient_overrides_are_rejected_in_owned_children_without_environment_mutation() {
    const CHILD: &str = "MOTION_OCIO_OVERRIDE_CHILD";
    const TEST: &str =
        "ambient_overrides_are_rejected_in_owned_children_without_environment_mutation";
    if let Ok(key) = std::env::var(CHILD) {
        let before = std::env::var(&key).unwrap();
        let runtime = runtime();
        let reference = reference();
        let item = &reference.configs[0];
        let config = config(item);
        let error = runtime
            .prepare(
                &config,
                &item.selection,
                ColorProcessorPhase::InputToWorking,
                AlphaMode::Straight,
                AlphaMode::Straight,
            )
            .err()
            .unwrap();
        assert!(
            matches!(error, Error::Native {category:1, phase:"prepare", ref cause} if cause.contains("ambient OCIO catalogue override")),
            "{error:?}"
        );
        assert_eq!(std::env::var(&key).unwrap(), before);
        return;
    }
    let keys = [
        "OCIO_ACTIVE_DISPLAYS",
        "OCIO_ACTIVE_VIEWS",
        "OCIO_INACTIVE_COLORSPACES",
    ];
    for key in keys {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", TEST, "--ignored", "--nocapture"]);
        for key in keys {
            child.env_remove(key);
        }
        let output = child
            .env(CHILD, key)
            .env(key, "deliberately unrelated override")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{key}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires explicit checksum-verified native OCIO 2.6.0 prefix"]
fn native_cpu_and_gpu_programs_match_unchanged_external_references() {
    let runtime = runtime();
    let reference = reference();
    let mut samples = 0;
    for (index, case) in reference.cases.iter().enumerate() {
        let item = &reference.configs[case.config];
        let config = config(item);
        let source = &case.processor;
        let native = runtime
            .prepare(
                &config,
                &item.selection,
                source.phase,
                source.input_alpha,
                source.output_alpha,
            )
            .unwrap();
        let actual = native.source();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(source).unwrap(),
            "case {index} original OCIO shader/tables/identity"
        );
        actual.compile_image(&config, &item.selection).unwrap();
        for (input, expected) in case.input.iter().zip(&case.expected) {
            let alpha = input[3];
            let mut rgb = [input[0], input[1], input[2]];
            if source.input_alpha == AlphaMode::Premultiplied {
                rgb = if alpha > 0. {
                    rgb.map(|v| v / alpha)
                } else {
                    [0.; 3]
                };
            }
            let mut output = native.apply_rgb(rgb).unwrap();
            if source.output_alpha == AlphaMode::Premultiplied {
                output = output.map(|v| v * alpha);
            }
            for channel in 0..3 {
                let tolerance = reference.absolute_tolerance
                    + reference.relative_tolerance * expected[channel].abs();
                assert!((output[channel]-expected[channel]).abs() <= tolerance, "case={index} sample={samples} channel={channel} actual={} expected={} tolerance={tolerance}", output[channel], expected[channel]);
            }
            assert_eq!(alpha, expected[3]);
            samples += 1;
        }
        assert!(matches!(
            native.apply_rgb([f32::NAN, 0., 0.]),
            Err(Error::Admission(_))
        ));
    }
    assert!(
        samples >= 100,
        "exercise real CPU results, not only preparation"
    );
    println!(
        "native OCIO cases={} samples={samples}",
        reference.cases.len()
    );
}

#[test]
#[ignore = "requires explicit checksum-verified native OCIO 2.6.0 prefix"]
fn native_parse_error_and_unselected_file_transform_are_not_silenced() {
    let runtime = runtime();
    let reference = reference();
    let item = &reference.configs[0];
    let mut manifest = item.manifest.clone();
    let bytes = b"not an OCIO configuration";
    manifest.config = ContentIdentity::of(bytes);
    let config =
        VerifiedColorConfig::new(manifest, Arc::from(bytes.as_slice()), BTreeMap::new()).unwrap();
    let mut selection = item.selection.clone();
    selection.config_key = config.key();
    let error = runtime
        .prepare(
            &config,
            &selection,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
        )
        .err()
        .unwrap();
    assert!(
        matches!(error, Error::Native {category:2, phase:"prepare", ref cause} if !cause.is_empty())
    );
    // The external transform is unselected, but must still be rejected before
    // any processor can resolve it through environment/search-path fallback.
    let bytes = item.config.replace("\nnamed_transforms:\n", "\n  - !<ColorSpace>\n    name: unselected external\n    isdata: false\n    to_scene_reference: !<FileTransform> {src: definitely-missing-original.cube}\n\nnamed_transforms:\n").into_bytes();
    assert_ne!(
        bytes,
        item.config.as_bytes(),
        "negative fixture must add a real native transform"
    );
    let mut manifest = item.manifest.clone();
    manifest.config = ContentIdentity::of(&bytes);
    let config =
        VerifiedColorConfig::new(manifest, Arc::from(bytes.as_slice()), BTreeMap::new()).unwrap();
    selection.config_key = config.key();
    let error = runtime
        .prepare(
            &config,
            &selection,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Straight,
        )
        .err()
        .unwrap();
    assert!(
        matches!(error, Error::Native {category:1, phase:"prepare", ref cause} if cause.contains("external FileTransform")),
        "{error:?}"
    );
}
