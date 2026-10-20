use motion_host::ops::images::{image_info, prepare_image};
use std::fs;

#[test]
fn external_image_metadata_is_read_only_and_does_not_register_or_prepare_an_asset() {
    let directory = tempfile::tempdir().unwrap();
    let external = directory.path().join("candidates");
    let project = directory.path().join("project");
    fs::create_dir(&external).unwrap();
    fs::create_dir(&project).unwrap();
    let source = external.join("candidate.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([17, 61, 143, 255]))
        .save(&source)
        .unwrap();
    let bytes = fs::read(&source).unwrap();
    let path = source.to_str().unwrap();
    let metadata = image_info(path).unwrap();
    assert_eq!(metadata["width"], 8);
    assert_eq!(metadata["height"], 4);
    assert_eq!(metadata["format"], "Png");
    assert_eq!(metadata["bytes"], bytes.len() as u64);
    assert_eq!(
        metadata["maxEncodedBytes"],
        motion_render::image_resources::MAX_ENCODED_BYTES
    );
    assert!(
        prepare_image(project.clone(), path.into()).is_err(),
        "asset preparation must still require a project-relative path"
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::read_dir(&external).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn missing_and_invalid_candidates_return_errors_and_preserve_source_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("candidate.png");
    assert!(image_info(path.to_str().unwrap()).is_err());
    assert!(!path.exists());
    let bytes = b"independently authored invalid image fixture";
    fs::write(&path, bytes).unwrap();
    assert!(image_info(path.to_str().unwrap()).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
