//! Actual desktop worker, source reader, capture and history boundaries.
use super::*;
use serde_json::json;
use std::{fs, io::Cursor};
#[path = "../../../../crates/motion-model/tests/support/tracking_camera.rs"]
mod authored;

fn capture(engine: &Engine) -> image::RgbaImage {
    let value = engine.render_preview(Some(0.)).unwrap();
    image::open(value["path"].as_str().unwrap())
        .unwrap()
        .to_rgba8()
}

fn stage(engine: &Engine, request: &str, path: &std::path::Path) {
    let state = engine.state().unwrap();
    engine
        .media(
            json!({"op":"start_asset_relink","request_id":request,
        "revision":state["revision"],"document_generation":state["document_generation"],
        "candidates":[{"target":{"kind":"image","id":1},"path":path}]})
            .to_string(),
        )
        .unwrap();
    for _ in 0..8 {
        let status = engine
            .media(json!({"op":"media_status","request_id":request}).to_string())
            .unwrap();
        if status["state"] == "ready" {
            return;
        }
        assert_eq!(status["state"], "running", "{status}");
    }
    panic!("image relink did not become ready");
}

#[test]
fn tracked_image_relink_keeps_native_output_and_records_wrong_source() {
    let observation = std::env::var_os("MOTION_TRACKING_RELINK_NATIVE_DIR").map(PathBuf::from);
    let _logger = observation
        .as_ref()
        .map(|p| crate::logging::init(p.join("logs")).unwrap());
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        320,
        240,
        image::Rgba([17, 83, 129, 255]),
    ))
    .write_to(&mut output, image::ImageFormat::Png)
    .unwrap();
    let bytes = output.into_inner();
    fs::write(root.join("assets/plate.png"), &bytes).unwrap();
    let mut project = authored::camera_fixture();
    let mut data = (*project.tracking_assets[0].data).clone();
    let mut binding = project.camera.tracking.take().unwrap();
    project.tracking_assets.clear();
    project.video_assets.clear();
    project.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/plate.png".into(),
        width: 320,
        height: 240,
    });
    let digest = motion_core::storage::resource_sha256(&bytes);
    data.source_sha256 =
        std::array::from_fn(|i| u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap());
    data.source_pts = vec![0];
    data.tracks[0].samples.truncate(1);
    binding.source_anchor_pts = 0;
    let mut layer = motion_core::Layer::solid(
        50,
        "Authored spatial plate",
        [320., 240.],
        [320., 240., 0.],
        [1.; 4],
    );
    layer.content = motion_core::Content::Image { asset: 1 };
    layer.three_d = true;
    project.layers.push(layer);
    motion_core::storage::save(&root, &project).unwrap();
    let document = folder.path().join("solve.mstrack");
    let original = motion_host::tracking::encode::to_vec(&data).unwrap();
    fs::write(&document, &original).unwrap();
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let imported = engine
        .import_tracking(
            document,
            motion_core::media_assets::AssetRef {
                kind: motion_core::media_assets::AssetKind::Image,
                id: 1,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    engine
        .edit_at_revision(
            json!([{"op":"set_tracking_camera","object":0,"binding":binding}]).to_string(),
            imported["revision"].as_u64().unwrap(),
        )
        .unwrap();
    let before = engine.state().unwrap();
    let warm = capture(&engine);
    assert_eq!(capture(&engine), warm);
    assert!(warm.pixels().filter(|p| p[3] > 0).count() > 100);
    // Another valid, same-extent source must not inherit the old camera solve.
    let wrong = folder.path().join("wrong.png");
    let mut replacement = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        320,
        240,
        image::Rgba([18, 83, 129, 255]),
    ))
    .write_to(&mut replacement, image::ImageFormat::Png)
    .unwrap();
    fs::write(&wrong, replacement.into_inner()).unwrap();
    stage(&engine, "desktop-relink-wrong", &wrong);
    let cause = engine
        .media(json!({"op":"finish_asset_relink","request_id":"desktop-relink-wrong"}).to_string())
        .unwrap_err();
    assert!(cause.contains("tracking source content differs"), "{cause}");
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(capture(&engine), warm);
    engine
        .media(json!({"op":"release_asset_relink","request_id":"desktop-relink-wrong"}).to_string())
        .unwrap();
    let same = folder.path().join("same.png");
    fs::write(&same, &bytes).unwrap();
    stage(&engine, "desktop-relink-same", &same);
    engine
        .media(json!({"op":"finish_asset_relink","request_id":"desktop-relink-same"}).to_string())
        .unwrap();
    let after = engine.state().unwrap();
    assert_eq!(capture(&engine), warm);
    assert_eq!(after["project"]["camera"], before["project"]["camera"]);
    assert_eq!(
        after["project"]["tracking_assets"],
        before["project"]["tracking_assets"]
    );
    assert_ne!(after["project"]["assets"], before["project"]["assets"]);
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    assert_eq!(capture(&engine), warm);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    assert_eq!(capture(&engine), warm);
    engine.save().unwrap();
    drop(engine);
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
    assert_eq!(capture(&reopened), warm);
    let managed = root.join(
        after["project"]["tracking_assets"][0]["path"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(fs::read(managed).unwrap(), original);
    drop(reopened);
    if let Some(dir) = observation {
        let text = fs::read_to_string(dir.join("logs/motion-studio.log")).unwrap();
        assert!(text.contains("asset relink tracking start request=desktop-relink-wrong"));
        assert!(text.contains("tracking_asset=2 source=Image:1 phase=verify_tracking"));
        assert!(text.contains("asset relink tracking failed request=desktop-relink-wrong"));
        fs::write(dir.join("before.png"), image_bytes(&warm)).unwrap();
    }
}

fn image_bytes(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
