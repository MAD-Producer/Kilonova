use motion_media::{decode_audio, fonts::FontStore, AudioMixer, Error};
use std::{error::Error as _, fs, io, path::Path, sync::Arc};

#[test]
fn missing_source_keeps_native_io_cause_across_clone_and_context() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing.wav");
    let error = decode_audio(
        &missing,
        &root.path().join("unused.pcm"),
        None,
        1024,
        &mut |_| Ok(()),
    )
    .unwrap_err();
    let cloned = error.clone();
    let (Error::Io(original), Error::Io(copy)) = (&error, &cloned) else {
        panic!("missing source lost its native IO type: {error:?}");
    };
    assert!(Arc::ptr_eq(original, copy));
    assert_eq!(original.kind(), io::ErrorKind::NotFound);
    assert!(original.raw_os_error().is_some());
    assert_eq!(error.to_string(), original.to_string());
    let original = original.clone();
    let error = error.context(format!("import {}", missing.display()));
    assert!(matches!(error.primary(), Error::Io(_)));
    let cause = error.source().unwrap().source().unwrap();
    let source = cause.downcast_ref::<Arc<io::Error>>().unwrap();
    assert!(Arc::ptr_eq(source, &original));
    assert_eq!(source.kind(), io::ErrorKind::NotFound);
    assert!(!root.path().join("unused.pcm").exists());
}

#[test]
fn malformed_audio_keeps_symphonia_source_and_probe_phase() {
    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/invalid.bin");
    let error = decode_audio(
        &source,
        &root.path().join("unused.pcm"),
        None,
        1024,
        &mut |_| Ok(()),
    )
    .unwrap_err();
    let Error::Decode { phase, cause, .. } = &error else {
        panic!("malformed audio lost its decoder cause: {error:?}");
    };
    assert_eq!(*phase, "probe");
    assert_eq!(error.to_string(), cause.to_string());
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<Arc<symphonia::core::errors::Error>>()
        .unwrap();
    assert!(Arc::ptr_eq(source, cause));
    assert!(!root.path().join("unused.pcm").exists());
}

#[test]
fn invalid_project_keeps_model_failure_before_touching_cache_files() {
    let root = tempfile::tempdir().unwrap();
    let mut project = motion_model::Project::new(64, 64, 30, 30).unwrap();
    project.fps = 0;
    let expected = project.validate().unwrap_err().to_string();
    let error = match AudioMixer::new(project, root.path()) {
        Ok(_) => panic!("invalid project was accepted"),
        Err(error) => error,
    };
    assert!(matches!(error, Error::Model(_)));
    assert_eq!(error.to_string(), expected);
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<Arc<motion_model::Error>>()
        .is_some());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn font_parse_and_corrupt_catalogue_keep_distinct_failure_types() {
    let root = tempfile::tempdir().unwrap();
    let mut fonts = FontStore::new(root.path()).unwrap();
    let error = fonts.import_bytes(b"not a font", "").unwrap_err();
    assert!(matches!(error, Error::Font(_)));
    assert!(error.to_string().starts_with("invalid TTF/OTF font: "));
    let metadata = root
        .path()
        .join(format!("{}.json", FontStore::builtin_id()));
    fs::write(&metadata, b"{").unwrap();
    let error = fonts.catalogue().unwrap_err();
    let Error::Json(source) = &error else {
        panic!("catalogue lost JSON cause: {error:?}");
    };
    assert!(source.is_eof());
    assert_eq!(source.line(), 1);
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<Arc<serde_json::Error>>()
        .is_some());
    assert_eq!(fs::read(metadata).unwrap(), b"{");
}
