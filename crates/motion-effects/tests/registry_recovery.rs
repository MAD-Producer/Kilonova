use motion_effects::Registry;
use std::fs;

#[test]
fn damaged_oversized_or_non_file_settings_preserve_plugins_and_report_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disabled.json");
    let clean = Registry::load(dir.path()).unwrap();
    let keys: Vec<_> = clean.packages.keys().cloned().collect();
    assert!(!keys.is_empty());
    for bytes in [b"[[\"unfinished".to_vec(), vec![b' '; 512 * 1024 + 1]] {
        fs::write(&path, &bytes).unwrap();
        let recovered = Registry::load(dir.path()).unwrap();
        assert_eq!(recovered.packages.keys().cloned().collect::<Vec<_>>(), keys);
        assert!(recovered.disabled.is_empty());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(recovered.diagnostics.len(), 1);
        assert!(recovered.diagnostics[0].contains("disabled.json"));
        assert!(recovered.diagnostics[0].contains("file was preserved"));
    }
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(Registry::load(dir.path()).unwrap().diagnostics.len(), 1);
    fs::remove_dir(&path).unwrap();
    let key = keys[0].clone();
    fs::write(&path, serde_json::to_vec(&vec![&key]).unwrap()).unwrap();
    let mut registry = Registry::load(dir.path()).unwrap();
    assert!(registry.diagnostics.is_empty());
    assert!(registry.resolve(&key.0, &key.1, &key.2).is_err());
    registry
        .enable(dir.path(), &key.0, &key.1, &key.2, true)
        .unwrap();
    assert!(Registry::load(dir.path()).unwrap().disabled.is_empty());
}
