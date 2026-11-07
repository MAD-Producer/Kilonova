use super::*;
use std::error::Error as _;

struct Source(std::path::PathBuf);
impl Source {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "motion-style-error-{}-{id}.json",
            std::process::id()
        )))
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        if self.0.is_dir() {
            std::fs::remove_dir(&self.0).expect("remove test directory");
        } else if self.0.exists() {
            std::fs::remove_file(&self.0).expect("remove test file");
        }
    }
}

#[test]
fn missing_file_keeps_native_io_kind_path_and_open_phase() {
    let path = Source::new();
    for error in [
        StyleSheet::load(&path.0).err().unwrap(),
        default_stylesheet().load_overrides(&path.0).err().unwrap(),
    ] {
        assert_eq!(error.path.as_ref(), Some(&path.0));
        assert_eq!(error.phase, Phase::Open);
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        assert!(matches!(error.cause, Cause::Io(_)));
    }
}

#[test]
fn directory_failure_keeps_original_os_error_in_open_or_read() {
    let path = Source::new();
    std::fs::create_dir(&path.0).unwrap();
    let error = StyleSheet::load(&path.0).err().unwrap();
    assert_eq!(error.path.as_ref(), Some(&path.0));
    assert!(matches!(error.phase, Phase::Open | Phase::Read));
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap()
        .raw_os_error()
        .is_some());
}

#[test]
fn syntax_and_schema_errors_keep_native_json_categories() {
    let path = Source::new();
    std::fs::write(&path.0, b"{\n").unwrap();
    for (error, phase) in [
        (StyleSheet::load(&path.0).err().unwrap(), Phase::Decode),
        (
            default_stylesheet().load_overrides(&path.0).err().unwrap(),
            Phase::DecodeOverride,
        ),
    ] {
        assert_eq!(error.path.as_ref(), Some(&path.0));
        assert_eq!(error.phase, phase);
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<serde_json::Error>()
            .unwrap();
        assert!(source.is_eof());
        assert_eq!(source.line(), 2);
    }
    let error = default_stylesheet()
        .with_overrides(br#"{"styles":{"button":{"unknown_property":1}}}"#)
        .err()
        .unwrap();
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .unwrap()
        .is_data());
    assert_eq!(error.phase, Phase::DecodeOverride);
    assert!(error.path.is_none());
}

#[test]
fn file_limits_reject_before_json_and_distinguish_validation() {
    let path = Source::new();
    std::fs::write(&path.0, vec![b' '; MAX_BYTES + 8]).unwrap();
    for error in [
        StyleSheet::load(&path.0).err().unwrap(),
        default_stylesheet().load_overrides(&path.0).err().unwrap(),
    ] {
        assert_eq!(error.path.as_ref(), Some(&path.0));
        assert!(matches!(error.cause, Cause::TooLarge { limit: MAX_BYTES }));
        assert!(error.source().is_none());
    }
    let error = default_stylesheet()
        .with_overrides(br#"{"version":2}"#)
        .err()
        .unwrap();
    assert_eq!(error.phase, Phase::Compile);
    assert!(matches!(error.cause, Cause::Invalid(_)));
    let error = default_stylesheet().with_overrides(b"[]").err().unwrap();
    assert_eq!(error.phase, Phase::DecodeOverride);
    assert!(matches!(error.cause, Cause::Invalid(_)));
}

#[test]
fn complete_sheet_survives_failed_reloads_then_recovers() {
    let path = Source::new();
    let mut sheet = default_stylesheet().clone();
    let document = serde_json::to_value(&sheet.document).unwrap();
    let compiled = sheet.compiled.clone();
    for bytes in [b"{".as_slice(), b"{}".as_slice(), b"[]".as_slice()] {
        std::fs::write(&path.0, bytes).unwrap();
        assert!(sheet.reload(&path.0).is_err());
        assert_eq!(serde_json::to_value(&sheet.document).unwrap(), document);
        assert_eq!(sheet.compiled, compiled);
    }
    let mut next = document.clone();
    next["tokens"]["sizes"]["row.height"] = serde_json::json!(37.0);
    std::fs::write(&path.0, serde_json::to_vec(&next).unwrap()).unwrap();
    sheet.reload(&path.0).unwrap();
    assert_eq!(sheet.metric("row.height", 0.), 37.);
    assert_eq!(serde_json::to_value(&sheet.document).unwrap(), next);
}
