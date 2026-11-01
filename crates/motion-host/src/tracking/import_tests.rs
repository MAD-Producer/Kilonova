use super::*;
use crate::tracking::{
    asset::{Asset, Coordinates, Kind, Observation, Rate, Sample, Track, Visibility, VERSION},
    encode,
    plate::Extent,
};
use std::{error::Error as _, io::Cursor};

struct Fixture {
    root: tempfile::TempDir,
    external: tempfile::TempDir,
    project: Project,
    original: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            6,
            image::Rgba([17, 83, 129, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let bytes = png.into_inner();
        fs::write(root.path().join("assets/plate.png"), &bytes).unwrap();
        let mut project = Project::new(64, 64, 30, 90).unwrap();
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/plate.png".into(),
            width: 8,
            height: 6,
        });
        storage::save(root.path(), &project).unwrap();
        let digest = storage::resource_sha256(&bytes);
        let asset = Asset {
            version: VERSION,
            source_id: "independent original plate".into(),
            source_sha256: std::array::from_fn(|i| {
                u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap()
            }),
            source_extent: Extent::new(8, 6).unwrap(),
            coordinates: Coordinates::RightDownForward,
            ticks_per_second: 90000,
            nominal_rate: Rate::new(30000, 1001).unwrap(),
            source_pts: vec![-3003],
            meters_per_world_unit: None,
            tracks: vec![Track {
                id: "manual point".into(),
                kind: Kind::Point,
                reference_index: None,
                samples: vec![Sample {
                    source_index: 0,
                    visibility: Visibility::Visible,
                    confidence: 0.8,
                    observation: Some(Observation::Point([2.5, 3.125])),
                    manual_correction: true,
                }],
            }],
        };
        let mut original = b" \n\t".to_vec();
        original.extend(encode::to_vec(&asset).unwrap());
        original.extend(b"\r\n ");
        fs::write(external.path().join("original.json"), &original).unwrap();
        Self {
            root,
            external,
            project,
            original,
        }
    }
    fn source() -> AssetRef {
        AssetRef {
            kind: AssetKind::Image,
            id: 1,
        }
    }
    fn read(&self) -> Result<Prepared, Error> {
        Prepared::read(
            self.root.path(),
            &self.project,
            0,
            &self.external.path().join("original.json"),
            Self::source(),
            || false,
        )
    }
    fn engine(&self) -> Engine {
        Engine::new(self.project.clone()).unwrap()
    }
}

#[test]
fn actual_file_import_is_saved_byte_exact_and_one_undo_step() {
    let f = Fixture::new();
    let mut prepared = f.read().unwrap();
    let descriptor = prepared.descriptor().clone();
    let path = f.root.path().join(&descriptor.path);
    let mut engine = f.engine();
    prepared.commit(&mut engine).unwrap();
    prepared.finish(Ok(())).unwrap();
    assert_eq!(fs::read(&path).unwrap(), f.original);
    assert_eq!(storage::load(f.root.path()).unwrap(), *engine.project());
    assert_eq!(engine.project().tracking_assets, vec![descriptor]);
    assert_eq!(engine.revision(), 1);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
    assert!(!engine.undo().unwrap());
    // Undo keeps the owned source for redo, even after the external original disappears.
    fs::remove_file(f.external.path().join("original.json")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), f.original);
    assert!(engine.redo().unwrap());
    assert!(!engine.redo().unwrap());
    storage::save(f.root.path(), engine.project()).unwrap();
    assert_eq!(storage::load(f.root.path()).unwrap(), *engine.project());
}

#[test]
fn abandoned_preparation_removes_only_the_new_original() {
    let f = Fixture::new();
    let prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    assert!(path.is_file());
    drop(prepared);
    assert!(!path.exists());
    assert_eq!(storage::load(f.root.path()).unwrap(), f.project);
    assert!(f.root.path().join("assets/plate.png").is_file());
}

#[test]
fn stale_revision_rejects_without_history_or_owned_file() {
    let f = Fixture::new();
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
    // The rejecting caller owns this stage. No source was published by this stage.
    prepared.cleanup_uncommitted().unwrap();
    assert!(!path.exists());
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), 1);
    assert!(engine.undo().unwrap());
    assert!(!engine.undo().unwrap());
}

#[test]
fn same_revision_different_document_is_rejected() {
    let f = Fixture::new();
    let mut prepared = f.read().unwrap();
    let mut changed = f.project.clone();
    changed.width = 96;
    let mut engine = Engine::new(changed.clone()).unwrap();
    assert_eq!(engine.revision(), 0);
    assert!(matches!(
        prepared.commit(&mut engine),
        Err(Error::Superseded)
    ));
    assert!(matches!(
        prepared.finish::<()>(Err(Error::Superseded)),
        Err(Error::Superseded)
    ));
    assert_eq!(engine.project(), &changed);
    assert!(!engine.undo().unwrap());
}

#[test]
fn gesture_is_preserved_and_import_is_not_committed() {
    let f = Fixture::new();
    let mut prepared = f.read().unwrap();
    let mut engine = f.engine();
    engine.begin_gesture().unwrap();
    let error = prepared.commit(&mut engine).unwrap_err();
    assert!(error.to_string().contains("active gesture"));
    prepared.cleanup_uncommitted().unwrap();
    assert!(engine.gesture_active());
    assert_eq!(engine.project(), &f.project);
    assert_eq!(engine.revision(), 0);
}

