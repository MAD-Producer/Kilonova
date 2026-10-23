use motion_core::{Command, Project};
use motion_host::{Platform, Session};
use motion_render::{resource_policy::MemoryPolicy, Renderer};
use std::{path::Path, sync::Arc};

struct Stub;

#[path = "memory_policy/package_budget.rs"]
mod package_budget;

#[test]
fn cpu_asset_entry_points_follow_session_policy_without_changing_legacy_pixels_or_history() {
    struct Open(i64);
    impl Drop for Open {
        fn drop(&mut self) { motion_host::session::close(self.0); }
    }
    fn rejection(error: &motion_host::Error) -> &motion_render::ResourceRejection {
        match error {
            motion_host::Error::Context { cause, .. } => rejection(cause),
            motion_host::Error::ImageResource(error) => error.resource_rejection().unwrap(),
            other => panic!("CPU capacity lost native identity: {other:?}"),
        }
    }
    // Preserve both entry points' existing conversion semantics, including
    // transparent colors and a 16-bit source converted to the RGBA8 result.
    for sixteen_bit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("assets")).unwrap();
        let path = directory.path().join("assets/image.png");
        if sixteen_bit {
            let encoder = image::codecs::png::PngEncoder::new(std::fs::File::create(&path).unwrap());
            let samples: [u16; 16] = [65535, 0, 0, 32768, 65535, 65535, 65535, 0,
                12345, 23789, 45678, 65535, 0, 0, 65535, 65535];
            use image::ImageEncoder;
            encoder.write_image(bytemuck::cast_slice(&samples), 4, 1, image::ExtendedColorType::Rgba16).unwrap();
        } else {
            image::save_buffer(&path,
                &[255, 0, 0, 128, 255, 255, 255, 0, 37, 113, 241, 255, 0, 0, 255, 255],
                4, 1, image::ColorType::Rgba8).unwrap();
        }
        let source_bytes = std::fs::read(&path).unwrap();
        let asset = motion_core::Asset { id: 17, path: "assets/image.png".into(), width: 4, height: 1 };
        let mut expected_legacy = image::ImageReader::open(&path).unwrap().decode().unwrap().into_rgba8().into_raw();
        motion_render::premultiply_pixels(&mut expected_legacy);
        let source = motion_render::image_resources::Source::new(directory.path(), &asset).unwrap();
        let expected_into = motion_render::image_resources::decode(
            &source, motion_render::image_resources::Resolution::Full).unwrap().rgba;
        let mut project = Project::new(64, 64, 30, 6).unwrap();
        project.assets.push(asset);
        let handle = Open(motion_host::session::open(directory.path().into(), project, Arc::new(Stub)).unwrap());
        let (before, revision, policy) = motion_host::session::with_session(handle.0, |session| {
            Ok((session.engine.snapshot(), session.engine.revision(), session.effects.memory_policy()))
        }).unwrap();
        let small = MemoryPolicy { decoded_image_budget_bytes: 8, ..policy };
        motion_host::ops::preview::configure_memory_policy(handle.0, small).unwrap();
        let error = motion_host::ops::export::asset_pixels(handle.0, 17).unwrap_err();
        assert_eq!(rejection(&error), &motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::DecodedImages, required_bytes: 16, budget_bytes: 8,
        });
        assert!(error.to_string().contains("asset=17"));
        let mut output = [91; 20];
        let error = motion_host::ops::images::asset_pixels_into(handle.0, 17, &mut output).unwrap_err();
        assert_eq!(rejection(&error), &motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::DecodedImages, required_bytes: 16, budget_bytes: 8,
        });
        assert_eq!(output, [91; 20]);
        motion_host::ops::preview::configure_memory_policy(handle.0,
            MemoryPolicy { decoded_image_budget_bytes: 16, ..policy }).unwrap();
        assert_eq!(motion_host::ops::export::asset_pixels(handle.0, 17).unwrap(), expected_legacy);
        let info = motion_host::ops::images::asset_pixels_into(handle.0, 17, &mut output).unwrap();
        assert_eq!(info["bytes"], 16);
        assert_eq!(&output[..16], expected_into);
        assert_eq!(output[16..], [91; 4]);
        motion_host::session::with_session(handle.0, |session| {
            assert_eq!(session.engine.snapshot(), before);
            assert_eq!(session.engine.revision(), revision);
            assert!(!session.engine.can_undo());
            assert_eq!(session.effects.memory_policy().source_texture_budget_bytes, policy.source_texture_budget_bytes);
            Ok(())
        }).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), source_bytes);
    }
}

