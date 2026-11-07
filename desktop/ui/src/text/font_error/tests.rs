use super::*;
use crate::text::{fallback, select_preferred, shaping, TextAtlas};
use ab_glyph::Font;
use std::{error::Error as _, sync::Arc};

fn fixture() -> fontdb::FaceInfo {
    fallback::system_catalog()
        .database
        .faces()
        .find(|face| {
            candidate(face)
                .is_ok_and(|font| font.glyph_id('A').0 != 0 && font.glyph_id('中').0 == 0)
        })
        .expect("an installed Latin font fixture")
        .clone()
}
fn collection(faces: impl IntoIterator<Item = fontdb::FaceInfo>) -> fallback::Catalog {
    let mut database = fontdb::Database::new();
    for face in faces {
        database.push_face_info(face);
    }
    fallback::Catalog::new(database)
}
fn load(
    catalog: &fallback::Catalog,
) -> Result<(ab_glyph::FontArc, Vec<String>, String, u32), FontError> {
    select_preferred(
        catalog,
        catalog.database.faces().map(|face| face.id),
        false,
        Some("fixture"),
        vec![],
    )
}

#[test]
fn missing_font_keeps_native_open_error_through_selection() {
    let path = std::env::temp_dir().join(format!("motion-font-missing-{}.ttf", std::process::id()));
    assert!(!path.exists());
    let mut face = fixture();
    face.source = fontdb::Source::File(path.clone());
    let catalog = collection([face]);
    let error = load(&catalog).err().unwrap();
    let FontError::NoUsableFont {
        requested,
        attempted,
        first_failure: Some(first),
        ..
    } = &error
    else {
        panic!("{error:?}")
    };
    assert_eq!(requested.as_deref(), Some("fixture"));
    assert_eq!(*attempted, 1);
    let FontError::Candidate {
        phase,
        path: actual,
        cause: FontCause::Io(cause),
        ..
    } = first.as_ref()
    else {
        panic!("{first:?}")
    };
    assert_eq!(*phase, FontPhase::Open);
    assert_eq!(actual.as_ref(), Some(&path));
    assert_eq!(cause.kind(), std::io::ErrorKind::NotFound);
    let source = error.source().unwrap().downcast_ref::<FontError>().unwrap();
    assert!(std::ptr::eq(source, first.as_ref()));
    assert!(std::ptr::eq(
        source
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap(),
        cause
    ));
}

