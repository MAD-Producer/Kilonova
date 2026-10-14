use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
};

fn closure(name: &str, packages: &BTreeMap<String, Value>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![name.to_owned()];
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(package) = packages.get(&name) {
            if let Some(dependencies) = package["dependencies"].as_array() {
                for dependency in dependencies {
                    if dependency["kind"].as_str() != Some("dev") {
                        if let Some(name) = dependency["name"].as_str() {
                            pending.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }
    seen
}

#[test]
fn production_dependencies_keep_data_render_and_media_outside_the_editing_runtime() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or_else(|| panic!("motion-model manifest is not inside the workspace"));
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .current_dir(workspace)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke cargo metadata: {error}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("cargo metadata returned invalid JSON: {error}"));
    let packages: BTreeMap<_, _> = metadata["packages"]
        .as_array()
        .unwrap_or_else(|| panic!("cargo metadata has no packages array"))
        .iter()
        .filter_map(|p| p["name"].as_str().map(|name| (name.to_owned(), p.clone())))
        .collect();
    for name in ["motion-model", "motion-render", "motion-media"] {
        let dependencies = closure(name, &packages);
        for forbidden in [
            "motion-core",
            "motion-host",
            "motion-android",
            "motion-desktop",
        ] {
            assert!(
                !dependencies.contains(forbidden),
                "{name} depends on {forbidden}"
            );
        }
        assert!(
            !dependencies
                .iter()
                .any(|d| d.starts_with("rquickjs") || d.starts_with("swc_")),
            "{name} contains a JS runtime dependency"
        );
    }
    let ui = closure("motion-ui", &packages);
    for forbidden in [
        "motion-core",
        "motion-host",
        "motion-android",
        "motion-desktop",
        "motion-render",
        "motion-media",
    ] {
        assert!(!ui.contains(forbidden), "motion-ui depends on {forbidden}");
    }
    assert!(
        !closure("motion-desktop", &packages).contains("motion-android"),
        "motion-desktop must not pull the Android crate"
    );
    let model = closure("motion-model", &packages);
    assert!(!model.contains("motion-render") && !model.contains("motion-media"));
    assert!(
        !closure("motion-media", &packages).contains("motion-render"),
        "motion-media must not depend on motion-render"
    );
    let core = closure("motion-core", &packages);
    assert!(core.contains("rquickjs") && core.contains("swc_ecma_parser"));
    for forbidden in [
        "motion-render",
        "motion-media",
        "wgpu",
        "winit",
        "egui",
        "cpal",
    ] {
        assert!(!core.contains(forbidden), "core depends on {forbidden}");
    }
    let effects = packages
        .get("motion-effects")
        .unwrap_or_else(|| panic!("cargo metadata omitted motion-effects"));
    assert!(!effects["dependencies"]
        .as_array()
        .unwrap_or_else(|| panic!("motion-effects has no dependency array"))
        .iter()
        .any(|d| d["kind"].as_str() != Some("dev")
            && d["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("motion-"))));
}
