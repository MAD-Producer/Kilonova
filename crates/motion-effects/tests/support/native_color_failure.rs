//! Test-only prefix. The product recipe never installs or emits this fixture.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn create(root: &Path) -> PathBuf {
    let manifest = PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit production prefix"),
    );
    let base = manifest.parent().unwrap();
    let directory = PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_FAILURE_LIBRARY_DIR")
            .expect("explicit fault library directory"),
    );
    let name = if cfg!(windows) {
        "motion_ocio_initialization_failure.dll"
    } else if cfg!(target_os = "macos") {
        "libmotion_ocio_initialization_failure.dylib"
    } else {
        "libmotion_ocio_initialization_failure.so"
    };
    let fault = directory.join(name);
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    document["qualificationOnly"] =
        "actual C++ callback-registration bad_alloc; never distribute".into();
    fs::create_dir(root).unwrap();
    for field in ["files", "licenses"] {
        for entry in document[field].as_array_mut().unwrap() {
            let relative = PathBuf::from(entry["path"].as_str().unwrap());
            assert!(relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))));
            let original = fs::read(base.join(&relative)).unwrap();
            assert_eq!(
                motion_effects::color_config::ContentIdentity::of(&original).sha256,
                entry["sha256"].as_str().unwrap()
            );
            let bytes = if relative
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("motion_ocio_bridge")
            {
                let bytes = fs::read(&fault).unwrap();
                entry["sha256"] = motion_effects::color_config::ContentIdentity::of(&bytes)
                    .sha256
                    .into();
                bytes
            } else {
                original
            };
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, bytes).unwrap();
        }
    }
    let output = root.join("ocio-runtime.json");
    fs::write(&output, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    output
}
