//! Real catalog failures, original native causes, unchanged owners and explicit recovery.
use super::*;
use motion_render::{ModelSourceCatalog, ModelSourceError, RenderError};
use std::{error::Error, sync::Arc};

fn corrupt_options(original: &[u8]) -> Vec<u8> {
    assert_eq!(&original[..8], b"MSCO\0\0\0\x01");
    let length = u32::from_le_bytes(original[8..12].try_into().unwrap()) as usize;
    let mut bytes = original.to_vec();
    bytes[44] = b'!';
    let digest = Sha256::digest(&bytes[44..44 + length]);
    bytes[12..44].copy_from_slice(&digest);
    bytes
}

fn missing_resource(original: &[u8]) -> Vec<u8> {
    let options = u32::from_le_bytes(original[8..12].try_into().unwrap()) as usize;
    let source = 44 + options;
    let length = u64::from_le_bytes(original[source..source + 8].try_into().unwrap()) as usize;
    let count = source + 8 + 32 + length;
    assert_eq!(
        u32::from_le_bytes(original[count..count + 4].try_into().unwrap()),
        1
    );
    let mut bytes = original[..count].to_vec();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

fn candidates(project: &Project, original: &[u8]) -> Vec<(Project, Vec<u8>, &'static str)> {
    let mut invalid = project.clone();
    invalid.model_assets[0].bytes = 0;
    let mut cases = vec![(invalid, original.to_vec(), "validate")];
    for (bytes, phase) in [
        (corrupt_options(original), "json"),
        (missing_resource(original), "io"),
    ] {
        let mut changed = project.clone();
        changed.model_assets[0].bytes = bytes.len() as u64;
        changed.model_assets[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
        changed.validate().unwrap();
        cases.push((changed, bytes, phase));
    }
    cases
}

fn cause<'a, T: Error + 'static>(error: &'a (dyn Error + 'static)) -> &'a T {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(native) = error.downcast_ref::<T>() {
            return native;
        }
        current = error.source();
    }
    panic!(
        "native source missing: {}: {error:?}",
        std::any::type_name::<T>()
    );
}

fn check_error(error: &RenderError, phase: &str) {
    assert!(error.resource_rejection().is_none());
    let source = cause::<ModelSourceError>(error);
    match (source, phase) {
        (
            ModelSourceError::Validation {
                source_id,
                cause: native,
            },
            "validate",
        ) => {
            assert_eq!(*source_id, 7);
            assert!(
                matches!(native, motion_model::Error::Invalid(message) if message == "invalid frozen model byte count")
            );
            assert!(std::ptr::eq(cause::<motion_model::Error>(error), native));
            assert_eq!(
                error.to_string(),
                format!("GPU validation: model source=7 phase=validate: {native}")
            );
        }
        (
            ModelSourceError::Import {
                source_id,
                cause: native,
            },
            "json" | "io",
        ) => {
            assert_eq!(*source_id, 7);
            assert!(std::ptr::eq(cause::<motion_scene::Error>(error), native));
            assert_eq!(
                error.to_string(),
                format!("GPU validation: model source=7 phase=import: {native}")
            );
            match &native.cause {
                motion_scene::Cause::Json(json) if phase == "json" => {
                    assert_eq!(native.phase, "unfreeze");
                    assert_eq!(native.object, "options");
                    assert_eq!(json.classify(), serde_json::error::Category::Syntax);
                    assert_eq!((json.line(), json.column()), (1, 1));
                    assert!(std::ptr::eq(cause::<serde_json::Error>(error), json));
                }
                motion_scene::Cause::Io(io) if phase == "io" => {
                    assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
                    assert_eq!(io.to_string(), "resource absent from frozen bundle");
                    assert!(std::ptr::eq(cause::<std::io::Error>(error), io));
                }
                other => panic!("unexpected native import failure: {phase}: {other:?}"),
            }
        }
        other => panic!("unexpected model source failure: {other:?}"),
    }
}

