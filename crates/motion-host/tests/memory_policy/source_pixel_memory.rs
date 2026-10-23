use super::*;
use motion_host::preview_inputs::{InputPixels, InputPoll, InputRequest, InputSource};
use std::time::{Duration, Instant};

#[test]
fn real_original_png_capacity_is_included_without_file_reads_pixel_changes_or_retained_owners() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("assets")).unwrap();
    let path = directory.path().join("assets/original.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([97, 149, 211, 128]))
        .save(&path)
        .unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    project.assets.push(motion_core::Asset {
        id: 17,
        path: "assets/original.png".into(),
        width: 4,
        height: 4,
    });
    let mut session =
        Session::new(project, directory.path().to_path_buf(), Arc::new(Stub)).unwrap();
    let revision = session.engine.revision();
    let view_revision = session.view_revision;
    let snapshot = session.engine.snapshot();
    let baseline = session.resource_memory_ledger().unwrap();
    session
        .preview_inputs
        .request(
            session.engine.project(),
            directory.path(),
            revision,
            InputRequest {
                sequence: 1,
                source: InputSource::ImageAsset { asset: 17 },
                frame: 0.,
                max_edge: 4,
            },
        )
        .unwrap();
    let pending = session.resource_memory_ledger().unwrap();
    assert_eq!(
        pending["cpu"]["sourcePixelCaches"]["originalSource"]["imageDecode"]["pending"],
        true
    );
    assert!(
        pending["cpu"]["sourcePixelCaches"]["originalSource"]["imageDecode"]["capacityBytes"]
            .is_null()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let packet = loop {
        assert!(Instant::now() < deadline, "PNG decode did not finish");
        match session.preview_inputs.poll(1, revision).unwrap() {
            InputPoll::Ready(packet) => break packet,
            InputPoll::Pending(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    let InputPixels::Image(pixels) = &packet.pixels else {
        panic!("PNG did not produce native image pixels")
    };
    let payload = pixels.rgba.len() as u64;
    let capacity = pixels.rgba.capacity() as u64;
    let expected_pixels = pixels.rgba.clone();
    let count = Arc::strong_count(pixels);
    // Observation must still succeed when a filesystem validity check would fail.
    std::fs::rename(&path, directory.path().join("assets/moved.png")).unwrap();
    let observed = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(observed["version"], 2);
    assert_eq!(
        observed["cpu"]["knownPayloadBytes"],
        baseline["cpu"]["knownPayloadBytes"].as_u64().unwrap() + payload
    );
    assert_eq!(
        observed["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"].as_u64().unwrap() + capacity
    );
    assert_eq!(observed["cpu"]["sourcePixelCaches"]["resourceCount"], 1);
    assert_eq!(
        observed["cpu"]["sourcePixelCaches"]["knownCapacityBytes"],
        capacity
    );
    assert_eq!(
        observed["cpu"]["sourcePixelCaches"]["originalSource"]["cachedImage"]["source"],
        17
    );
    assert_eq!(
        observed["cpu"]["sourcePixelCaches"]["resources"][0]["references"][0]["owner"],
        "original_source_image"
    );
    assert_eq!(pixels.rgba, expected_pixels);
    assert_eq!(Arc::strong_count(pixels), count);
    assert_eq!(session.resource_memory_ledger().unwrap(), observed);
    assert_eq!(session.engine.snapshot(), snapshot);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view_revision);
    assert!(session.graphics.is_none() && session.editor_renderer.is_none());
    assert_eq!(observed["textures"], baseline["textures"]);
    assert!(!session.engine.can_undo() && !session.engine.can_redo());
    session.preview_inputs.cancel();
    let released = session.resource_memory_ledger().unwrap();
    assert_eq!(released["cpu"]["sourcePixelCaches"]["resourceCount"], 0);
    assert_eq!(
        released["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(
        pixels.rgba, expected_pixels,
        "external packet survives Session cache release"
    );
    assert_eq!(Arc::strong_count(pixels), 1);
    assert!(released["cpu"]["unknownBytes"].is_null());
    assert_eq!(released["completeCoverage"], false);
}
