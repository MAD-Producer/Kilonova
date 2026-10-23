use motion_graph::{demo_graph, export_msfx, CompileOptions, ErrorCode};
use std::{error::Error as _, fs, io::Write};

fn effect_cause(error: &motion_graph::Error) -> &motion_effects::Error {
    let source = error
        .source()
        .expect("graph boundary discarded the native effect cause");
    source
        .downcast_ref::<Box<motion_effects::Error>>()
        .expect("graph effect source must retain its declared Box wrapper")
        .as_ref()
}

#[test]
fn existing_corrupt_zip_keeps_native_loader_cause_and_preserves_both_destinations() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("existing.msfx");
    let source = output.with_extension("msgraph");
    fs::write(&output, b"this is not a ZIP archive").unwrap();
    fs::write(&source, b"previous editable graph").unwrap();
    let error = export_msfx(&demo_graph(), &CompileOptions::default(), &output).unwrap_err();
    assert_eq!(error.diagnostic().code, ErrorCode::Publication);
    assert!(matches!(
        effect_cause(&error),
        motion_effects::Error::Zip(_)
    ));
    assert_eq!(
        error.to_string(),
        format!(
            "Publication: existing destination is not a valid effect package: {}",
            effect_cause(&error)
        )
    );
    assert_eq!(fs::read(&output).unwrap(), b"this is not a ZIP archive");
    assert_eq!(fs::read(&source).unwrap(), b"previous editable graph");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn existing_corrupt_manifest_retains_native_json_location_without_overwriting_files() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("manifest.msfx");
    let source = output.with_extension("msgraph");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"{\n  \"format_version\":\n}").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    fs::write(&output, &bytes).unwrap();
    fs::write(&source, b"old source").unwrap();
    let error = export_msfx(&demo_graph(), &CompileOptions::default(), &output).unwrap_err();
    let motion_effects::Error::Json(json) = effect_cause(&error) else {
        panic!("native JSON error missing: {error:?}");
    };
    assert_eq!(json.line(), 3);
    assert!(json.column() > 0 && json.is_syntax());
    assert_eq!(error.diagnostic().code, ErrorCode::Publication);
    assert_eq!(fs::read(&output).unwrap(), bytes);
    assert_eq!(fs::read(source).unwrap(), b"old source");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn version_identity_refusal_keeps_the_registry_cause_and_explicit_next_version_recovers() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("identity.msfx");
    let graph = demo_graph();
    let before = graph.clone();
    let options = CompileOptions::default();
    let first = export_msfx(&graph, &options, &output).unwrap();
    let package = fs::read(&output).unwrap();
    let source = fs::read(&first.source_path).unwrap();
    let mut changed = graph.clone();
    changed.public_parameters[0].default[0] = 18.;
    let error = export_msfx(&changed, &options, &output).unwrap_err();
    assert!(matches!(
        effect_cause(&error),
        motion_effects::Error::Invalid(_)
    ));
    assert_eq!(error.diagnostic().code, ErrorCode::Publication);
    assert_eq!(
        error.to_string(),
        format!(
            "Publication: increment the package version before replacing its contents: {}",
            effect_cause(&error)
        )
    );
    assert_eq!(graph, before);
    assert_eq!(fs::read(&output).unwrap(), package);
    assert_eq!(fs::read(&first.source_path).unwrap(), source);
    let next = export_msfx(
        &changed,
        &CompileOptions {
            version: "0.1.1".into(),
            ..options
        },
        &output,
    )
    .unwrap();
    assert_ne!(next.package_hash, first.package_hash);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}
