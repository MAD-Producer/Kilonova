use super::*;
use motion_host::{depth::cache::{Config, Execution}, depth_snapshot::DepthSnapshot};

struct Qualified(Scaled);
impl Backend for Qualified {
    fn execution_identity(&mut self, input: &Input, cancel: &Cancellation)
        -> motion_host::depth::Result<Option<Execution>> {
        cancel.check()?;
        Ok(Some(Execution { backend: "controlled".into(),
            provider: if input.key().model.processing == "settled" { "actual_settled_provider" } else { "controlled_cpu" }.into(),
            runtime_sha256: [1;32], plan_sha256: [2;32], precision: "float32".into() }))
    }
    fn infer(&mut self, input: &Input, cancel: &Cancellation) -> motion_host::depth::Result<Estimate> {
        self.0.infer(input, cancel)
    }
}
fn restore(project: motion_core::Project, root: &Path, cache: &Path)
    -> (Session, mpsc::Receiver<Seen>, DepthSnapshot, motion_model::depth::DepthResourceIdentity) {
    let mut session = Session::new(project, root.into(), Arc::new(NoMedia)).unwrap();
    let (echo, seen, _) = backend(false);
    session.configure_depth_cached_in(Qualified(Scaled { echo, blocked: None, fail: false }), cache.into(),
        Config { directory: cache.join("persistent"), byte_limit: 1 << 20, entry_limit: 8 }).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    let identity = session.poll_depth().unwrap().unwrap().resource.cache_identity();
    assert!(session.bound_depth_calibration(1).unwrap().is_some());
    let raw = DepthSnapshot::capture(&session).unwrap();
    session.begin_image_depth_transition(settled(), policy(), limits()).unwrap();
    request_settled(&mut session);
    wait_done(&mut session, 3);
    assert!(session.poll_image_depth_transition().unwrap());
    assert!(session.bound_depth_calibration(1).unwrap().is_some());
    (session, seen, raw, identity)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn persistent_reopened_session_refits_fresh_instances_and_matches_frozen_metric_pixels() {
    let root = tempfile::tempdir().unwrap(); let cache = tempfile::tempdir().unwrap();
    let mut authored = calibrated_source_session(root.path());
    let (echo, seed_seen, _) = backend(false);
    authored.configure_depth(echo).unwrap();
    authored.request_image_depth(7, geometry(), model()).unwrap(); wait_done(&mut authored, 1);
    authored.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut authored); authored.apply_command_batch(vec![command]).unwrap();
    assert_eq!(seed_seen.try_iter().count(), 1);
    let mut project = authored.engine.snapshot(); drop(authored);
    project.layers[0].three_d = true; project.layers[0].size = [80.;2];
    project.camera.created = true;
    project.camera.position.set_at(0,[16.,16.,-120.]).unwrap();
    project.camera.position.set_animated(0,true).unwrap();
    project.camera.position.set_at(30,[40.,16.,-120.]).unwrap();
    project.camera.target.set_at(0,[16.,16.,0.]).unwrap();
    project.camera.fov.set_at(0,60.).unwrap();
    let mut blue = motion_core::Layer::solid(2,"Cached metric oracle",[80.;2],[16.,16.,0.],[0.,0.,1.,1.]);
    blue.three_d = true; project.layers.push(blue);
    let (mut first, seen, raw, original_identity) = restore(project.clone(),root.path(),cache.path());
    assert_eq!(seen.try_iter().count(), 2, "raw and settled infer; interactive reference reuses cache");
    assert_eq!(first.depth_status().unwrap().unwrap().counts.cache_hits, 1);
    let before = DepthSnapshot::capture(&first).unwrap(); drop(first);
    let (mut second, seen, reopened_raw, restored_identity) = restore(project.clone(),root.path(),cache.path());
    assert_ne!(original_identity,restored_identity);
    assert_eq!(seen.try_iter().count(), 0, "reopening must not invoke either inference route");
    assert_eq!(second.depth_status().unwrap().unwrap().counts.cache_hits, 3);
    let snapshot = DepthSnapshot::capture(&second).unwrap();
    let mut wire = Vec::new(); snapshot.write_to(&mut wire,&||Ok(())).unwrap();
    drop(second);
    let snapshot = DepthSnapshot::read_from(&mut &wire[..],&||Ok(())).unwrap();
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    renderer.configure_assets(&project,root.path()).unwrap();
    let target = renderer.sdr_capture_target(32,32,motion_render::OutputTransform::SDR_CLIP).unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    let mut frames = Vec::new();
    for frame in [0.,15.,30.] {
        let mut outputs = Vec::new();
        for frozen in [&raw,&reopened_raw,&before,&snapshot] {
            scene.sample(&project,frame,None,&motion_core::ExpressionEvaluator).unwrap();
            frozen.verify_sources(root.path(),&||Ok(())).unwrap().attach(&mut scene).unwrap();
            assert!(renderer.prepare_scene_assets(&scene,motion_render::image_resources::Resolution::Full,false).unwrap());
            outputs.push(renderer.capture_sdr(&scene,&target).unwrap().0.pixels);
        }
        assert!(outputs.windows(2).all(|pair|pair[0]==pair[1]),"cached refit must preserve metric pixels at {frame}");
        if frame == 0. { assert_eq!(&outputs[0][(10*32+16)*4..][..4], &[0,0,255,255]); }
        frames.push(outputs.remove(0));
    }
    assert_ne!(frames.first(),frames.last(),"camera animation must change visible output");
}
