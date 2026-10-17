use motion_core::{storage, Project};
use std::fs;

#[test]
fn failed_project_rename_cleans_its_temporary_file_without_removing_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("project.json");
    fs::create_dir(&destination).unwrap();
    let project = Project::new(64, 64, 30, 60).unwrap();
    assert!(storage::save(directory.path(), &project).is_err());
    assert!(destination.is_dir());
    assert!(!directory.path().join("project.json.tmp").exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn failure_before_open_preserves_existing_project_and_foreign_temporary_directory() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    storage::save(directory.path(), &project).unwrap();
    let before = fs::read(directory.path().join("project.json")).unwrap();
    let temporary = directory.path().join("project.json.tmp");
    fs::create_dir(&temporary).unwrap();
    project.background = [1., 0., 0., 1.];
    assert!(storage::save(directory.path(), &project).is_err());
    assert_eq!(
        fs::read(directory.path().join("project.json")).unwrap(),
        before
    );
    assert!(temporary.is_dir());
}