#[test]
fn real_prepublication_io_failure_rolls_back_model_history_and_original() {
    let f = Fixture::new();
    let old = fs::read(f.root.path().join("project.json")).unwrap();
    let mut prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    fs::create_dir(f.root.path().join("project.json.tmp")).unwrap();
    let mut engine = f.engine();
    let error = prepared.commit(&mut engine).unwrap_err();
    assert!(error.source().is_some(), "{error}");
    assert!(prepared.finish::<()>(Err(error)).is_err());
    assert!(!path.exists());
    assert_eq!(engine.project(), &f.project);
    assert_eq!(engine.revision(), 0);
    assert!(!engine.undo().unwrap());
    assert_eq!(fs::read(f.root.path().join("project.json")).unwrap(), old);
}

#[test]
fn published_save_acknowledgement_error_retains_matching_model_and_original() {
    let f = Fixture::new();
    let mut prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    let mut engine = f.engine();
    let error = prepared
        .commit_using(&mut engine, |root, project| {
            // Publish real bytes first. Only the post-publication acknowledgement is injected.
            storage::save(root, project)?;
            Err(motion_core::Error::Storage {
                phase: "test_directory_ack",
                path: root.into(),
                published: true,
                source: std::io::Error::other("injected durability acknowledgement"),
            })
        })
        .unwrap_err();
    assert!(matches!(
        &error,
        Error::Model(motion_core::Error::Storage {
            published: true,
            ..
        })
    ));
    assert!(error
        .to_string()
        .contains("injected durability acknowledgement"));
    assert!(prepared.finish::<()>(Err(error)).is_err());
    assert_eq!(fs::read(path).unwrap(), f.original);
    assert_eq!(storage::load(f.root.path()).unwrap(), *engine.project());
    assert_eq!(engine.revision(), 1);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.project);
    assert!(!engine.undo().unwrap());
}

#[test]
fn panic_after_publication_retains_original_for_safe_reopen() {
    let f = Fixture::new();
    let mut prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    let mut engine = f.engine();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared.commit_using(&mut engine, |root, project| {
            storage::save(root, project)?;
            panic!("injected after real project publication");
        })
    }));
    assert!(panic.is_err());
    drop(prepared);
    assert_eq!(fs::read(path).unwrap(), f.original);
    // The core restores author memory during unwind. The worker's panic path
    // closes the session; the published disk document must remain reopenable.
    assert_eq!(engine.project(), &f.project);
    assert_eq!(
        storage::load(f.root.path()).unwrap().tracking_assets.len(),
        1
    );
}

#[test]
fn verified_preexisting_original_is_never_owned_or_deleted_on_rejection() {
    let f = Fixture::new();
    let mut first = f.read().unwrap();
    let path = f.root.path().join(&first.descriptor().path);
    first.owned.rollback = false; // Emulate an original retained by an earlier operation.
    drop(first);
    let prepared = f.read().unwrap();
    assert!(prepared.owned.path.is_none());
    assert!(prepared.finish::<()>(Err(Error::Superseded)).is_err());
    assert_eq!(fs::read(path).unwrap(), f.original);
}

#[test]
fn corrupt_preexisting_original_is_not_overwritten_or_deleted() {
    let f = Fixture::new();
    let mut first = f.read().unwrap();
    let path = f.root.path().join(&first.descriptor().path);
    first.owned.rollback = false;
    drop(first);
    fs::write(&path, b"foreign bytes").unwrap();
    assert!(f.read().is_err());
    assert_eq!(fs::read(path).unwrap(), b"foreign bytes");
    assert_eq!(storage::load_document(f.root.path()).unwrap(), f.project);
}

#[test]
fn source_tampering_and_cancellation_never_stage_an_original() {
    let f = Fixture::new();
    let path = f.external.path().join("original.json");
    let cancelled = Prepared::read(
        f.root.path(),
        &f.project,
        0,
        &path,
        Fixture::source(),
        || true,
    );
    assert!(matches!(cancelled, Err(Error::Cancelled)));
    fs::write(
        f.root.path().join("assets/plate.png"),
        b"changed encoded source",
    )
    .unwrap();
    let error = match f.read() {
        Err(error) => error,
        Ok(_) => panic!("tampered plate accepted"),
    };
    assert!(matches!(
        error,
        Error::Attachment(super::super::project::Error::Source(_))
    ));
    assert!(!f.root.path().join("assets/tracking").exists());
}

#[test]
fn rollback_failure_preserves_primary_and_secondary_causes() {
    let f = Fixture::new();
    let prepared = f.read().unwrap();
    let path = f.root.path().join(&prepared.descriptor().path);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let error = prepared.finish::<()>(Err(Error::Superseded)).unwrap_err();
    match error {
        Error::Cleanup { primary, secondary } => {
            assert!(matches!(*primary, Error::Superseded));
            assert!(matches!(
                *secondary,
                Error::Io {
                    phase: "rollback_original",
                    ..
                }
            ));
            assert!(secondary.source().is_some());
        }
        other => panic!("lost rollback causes: {other}"),
    }
    assert!(path.is_dir());
}
