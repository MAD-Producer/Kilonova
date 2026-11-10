//! Analytic physical depth exercises real worker/Session/export ownership.
//! It is not learned-model accuracy or automatic profile qualification.
use super::*;
use motion_host::depth_snapshot::DepthSnapshot;
use motion_model::depth::DepthAxis;

struct Physical {
    inner: Echo,
    inverse: bool,
}
impl Backend for Physical {
    fn infer(
        &mut self,
        input: &Input,
        canceled: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        let original = self.inner.infer(input, canceled)?;
        let mut descriptor = original.resource.descriptor().clone();
        descriptor.measure = if self.inverse {
            DepthMeasure::InverseMetric {
                inverse_meters_per_unit: 0.25,
                axis: DepthAxis::CameraZ,
            }
        } else {
            DepthMeasure::Metric {
                meters_per_unit: 0.25,
                axis: DepthAxis::CameraZ,
            }
        };
        let values = (0..9)
            .map(|i| {
                let z = 8. - (original.resource.sample(i).unwrap() - 10.) * 0.05;
                if self.inverse {
                    (4. / z) as f32
                } else {
                    (z * 4.) as f32
                }
            })
            .collect();
        Ok(Estimate {
            resource: DepthResource::new(descriptor, values, None, None)?,
            provider: "authored-physical-control".into(),
            costs: Default::default(),
        })
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn physical_image_session_and_frozen_export_preserve_units_ownership_and_real_occlusion() {
    for inverse in [false, true] {
        let original = tempfile::tempdir().unwrap();
        let copied = tempfile::tempdir().unwrap();
        let initial = calibrated_source_session(original.path());
        let mut project = initial.engine.project().clone();
        project.layers[0].three_d = true;
        project.layers[0].size = [80.; 2];
        project.camera.created = true;
        project
            .camera
            .position
            .set_at(0, [16., 16., -120.])
            .unwrap();
        project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
        project.camera.fov.set_at(0, 60.).unwrap();
        let mut blue = motion_core::Layer::solid(
            2,
            "Physical occlusion oracle",
            [80.; 2],
            [16., 16., 0.],
            [0., 0., 1., 1.],
        );
        blue.three_d = true;
        project.layers.push(blue);
        drop(initial);
        let id =
            motion_host::session::open(original.path().into(), project, Arc::new(NoMedia)).unwrap();
        struct Close(i64);
        impl Drop for Close {
            fn drop(&mut self) {
                motion_host::session::close(self.0);
            }
        }
        let close = Close(id);
        let (worker, seen, _) = backend(false);
        motion_host::session::with_session(id, |s| {
            s.configure_depth_in(
                Physical {
                    inner: worker,
                    inverse,
                },
                original.path().join("cache"),
            )?;
            s.request_image_depth(7, geometry(), model())?;
            Ok(())
        })
        .unwrap();
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        motion_host::session::with_session(id, |s| {
            wait_done(s, 1);
            s.poll_depth()?;
            s.observing = true;
            let command = s.physical_depth_placement_command(1, 20.)?;
            let Command::DepthCalibration {
                calibration: Some(saved),
                ..
            } = &command
            else {
                panic!("placement required")
            };
            assert!(saved.is_physical());
            assert_eq!(
                saved.reference().unwrap().camera.eye,
                glam::Vec3::new(0., 0., 120.)
            );
            assert_ne!(saved.reference().unwrap().camera.eye, s.scene.camera.eye);
            let before = s.engine.project().clone();
            s.apply_command_batch(vec![command])?;
            let authored = s.engine.project().clone();
            assert!(s.engine.undo()?);
            s.sample()?;
            assert_eq!(s.engine.project(), &before);
            assert!(s.engine.redo()?);
            s.sample()?;
            assert_eq!(s.engine.project(), &authored);
            s.observing = false;
            Ok(())
        })
        .unwrap();
        assert!(motion_host::ops::export::capture(id)
            .unwrap_err()
            .to_string()
            .contains("pending"));
        assert!(!original.path().join("exports/frame-000000.png").exists());
        motion_host::session::with_session(id, |s| {
            s.request_image_depth(7, geometry(), model())?;
            Ok(())
        })
        .unwrap();
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        let (snapshot, project, weak) = motion_host::session::with_session(id, |s| {
            wait_done(s, 2);
            let estimate = s.poll_depth()?.unwrap();
            let input = s.depth_input_fingerprint()?.unwrap();
            let saved = s.engine.project().layers[0]
                .depth_calibration
                .as_ref()
                .unwrap();
            saved.validate_binding(&estimate.resource, input)?;
            // The relative-only API remains a refusal, not fabricated anchors.
            assert!(saved.rebind(&estimate.resource, input).is_err());
            Ok((
                DepthSnapshot::capture(s)?,
                s.engine.project().clone(),
                Arc::downgrade(&estimate),
            ))
        })
        .unwrap();
        let capture = motion_host::ops::export::capture(id).unwrap();
        let expected = image::open(capture["path"].as_str().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(expected.get_pixel(16, 10).0, [0, 0, 255, 255]);
        let near = expected.get_pixel(16, 22).0;
        assert!(
            near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255,
            "{near:?}"
        );
        let mut wire = Vec::new();
        snapshot.write_to(&mut wire, &|| Ok(())).unwrap();
        let restored = DepthSnapshot::read_from(&mut &wire[..], &|| Ok(())).unwrap();
        restored.validate_project(&project).unwrap();
        std::fs::create_dir_all(copied.path().join("assets")).unwrap();
        std::fs::copy(
            original.path().join("assets/input.png"),
            copied.path().join("assets/input.png"),
        )
        .unwrap();
        drop(close);
        original.close().unwrap();
        assert!(weak.upgrade().is_some());
        let mut scene = motion_render::Scene::new(&project).unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        renderer.configure_assets(&project, copied.path()).unwrap();
        let target = renderer
            .sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        let verified = restored.verify_sources(copied.path(), &|| Ok(())).unwrap();
        for frame in [0., 30.] {
            scene
                .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            verified.attach(&mut scene).unwrap();
            assert!(renderer
                .prepare_scene_assets(
                    &scene,
                    motion_render::image_resources::Resolution::Full,
                    false
                )
                .unwrap());
            assert_eq!(
                renderer.capture_sdr(&scene, &target).unwrap().0.pixels,
                expected.as_raw().as_slice()
            );
        }
        let cause = verified
            .attach_checked(&mut scene, &|| {
                Err("primary physical export cancellation".into())
            })
            .unwrap_err();
        assert_eq!(cause.to_string(), "primary physical export cancellation");
        drop(verified);
        image::RgbaImage::from_pixel(3, 3, image::Rgba([201, 2, 3, 255]))
            .save(copied.path().join("assets/input.png"))
            .unwrap();
        assert!(restored
            .verify_sources(copied.path(), &|| Ok(()))
            .err()
            .unwrap()
            .to_string()
            .contains("fingerprint changed"));
        drop(snapshot);
        assert!(weak.upgrade().is_none());
    }
}

#[path = "physical/transition.rs"]
mod transition;
