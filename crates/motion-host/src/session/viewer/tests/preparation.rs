use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native Windows surfaces and GPU")]
fn native_multiview_effects_and_shutter_phases_use_prepared_pipelines_and_preserve_camera_pixels() {
    if !child("session::viewer::tests::preparation::native_multiview_effects_and_shutter_phases_use_prepared_pipelines_and_preserve_camera_pixels",
        "MOTION_VIEW_PREPARATION_CHILD") { return; }
    let events = winit::event_loop::EventLoop::builder().with_any_thread(true).build().unwrap();
    for blur in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut project = fixture(blur);
        let package = motion_effects::builtin::package().unwrap();
        for (layer, effect) in project.layers.iter_mut().filter(|layer| matches!(layer.content, Content::Solid { .. })).zip(["invert", "tint"]) {
            let definition = package.manifest.effects.iter().find(|item| item.id == effect).unwrap();
            layer.effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
                &package.manifest.version, &package.hash, definition, layer.size));
        }
        let mut future = Layer::solid(9, "Future effect", [6.; 2], [16., 16., 0.], [0.2,0.7,0.4,1.]);
        future.three_d = true;
        future.timeline = Some(motion_core::LayerTimeline { in_frame: 30, out_frame: 60, offset_frame: 0 });
        let definition = package.manifest.effects.iter().find(|item| item.id == "gaussian_blur").unwrap();
        future.effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
            &package.manifest.version, &package.hash, definition, future.size));
        project.layers.push(future);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let mut session = Session::new(project.clone(), root.path().into(),
            Arc::new(crate::session::last_error_observation_tests::Stub)).unwrap();
        attach(&mut session, &events);
        let observer = Observer::new(32, 32);
        let cells = views(&observer);
        let foreground = session.graphics.as_ref().unwrap().renderer.effect_pipeline_cache_info().foreground_builds;
        ready(&mut session, 41, 10., &cells);
        assert!(!session.pipeline_preparation_observation().preparing());
        assert_eq!(session.graphics.as_ref().unwrap().renderer.effect_pipeline_cache_info().foreground_builds, foreground,
            "multiview blur={blur} created effect pipelines on its renderer owner");
        let original: Vec<_> = (0..4).map(|index| pixels(session.graphics.as_ref().unwrap(), index)).collect();
        for index in 0..4 {
            let mut cell = cells[index];
            cell.placement.viewport = [0., 0., 32., 32.];
            ready(&mut session, 41, 10., &[cell]);
            assert_eq!(pixels(session.graphics.as_ref().unwrap(), 0), original[index]);
            assert_eq!(session.graphics.as_ref().unwrap().renderer.effect_pipeline_cache_info().foreground_builds, foreground);
        }
        ready(&mut session, 99, 11., &cells);
        assert_eq!(session.graphics.as_ref().unwrap().views.published.as_ref().unwrap().owner, 99);
        assert_eq!(session.graphics.as_ref().unwrap().renderer.effect_pipeline_cache_info().foreground_builds, foreground);
        assert_eq!(session.pipeline_preparation.status()["workerLimit"], 1);
        assert_eq!(session.pipeline_preparation.status()["queueLimit"], 1);
        ready(&mut session, 99, 31., &cells);
        assert_eq!(session.graphics.as_ref().unwrap().renderer.effect_pipeline_cache_info().foreground_builds, foreground,
            "lookahead did not prepare the upcoming multiview effect blur={blur}");
        session.graphics.as_ref().unwrap().renderer.check_health().unwrap();
    }
}
