use motion_core::{Command, Project};
use motion_host::{Platform, Result, Session};
use motion_render::{resource_policy::MemoryPolicy, Renderer};
use std::{path::Path, sync::Arc};

struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "memory-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> Result<Box<dyn motion_host::VideoDecoder>> {
        Err("unused".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(f64) -> Result<()>,
    ) -> Result<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&motion_host::VideoQuery>) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::WindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> Result<motion_host::SurfaceTarget> {
        Err("unused".into())
    }
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
        source_texture_budget_bytes: 32,
        scratch_budget_bytes: 96 << 20,
    };
    let result = session
        .plugin_request(&serde_json::json!({"op":"memory_policy","policy":policy}).to_string())
        .unwrap();
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
    assert!(session.plugin_request("{\"op\":\"memory_policy\",\"policy\":{\"sourceTextureBudgetBytes\":0,\"scratchBudgetBytes\":67108864}}").is_err());
    assert!(session.plugin_request("{\"op\":\"memory_policy\",\"policy\":{\"sourceTextureBudgetBytes\":32,\"scratchBudgetBytes\":67108864,\"autoVram\":true}}").is_err());
    assert_eq!(session.effects.memory_policy(), policy);
    assert_eq!(
        session.editor_renderer.as_ref().unwrap().memory_policy(),
        policy
    );
    assert_eq!(session.view_revision, view);
    let info = session.preview_info();
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
    renderer.device.poll(wgpu::Maintain::Wait);
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
