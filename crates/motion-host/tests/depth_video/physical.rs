//! Authored physical samples exercise video ownership, not learned accuracy.
use super::*;
use motion_host::depth_snapshot::DepthSnapshot;
use motion_model::depth::DepthAxis;

struct Physical {
    echo: Echo,
    inverse: bool,
}
impl Backend for Physical {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        let original = self.echo.infer(input, cancel)?;
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
        let values = (0..6)
            .map(|i| {
                let z = 8. - 4. * i as f32 / 5.;
                if self.inverse {
                    4. / z
                } else {
                    4. * z
                }
            })
            .collect();
        Ok(Estimate {
            resource: DepthResource::new(descriptor, values, None, None)?,
            provider: "authored-physical-video-control".into(),
            costs: Default::default(),
        })
    }
}

fn physical_fixture(inverse: bool, storage: &Path) -> Fixture {
    let mut f = fixture(Mode::Rgba);
    f.session = Session::new(
        f.session.engine.snapshot(),
        f.dir.path().into(),
        f.platform.clone(),
    )
    .unwrap();
    let (sender, seen) = mpsc::channel();
    f.seen = seen;
    f.session
        .configure_depth_in(
            Physical {
                echo: Echo {
                    seen: sender,
                    release: None,
                },
                inverse,
            },
            storage.into(),
        )
        .unwrap();
    f
}

