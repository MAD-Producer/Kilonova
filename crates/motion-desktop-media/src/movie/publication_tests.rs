use super::*;

fn frozen(root: &Path) -> FrozenMovie {
    let mut project = Project::new(32, 24, 30, 2).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Publication fixture",
        [32., 24.],
        [16., 12., 0.],
        [0.8, 0.2, 0.1, 1.],
    ));
    FrozenMovie::new(
        project,
        root,
        Registry::new_with_builtins().unwrap(),
        motion_render::resource_policy::MAX_SCRATCH_BUDGET,
        17,
    )
    .unwrap()
}
fn assert_no_temporary(root: &Path) {
    assert!(!fs::read_dir(root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".motion-movie-")
    }));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn new_movie_publication_refuses_racing_writer_and_keeps_cancellation_atomic() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("race.mp4");
    fs::write(&output, b"existing completed output").unwrap();
    assert!(frozen(root.path())
        .export_new_file(&output, &ExportControl::default(), |_| {})
        .is_err());
    assert_eq!(fs::read(&output).unwrap(), b"existing completed output");
    fs::remove_file(&output).unwrap();
    let mut created = false;
    let error = frozen(root.path())
        .export_new_file(&output, &ExportControl::default(), |progress| {
            if !created && progress > 0.1 && progress < 1. {
                fs::write(&output, b"concurrent completed output").unwrap();
                created = true;
            }
        })
        .err()
        .unwrap();
    assert!(created && error.contains("publish MP4"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), b"concurrent completed output");
    assert_no_temporary(root.path());

    let cancelled = root.path().join("cancelled.mp4");
    let control = ExportControl::default();
    let error = frozen(root.path())
        .export_new_file(&cancelled, &control, |progress| {
            if progress > 0.1 && progress < 1. {
                assert!(control.cancel());
            }
        })
        .err()
        .unwrap();
    assert_eq!(error, "movie export cancelled");
    assert!(!cancelled.exists());
    assert_no_temporary(root.path());

    let completed = root.path().join("completed.mp4");
    let control = ExportControl::default();
    let result = frozen(root.path())
        .export_new_file(&completed, &control, |progress| {
            if progress == 1. {
                assert!(!control.cancel(), "publication wins before final progress");
            }
        })
        .unwrap();
    assert!(result.bytes > 0 && result.frames == 2);
    let probe = crate::av::probe(&completed, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), 2);
    assert_no_temporary(root.path());
}
