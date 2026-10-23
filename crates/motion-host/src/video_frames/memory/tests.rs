use super::*;

fn frame(pixels: VideoPixels) -> Arc<DecodedFrame> {
    Arc::new(DecodedFrame {
        pixels,
        pts: 0,
        end: 33333,
        width: 2,
        height: 2,
        decode_us: 0,
        codec_us: 0,
        transfer_us: 0,
        pack_us: 0,
        source_transfer: "CPU ownership fixture",
        decoder_name: String::new(),
    })
}

fn add_stream(frames: &mut VideoFrames, object: u64, frame: Arc<DecodedFrame>) {
    let asset: VideoAsset = serde_json::from_value(json!({
        "id":17,"path":"assets/cpu-memory-fixture.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":33333,"duration_us":33333,
        "frame_count":1,"variable_frame_rate":false,"nominal_frame_rate":30.0,
        "color_standard":1,"color_range":1,"audio_asset":null
    }))
    .unwrap();
    let mut cache = FrameCache::new();
    assert!(cache.insert(frame, 0));
    frames.streams.insert(
        object,
        Stream {
            object,
            asset,
            pts: Arc::new(vec![0]),
            worker: None,
            shared: Arc::new((
                Mutex::new(State {
                    cache,
                    desired: None,
                    error: None,
                    stop: false,
                    prefetch: false,
                    failed_prefetch: None,
                    prefetch_failures: 0,
                    last_prefetch_error: None,
                    hits: 0,
                    misses: 0,
                    decoded: 0,
                    cancelled: 0,
                    seeks: 0,
                    in_flight: None,
                    pending_polls: 0,
                    worker_stopped: false,
                }),
                Condvar::new(),
            )),
        },
    );
}

fn value(frames: &VideoFrames) -> Value {
    serde_json::to_value(frames.cache_memory_ledger().unwrap()).unwrap()
}

