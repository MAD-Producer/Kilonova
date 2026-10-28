use super::*;
use motion_host::depth_snapshot::DepthSnapshot;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn admitted_static_depth_follows_camera_playback_without_reinference_or_late_installation() {
    let root = tempfile::tempdir().unwrap();
    let original = calibrated_source_session(root.path());
    let mut project = original.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.camera.created = true;
    project.camera.position.set_at(0, [16., 16., -120.]).unwrap();
    project.camera.position.set_animated(0, true).unwrap();
    project.camera.position.set_at(30, [40., 16., -120.]).unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let mut blue = motion_core::Layer::solid(
        2, "Static playback metric oracle", [80.; 2], [16., 16., 0.], [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    drop(original);
    let id = motion_host::session::open(root.path().into(), project, Arc::new(NoMedia)).unwrap();
    struct Close(i64);
    impl Drop for Close {
        fn drop(&mut self) { motion_host::session::close(self.0); }
    }
    let _close = Close(id);
    let (worker, seen, _) = backend(false);
    motion_host::session::with_session(id, |s| {
        s.configure_depth(worker)?;
        s.request_image_depth(7, geometry(), model())?;
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id, |s| {
        wait_done(s, 1);
        s.poll_depth()?;
        let command = calibration_command(s);
        s.apply_command_batch(vec![command])?;
        s.request_image_depth(7, geometry(), model())?;
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let (snapshot, project) = motion_host::session::with_session(id, |s| {
        wait_done(s, 2);
        s.poll_depth()?;
        Ok((DepthSnapshot::capture(s)?, s.engine.project().clone()))
    }).unwrap();
    // The independently verified frozen path is already capable of replaying
    // static geometry at different camera times. Live playback must match it.
    let verified = snapshot.verify_sources(root.path(), &|| Ok(())).unwrap();
    let mut oracle = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    oracle.configure_assets(&project, root.path()).unwrap();
    let target = oracle.sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP).unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    let mut outputs = Vec::new();
    for frame in [0., 1., 2., 3., 15., 30.] {
        scene.sample(&project, frame, None, &motion_core::ExpressionEvaluator).unwrap();
        verified.attach(&mut scene).unwrap();
        assert!(oracle.prepare_scene_assets(&scene, motion_render::image_resources::Resolution::Full, false).unwrap());
        let expected = oracle.capture_sdr(&scene, &target).unwrap().0.pixels;
        if frame == 0. {
            let at = |x: usize, y: usize| &expected[(y*32+x)*4..][..4];
            assert_eq!(at(16,10), &[0,0,255,255], "the reference camera must exercise far occlusion");
            assert!(at(16,22)[0] > 40 && at(16,22)[2] < 10,
                "the reference camera must exercise near calibrated geometry");
        }
        motion_host::session::with_session(id, |s| { s.frame = frame; s.sample() }).unwrap();
        let result = motion_host::ops::export::capture(id)
            .expect("admitted static source reuse must not block advancing playback frames");
        let actual = image::open(result["path"].as_str().unwrap()).unwrap().to_rgba8().into_raw();
        assert_eq!(actual, expected, "live camera frame={frame} must match independently frozen geometry");
        if let Some(path) = std::env::var_os("MOTION_STATIC_DEPTH_PLAYBACK_ARTIFACTS") {
            let path = std::path::PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            let frame = frame as u32;
            image::save_buffer(path.join(format!("live-{frame:03}.png")), &actual, 32,32,image::ColorType::Rgba8).unwrap();
            image::save_buffer(path.join(format!("frozen-{frame:03}.png")), &expected,32,32,image::ColorType::Rgba8).unwrap();
            std::fs::write(path.join(format!("comparison-{frame:03}.json")),
                serde_json::to_vec_pretty(&serde_json::json!({"frame":frame,"width":32,"height":32,"equal":true,"rgbaCompared":actual.len(),"maxAbsoluteDifference":0})).unwrap()).unwrap();
        }
        motion_host::session::with_session(id, |s| {
            let status = s.depth_status()?.unwrap();
            assert_eq!(status.counts.completed, 2, "camera playback must not rerun inference");
            assert_eq!(status.retained_samples, 1);
            assert_eq!(status.retained_image_bytes, 36);
            if frame != 0. {
                assert!(!status.installed, "reusable immutable ownership is distinct from live worker installation");
                assert!(s.poll_depth()?.is_none(), "old worker tickets remain frame-bound");
            }
            Ok(())
        }).unwrap();
        outputs.push(actual);
    }
    assert_ne!(outputs.first(), outputs.last(), "the oracle must exercise actual camera animation");
    motion_host::session::with_session(id, |s| { s.cancel_depth() }).unwrap();
    assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
}