#[test]
fn session_envelope_survives_project_replacement_and_tracks_retained_native_views() {
    let directory=tempfile::tempdir().unwrap();
    let mut session=Session::new(Project::new(16,16,30,6).unwrap(),
        directory.path().to_owned(),Arc::new(Stub)).unwrap();
    let budget=session.allocation_budget().clone();
    let mut peak=0;
    for size in [16,24,32] {
        let mut project=Project::new(size,size,30,6).unwrap();
        project.background=[0.,1.,0.,1.];
        let root=directory.path().join(format!("project-{size}"));
        std::fs::create_dir(&root).unwrap();
        session.replace_project(motion_core::Engine::new(project).unwrap(),root).unwrap();
        let snapshot=session.engine.snapshot();
        let revision=session.engine.revision();
        let mut renderer=pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
        let target=renderer.capture_target(size,size).unwrap();
        for frame in [0.,3.,1.] {
            session.frame=frame;
            session.sample().unwrap();
            let pixels=renderer.capture(&session.scene,&target).unwrap().0;
            assert!(pixels.chunks_exact(4).all(|p|p==[0,255,0,255]));
        }
        let view=target.view.clone();
        let device=renderer.device.clone();
        let queue=renderer.queue.clone();
        session.editor_target=Some(target);
        session.editor_renderer=Some(renderer);
        peak=peak.max(budget.info().unwrap().admitted_bytes);
        assert_eq!(session.engine.snapshot(),snapshot);
        assert_eq!(session.engine.revision(),revision);
        assert!(!session.engine.can_undo());
        let next=session.engine.snapshot();
        session.replace_project(motion_core::Engine::new(next).unwrap(),session.root.clone()).unwrap();
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert!(session.allocation_budget().same_budget(&budget));
        assert_eq!(budget.info().unwrap().admitted_bytes,u64::from(size)*u64::from(size)*4);
        assert_eq!(budget.info().unwrap().live_allocations,1);
        drop(view);
        assert_eq!(budget.info().unwrap().admitted_bytes,0);
        assert_eq!(budget.info().unwrap().live_allocations,0);
        assert_eq!(budget.info().unwrap().peak_admitted_bytes,peak);
        drop(queue);
    }
}

