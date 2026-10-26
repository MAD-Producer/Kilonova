use super::*;

#[test]
fn directory_exclusive_ownership_preserves_foreign_files_and_rejects_nested_names() {
    let temp = tempfile::tempdir().unwrap();
    let directory = Directory::unique(temp.path(), "owned").unwrap();
    let path = directory.path.clone();
    fs::write(path.join("payload"), b"owned").unwrap();
    assert!(Directory::exact(temp.path(), path.file_name().unwrap()).is_err());
    assert_eq!(fs::read(path.join("payload")).unwrap(), b"owned");
    assert!(Directory::exact(temp.path(), std::ffi::OsStr::new("nested/name")).is_err());
    drop(directory);
    assert!(!path.exists());
}