fn cpu_failures(only: Option<&str>) {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let path = root.path().join(&project.model_assets[0].path);
    let original = std::fs::read(&path).unwrap();
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let owned = catalog.scene(7).unwrap().clone();
    let project_bytes = serde_json::to_vec(&project).unwrap();
    for (candidate, bytes, phase) in candidates(&project, &original) {
        if only.is_some_and(|only| only != phase) {
            continue;
        }
        std::fs::write(&path, &bytes).unwrap();
        let before = serde_json::to_vec(&candidate).unwrap();
        let error = ModelSourceCatalog::prepare(&candidate, root.path(), &catalog).unwrap_err();
        check_error(&error, phase);
        assert_eq!(serde_json::to_vec(&candidate).unwrap(), before);
        assert_eq!(serde_json::to_vec(&project).unwrap(), project_bytes);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(Arc::ptr_eq(catalog.scene(7).unwrap(), &owned));
        assert_eq!(catalog.retained_bytes(), owned.retained_bytes());
        assert_eq!(catalog.len(), 1);
    }
    std::fs::write(&path, &original).unwrap();
    let recovered = ModelSourceCatalog::prepare(&project, root.path(), &catalog).unwrap();
    assert!(recovered.shares_sources_with(&catalog));
    assert!(Arc::ptr_eq(recovered.scene(7).unwrap(), &owned));
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn catalog_failure_retains_real_validation_cause_and_owned_sources() {
    cpu_failures(Some("validate"));
}

#[test]
fn catalog_failure_retains_real_json_cause_and_owned_sources() {
    cpu_failures(Some("json"));
}

#[test]
fn catalog_failure_retains_real_io_cause_and_owned_sources() {
    cpu_failures(Some("io"));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failed_native_reconfiguration_preserves_catalog_and_complete_rendered_pixels() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let path = root.path().join(&project.model_assets[0].path);
    let original = std::fs::read(&path).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let catalog = renderer.model_source_catalog();
    let expected = render(&mut renderer, &project, 0.);
    assert!(expected.chunks_exact(4).any(|p| p[0] == 255 && p[3] == 255));
    for (candidate, bytes, phase) in candidates(&project, &original) {
        std::fs::write(&path, &bytes).unwrap();
        let before = serde_json::to_vec(&candidate).unwrap();
        let error = renderer
            .configure_assets(&candidate, root.path())
            .unwrap_err();
        check_error(&error, phase);
        assert_eq!(serde_json::to_vec(&candidate).unwrap(), before);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(
            renderer
                .model_source_catalog()
                .shares_sources_with(&catalog)
        );
        assert_eq!(render(&mut renderer, &project, 0.), expected);
        renderer.check_health().unwrap();
    }
    std::fs::write(&path, &original).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    assert!(
        renderer
            .model_source_catalog()
            .shares_sources_with(&catalog)
    );
    assert_eq!(render(&mut renderer, &project, 0.), expected);
    renderer.check_health().unwrap();
}

#[test]
fn actual_catalog_failures_emit_existing_default_info_and_error_records() {
    use std::io::Write;
    const CHILD: &str = "MOTION_MODEL_SOURCE_ERROR_LOG_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        struct Capture(std::sync::Mutex<std::fs::File>);
        impl log::Log for Capture {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() <= log::Level::Info
            }
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    let line = format!("{} {}", record.level(), record.args());
                    writeln!(self.0.lock().unwrap(), "{line}").unwrap();
                    if record.level() == log::Level::Error {
                        eprintln!("{line}");
                    }
                }
            }
            fn flush(&self) {}
        }
        let capture = Capture(std::sync::Mutex::new(std::fs::File::create(path).unwrap()));
        log::set_logger(Box::leak(Box::new(capture))).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        cpu_failures(None);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("events.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "source_errors::actual_catalog_failures_emit_existing_default_info_and_error_records",
            "--nocapture",
        ])
        .env_remove("MOTION_LOG")
        .env(CHILD, &path)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let records = std::fs::read_to_string(path).unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_MODEL_SOURCE_ERROR_EVIDENCE") {
        let folder = std::path::PathBuf::from(evidence);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("events.log"), &records).unwrap();
        std::fs::write(folder.join("child-stdout.txt"), &stdout).unwrap();
        std::fs::write(folder.join("child-stderr.txt"), &stderr).unwrap();
    }
    let errors: Vec<_> = records
        .lines()
        .filter(|line| line.starts_with("ERROR "))
        .collect();
    assert!(errors.len() >= 5);
    assert!(
        errors
            .iter()
            .all(|line| stderr.lines().any(|actual| actual == *line))
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.starts_with("ERROR "))
            .count(),
        errors.len()
    );
    assert_eq!(
        errors
            .iter()
            .filter(|line| line.contains("model catalog phase=prepare"))
            .count(),
        3
    );
    for cause in [
        "model source=7 phase=validate:",
        "model source=7 phase=import:",
        "expected value at line 1 column 1",
        "resource absent from frozen bundle",
    ] {
        assert!(
            records.contains(cause) && stderr.contains(cause),
            "{cause}: {records}"
        );
    }
    let start = records
        .find("INFO start model_catalog phase=prepare")
        .unwrap();
    let failure = records.find("ERROR model catalog phase=prepare").unwrap();
    assert!(start < failure);
    assert!(!stderr.contains("INFO ") && !records.contains("DEBUG "));
    assert!(!stdout.contains("model catalog") && !stdout.contains("ERROR "));
}
