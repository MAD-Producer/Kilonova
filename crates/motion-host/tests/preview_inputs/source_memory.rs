//! Real asynchronous cache/Vec ownership, with the existing authored YUV decoder
//! fixture. This does not claim native codec/file-decoding performance.
use super::*;

#[test]
fn original_video_worker_cache_enters_session_totals_without_rgba_conversion_or_source_io() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("assets")).unwrap();
    let path = folder.path().join("assets/cache-fixture.mp4");
    std::fs::write(&path, b"authored decoder fixture, not a media file").unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    let asset: VideoAsset = serde_json::from_value(serde_json::json!({
        "id":2,"path":"assets/cache-fixture.mp4","bytes":std::fs::metadata(&path).unwrap().len(),
        "mime":"video/avc","track":0,"width":4,"height":2,"rotation":90,
        "display_width":2,"display_height":4,"video_start_us":0,"video_end_us":33333,
        "duration_us":33333,"frame_count":1,"variable_frame_rate":false,"nominal_frame_rate":30.0,
        "color_standard":1,"color_range":2,"audio_asset":null
    }))
    .unwrap();
    asset.validate().unwrap();
    let cache = motion_media::video_cache_path(folder.path(), &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    motion_media::save_index(&cache, &asset, &[0]).unwrap();
    project.video_assets.push(asset);
    let mut layer = Layer::solid(8, "Authored cache fixture", [2., 4.], [0.; 3], [1.; 4]);
    // The authored decoder/index contains exactly one frame. Limit this layer
    // to that source interval instead of the two-second composition default.
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 1,
        offset_frame: 0,
    });
    layer.content = Content::Video {
        video: VideoClip {
            asset: 2,
            source_offset_us: 0,
            volume: 1.,
            muted: false,
        },
    };
    project.layers.push(layer);
    let mut session =
        motion_host::Session::new(project, folder.path().to_path_buf(), Arc::new(PlatformStub))
            .unwrap();
    let revision = session.engine.revision();
    let before = session.engine.snapshot();
    let baseline = session.resource_memory_ledger().unwrap();
    session
        .preview_inputs
        .request(
            session.engine.project(),
            folder.path(),
            revision,
            request(
                1,
                InputSource::Layer {
                    composition: "comp-main".into(),
                    object: 8,
                },
                0.,
            ),
        )
        .unwrap();
    let packet = ready(&mut session.preview_inputs, 1, revision);
    let InputPixels::Video(frame) = &packet.pixels else {
        panic!("native YUV cache expected")
    };
    let VideoPixels::Yuv(planes) = &frame.pixels else {
        panic!("query must not materialize RGBA")
    };
    let capacity = (planes.y.capacity() + planes.uv.capacity()) as u64;
    std::fs::rename(&path, folder.path().join("assets/moved-fixture.mp4")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let observed = loop {
        let value = session.resource_memory_ledger().unwrap();
        if value["cpu"]["sourcePixelCaches"]["originalSource"]["videoStreams"]
            ["allActiveStreamCachesObserved"]
            == true
        {
            break value;
        }
        assert!(Instant::now() < deadline, "cache state stayed busy");
        std::thread::sleep(Duration::from_millis(2));
    };
    let pixels = &observed["cpu"]["sourcePixelCaches"];
    assert_eq!(pixels["resourceCount"], 1);
    assert_eq!(pixels["knownPayloadBytes"], 12);
    assert_eq!(pixels["knownCapacityBytes"], capacity);
    assert_eq!(pixels["activeVideoStreams"]["streamCount"], 0);
    assert_eq!(pixels["originalSource"]["videoStreams"]["streamCount"], 1);
    assert_eq!(pixels["resources"][0]["format"], "yuv420");
    assert_eq!(
        pixels["resources"][0]["references"][0]["owner"],
        "original_source_video"
    );
    assert_eq!(
        observed["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"].as_u64().unwrap() + capacity
    );
    assert_eq!(planes.y, [90; 8]);
    assert_eq!(planes.uv, [128; 4]);
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    assert!(session.graphics.is_none() && session.editor_renderer.is_none());
    session.preview_inputs.cancel();
    let released = session.resource_memory_ledger().unwrap();
    assert_eq!(released["cpu"]["sourcePixelCaches"]["resourceCount"], 0);
    assert_eq!(
        released["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(
        planes.y, [90; 8],
        "escaped packet remains readable after cache release"
    );
    assert!(released["external"]["decoderBytes"].is_null());
}
