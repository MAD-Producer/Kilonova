use super::*;
use crate::tracking::{colmap, document, pose};

fn original(f: &Fixture) -> Vec<u8> {
    let source = decode::from_slice(&f.original).unwrap();
    document::colmap_to_vec(
        b"# exact original\r\n7 PINHOLE 8 6 7.500 7.25 4 3\r\n",
        b"83 1 0 0 0 -1 -2 -3 7 plate.png\n\n",
        colmap::Source {
            id: source.source_id,
            sha256: source.source_sha256,
            extent: source.source_extent,
            ticks_per_second: source.ticks_per_second,
            nominal_rate: source.nominal_rate,
            pts: source.source_pts,
        },
        &[colmap::Frame {
            source_index: 0,
            image_name: "plate.png".into(),
            confidence: 0.8,
        }],
        colmap::Options {
            track_id: "external camera".into(),
            world_basis: pose::Basis::RIGHT_UP_BACK,
            source_meters_per_unit: 1.,
            target_meters_per_unit: 1.,
            target_origin: [0.; 3],
        },
    )
    .unwrap()
}
fn install(f: &Fixture) -> Vec<u8> {
    let bytes = original(f);
    fs::write(f.external.path().join("original.json"), &bytes).unwrap();
    bytes
}

#[test]
fn original_vendor_documents_mapping_and_settings_survive_undo_reopen_and_packaging() {
    let f = Fixture::new();
    let bytes = install(&f);
    let mut prepared = f.read().unwrap();
    let descriptor = prepared.descriptor().clone();
    assert_eq!(
        descriptor.format,
        motion_core::TrackingFormat::ColmapCameraTextV1
    );
    let mut engine = f.engine();
    prepared.commit(&mut engine).unwrap();
    prepared.finish(Ok(())).unwrap();
    assert_eq!(engine.project().camera, f.project.camera);
    assert_eq!(engine.revision(), 1);
    let attached = engine.project().clone();
    assert_eq!(storage::load(f.root.path()).unwrap(), attached);
    assert_eq!(
        storage::read_tracking_resource(f.root.path(), &descriptor).unwrap(),
        bytes
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
    assert!(!engine.undo().unwrap());
    fs::remove_file(f.external.path().join("original.json")).unwrap();
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &attached);
    let plate = fs::read(f.root.path().join("assets/plate.png")).unwrap();
    let solve = crate::tracking::project::load_original(
        f.root.path(),
        &attached,
        descriptor.id,
        &mut Cursor::new(&plate),
        plate.len() as u64,
        || false,
    )
    .unwrap();
    assert_eq!(solve, *descriptor.data);
    let package = f.external.path().join("camera.msproj");
    storage::export_package(f.root.path(), &attached, &package).unwrap();
    let restored = f.external.path().join("restored");
    storage::import_package(&package, &restored).unwrap();
    assert_eq!(storage::load(&restored).unwrap(), attached);
    let stored = storage::read_tracking_resource(&restored, &descriptor).unwrap();
    assert_eq!(stored, bytes);
    let raw = document::colmap_from_slice(&stored).unwrap();
    assert_eq!(
        raw.converted.originals.cameras(),
        b"# exact original\r\n7 PINHOLE 8 6 7.500 7.25 4 3\r\n"
    );
    assert_eq!(
        raw.converted.originals.images(),
        b"83 1 0 0 0 -1 -2 -3 7 plate.png\n\n"
    );
    assert_eq!(raw.frames[0].image_name, "plate.png");
    assert_eq!(
        raw.converted.options.world_basis,
        pose::Basis::RIGHT_UP_BACK
    );
}

#[test]
fn managed_vendor_format_cannot_be_mislabelled_or_substituted_after_import() {
    let f = Fixture::new();
    install(&f);
    let mut prepared = f.read().unwrap();
    let descriptor = prepared.descriptor().clone();
    prepared.commit(&mut f.engine()).unwrap();
    prepared.finish(Ok(())).unwrap();
    let mut wrong = descriptor.clone();
    wrong.format = motion_core::TrackingFormat::MotionTrackingJsonV1;
    let error = storage::open_tracking_resource(f.root.path(), &wrong).unwrap_err();
    assert!(error.to_string().contains("format differs"));
    fs::write(
        f.root.path().join(&descriptor.path),
        b"controlled tampering",
    )
    .unwrap();
    assert!(storage::open_tracking_resource(f.root.path(), &descriptor).is_err());
}

#[test]
fn malformed_vendor_text_and_wrong_encoded_plate_publish_no_file_or_history() {
    let f = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&original(&f)).unwrap();
    value["images"] = "83 0 0 0 0 0 0 0 7 plate.png\n\n".into();
    fs::write(
        f.external.path().join("original.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let error = f.read().err().unwrap();
    assert!(error.to_string().contains("Quaternion"));
    assert!(!f.root.path().join("assets/tracking").exists());
    install(&f);
    fs::write(
        f.root.path().join("assets/plate.png"),
        b"not the declared original",
    )
    .unwrap();
    assert!(f.read().is_err());
    assert!(!f.root.path().join("assets/tracking").exists());
    assert_eq!(storage::load_document(f.root.path()).unwrap(), f.project);
    assert_eq!(f.engine().revision(), 0);
}

#[test]
fn cancelled_and_superseded_vendor_preparations_preserve_project_and_release_only_owned_bytes() {
    let f = Fixture::new();
    install(&f);
    assert!(matches!(
        Prepared::read(
            f.root.path(),
            &f.project,
            0,
            &f.external.path().join("original.json"),
            Fixture::source(),
            None,
            || true
        ),
        Err(Error::Cancelled)
    ));
    let mut prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    let mut engine = f.engine();
    engine
        .apply(motion_core::Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    let before = engine.project().clone();
    assert!(matches!(
        prepared.commit(&mut engine),
        Err(Error::Superseded)
    ));
    drop(prepared);
    assert!(!path.exists());
    assert_eq!(engine.project(), &before);
    assert_eq!(storage::load(f.root.path()).unwrap(), f.project);
}
