use motion_effects::{EffectPackage, Registry};
use motion_graph::*;
use std::sync::Arc;

#[test]
fn actual_msfx_reload_registry_and_sibling_source_preserve_public_parameters() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("circle.msfx");
    let graph = demo_graph();
    let options = CompileOptions::default();
    let report = export_msfx(&graph, &options, &output).unwrap();
    assert!(report.bytes > 0);
    assert_eq!(GraphDocument::load(&report.source_path).unwrap(), graph);
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    assert_eq!(package.hash, report.package_hash);
    assert_eq!(package.files.len(), 2);
    assert_eq!(package.manifest.sdk_version, 6);
    assert_eq!(
        package.manifest.effects[0].params.len(),
        graph.public_parameters.len()
    );
    assert!(package.manifest.effects[0].native_editor.is_some());
    assert!(package.manifest.effects[0]
        .params
        .iter()
        .all(|p| p.animatable));
    let shader = &package.shaders[&(graph.id.clone(), 0)];
    assert!(shader.glsl.fragment.contains("#version 300 es"));
    assert_eq!(shader.loop_work, report.compile.loop_work);
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    assert_eq!(
        registry
            .resolve(&report.plugin_id, &report.version, &report.package_hash)
            .unwrap()
            .bytes,
        package.bytes
    );
    let same = export_msfx(&graph, &options, &output).unwrap();
    assert_eq!(same.package_hash, report.package_hash);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    let package_before = std::fs::read(&output).unwrap();
    let source_before = std::fs::read(&report.source_path).unwrap();
    let mut changed = graph.clone();
    changed.public_parameters[0].default[0] = 18.0;
    assert_eq!(
        export_msfx(&changed, &options, &output)
            .err()
            .unwrap()
            .diagnostic()
            .code,
        ErrorCode::Publication
    );
    assert_eq!(std::fs::read(&output).unwrap(), package_before);
    assert_eq!(std::fs::read(&report.source_path).unwrap(), source_before);
    let next = CompileOptions {
        version: "0.1.1".into(),
        ..options
    };
    let next_report = export_msfx(&changed, &next, &output).unwrap();
    assert_ne!(next_report.package_hash, report.package_hash);
    assert_eq!(next_report.version, "0.1.1");
}
