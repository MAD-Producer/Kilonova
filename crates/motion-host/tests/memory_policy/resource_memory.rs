use super::*;

#[test]
fn unified_resource_query_keeps_cpu_capacity_separate_and_does_not_initialize_graphics() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(
        Project::new(64, 64, 30, 60).unwrap(),
        directory.path().to_path_buf(),
        Arc::new(Stub),
    )
    .unwrap();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let project = session.engine.snapshot();
    let packages = session.effects.package_memory_info();
    assert!(
        packages.known_capacity_bytes > 0,
        "startup retains actual built-in packages"
    );
    session.audio_pcm = Vec::with_capacity(32);
    session.audio_pcm.extend([0.; 8]);
    session.composition_bundle = Vec::with_capacity(64);
    session.composition_bundle.extend([1, 2, 3, 4]);
    let capacity = session.audio_pcm.capacity() * 4 + session.composition_bundle.capacity();
    let observed = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(
        observed["cpu"]["knownPayloadBytes"],
        packages.known_payload_bytes + 36
    );
    assert_eq!(
        observed["cpu"]["knownCapacityBytes"],
        packages.known_capacity_bytes + capacity as u64
    );
    assert_eq!(observed["cpu"]["audioOutputScratch"]["payloadBytes"], 32);
    assert_eq!(observed["cpu"]["compositionBundle"]["payloadBytes"], 4);
    assert_eq!(observed["textures"]["knownBytes"], 0);
    assert_eq!(observed["cpu"]["videoDecodedCache"]["resourceCount"], 0);
    assert!(observed["external"]["driverResidentBytes"].is_null());
    assert!(observed["external"]["decoderBytes"].is_null());
    assert!(observed["cpu"]["unknownBytes"].is_null());
    assert_eq!(observed["completeCoverage"], false);
    assert_eq!(observed, session.resource_memory_ledger().unwrap());
    assert!(session.graphics.is_none());
    assert!(session.editor_renderer.is_none());
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert_eq!(session.engine.snapshot(), project);
    // Clear payloads without freeing Vec backing storage: capacity must remain.
    session.audio_pcm.clear();
    session.composition_bundle.clear();
    let cleared = session.resource_memory_ledger().unwrap();
    assert_eq!(
        cleared["cpu"]["knownPayloadBytes"],
        packages.known_payload_bytes
    );
    assert_eq!(
        cleared["cpu"]["knownCapacityBytes"],
        packages.known_capacity_bytes + capacity as u64
    );
    assert_eq!(cleared["cpu"]["audioOutputScratch"]["payloadBytes"], 0);
    assert_eq!(cleared["cpu"]["compositionBundle"]["payloadBytes"], 0);
    session.audio_pcm = Vec::new();
    session.composition_bundle = Vec::new();
    assert_eq!(
        session.resource_memory_ledger().unwrap()["cpu"]["knownCapacityBytes"],
        packages.known_capacity_bytes
    );
}

#[test]
fn public_resource_query_uses_the_same_session_read_protocol() {
    let directory = tempfile::tempdir().unwrap();
    let handle = motion_host::session::open(
        directory.path().into(),
        Project::new(64, 64, 30, 60).unwrap(),
        Arc::new(Stub),
    )
    .unwrap();
    let expected = motion_host::session::with_session(handle, |session| {
        Ok(session.effects.package_memory_info().known_capacity_bytes)
    })
    .unwrap();
    let result = motion_host::ops::preview::resource_memory(handle);
    motion_host::session::close(handle);
    let value = result.unwrap();
    assert_eq!(value["scope"], "session_instrumented_resource_ownership");
    assert_eq!(value["cpu"]["knownCapacityBytes"], expected);
    assert!(expected > 0);
    assert_eq!(value["textures"]["graphicsAvailable"], false);
}