#[test]
fn session_envelope_updates_are_atomic_and_metadata_queries_are_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(Project::new(16,16,30,6).unwrap(),
        directory.path().to_owned(),Arc::new(Stub)).unwrap();
    let budget = session.allocation_budget().clone();
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let original = session.effects.memory_policy();
    for limit in [0, (4u64 << 30) + 1] {
        let mut policy = original;
        policy.session_allocation_budget_bytes = limit;
        assert!(session.configure_memory_policy(policy).unwrap_err().to_string().contains("session allocation budget"));
        let error = session.plugin_request(&serde_json::json!({"op":"memory_policy","policy":policy}).to_string()).unwrap_err();
        assert!(error.to_string().contains("session allocation budget"));
        assert_eq!(session.effects.memory_policy(),original);
        assert_eq!(budget.info().unwrap().budget_bytes,original.session_allocation_budget_bytes);
        assert_eq!(session.view_revision,view);
    }
    let query = "{\"op\":\"allocation_budget\"}";
    let empty = session.plugin_request(query).unwrap();
    assert_eq!(empty["allocationBudget"]["admittedBytes"],0);
    assert_eq!(empty["allocationBudget"]["peakAdmittedBytes"],0);
    assert_eq!(empty["isFramePeak"],false);
    assert_eq!(empty["isDriverResidency"],false);
    let live = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let occupied = budget.info().unwrap();
    let info = session.plugin_request(query).unwrap();
    assert_eq!(info["allocationBudget"]["admittedBytes"],occupied.admitted_bytes);
    assert_eq!(info,session.plugin_request(query).unwrap());
    assert_eq!(session.allocation_budget_info().unwrap(),occupied);
    let mut policy = original;
    policy.session_allocation_budget_bytes = 1;
    session.configure_memory_policy(policy).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes,occupied.admitted_bytes);
    assert_eq!(budget.info().unwrap().budget_bytes,1);
    let error = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).err().unwrap();
    assert_eq!(error.resource_rejection(),Some(&motion_render::ResourceRejection::Budget {
        scope:motion_render::ResourceBudgetScope::RenderAllocations,
        required_bytes:occupied.admitted_bytes+4,budget_bytes:1}));
    assert_eq!(budget.info().unwrap().admitted_bytes,occupied.admitted_bytes);
    assert_eq!(session.engine.snapshot(),before);
    assert_eq!(session.engine.revision(),revision);
    assert!(!session.engine.can_undo());
    let device=live.device.clone();
    let queue=live.queue.clone();
    drop(live);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes,0);
    assert_eq!(budget.info().unwrap().live_allocations,0);
    assert_eq!(budget.info().unwrap().peak_admitted_bytes,occupied.peak_admitted_bytes);
    session.configure_memory_policy(original).unwrap();
    let recovered=pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes,occupied.admitted_bytes);
    drop(recovered);
    drop(queue);
}
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "memory-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn motion_host::VideoDecoder>> {
        Err("unused".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&motion_host::VideoQuery>) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        Err("unused".into())
    }
}

#[test]
fn pressure_metadata_keeps_unattached_state_unknown_and_existing_editor_query_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut session = Session::new(Project::new(32, 24, 30, 60).unwrap(), root, Arc::new(Stub)).unwrap();
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    let policy = session.effects.memory_policy();
    let allocation = session.allocation_budget_info().unwrap();
    let unknown = session.plugin_request(r#"{"op":"memory_pressure"}"#).unwrap();
    assert_eq!(unknown["memoryPressure"]["reading"]["status"], "unavailable");
    assert_eq!(unknown["memoryPressure"]["reading"]["reason"], "no_active_renderer");
    assert!(unknown["memoryPressure"]["reading"].get("pools").is_none());
    assert!(session.editor_renderer.is_none() && session.graphics.is_none());
    assert_eq!(session.allocation_budget_info().unwrap(), allocation);

    session.editor_renderer = Some(pollster::block_on(Renderer::headless_with_allocation_budget(
        session.allocation_budget().clone())).unwrap());
    let occupied = session.allocation_budget_info().unwrap();
    let first = session.plugin_request(r#"{"op":"memory_pressure"}"#).unwrap();
    let renderer = session.editor_renderer.as_ref().unwrap();
    assert_eq!(first["memoryPressure"]["backend"], format!("{:?}", renderer.adapter_info.backend));
    for _ in 0..100 {
        let next = session.plugin_request(r#"{"op":"memory_pressure"}"#).unwrap();
        assert_eq!(next["memoryPressure"]["backend"], first["memoryPressure"]["backend"]);
        assert_eq!(next["memoryPressure"]["minimumQueryIntervalMs"], 1000);
    }
    assert_eq!(session.allocation_budget_info().unwrap(), occupied);
    assert_eq!(session.effects.memory_policy(), policy);
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), revision);
    session.editor_renderer = None;
    assert_eq!(session.memory_pressure_info().reading, motion_render::MemoryPressureSnapshot::without_renderer().reading);
}

