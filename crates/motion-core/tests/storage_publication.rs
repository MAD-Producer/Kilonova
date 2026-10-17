use motion_core::{storage, Error, Project};
use std::{error::Error as _, fs};

#[test]
fn publication_replaces_and_reopens_unicode_projects_without_a_temporary_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("工程-保存-é");
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    storage::save(&path, &project).unwrap();
    let original = fs::read(path.join("project.json")).unwrap();
    project.background = [1., 0., 0., 1.];
    storage::save(&path, &project).unwrap();
    assert_ne!(fs::read(path.join("project.json")).unwrap(), original);
    assert_eq!(storage::load(&path).unwrap(), project);
    assert!(!path.join("project.json.tmp").exists());
}

#[test]
fn failed_publication_preserves_the_native_cause_and_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("project.json");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"untouched").unwrap();
    let project = Project::new(64, 64, 30, 60).unwrap();
    let error = storage::save(root.path(), &project).unwrap_err();
    let Error::Storage {
        phase,
        path,
        published,
        source,
    } = &error
    else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(*phase, "publish");
    assert!(!published);
    assert_eq!(
        path,
        &root.path().canonicalize().unwrap().join("project.json")
    );
    assert!(source.raw_os_error().is_some());
    assert_eq!(error.source().unwrap().to_string(), source.to_string());
    assert!(error.to_string().contains(&source.to_string()));
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"untouched");
    assert!(!root.path().join("project.json.tmp").exists());
}

#[cfg(windows)]
#[test]
fn sharing_violation_preserves_old_bytes_and_a_later_explicit_save_succeeds() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    storage::save(root.path(), &project).unwrap();
    let path = root.path().join("project.json");
    let original = fs::read(&path).unwrap();
    // Readers may coexist; replacement is denied by withholding FILE_SHARE_DELETE.
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    project.background = [1., 0., 0., 1.];
    let error = storage::save(root.path(), &project).unwrap_err();
    match error {
        Error::Storage {
            phase,
            published,
            source,
            ..
        } => {
            assert_eq!(phase, "publish");
            assert!(!published);
            // MoveFileExW may report ACCESS_DENIED (5) for a destination whose
            // open handle denies deletion, or SHARING_VIOLATION (32).
            assert!(matches!(source.raw_os_error(), Some(5 | 32)), "{source}");
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!root.path().join("project.json.tmp").exists());
    drop(locked);
    storage::save(root.path(), &project).unwrap();
    assert_eq!(storage::load(root.path()).unwrap(), project);
}

#[cfg(windows)]
#[test]
fn publication_supports_native_extended_length_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut path = root.path().canonicalize().unwrap();
    for _ in 0..8 {
        path.push("project-publication-long-directory");
    }
    assert!(path.as_os_str().len() > 260);
    let project = Project::new(64, 64, 30, 60).unwrap();
    storage::save(&path, &project).unwrap();
    storage::save(&path, &project).unwrap();
    assert_eq!(storage::load(&path).unwrap(), project);
    assert!(!path.join("project.json.tmp").exists());
}
