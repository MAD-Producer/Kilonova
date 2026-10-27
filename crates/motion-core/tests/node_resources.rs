use motion_core::{
    storage::{self, NodeResourceStage},
    EffectInstance, Layer, NodeResource, Project,
};
use serde_json::json;

fn resource(bytes: &[u8], extension: &str) -> NodeResource {
    let sha256 = storage::resource_sha256(bytes);
    NodeResource {
        path: format!("assets/node-{sha256}.{extension}"),
        bytes: bytes.len() as u64,
        sha256,
    }
}
fn project(resource: &NodeResource) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(1, "node", [64.; 2], [32., 32., 0.], [1.; 4]);
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    );
    effect.node_graph = Some(
        json!({"format_version":2,"key":"test","document":{},"options":{},"runtime":resource}),
    );
    layer.effects = vec![effect.clone(), EffectInstance { id: 2, ..effect }];
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

#[test]
fn only_owned_files_roll_back_and_verified_existing_content_survives_failure() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"independently authored resource bytes";
    let first = resource(bytes, "msfx");
    {
        let mut stage = NodeResourceStage::new(root.path()).unwrap();
        stage.stage(&first, bytes).unwrap();
        stage.keep_resources();
    }
    let second = resource(b"second", "png");
    {
        let mut stage = NodeResourceStage::new(root.path()).unwrap();
        stage.stage(&first, bytes).unwrap();
        stage.stage(&second, b"second").unwrap();
        assert!(stage.stage(&second, b"unrelated").is_err());
    }
    assert_eq!(std::fs::read(root.path().join(&first.path)).unwrap(), bytes);
    assert!(!root.path().join(&second.path).exists());
    std::fs::write(root.path().join(&first.path), vec![b'x'; bytes.len()]).unwrap();
    let error = NodeResourceStage::new(root.path())
        .unwrap()
        .stage(&first, bytes)
        .unwrap_err();
    assert!(error.to_string().contains("content mismatch"));
    assert_eq!(
        std::fs::read(root.path().join(&first.path)).unwrap(),
        vec![b'x'; bytes.len()]
    );
}

#[test]
fn panic_rolls_back_only_exclusively_created_resources() {
    let root = tempfile::tempdir().unwrap();
    let owned = resource(b"owned", "png");
    let foreign = root.path().join("foreign");
    std::fs::write(&foreign, b"keep").unwrap();
    assert!(std::panic::catch_unwind(|| {
        let mut stage = NodeResourceStage::new(root.path()).unwrap();
        stage.stage(&owned, b"owned").unwrap();
        panic!("controlled node resource unwind");
    })
    .is_err());
    assert!(!root.path().join(owned.path).exists());
    assert_eq!(std::fs::read(foreign).unwrap(), b"keep");
}

#[test]
fn project_archive_deduplicates_node_files_and_reopens_without_original_directory() {
    let folder = tempfile::tempdir().unwrap();
    let original = folder.path().join("original");
    std::fs::create_dir(&original).unwrap();
    let bytes = motion_effects::builtin::package().unwrap().bytes.clone();
    let record = resource(&bytes, "msfx");
    let project = project(&record);
    let mut stage = NodeResourceStage::new(&original).unwrap();
    stage.stage(&record, &bytes).unwrap();
    stage.keep_resources();
    assert_eq!(project.node_resources().unwrap(), vec![record.clone()]);
    storage::save(&original, &project).unwrap();
    assert!(
        std::fs::metadata(original.join("project.json"))
            .unwrap()
            .len()
            < 16 * 1024
    );
    let snapshot = storage::PackageSnapshot::new(&original, &project).unwrap();
    assert_eq!(snapshot.source_count(), 1);
    let archive = folder.path().join("portable.msproj");
    snapshot
        .prepare(&archive, |_, _| Ok(()))
        .unwrap()
        .publish()
        .unwrap();
    // Renaming prevents accidental fallback to the original absolute paths.
    std::fs::rename(&original, folder.path().join("unavailable")).unwrap();
    let restored = folder.path().join("restored");
    assert_eq!(
        storage::import_package(&archive, &restored).unwrap(),
        project
    );
    assert_eq!(
        storage::read_node_resource(&restored, &record).unwrap(),
        bytes
    );
    assert_eq!(storage::load(&restored).unwrap(), project);
}

#[test]
fn changed_node_content_cannot_publish_an_archive_even_at_the_same_file_length() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"fixed resource";
    let record = resource(bytes, "msfx");
    let mut stage = NodeResourceStage::new(root.path()).unwrap();
    stage.stage(&record, bytes).unwrap();
    stage.keep_resources();
    let snapshot = storage::PackageSnapshot::new(root.path(), &project(&record)).unwrap();
    std::fs::write(root.path().join(record.path), vec![b'x'; bytes.len()]).unwrap();
    let output = root.path().join("output.msproj");
    std::fs::write(&output, b"existing archive").unwrap();
    assert!(snapshot
        .prepare(&output, |_, _| Ok(()))
        .err()
        .unwrap()
        .to_string()
        .contains("content changed"));
    assert_eq!(std::fs::read(output).unwrap(), b"existing archive");
}

#[test]
fn missing_resource_keeps_native_io_cause_and_relative_resource_context() {
    let root = tempfile::tempdir().unwrap();
    let record = resource(b"missing", "msfx");
    let error = storage::open_node_resource(root.path(), &record).unwrap_err();
    assert!(
        matches!(&error, motion_core::Error::NodeResourceIo {phase:"resolve",path,source} if path == &record.path && source.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(std::error::Error::source(&error)
        .unwrap()
        .is::<std::io::Error>());
    // Project validation must retain the same native cause, rather than
    // flattening node-file failures in the general asset validation loop.
    let error = storage::validate_assets(root.path(), &project(&record)).unwrap_err();
    assert!(
        matches!(&error, motion_core::Error::NodeResourceIo {phase:"resolve",path,source}
        if path == &record.path && source.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(std::error::Error::source(&error)
        .unwrap()
        .is::<std::io::Error>());
}