#[test]
fn directory_source_keeps_actual_open_or_read_os_failure() {
    let path = std::env::temp_dir().join(format!("motion-font-directory-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let mut face = fixture();
    face.source = fontdb::Source::File(path.clone());
    let failure = candidate(&face).err();
    std::fs::remove_dir(&path).unwrap();
    let failure = failure.expect("a directory cannot supply font bytes");
    assert!(matches!(
        failure,
        FontError::Candidate {
            phase: FontPhase::Open | FontPhase::Read,
            ..
        }
    ));
    assert!(failure
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap()
        .raw_os_error()
        .is_some());
}

#[test]
fn invalid_bytes_keep_native_parser_and_shaping_causes() {
    let mut face = fixture();
    face.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
    let failure = candidate(&face).err().unwrap();
    assert!(matches!(
        failure,
        FontError::Candidate {
            phase: FontPhase::Parse,
            ..
        }
    ));
    assert!(failure
        .source()
        .unwrap()
        .downcast_ref::<ab_glyph::InvalidFont>()
        .is_some());
    let font = candidate(&fixture()).unwrap();
    let failure = TextAtlas::new(font, u32::MAX, 12., vec![]).err().unwrap();
    assert!(matches!(
        failure,
        FontError::Shaping(shaping::ShapeError::InvalidFont {
            face_index: u32::MAX,
            ..
        })
    ));
    assert!(failure
        .source()
        .unwrap()
        .downcast_ref::<shaping::ShapeError>()
        .is_some());
}

#[test]
fn shared_font_data_survives_a_missing_original_path() {
    let original = fixture();
    let font = candidate(&original).unwrap();
    let bytes = font.font_data().to_vec();
    let path = std::env::temp_dir().join(format!(
        "motion-font-shared-missing-{}.ttf",
        std::process::id()
    ));
    assert!(!path.exists());
    let mut face = original.clone();
    face.source = fontdb::Source::SharedFile(path, Arc::new(bytes));
    let actual = candidate(&face).unwrap();
    assert_eq!(actual.font_data(), font.font_data());
    for ch in "Aé09".chars() {
        assert_eq!(actual.glyph_id(ch), font.glyph_id(ch));
    }
    let catalog = collection([face]);
    let (_, _, family, index) = load(&catalog).unwrap();
    assert_eq!(family, original.families[0].0);
    assert_eq!(index, original.index);
}

#[test]
fn rejected_source_does_not_change_healthy_fallback_or_normal_missing_coverage() {
    let good = fixture();
    let mut broken = good.clone();
    broken.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
    let catalog = collection([broken, good.clone()]);
    let selection = crate::text::select_candidate(
        &catalog.database,
        catalog.database.faces().map(|face| face.id),
        true,
    );
    assert_eq!(selection.rejected, 1);
    assert!(matches!(
        selection.first_failure,
        Some(FontError::Candidate {
            phase: FontPhase::Parse,
            ..
        })
    ));
    let (font, family, index) = selection.selected.unwrap();
    assert_eq!(family, good.families[0].0);
    assert_eq!(index, good.index);
    assert_eq!(font.font_data(), candidate(&good).unwrap().font_data());
    let clean = collection([good]);
    let selection = crate::text::select_candidate(
        &clean.database,
        clean.database.faces().map(|face| face.id),
        true,
    );
    assert!(selection.selected.is_some());
    assert_eq!(selection.rejected, 0);
    assert!(selection.first_failure.is_none());
    let empty = collection([]);
    let error = load(&empty).err().unwrap();
    assert!(matches!(
        error,
        FontError::NoUsableFont {
            attempted: 0,
            first_failure: None,
            ..
        }
    ));
}

#[test]
fn real_degraded_selection_reports_at_sixty_resets_and_stays_bounded() {
    const CHILD: &str = "MOTION_FONT_SELECTION_CHILD";
    const TEST:&str = "text::font_error::tests::real_degraded_selection_reports_at_sixty_resets_and_stays_bounded";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    struct Capture(std::sync::Mutex<Vec<(log::Level, String)>>);
    impl log::Log for Capture {
        fn enabled(&self, record: &log::Metadata<'_>) -> bool {
            record.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata())
                && record
                    .args()
                    .to_string()
                    .contains("font selection degradation")
            {
                self.0
                    .lock()
                    .unwrap()
                    .push((record.level(), record.args().to_string()));
            }
        }
        fn flush(&self) {}
    }
    let capture = Box::leak(Box::new(Capture(Default::default())));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let good = fixture();
    let mut broken = good.clone();
    broken.families[0].0 = "long-family\n".repeat(1000);
    broken.source = fontdb::Source::Binary(Arc::new(Vec::<u8>::new()));
    let catalog = collection([broken, good]);
    let ids: Vec<_> = catalog.database.faces().map(|face| face.id).collect();
    for episode in 0..2 {
        for _ in 0..59 {
            assert!(load(&catalog).is_ok());
        }
        assert_eq!(capture.0.lock().unwrap().len(), episode);
        assert!(load(&catalog).is_ok());
        assert_eq!(capture.0.lock().unwrap().len(), episode + 1);
        for _ in 0..60 {
            assert!(load(&catalog).is_ok());
        }
        assert_eq!(capture.0.lock().unwrap().len(), episode + 1);
        let actual = select_preferred(&catalog, [ids[1]], false, Some("fixture"), vec![]).unwrap();
        assert!(!actual.2.is_empty());
    }
    let records = capture.0.lock().unwrap();
    assert_eq!(records.len(), 2, "{records:?}");
    for (level, record) in records.iter() {
        assert_eq!(*level, log::Level::Error);
        assert!(
            record.contains("count=60 ")
                && record.contains("rejected_candidates=1 ")
                && record.contains("phase=Parse")
        );
        assert!(!record.contains('\n'));
        assert!(record.chars().count() < 700);
    }
}
