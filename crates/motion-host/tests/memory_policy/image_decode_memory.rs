use super::*;
use motion_host::preview_inputs::{
    InputPixels, InputPoll, InputRequest, InputSource, PreviewInputReader,
};
use motion_render::{image_resources::Resolution, Scene};
use std::time::{Duration, Instant};

fn fixture() -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]))
        .save(root.path().join("assets/input.png"))
        .unwrap();
    let mut project = Project::new(32, 32, 30, 3).unwrap();
    project.assets.push(motion_core::Asset {
        id: 17,
        path: "assets/input.png".into(),
        width: 4,
        height: 4,
    });
    let mut layer = motion_core::Layer::solid(
        1,
        "Authored image memory fixture",
        [32.; 2],
        [16., 16., 0.],
        [1.; 4],
    );
    layer.content = motion_core::Content::Image { asset: 17 };
    project.layers.push(layer);
    let session = Session::new(project, root.path().into(), Arc::new(Stub)).unwrap();
    (root, session)
}
fn request(session: &mut Session, root: &Path) {
    session
        .preview_inputs
        .request(
            session.engine.project(),
            root,
            session.engine.revision(),
            InputRequest {
                sequence: 1,
                source: InputSource::ImageAsset { asset: 17 },
                frame: 0.,
                max_edge: 4,
            },
        )
        .unwrap();
}
fn completed(session: &Session, owners: usize) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let info = session.resource_memory_ledger().unwrap();
        let ready = info["cpu"]["imageDecodeTasks"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["observation"]["details"]["phase"] == "completed_output_retained")
            .count();
        if ready == owners {
            return info;
        }
        assert!(
            Instant::now() < deadline,
            "real image workers did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn packet(session: &mut Session) -> motion_host::preview_inputs::PreparedInput {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match session
            .preview_inputs
            .poll(1, session.engine.revision())
            .unwrap()
        {
            InputPoll::Ready(packet) => return packet,
            InputPoll::Pending(_) => {}
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn public_query_keeps_real_queued_png_then_transfers_one_capacity_to_cache_without_io_or_state_changes(
) {
    let (root, mut session) = fixture();
    let baseline = session.resource_memory_ledger().unwrap();
    let snapshot = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let gpu = session.texture_resource_ledger();
    request(&mut session, root.path());
    let complete = completed(&session, 1);
    let capacity = complete["cpu"]["imageDecodeTasks"]["knownCapacityBytes"]
        .as_u64()
        .unwrap();
    assert_eq!(complete["cpu"]["imageDecodeTasks"]["knownPayloadBytes"], 64);
    assert_eq!(complete["cpu"]["sourcePixelCaches"]["resourceCount"], 0);
    assert_eq!(
        complete["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"].as_u64().unwrap() + capacity
    );
    let old = root.path().join("assets/input.png");
    let moved = root.path().join("assets/moved.png");
    std::fs::rename(&old, &moved).unwrap();
    let read = session
        .plugin_request(r#"{"op":"resource_memory"}"#)
        .unwrap();
    assert_eq!(
        read["cpu"]["imageDecodeTasks"]["knownCapacityBytes"],
        capacity
    );
    assert_eq!(
        read["cpu"]["sourcePixelCaches"]["resourceCount"], 0,
        "query cannot drain completion"
    );
    assert_eq!(session.texture_resource_ledger(), gpu);
    assert!(session.graphics.is_none() && session.editor_renderer.is_none());
    // Restore before the real reader's existing source-fingerprint validation.
    std::fs::rename(&moved, &old).unwrap();
    let packet = packet(&mut session);
    let InputPixels::Image(pixels) = &packet.pixels else {
        panic!("real PNG packet expected")
    };
    assert_eq!(pixels.rgba, [255, 0, 0, 255].repeat(16));
    assert_eq!(capacity, pixels.rgba.capacity() as u64);
    let strong = Arc::strong_count(pixels);
    let cached = session.resource_memory_ledger().unwrap();
    assert_eq!(cached["cpu"]["imageDecodeTasks"]["knownCapacityBytes"], 0);
    assert_eq!(
        cached["cpu"]["sourcePixelCaches"]["knownCapacityBytes"],
        capacity
    );
    assert_eq!(
        cached["cpu"]["knownCapacityBytes"],
        complete["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(Arc::strong_count(pixels), strong);
    session.preview_inputs.cancel();
    let released = session.resource_memory_ledger().unwrap();
    assert_eq!(
        released["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(Arc::strong_count(pixels), 1);
    assert_eq!(pixels.rgba, [255, 0, 0, 255].repeat(16));
    assert_eq!(session.engine.snapshot(), snapshot);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert!(!session.engine.can_undo() && !session.engine.can_redo());
    assert!(released["external"]["processResidentBytes"].is_null());
}

#[test]
fn cancelling_original_input_preserves_actual_queued_storage_until_its_owner_is_dropped() {
    let (root, mut session) = fixture();
    let baseline = session.resource_memory_ledger().unwrap();
    request(&mut session, root.path());
    let complete = completed(&session, 1);
    session.preview_inputs.cancel();
    let cancelled = session.resource_memory_ledger().unwrap();
    assert_eq!(
        cancelled["cpu"]["imageDecodeTasks"]["knownCapacityBytes"],
        complete["cpu"]["imageDecodeTasks"]["knownCapacityBytes"]
    );
    assert_eq!(
        cancelled["cpu"]["knownCapacityBytes"],
        complete["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(
        cancelled["cpu"]["imageDecodeTasks"]["tasks"][0]["observation"]["details"]
            ["cancelRequested"],
        true
    );
    assert_eq!(cancelled["cpu"]["sourcePixelCaches"]["resourceCount"], 0);
    // Explicit owner destruction is different from merely requesting cancel.
    // The diagnostic query must not consume that owner's queued result.
    let old = std::mem::replace(
        &mut session.preview_inputs,
        PreviewInputReader::new(Arc::new(Stub)),
    );
    drop(old);
    let fresh = session.resource_memory_ledger().unwrap();
    assert_eq!(
        fresh["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"]
    );
    assert_eq!(fresh["cpu"]["imageDecodeTasks"]["knownCapacityBytes"], 0);
    assert_eq!(
        fresh["cpu"]["imageDecodeTasks"]["tasks"][0]["observation"]["details"]["pending"],
        false
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_editor_and_original_image_tasks_count_independently_then_transfer_to_cache_and_gpu() {
    let (root, mut session) = fixture();
    let budget = session.allocation_budget().clone();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    renderer.set_effect_registry(session.effects.registry.clone());
    renderer
        .configure_assets(session.engine.project(), root.path())
        .unwrap();
    let target = renderer.capture_target(32, 32).unwrap();
    session.editor_renderer = Some(renderer);
    let mut scene = Scene::new(session.engine.project()).unwrap();
    scene
        .sample(
            session.engine.project(),
            0.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    let baseline = session.resource_memory_ledger().unwrap();
    let snapshot = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let gpu = session.texture_resource_ledger();
    let allowance = budget.info().unwrap();
    request(&mut session, root.path());
    assert!(!session
        .editor_renderer
        .as_mut()
        .unwrap()
        .prepare_scene_assets(&scene, Resolution::Full, true)
        .unwrap());
    let complete = completed(&session, 2);
    let tasks = complete["cpu"]["imageDecodeTasks"]["tasks"]
        .as_array()
        .unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["owner"], "original_source");
    assert_eq!(tasks[1]["owner"], "editor_renderer");
    assert_eq!(
        complete["cpu"]["imageDecodeTasks"]["knownPayloadBytes"],
        128
    );
    let total = complete["cpu"]["imageDecodeTasks"]["knownCapacityBytes"]
        .as_u64()
        .unwrap();
    let original = tasks[0]["observation"]["knownCapacityBytes"]
        .as_u64()
        .unwrap();
    let editor = tasks[1]["observation"]["knownCapacityBytes"]
        .as_u64()
        .unwrap();
    assert_eq!(total, original + editor);
    assert_eq!(
        complete["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"].as_u64().unwrap() + total
    );
    assert_eq!(session.texture_resource_ledger(), gpu);
    assert_eq!(budget.info().unwrap(), allowance);
    let retained = packet(&mut session);
    let after_source = session.resource_memory_ledger().unwrap();
    assert_eq!(
        after_source["cpu"]["imageDecodeTasks"]["knownCapacityBytes"],
        editor
    );
    assert_eq!(
        after_source["cpu"]["sourcePixelCaches"]["knownCapacityBytes"],
        original
    );
    assert_eq!(
        after_source["cpu"]["knownCapacityBytes"],
        complete["cpu"]["knownCapacityBytes"]
    );
    let renderer = session.editor_renderer.as_mut().unwrap();
    assert!(renderer
        .prepare_scene_assets(&scene, Resolution::Full, true)
        .unwrap());
    assert_eq!(renderer.image_dimensions(17), Some((4, 4)));
    renderer.draw(&scene, &target.view, 32, 32).unwrap();
    let rendered = renderer.read_target(&target).unwrap();
    assert_eq!(
        &rendered[(16 * 32 + 16) * 4..(16 * 32 + 17) * 4],
        &[255, 0, 0, 255]
    );
    let uploaded = session.resource_memory_ledger().unwrap();
    assert_eq!(uploaded["cpu"]["imageDecodeTasks"]["knownCapacityBytes"], 0);
    assert_eq!(
        uploaded["cpu"]["knownCapacityBytes"],
        baseline["cpu"]["knownCapacityBytes"].as_u64().unwrap() + original
    );
    assert_eq!(session.engine.snapshot(), snapshot);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert!(!session.engine.can_undo() && !session.engine.can_redo());
    let InputPixels::Image(pixels) = retained.pixels else {
        panic!("real packet expected")
    };
    assert_eq!(pixels.rgba, [255, 0, 0, 255].repeat(16));
}
