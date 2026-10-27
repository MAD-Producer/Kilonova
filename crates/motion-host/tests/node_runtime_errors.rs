use motion_core::{EffectInstance, Engine, Layer, Project};
use motion_host::{ops, Error, Platform, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

struct NoMedia;
fn directory(root: &Path, name: &str) -> std::path::PathBuf {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}
impl Platform for NoMedia {
    fn name(&self) -> &'static str { "node-runtime-errors" }
    fn probe_video(&self, _: &Path, _: Option<u32>, _: Option<u32>, _: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("node runtime restoration must not probe video")
    }
    fn open_decoder(&self, _: &Path, _: motion_core::VideoAsset, _: Vec<u64>) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("node runtime restoration must not open a decoder")
    }
    fn decode_audio(&self, _: &Path, _: &Path, _: Option<u32>, _: u64, _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("node runtime restoration must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> { Ok(json!({})) }
    fn attach_surface(&self, _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>, _: u32, _: u32, _: motion_render::AllocationBudget) -> motion_host::Result<SurfaceTarget> {
        panic!("node runtime restoration must not create a surface")
    }
}

fn project(root: &Path, already_registered: bool) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(1, "Runtime", [32.; 2], [32., 32., 0.], [1.; 4]);
    let (plugin, version, hash) = if already_registered {
        (package.manifest.id.clone(), package.manifest.version.clone(), package.hash.clone())
    } else {
        ("test.native-runtime-error".into(), "1.0.0".into(), "0".repeat(64))
    };
    let mut effect = EffectInstance::new(1, &plugin, &version, &hash, &package.manifest.effects[0], [64.; 2]);
    let bytes = [0, 0];
    let sha256 = motion_core::storage::resource_sha256(&bytes);
    let resource = motion_core::NodeResource {path:format!("assets/node-{sha256}.msfx"),bytes:2,sha256};
    let mut stage = motion_core::storage::NodeResourceStage::new(root).unwrap();
    stage.stage(&resource, &bytes).unwrap();
    stage.keep_resources();
    effect.node_graph = Some(json!({"format_version":2,"key":"runtime-errors","document":{},"options":{},"runtime":resource}));
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

fn zip_cause(error: &Error) -> &motion_effects::Error {
    let Error::Context { context, cause } = error else { panic!("runtime error lost its context: {error:?}") };
    assert_eq!(context, "invalid compiled node runtime");
    let Error::Effects(cause) = cause.as_ref() else { panic!("runtime error lost its native effect cause: {error:?}") };
    assert!(matches!(cause, motion_effects::Error::Zip(_)), "not the original archive failure: {cause:?}");
    cause
}

#[test]
fn session_creation_preserves_real_owned_archive_failure() {
    let root = tempfile::tempdir().unwrap();
    let directory = directory(root.path(), "project");
    let candidate = project(&directory, false);
    let before = serde_json::to_value(&candidate).unwrap();
    let error = match Session::new(candidate.clone(), directory, Arc::new(NoMedia)) {
        Ok(_) => panic!("invalid owned archive unexpectedly opened"),
        Err(error) => error,
    };
    let cause = zip_cause(&error);
    let direct = match motion_effects::EffectPackage::from_bytes(vec![0, 0]) {
        Ok(_) => panic!("invalid archive unexpectedly parsed"),
        Err(error) => error,
    };
    assert_eq!(cause.to_string(), direct.to_string());
    assert_eq!(error.to_string(), format!("invalid compiled node runtime: {direct}"));
    let source = std::error::Error::source(&error).unwrap();
    let boxed = source.downcast_ref::<Box<Error>>().expect("host context must retain its declared source wrapper");
    assert!(matches!(boxed.as_ref(), Error::Effects(motion_effects::Error::Zip(_))));
    let reply = ops::typed_error_envelope(&error);
    assert_eq!(reply["error"], error.to_string());
    assert!(reply["error_detail"].is_null());
    assert_eq!(serde_json::to_value(candidate).unwrap(), before);
}

#[test]
fn rejected_runtime_replacement_preserves_live_session_then_explicit_replacement_recovers() {
    let root = tempfile::tempdir().unwrap();
    let original = Project::new(64, 64, 30, 120).unwrap();
    let initial_root = directory(root.path(), "initial");
    let mut session = Session::new(original.clone(), initial_root.clone(), Arc::new(NoMedia)).unwrap();
    let before = session.engine.snapshot();
    let history = session.engine.history_diagnostics();
    let registry = session.effects.registry.clone();
    let rejected = directory(root.path(), "rejected");
    let error = session.replace_project(Engine::new(project(&rejected, false)).unwrap(), rejected).unwrap_err();
    zip_cause(&error);
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.engine.revision(), 0);
    assert_eq!(session.engine.history_diagnostics(), history);
    assert_eq!(session.root, initial_root);
    assert_eq!(session.effects.registry.disabled, registry.disabled);
    assert_eq!(session.effects.registry.diagnostics, registry.diagnostics);
    assert_eq!(session.effects.registry.packages.len(), registry.packages.len());
    for (key, package) in &registry.packages {
        assert!(Arc::ptr_eq(session.effects.registry.packages.get(key).unwrap(), package));
    }
    assert!(session.last_error.is_none());
    let next = Project::new(128, 64, 30, 60).unwrap();
    let next_root = directory(root.path(), "recovered");
    session.replace_project(Engine::new(next.clone()).unwrap(), next_root.clone()).unwrap();
    assert_eq!(session.engine.project(), &next);
    assert_eq!(session.root, next_root);
    assert_eq!(session.engine.revision(), 0);
    assert!(session.last_error.is_none());
}

#[test]
fn invalid_reference_and_already_registered_runtime_keep_their_defined_behavior() {
    let root = tempfile::tempdir().unwrap();
    let invalid_root = directory(root.path(), "invalid");
    let mut invalid = project(&invalid_root, false);
    invalid.layers[0].effects[0].node_graph.as_mut().unwrap()["runtime"]["path"] = "../foreign.msfx".into();
    let error = match Session::new(invalid, invalid_root, Arc::new(NoMedia)) {
        Ok(_) => panic!("invalid runtime reference unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(error, Error::Model(motion_core::Error::Invalid(ref message)) if message == "invalid project-owned node resource"));
    let cached_root = directory(root.path(), "cached");
    let cached = project(&cached_root, true);
    for resource in cached.node_resources().unwrap() { std::fs::remove_file(cached_root.join(resource.path)).unwrap(); }
    let session = Session::new(cached.clone(), cached_root, Arc::new(NoMedia)).unwrap();
    assert_eq!(session.engine.project(), &cached);
    assert!(session.last_error.is_none());
}