#[test]
fn shared_json_policy_is_atomic_runtime_state_and_inherited_by_editor_rendering() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let project = Project::new(64, 64, 30, 60).unwrap();
    let mut session = Session::new(project.clone(), root, Arc::new(Stub)).unwrap();
    session
        .engine
        .apply(Command::SetColor {
            object: 0,
            value: [1., 0., 0., 1.],
        })
        .unwrap();
    let before = session.engine.snapshot();
    let revision = session.engine.revision();
    session.editor_renderer = Some(pollster::block_on(Renderer::headless()).unwrap());
    let policy = MemoryPolicy {
        session_allocation_budget_bytes: 256 << 20,
        source_texture_budget_bytes: 32,
        decoded_image_budget_bytes: 192 << 20,
        scratch_budget_bytes: 96 << 20,
        preview_target_budget_bytes: 4 << 20,
    };
    let result = session
        .plugin_request(&serde_json::json!({"op":"memory_policy","policy":policy}).to_string())
        .unwrap();
    assert_eq!(result["policyVersion"], 5);
    assert_eq!(result["memoryPolicy"]["decodedImageBudgetBytes"], 192 << 20);
    let queried = session.plugin_request("{\"op\":\"memory_policy\"}").unwrap();
    assert_eq!(queried, result);
    assert_eq!(
        result["memoryPolicy"],
        serde_json::to_value(policy).unwrap()
    );
    assert_eq!(session.effects.memory_policy(), policy);
    assert_eq!(
        session.editor_renderer.as_ref().unwrap().memory_policy(),
        policy
    );
    let view = session.view_revision;
    assert!(session.plugin_request("{\"op\":\"memory_policy\",\"policy\":{\"sessionAllocationBudgetBytes\":536870912,\"sourceTextureBudgetBytes\":0,\"scratchBudgetBytes\":67108864}}").is_err());
    assert!(session.plugin_request("{\"op\":\"memory_policy\",\"policy\":{\"sessionAllocationBudgetBytes\":536870912,\"sourceTextureBudgetBytes\":32,\"scratchBudgetBytes\":67108864,\"autoVram\":true}}").is_err());
    assert_eq!(session.effects.memory_policy(), policy);
    assert_eq!(
        session.editor_renderer.as_ref().unwrap().memory_policy(),
        policy
    );
    assert_eq!(session.view_revision, view);
    let info = session.preview_info();
    assert_eq!(info["memoryPolicyVersion"], 5);
    assert_eq!(info["memoryPolicy"]["previewTargetBudgetBytes"], 4 << 20);
    assert_eq!(info["sourceTextureBudgetBytes"], 32);
    assert_eq!(info["memoryPolicy"], result["memoryPolicy"]);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.snapshot(), before);
    session.engine.undo().unwrap();
    assert_eq!(session.engine.snapshot(), project);
    session.engine.redo().unwrap();
    assert_eq!(session.engine.snapshot(), before);
}

#[test]
fn resource_ledger_json_query_is_read_only_and_available_without_a_surface() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(Project::new(64, 64, 30, 60).unwrap(), directory.path().to_path_buf(), Arc::new(Stub)).unwrap();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let project = session.engine.snapshot();
    let query = "{\"op\":\"resource_ledger\"}";
    let empty = session.plugin_request(query).unwrap();
    assert_eq!(empty["knownBytes"], 0);
    assert_eq!(empty["graphicsAvailable"], false);
    assert_eq!(empty["editorRendererAvailable"], false);
    assert!(empty["driverResidentBytes"].is_null());
    assert!(empty["gpuRetirementBytes"].is_null());
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    session.editor_renderer = Some(renderer);
    let occupied = session.plugin_request(query).unwrap();
    assert_eq!(occupied["knownBytes"], 1032);
    assert_eq!(occupied["unknownCapacityCount"], 0);
    assert_eq!(occupied["editorRendererAvailable"], true);
    assert_eq!(occupied["resources"].as_array().unwrap().len(), 3);
    assert_eq!(occupied, session.plugin_request(query).unwrap());
    assert_eq!(session.engine.snapshot(), project);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
}

