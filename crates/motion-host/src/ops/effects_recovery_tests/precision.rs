use super::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::TextureCategory;
#[path = "../../../../motion-render/tests/support/sdr_fixture.rs"]
mod fixture;

fn hdr_editor() -> (tempfile::TempDir, Session, String) {
    let root = tempfile::tempdir().unwrap();
    let (mut project, mut registry) = fixture::project(ChannelPrecision::Unorm8, false);
    // Open a real packaged editor without letting its disabled effect alter
    // the independently authored HDR/transparent output probe.
    let package = scene_fixtures::particle_package().unwrap();
    let mut editor_effect = motion_core::EffectInstance::new(
        2,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [18.; 2],
    );
    editor_effect.enabled = false;
    project.layers[0].effects.push(editor_effect);
    project.rebuild_plugin_dependencies();
    registry.insert(package).unwrap();
    let mut session = Session::new(project, root.path().into(), Arc::new(Stub)).unwrap();
    session.effects.set_registry(registry);
    session.frame = 2.;
    let opened = session
        .plugin_request(r#"{"op":"editor_open","object":1,"instance":2}"#)
        .unwrap();
    (root, session, opened["token"].as_str().unwrap().into())
}
fn request(token: &str, size: u32) -> String {
    json!({"op":"editor_message","token":token,
        "message":{"op":"preview","width":size,"height":size}})
    .to_string()
}
fn target_ids(session: &Session) -> Vec<String> {
    session
        .editor_target
        .as_ref()
        .unwrap()
        .texture_resource_ledger()
        .resources()
        .map(|r| r.id.clone())
        .collect()
}
fn system_ids(session: &Session) -> Vec<String> {
    session
        .editor_renderer
        .as_ref()
        .unwrap()
        .texture_resource_ledger()
        .resources()
        .filter(|r| {
            r.references
                .iter()
                .any(|r| r.category == TextureCategory::SystemTextures)
        })
        .map(|r| r.id.clone())
        .collect()
}
fn assert_protocol_pixels(session: &Session, response: &Value) -> Vec<u8> {
    let renderer = session.editor_renderer.as_ref().unwrap();
    let pixels = renderer
        .read_sdr_target(session.editor_target.as_ref().unwrap())
        .unwrap()
        .pixels;
    fixture::assert_png(&pixels);
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &pixels,
        18,
        18,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    assert_eq!(response["png"], crate::encode_base64(&png));
    png
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn packaged_editor_preview_tracks_four_precisions_reuses_targets_and_keeps_history() {
    let (_root, mut session, token) = hdr_editor();
    let mut original_system = None;
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
        ChannelPrecision::Unorm8,
    ] {
        session
            .engine
            .apply(Command::SetWorkingPrecision { precision })
            .unwrap();
        let before = (
            session.engine.snapshot(),
            session.engine.revision(),
            session.view_revision,
        );
        let response = session.plugin_request(&request(&token, 18)).unwrap();
        let renderer = session.editor_renderer.as_ref().unwrap();
        assert_eq!(renderer.working_precision(), precision);
        let target = session.editor_target.as_ref().unwrap();
        assert_eq!(target.working_format(), renderer.target_format);
        assert_eq!((target.width(), target.height()), (18, 18));
        assert_eq!(response["revision"], before.1);
        assert_eq!(response["frame"], 2.);
        let png = assert_protocol_pixels(&session, &response);
        if let Some(directory) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{precision:?}-editor.png")), png).unwrap();
        }
        let system = system_ids(&session);
        assert!(!system.is_empty());
        if let Some(original) = &original_system {
            assert_eq!(original, &system);
        } else {
            original_system = Some(system);
        }
        let ids = target_ids(&session);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let occupied = session.allocation_budget().info().unwrap();
        let repeated = session.plugin_request(&request(&token, 18)).unwrap();
        assert_eq!(response["png"], repeated["png"]);
        assert_eq!(ids, target_ids(&session));
        let after = session.allocation_budget().info().unwrap();
        assert_eq!(after.admitted_bytes, occupied.admitted_bytes);
        assert_eq!(after.live_allocations, occupied.live_allocations);
        assert_eq!(
            (
                session.engine.snapshot(),
                session.engine.revision(),
                session.view_revision
            ),
            before
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn editor_sdr_resize_refusal_retains_old_target_and_recovers_without_project_edits() {
    let (_root, mut session, token) = hdr_editor();
    session
        .engine
        .apply(Command::SetWorkingPrecision {
            precision: ChannelPrecision::Float32,
        })
        .unwrap();
    let original = session.plugin_request(&request(&token, 18)).unwrap();
    let ids = target_ids(&session);
    let system = system_ids(&session);
    let device = session.editor_renderer.as_ref().unwrap().device.clone();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let budget = session.allocation_budget().clone();
    let occupied = budget.info().unwrap();
    let before = (
        session.engine.snapshot(),
        session.engine.revision(),
        session.view_revision,
    );
    let mut policy = session.effects.memory_policy();
    policy.session_allocation_budget_bytes = occupied.admitted_bytes;
    session.configure_memory_policy(policy).unwrap();
    // Each policy change deliberately invalidates retained presentation once.
    // The failed preview itself must not introduce an extra invalidation.
    assert_eq!(session.view_revision, before.2 + 1);
    let error = session.plugin_request(&request(&token, 512)).unwrap_err();
    let crate::Error::Render(cause) = error else {
        panic!("native refusal cause was erased");
    };
    assert!(
        matches!(cause.resource_rejection(), Some(motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations, required_bytes, budget_bytes,
    }) if *required_bytes > *budget_bytes && *budget_bytes == occupied.admitted_bytes)
    );
    assert_eq!(target_ids(&session), ids);
    assert_eq!(system_ids(&session), system);
    assert_eq!(session.view_revision, before.2 + 1);
    assert_protocol_pixels(&session, &original);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        occupied.admitted_bytes
    );
    policy.session_allocation_budget_bytes = 64 << 20;
    session.configure_memory_policy(policy).unwrap();
    assert_eq!(session.view_revision, before.2 + 2);
    let recovered = session.plugin_request(&request(&token, 36)).unwrap();
    assert_eq!(
        (recovered["width"].as_u64(), recovered["height"].as_u64()),
        (Some(36), Some(36))
    );
    assert_ne!(target_ids(&session), ids);
    assert_eq!(system_ids(&session), system);
    assert_eq!(
        (
            session.engine.snapshot(),
            session.engine.revision(),
            session.view_revision
        ),
        (before.0, before.1, before.2 + 2)
    );
    session
        .plugin_request(&json!({"op":"editor_close","token":token,"commit":false}).to_string())
        .unwrap();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(session.editor_target.is_none() && session.editor_renderer.is_none());
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}