#[test]
fn physical_video_authoring_requires_current_physical_samples_and_keeps_decoder_intervals() {
    let mut relative = fixture(Mode::Rgba);
    request(&mut relative.session).unwrap();
    done(&mut relative.session, 1);
    relative.session.poll_depth().unwrap().unwrap();
    assert!(relative
        .session
        .physical_video_depth_placement_command(1, 20.)
        .is_err());
    assert!(relative.session.engine.project().layers[0]
        .video_depth_calibration
        .is_none());

    for inverse in [false, true] {
        let storage = tempfile::tempdir().unwrap();
        let mut f = physical_fixture(inverse, storage.path());
        assert!(f
            .session
            .physical_video_depth_placement_command(1, 20.)
            .is_err());
        for (count, frame, start, end) in [
            (1, 0., 10_000, 40_000),
            (2, 1., 40_000, 110_000),
            (3, 3., 110_000, 150_000),
        ] {
            f.session.frame = frame;
            request(&mut f.session).unwrap();
            done(&mut f.session, count);
            let estimate = f.session.poll_depth().unwrap().unwrap();
            let input = f.session.depth_input_fingerprint().unwrap().unwrap();
            assert_eq!(
                f.seen
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .key
                    .source
                    .pts
                    .ticks,
                start
            );
            assert!(f
                .session
                .physical_video_depth_placement_command(99, 20.)
                .is_err());
            let before = f.session.engine.snapshot();
            let command = f
                .session
                .physical_video_depth_placement_command(1, 20.)
                .unwrap();
            let motion_core::Command::VideoDepthCalibration { object, sample } = &command else {
                panic!("indexed physical placement required")
            };
            assert_eq!(*object, 1);
            assert_eq!((sample.start_us(), sample.end_us()), (start as u64, end));
            assert!(sample.calibration().is_physical());
            sample
                .calibration()
                .validate_binding(&estimate.resource, input)
                .unwrap();
            assert!(sample
                .calibration()
                .rebind(&estimate.resource, input)
                .is_err());
            assert_eq!(
                f.session.engine.project(),
                &before,
                "preparing is not committing"
            );
            f.session.apply_command_batch(vec![command]).unwrap();
            let after = f.session.engine.snapshot();
            assert!(f.session.engine.undo().unwrap());
            assert_eq!(f.session.engine.project(), &before);
            assert!(f.session.engine.redo().unwrap());
            assert_eq!(f.session.engine.project(), &after);
        }
        let track = f.session.engine.project().layers[0]
            .video_depth_calibration
            .as_ref()
            .unwrap();
        assert_eq!(track.samples().len(), 3);
        assert_eq!(track.at(39_999).unwrap().start_us(), 10_000);
        assert_eq!(track.at(40_000).unwrap().start_us(), 40_000);
        assert_eq!(track.at(110_000).unwrap().start_us(), 110_000);
        assert!(track.at(150_000).is_none());
        f.session.frame = 0.;
        assert!(
            f.session
                .physical_video_depth_placement_command(1, 20.)
                .is_err(),
            "a different source interval cannot reuse the last installed result"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn physical_video_frozen_render_preserves_pts_units_colors_and_occlusion_after_session_closure() {
    for inverse in [false, true] {
        // Frozen anonymous scalar planes outlive Session and the source root.
        // Their scratch directory is a distinct owned resource, not source IO.
        let storage = tempfile::tempdir().unwrap();
        let mut f = physical_fixture(inverse, storage.path());
        let mut project = f.session.engine.snapshot();
        project.layers[0].three_d = true;
        project.layers[0].size = [80.; 2];
        project.layers[0]
            .transform
            .position
            .set_at(0, [16., 16., 0.])
            .unwrap();
        project.camera.created = true;
        project
            .camera
            .position
            .set_at(0, [16., 16., -120.])
            .unwrap();
        project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
        project.camera.fov.set_at(0, 60.).unwrap();
        let mut plane = Layer::solid(
            2,
            "Physical video occlusion oracle",
            [80.; 2],
            [16., 16., 0.],
            [0., 0., 1., 1.],
        );
        plane.three_d = true;
        project.layers.push(plane);
        f.session
            .replace_project(
                motion_core::Engine::new(project).unwrap(),
                f.dir.path().into(),
            )
            .unwrap();
        for (count, frame) in [(1, 0.), (2, 1.), (3, 3.)] {
            f.session.frame = frame;
            request(&mut f.session).unwrap();
            done(&mut f.session, count);
            f.session.poll_depth().unwrap().unwrap();
            let command = f
                .session
                .physical_video_depth_placement_command(1, 20.)
                .unwrap();
            f.session.apply_command_batch(vec![command]).unwrap();
        }
        assert!(DepthSnapshot::capture(&mut f.session)
            .unwrap_err()
            .to_string()
            .contains("indexed video depth export"));
        for (count, frame) in [(4, 0.), (5, 1.), (6, 3.)] {
            f.session.frame = frame;
            request(&mut f.session).unwrap();
            done(&mut f.session, count);
            f.session.poll_depth().unwrap().unwrap();
        }
        let snapshot = DepthSnapshot::capture(&mut f.session).unwrap();
        assert_eq!(snapshot.layer_count(), 1);
        let project = f.session.engine.snapshot();
        let mut wire = Vec::new();
        snapshot.write_to(&mut wire, &|| Ok(())).unwrap();
        let restored = DepthSnapshot::read_from(&mut &wire[..], &|| Ok(())).unwrap();
        restored.validate_project(&project).unwrap();
        assert_eq!(f.seen.try_iter().count(), 6);
        let copied = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(copied.path().join("assets")).unwrap();
        std::fs::copy(
            f.dir.path().join("assets/source.mp4"),
            copied.path().join("assets/source.mp4"),
        )
        .unwrap();
        let asset = &project.video_assets[0];
        let index = motion_media::video_cache_path(copied.path(), asset).unwrap();
        std::fs::create_dir_all(index.parent().unwrap()).unwrap();
        motion_media::save_index(&index, asset, &[10_000, 40_000, 110_000]).unwrap();
        let Fixture {
            dir,
            session,
            platform,
            ..
        } = f;
        drop(session);
        dir.close().unwrap();
        let verified = restored
            .verify_sources_with_platform(copied.path(), platform.clone(), &|| Ok(()))
            .unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        renderer.configure_assets(&project, copied.path()).unwrap();
        let target = renderer
            .sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        let mut outputs = Vec::new();
        for (frame, low) in [(0., 10), (0.5, 10), (1., 40), (2., 40), (3., 110)] {
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
            let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
            let image = image::RgbaImage::from_raw(32, 32, pixels).unwrap();
            assert_eq!(image.get_pixel(16, 10).0, [0, 0, 255, 255]);
            let near = image.get_pixel(16, 22).0;
            assert!(
                near[0] >= low
                    && near[0] <= low + 5
                    && near[1] <= 3
                    && near[2] <= 4
                    && near[3] == 255,
                "inverse={inverse} frame={frame} physical video color/geometry differs: {near:?}"
            );
            outputs.push(image);
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(outputs[2], outputs[3]);
        assert_ne!(outputs[0], outputs[2]);
        assert_ne!(outputs[2], outputs[4]);
        let cause = verified
            .attach_checked(&mut scene, &|| {
                Err("primary physical video cancellation".into())
            })
            .unwrap_err();
        assert_eq!(cause.to_string(), "primary physical video cancellation");
        let (events, _received) = mpsc::channel();
        let wrong_pts = Arc::new(VideoPlatform {
            mode: Mode::WrongPts,
            events,
            release: Mutex::new(None),
            opens: AtomicUsize::new(0),
        });
        assert!(
            restored
                .verify_sources_with_platform(copied.path(), wrong_pts, &|| Ok(()))
                .is_err(),
            "physical units do not waive original decoded source-time verification"
        );
    }
}