#[test]
fn resource_tracking_is_runtime_only_and_profiling_keeps_it_enabled() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(Project::new(64, 64, 30, 60).unwrap(), directory.path().to_path_buf(), Arc::new(Stub)).unwrap();
    session.editor_renderer = Some(pollster::block_on(Renderer::headless()).unwrap());
    let revision = session.engine.revision();
    let view = session.view_revision;
    let project = session.engine.snapshot();
    let enable = "{\"op\":\"resource_tracking\",\"enabled\":true}";
    let disable = "{\"op\":\"resource_tracking\",\"enabled\":false}";
    session.plugin_request(enable).unwrap();
    assert!(session.plugin_request("{\"op\":\"resource_tracking\",\"enabled\":1}").is_err());
    let query = "{\"op\":\"resource_ledger\"}";
    let info = session.plugin_request(query).unwrap();
    assert_eq!(info["submissionTracking"]["editor"]["enabled"], true);
    assert!(info["gpuRetirementBytes"].is_null());
    let renderer = session.editor_renderer.as_ref().unwrap();
    renderer.submit_tracked(renderer.device.create_command_encoder(&Default::default()).finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    let info = session.plugin_request(query).unwrap();
    assert_eq!(info["submissionTracking"]["editor"]["submitted"], 1);
    assert_eq!(info["submissionTracking"]["editor"]["completed"], 1);
    session.editor_target = Some(session.editor_renderer.as_ref().unwrap().capture_target(5, 7).unwrap());
    assert_eq!(session.plugin_request(query).unwrap()["knownBytes"], 1032+5*7*4);
    session.editor_target = None;
    assert_eq!(session.plugin_request(query).unwrap()["knownBytes"], 1032);
    session.recorder = Some(motion_render::FrameRecorder::new(1, 1));
    session.plugin_request(disable).unwrap();
    assert_eq!(session.plugin_request(query).unwrap()["submissionTracking"]["editor"]["enabled"], true);
    session.recorder = None;
    session.plugin_request(disable).unwrap();
    assert_eq!(session.plugin_request(query).unwrap()["submissionTracking"]["editor"]["enabled"], false);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
    assert_eq!(session.engine.snapshot(), project);
}


#[test]
fn explicit_capacity_query_includes_caller_target_and_preserves_runtime_only_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::new(Project::new(64, 64, 30, 60).unwrap(),
        directory.path().to_path_buf(), Arc::new(Stub)).unwrap();
    let project = session.engine.snapshot();
    let revision = session.engine.revision();
    let view = session.view_revision;
    let query = "{\"op\":\"resource_ledger\"}";
    let empty = session.plugin_request(query).unwrap();
    let observed = &empty["combinedCapacity"]["observation"];
    assert_eq!(observed["activeRenderers"], 0);
    assert_eq!(observed["allActiveRenderersTracked"], false);
    assert_eq!(observed["knownCombinedBytes"], 0);
    session.editor_renderer = Some(pollster::block_on(Renderer::headless()).unwrap());
    let idle = session.plugin_request(query).unwrap();
    let base = idle["knownBytes"].as_u64().unwrap();
    assert!(base > 0);
    session.editor_target = Some(session.editor_renderer.as_ref().unwrap().capture_target(5, 7).unwrap());
    session.plugin_request("{\"op\":\"resource_tracking\",\"enabled\":true}").unwrap();
    let occupied = session.plugin_request(query).unwrap();
    let combined = &occupied["combinedCapacity"];
    let observed = &combined["observation"];
    assert_eq!(observed["knownOwnedBytes"], base + 5 * 7 * 4);
    assert_eq!(observed["knownCombinedBytes"], occupied["knownBytes"]);
    assert_eq!(observed["knownRetiredBytes"], 0);
    assert_eq!(observed["unknownCapacityCount"], 0);
    assert_eq!(observed["activeRenderers"], 1);
    assert_eq!(observed["allActiveRenderersTracked"], true);
    assert_eq!(observed["pendingUntracked"], 0);
    assert_eq!(combined["isFramePeak"], false);
    assert_eq!(combined["isDriverResidency"], false);
    assert_eq!(combined["completeCoverage"], false);
    assert_eq!(combined["atomicAcrossRenderers"], false);
    assert_eq!(occupied, session.plugin_request(query).unwrap());
    session.editor_target = None;
    session.plugin_request("{\"op\":\"resource_tracking\",\"enabled\":false}").unwrap();
    let released = session.plugin_request(query).unwrap();
    assert_eq!(released["combinedCapacity"]["observation"]["knownCombinedBytes"], base);
    assert_eq!(released["combinedCapacity"]["observation"]["allActiveRenderersTracked"], false);
    assert_eq!(session.engine.snapshot(), project);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.view_revision, view);
}
