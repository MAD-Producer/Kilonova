use super::*;

#[cfg(windows)]
mod native_log;

#[test]
fn private_leaf_is_created_reused_and_regular_files_are_checked() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture
        .path()
        .join("MotionStudio/driver-pipeline-cache-v1/test.cache");
    prepare(&path).unwrap();
    prepare(&path).unwrap();
    fs::write(&path, b"storage fixture only, never restored into a GPU").unwrap();
    prepare(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(
        prepare(&path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn insecure_existing_leaf_is_refused_without_changing_its_permissions() {
    let fixture = tempfile::tempdir().unwrap();
    let leaf = fixture.path().join("MotionStudio/driver-pipeline-cache-v1");
    fs::create_dir_all(&leaf).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&leaf, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let error = prepare(&leaf.join("test.cache")).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&leaf).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
    #[cfg(windows)]
    {
        // A second independent validation still rejects the unchanged DACL.
        assert_eq!(
            windows::private_directory(&leaf).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}

#[cfg(unix)]
#[test]
fn linked_directory_file_and_imported_hard_link_are_refused() {
    use std::os::unix::fs::symlink;
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture
        .path()
        .join("MotionStudio/driver-pipeline-cache-v1/test.cache");
    prepare(&path).unwrap();
    let source = fixture.path().join("source");
    fs::write(&source, b"not a driver snapshot").unwrap();
    symlink(&source, &path).unwrap();
    assert_eq!(
        prepare(&path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::remove_file(&path).unwrap();
    fs::hard_link(&source, &path).unwrap();
    assert_eq!(
        prepare(&path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::remove_file(&path).unwrap();
    fs::remove_dir(path.parent().unwrap()).unwrap();
    symlink(fixture.path(), path.parent().unwrap()).unwrap();
    assert_eq!(
        prepare(&path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}
