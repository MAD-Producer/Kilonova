use motion_core::{storage, Project};
use std::fs;

#[test]
fn unsupported_versions_cannot_migrate_load_or_replace_an_existing_project() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.json");
    let valid = Project::new(64, 64, 30, 120).unwrap();
    for version in [0, u32::MAX] {
        let mut invalid = valid.clone();
        invalid.version = version;
        assert!(invalid
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unsupported project format"));
        assert!(invalid
            .clone()
            .migrate()
            .unwrap_err()
            .to_string()
            .contains("unsupported project format"));
        let unsupported_bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, &unsupported_bytes).unwrap();
        assert!(storage::load(directory.path())
            .unwrap_err()
            .to_string()
            .contains("unsupported project format"));
        assert_eq!(
            fs::read(&path).unwrap(),
            unsupported_bytes,
            "failed load rewrote version {version}"
        );
        assert!(!directory.path().join("project.json.tmp").exists());
        storage::save(directory.path(), &valid).unwrap();
        let saved = fs::read(&path).unwrap();
        assert!(storage::save(directory.path(), &invalid).is_err());
        assert_eq!(
            fs::read(&path).unwrap(),
            saved,
            "failed save published version {version}"
        );
        assert_eq!(storage::load(directory.path()).unwrap(), valid);
        assert!(!directory.path().join("project.json.tmp").exists());
    }
}