#[test]
fn cached_pixel_capacity_deduplicates_shared_frames_and_preserves_distinct_allocations() {
    let mut pixels = Vec::with_capacity(64);
    pixels.extend([1, 2, 3, 255]);
    let capacity = pixels.capacity() as u64;
    let shared = frame(VideoPixels::Rgba(pixels));
    let mut frames = VideoFrames::default();
    add_stream(&mut frames, 20, shared.clone());
    add_stream(&mut frames, 10, shared.clone());
    let before = Arc::strong_count(&shared);
    assert_eq!(
        frames.cache_bytes().unwrap(),
        8,
        "legacy per-cache payload semantics remain unchanged"
    );
    let observed = value(&frames);
    assert_eq!(observed["knownPayloadBytes"], 4);
    assert_eq!(observed["knownCapacityBytes"], capacity);
    assert_eq!(observed["resourceCount"], 1);
    assert_eq!(
        observed["resources"][0]["references"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(observed["resources"][0]["references"][0]["object"], 10);
    assert_eq!(
        Arc::strong_count(&shared),
        before,
        "query must not retain frame ownership"
    );
    assert_eq!(observed, value(&frames));
    // Same source/PTS/content is not the same allocation.
    add_stream(
        &mut frames,
        30,
        frame(VideoPixels::Rgba(vec![1, 2, 3, 255])),
    );
    assert_eq!(value(&frames)["resourceCount"], 2);
    frames
        .streams
        .get_mut(&10)
        .unwrap()
        .shared
        .0
        .lock()
        .unwrap()
        .cache = FrameCache::new();
    assert_eq!(value(&frames)["knownCapacityBytes"], capacity + 4);
    frames.clear();
    let empty = value(&frames);
    assert_eq!(empty["knownCapacityBytes"], 0);
    assert_eq!(empty["resourceCount"], 0);
    assert!(empty["unknownBytes"].is_null());
    assert_eq!(empty["completeCoverage"], false);
}

#[test]
fn yuv_capacity_counts_both_owned_planes_without_materializing_rgba() {
    let mut y = Vec::with_capacity(16);
    y.extend([128; 4]);
    let mut uv = Vec::with_capacity(8);
    uv.extend([100, 160]);
    let reserved = y.capacity() + uv.capacity();
    let mut frames = VideoFrames::default();
    add_stream(
        &mut frames,
        1,
        frame(VideoPixels::Yuv(motion_render::Yuv420Frame {
            width: 2,
            height: 2,
            rotation: 0,
            standard: 1,
            range: 2,
            phase: [0, 0],
            chroma_layout: motion_render::ChromaLayout::Uv,
            y,
            uv,
        })),
    );
    let observed = value(&frames);
    assert_eq!(observed["knownPayloadBytes"], 6);
    assert_eq!(observed["knownCapacityBytes"], reserved);
    assert_eq!(observed["resources"][0]["format"], "yuv420");
}

#[test]
fn busy_cache_is_unknown_without_blocking_and_recovers_after_release() {
    let mut frames = VideoFrames::default();
    add_stream(&mut frames, 7, frame(VideoPixels::Rgba(vec![0; 16])));
    let shared = frames.streams[&7].shared.clone();
    let guard = shared.0.lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let query = std::thread::spawn(move || {
        send.send(value(&frames)).unwrap();
        frames
    });
    let observed = receive.recv_timeout(std::time::Duration::from_secs(5));
    // Always release/join before asserting, including a future blocking regression.
    drop(guard);
    let frames = query.join().unwrap();
    let observed = observed.expect("diagnostic read blocked on decoder state");
    assert_eq!(observed["allActiveStreamCachesObserved"], false);
    assert!(observed["unavailableStreams"][0]["capacityBytes"].is_null());
    assert_eq!(observed["unavailableStreams"][0]["reason"], "busy");
    let recovered = value(&frames);
    assert_eq!(recovered["allActiveStreamCachesObserved"], true);
    assert_eq!(recovered["knownCapacityBytes"], 16);
}

#[test]
fn poisoned_cache_preserves_object_source_and_phase_in_an_actual_error() {
    let mut frames = VideoFrames::default();
    add_stream(&mut frames, 7, frame(VideoPixels::Rgba(vec![0; 16])));
    let shared = frames.streams[&7].shared.clone();
    assert!(std::panic::catch_unwind(|| {
        let _guard = shared.0.lock().unwrap();
        panic!("controlled cache poison");
    })
    .is_err());
    let error = frames
        .cache_memory_ledger()
        .err()
        .expect("poison cannot become zero capacity")
        .to_string();
    for context in [
        "object=7",
        "source=17",
        "phase=cache_memory_ledger",
        "poisoned",
    ] {
        assert!(error.contains(context), "{error}");
    }
}

#[test]
fn independent_stream_collections_share_one_identity_scope_and_release_each_owner() {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend([1, 2, 3, 255]);
    let capacity = bytes.capacity() as u64;
    let shared = frame(VideoPixels::Rgba(bytes));
    let mut active = VideoFrames::default();
    let mut original = VideoFrames::default();
    add_stream(&mut active, 10, shared.clone());
    add_stream(&mut original, 20, shared.clone());
    add_stream(
        &mut original,
        30,
        frame(VideoPixels::Rgba(vec![1, 2, 3, 255])),
    );
    let query = |active: &VideoFrames, original: &VideoFrames| {
        let mut collector = PixelCacheCollector::default();
        active
            .collect_cache_memory(&mut collector, "active_video_stream")
            .unwrap();
        original
            .collect_cache_memory(&mut collector, "original_source_video")
            .unwrap();
        collector.finish("global-stream-test")
    };
    let before = Arc::strong_count(&shared);
    let observed = query(&active, &original);
    assert_eq!(observed.known_payload_bytes, 8);
    assert_eq!(observed.known_capacity_bytes, capacity + 4);
    assert_eq!(observed.details["resourceCount"], 2);
    let aliases = observed.details["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["references"].as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(aliases["references"][0]["owner"], "active_video_stream");
    assert_eq!(aliases["references"][1]["owner"], "original_source_video");
    assert_eq!(Arc::strong_count(&shared), before);
    active.clear();
    assert_eq!(
        query(&active, &original).known_capacity_bytes,
        observed.known_capacity_bytes
    );
    original.clear();
    assert_eq!(query(&active, &original).known_capacity_bytes, 0);
    assert_eq!(Arc::strong_count(&shared), 1);
}

#[test]
fn busy_original_cache_preserves_known_active_pixels_and_recovers_without_blocking() {
    let mut active = VideoFrames::default();
    let mut original = VideoFrames::default();
    add_stream(&mut active, 1, frame(VideoPixels::Rgba(vec![0; 16])));
    add_stream(&mut original, 2, frame(VideoPixels::Rgba(vec![0; 32])));
    let shared = original.streams[&2].shared.clone();
    let guard = shared.0.lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut collector = PixelCacheCollector::default();
        active
            .collect_cache_memory(&mut collector, "active_video_stream")
            .unwrap();
        let info = original
            .collect_cache_memory(&mut collector, "original_source_video")
            .unwrap();
        send.send((collector.finish("busy-original-test"), info))
            .unwrap();
        (active, original)
    });
    let result = receive.recv_timeout(std::time::Duration::from_secs(5));
    drop(guard);
    let (active, original) = worker.join().unwrap();
    let (observed, info) = result.expect("original cache observation blocked on decoder state");
    assert_eq!(observed.known_capacity_bytes, 16);
    assert_eq!(info["allActiveStreamCachesObserved"], false);
    assert_eq!(
        info["unavailableStreams"][0]["owner"],
        "original_source_video"
    );
    assert!(info["unavailableStreams"][0]["capacityBytes"].is_null());
    let mut collector = PixelCacheCollector::default();
    active
        .collect_cache_memory(&mut collector, "active_video_stream")
        .unwrap();
    let info = original
        .collect_cache_memory(&mut collector, "original_source_video")
        .unwrap();
    assert_eq!(info["allActiveStreamCachesObserved"], true);
    assert_eq!(
        collector.finish("busy-original-test").known_capacity_bytes,
        48
    );
}

#[test]
fn stream_observation_bounds_mark_unvisited_caches_unknown_without_changing_them() {
    let shared = frame(VideoPixels::Rgba(vec![0; 16]));
    let mut frames = VideoFrames::default();
    for object in 0..=MAX_OBSERVED_STREAMS {
        add_stream(&mut frames, object as u64, shared.clone());
    }
    let before = Arc::strong_count(&shared);
    let observed = value(&frames);
    assert_eq!(observed["streamCount"], MAX_OBSERVED_STREAMS + 1);
    assert_eq!(observed["observedStreams"], MAX_OBSERVED_STREAMS);
    assert_eq!(observed["unobservedStreams"], 1);
    assert_eq!(observed["allActiveStreamCachesObserved"], false);
    assert_eq!(observed["knownCapacityBytes"], 16);
    assert_eq!(frames.streams.len(), MAX_OBSERVED_STREAMS + 1);
    assert_eq!(Arc::strong_count(&shared), before);
    assert!(observed["unknownBytes"].is_null());
}