#[test]
fn unified_query_includes_real_font_and_mixer_capacity_without_changing_pcm_or_history() {
    use motion_core::{AudioAsset, AudioClip, Content, Layer, LayerTimeline};
    use motion_media::{fonts::FontStore, AudioMixer};

    let directory = tempfile::tempdir().unwrap();
    let asset = AudioAsset {
        id: 1,
        path: "assets/memory.wav".into(),
        mime: "audio/wav".into(),
        bytes: 8192,
        track: 0,
        sample_rate: 48000,
        channels: 1,
        sample_frames: 2048,
        duration_us: 2048 * 1_000_000 / 48000,
    };
    let cache = motion_media::cache_path(directory.path(), &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(cache, 0.125f32.to_le_bytes().repeat(2048)).unwrap();
    let mut project = Project::new(64, 64, 30, 3).unwrap();
    project.audio_assets.push(asset);
    let mut layer = Layer::solid(1, "Original PCM fixture", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 1,
        offset_frame: 0,
    });
    project.layers.push(layer);
    project.validate().unwrap();
    let mut control = AudioMixer::new(project.clone(), directory.path()).unwrap();
    let mut session = Session::new(project, directory.path().into(), Arc::new(Stub)).unwrap();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let project_before = session.engine.snapshot();
    let packages = session.effects.package_memory_info();
    let mut store = FontStore::new(&directory.path().join("fonts")).unwrap();
    let glyph = store.glyph(&FontStore::builtin_id(), 'A', 32.).unwrap();
    let font_info = store.memory_info();
    let mixer = AudioMixer::new(session.engine.project().clone(), directory.path()).unwrap();
    let mixer_info = mixer.memory_info();
    session.font_store = Some(store);
    session.audio_mixer = Some((revision, mixer));
    session.audio_pcm = vec![0.; 256];
    let output_payload = session.audio_pcm.len() as u64 * 4;
    let output_capacity = session.audio_pcm.capacity() as u64 * 4;
    let observed = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(
        observed["cpu"]["knownPayloadBytes"],
        font_info.known_payload_bytes
            + mixer_info.known_payload_bytes
            + output_payload
            + packages.known_payload_bytes
    );
    assert_eq!(
        observed["cpu"]["knownCapacityBytes"],
        font_info.known_capacity_bytes
            + mixer_info.known_capacity_bytes
            + output_capacity
            + packages.known_capacity_bytes
    );
    assert_eq!(
        observed["cpu"]["fontGlyphCache"],
        serde_json::to_value(font_info).unwrap()
    );
    assert_eq!(
        observed["cpu"]["audioMixer"],
        serde_json::to_value(mixer_info).unwrap()
    );
    assert!(observed["cpu"]["fontGlyphCache"]["faceCapacityBytes"].is_null());
    assert!(observed["cpu"]["audioMixer"]["fileBackedPcmResidentBytes"].is_null());
    let mut actual = vec![f32::NAN; 256];
    let mut expected = vec![f32::NAN; 256];
    for start in [0, 128, 32] {
        session.resource_memory_ledger().unwrap();
        assert_eq!(
            session
                .audio_mixer
                .as_mut()
                .unwrap()
                .1
                .mix(start, &mut actual)
                .unwrap(),
            control.mix(start, &mut expected).unwrap()
        );
        assert_eq!(
            actual, expected,
            "resource query must preserve exact PCM output"
        );
    }
    assert!(session.graphics.is_none());
    assert!(session.editor_renderer.is_none());
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert_eq!(session.engine.snapshot(), project_before);
    session.font_store = None;
    session.audio_mixer = None;
    let released = session.resource_memory_ledger().unwrap();
    assert_eq!(
        released["cpu"]["knownCapacityBytes"],
        output_capacity + packages.known_capacity_bytes
    );
    assert!(released["cpu"]["fontGlyphCache"].is_null());
    assert!(released["cpu"]["audioMixer"].is_null());
    assert!(
        !glyph.coverage.is_empty(),
        "escaped glyph remains outside Session's observed subtotal"
    );
}
