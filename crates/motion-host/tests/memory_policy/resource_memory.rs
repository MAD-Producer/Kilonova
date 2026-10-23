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
    session.audio_pcm = Vec::with_capacity(32);
    session.audio_pcm.extend([0.; 8]);
    session.composition_bundle = Vec::with_capacity(64);
    session.composition_bundle.extend([1, 2, 3, 4]);
    let capacity = session.audio_pcm.capacity() * 4 + session.composition_bundle.capacity();
    let observed = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(observed["cpu"]["knownPayloadBytes"], 36);
    assert_eq!(observed["cpu"]["knownCapacityBytes"], capacity);
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
    assert_eq!(cleared["cpu"]["knownPayloadBytes"], 0);
    assert_eq!(cleared["cpu"]["knownCapacityBytes"], capacity);
    session.audio_pcm = Vec::new();
    session.composition_bundle = Vec::new();
    assert_eq!(
        session.resource_memory_ledger().unwrap()["cpu"]["knownCapacityBytes"],
        0
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
    let result = motion_host::ops::preview::resource_memory(handle);
    motion_host::session::close(handle);
    let value = result.unwrap();
    assert_eq!(value["scope"], "session_instrumented_resource_ownership");
    assert_eq!(value["cpu"]["knownCapacityBytes"], 0);
    assert_eq!(value["textures"]["graphicsAvailable"], false);
}
