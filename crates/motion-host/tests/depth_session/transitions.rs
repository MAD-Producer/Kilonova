use super::*;
use motion_host::depth::{
    alignment::Limits,
    transition::{Activity, Policy},
};
#[path = "transitions/frozen.rs"]
mod frozen;
#[path = "transitions/cached.rs"]
mod cached;

fn policy() -> Policy {
    Policy {
        idle_delay: Duration::from_millis(100),
        minimum_interactive_residence: Duration::from_millis(300),
    }
}
fn limits() -> Limits {
    Limits {
        minimum_fit_pairs: 3,
        minimum_validation_pairs: 3,
        minimum_pair_fraction: 1.,
        minimum_confidence: None,
        minimum_reference_stddev: 1e-8,
        scale_bounds: [0.1, 10.],
        maximum_normalized_rms: 1e-8,
        maximum_normalized_error: 1e-8,
    }
}
fn settled() -> DepthModel {
    let mut value = model();
    value.processing = "settled".into();
    value
}
struct Scaled {
    echo: Echo,
    blocked: Option<mpsc::Receiver<()>>,
    fail: bool,
}
impl Backend for Scaled {
    fn infer(
        &mut self,
        input: &Input,
        canceled: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        let original = self.echo.infer(input, canceled)?;
        if input.key().model.processing != "settled" {
            return Ok(original);
        }
        if let Some(blocked) = self.blocked.take() {
            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        if self.fail {
            return Err(motion_host::depth::Error::Backend(
                "original managed transition failure".into(),
            ));
        }
        Ok(Estimate {
            resource: DepthResource::new(
                original.resource.descriptor().clone(),
                original
                    .resource
                    .values()
                    .iter()
                    .map(|v| (*v - 3.) / 2.)
                    .collect(),
                None,
                None,
            )?,
            provider: "actual_settled_provider".into(),
            costs: Costs {
                inference: Duration::from_millis(7),
                ..Costs::default()
            },
        })
    }
}
fn configure(
    s: &mut Session,
    fail: bool,
    blocked: Option<mpsc::Receiver<()>>,
) -> mpsc::Receiver<Seen> {
    let (echo, seen, _) = backend(false);
    s.configure_depth(Scaled {
        echo,
        blocked,
        fail,
    })
    .unwrap();
    s.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(s, 1);
    assert!(s.poll_depth().unwrap().is_some());
    s.begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    seen
}
fn request_settled(s: &mut Session) {
    assert!(s
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .is_some());
    wait_done(s, 2);
    assert!(s.poll_image_depth_transition().unwrap());
    assert!(s
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap()
        .is_none());
    assert!(s
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .is_some());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn session_transition_capture_preserves_occlusion_and_owned_pixels_after_source_replacement() {
    let root = tempfile::tempdir().unwrap();
    let original = calibrated_source_session(root.path());
    let mut project = original.engine.project().clone();
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
        "Transition occlusion oracle",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    drop(original);
    let id = motion_host::session::open(root.path().into(), project, Arc::new(NoMedia)).unwrap();
    struct Close(i64);
    impl Drop for Close {
        fn drop(&mut self) {
            motion_host::session::close(self.0);
        }
    }
    let _close = Close(id);
    let (echo, seen, _) = backend(false);
    motion_host::session::with_session(id, |s| {
        s.configure_depth(Scaled {
            echo,
            blocked: None,
            fail: false,
        })?;
        s.request_image_depth(7, geometry(), model())?;
        wait_done(s, 1);
        s.poll_depth()?;
        let command = calibration_command(s);
        s.apply_command_batch(vec![command])?;
        s.request_image_depth(7, geometry(), model())?;
        wait_done(s, 2);
        s.poll_depth()?;
        Ok(())
    })
    .unwrap();
    let capture = || {
        let result = motion_host::ops::export::capture(id).unwrap();
        image::open(result["path"].as_str().unwrap())
            .unwrap()
            .to_rgba8()
    };
    let expected = capture();
    assert_eq!(expected.get_pixel(16, 10).0, [0, 0, 255, 255]);
    let near = expected.get_pixel(16, 22).0;
    assert!(near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255);
    motion_host::session::with_session(id, |s| {
        s.begin_image_depth_transition(settled(), policy(), limits())?;
        s.request_image_depth_transition(Activity::Interactive, Duration::ZERO)?;
        wait_done(s, 3);
        assert!(s.poll_image_depth_transition()?);
        s.request_image_depth_transition(Activity::Idle, Duration::from_millis(100))?;
        s.request_image_depth_transition(Activity::Idle, Duration::from_millis(400))?;
        wait_done(s, 4);
        assert!(s.poll_image_depth_transition()?);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        capture(),
        expected,
        "actual Session rendering must use the qualified candidate calibration"
    );
    image::RgbaImage::from_pixel(3, 3, image::Rgba([0, 255, 0, 255]))
        .save(root.path().join("assets/input.png"))
        .unwrap();
    assert_eq!(
        capture(),
        expected,
        "fresh GPU caches must use the candidate worker's original paired pixels"
    );
    std::fs::remove_file(root.path().join("assets/input.png")).unwrap();
    assert_eq!(
        capture(),
        expected,
        "a retained transition must not reopen its source photograph"
    );
    motion_host::session::with_session(id, |s| {
        assert_eq!(s.depth_status()?.unwrap().counts.completed, 4);
        assert_eq!(s.depth_status()?.unwrap().counts.submitted, 4);
        assert_eq!(
            s.image_depth_transition()?.unwrap().descriptor().model,
            settled()
        );
        s.cancel_depth()?;
        Ok(())
    })
    .unwrap();
    assert!(motion_host::ops::export::capture(id)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    assert_eq!(
        image::open(root.path().join("exports/frame-000000.png"))
            .unwrap()
            .to_rgba8(),
        expected
    );
    assert_eq!(
        seen.try_iter().count(),
        4,
        "rendering and GPU eviction never trigger inference"
    );
}

#[test]
fn session_transition_decodes_managed_image_and_samples_affine_result_without_relabeling() {
    let root = tempfile::tempdir().unwrap();
    let mut s = session(root.path());
    let seen = configure(&mut s, false, None);
    let before = s.depth_status().unwrap().unwrap();
    request_settled(&mut s);
    wait_done(&mut s, 3);
    {
        let view = s.image_depth_transition().unwrap().unwrap();
        for i in 0..9 {
            assert!((view.sample(i).unwrap().unwrap() - (i + 1) as f64 * 10.).abs() < 1e-8);
        }
        assert_eq!(view.descriptor().model, settled());
        assert_eq!(view.provider(), "actual_settled_provider");
        assert_eq!(view.costs().inference, Duration::from_millis(7));
    }
    assert!(s
        .poll_depth()
        .unwrap_err()
        .to_string()
        .contains("guarded sampling"));
    assert!(motion_host::depth_snapshot::DepthSnapshot::capture(&s)
        .unwrap_err()
        .to_string()
        .contains("explicit qualified export binding"));
    let after = s.depth_status().unwrap().unwrap();
    assert!(after.installed);
    assert_eq!(after.retained_samples, before.retained_samples + 1);
    assert_eq!(after.retained_bytes, before.retained_bytes + 9 * 4);
    assert_eq!(after.retained_image_bytes, before.retained_image_bytes * 2);
    let calls: Vec<_> = (0..3)
        .map(|_| seen.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert!(calls
        .iter()
        .all(|call| call.thread == calls[0].thread && call.rgb == calls[0].rgb));
    assert_ne!(calls[0].thread, std::thread::current().id());
    assert_eq!(calls[2].key.model, settled());
    assert!(s
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(500))
        .unwrap()
        .is_none());
    assert_eq!(s.depth_status().unwrap().unwrap().counts.submitted, 3);
}

#[test]
fn session_transition_refuses_changed_file_pixels_and_recovers_only_after_explicit_raw_request() {
    let root = tempfile::tempdir().unwrap();
    let mut s = session(root.path());
    let _seen = configure(&mut s, false, None);
    request_settled(&mut s);
    // A completed but not yet admitted sample still owns its actual old pixels.
    wait_done(&mut s, 3);
    assert!(s.poll_image_depth_transition().unwrap());
    image::RgbaImage::from_pixel(3, 3, image::Rgba([99, 2, 3, 255]))
        .save(root.path().join("assets/input.png"))
        .unwrap();
    s.request_image_depth_transition(Activity::Interactive, Duration::from_millis(600))
        .unwrap()
        .unwrap();
    wait_done(&mut s, 4);
    assert!(s
        .poll_image_depth_transition()
        .unwrap_err()
        .to_string()
        .contains("pixels differ"));
    assert!(s.image_depth_transition().unwrap().is_none());
    assert!(!s.depth_status().unwrap().unwrap().installed);
    assert_eq!(s.depth_status().unwrap().unwrap().counts.submitted, 4);
    s.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut s, 5);
    assert_eq!(
        s.poll_depth().unwrap().unwrap().resource.sample(0),
        Some(99.)
    );
    assert!(s.image_depth_transition().unwrap().is_none());
}

#[test]
fn session_transition_preserves_backend_cause_without_retries_or_raw_fallback() {
    let root = tempfile::tempdir().unwrap();
    let mut s = session(root.path());
    let _seen = configure(&mut s, true, None);
    request_settled(&mut s);
    wait_done(&mut s, 3);
    let cause = s.poll_image_depth_transition().unwrap_err();
    assert!(cause
        .to_string()
        .contains("original managed transition failure"));
    assert!(s.image_depth_transition().unwrap().is_none());
    assert_eq!(s.depth_status().unwrap().unwrap().counts.failed, 1);
    assert_eq!(s.depth_status().unwrap().unwrap().counts.submitted, 3);
}

#[test]
fn session_transition_capacity_refusal_preserves_reference_and_explicit_recovery() {
    init_log();
    let root = tempfile::tempdir().unwrap();
    let initial = session(root.path());
    let mut project = initial.engine.project().clone();
    let asset = project.assets[0].clone();
    project.assets = (1..=motion_model::MAX_LAYERS)
        .map(|id| Asset {
            id: id as u64,
            ..asset.clone()
        })
        .collect();
    drop(initial);
    let mut s = Session::new(project, root.path().into(), Arc::new(NoMedia)).unwrap();
    let (echo, _seen, _) = backend(false);
    s.configure_depth(Scaled {
        echo,
        blocked: None,
        fail: false,
    })
    .unwrap();
    for id in 1..=motion_model::MAX_LAYERS {
        s.request_image_depth(id as u64, geometry(), model())
            .unwrap();
        wait_done(&mut s, id as u64);
        assert!(s.poll_depth().unwrap().is_some());
    }
    let full = s.depth_status().unwrap().unwrap();
    assert_eq!(full.retained_samples, motion_model::MAX_LAYERS);
    s.begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    let rejected_request = s
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(&mut s, motion_model::MAX_LAYERS as u64 + 1);
    let cause = s.poll_image_depth_transition().unwrap_err().to_string();
    assert!(cause.contains("phase=retain: capacity exceeded"), "{cause}");
    assert!(cause.contains("samples=129/128"), "{cause}");
    let rejected = s.depth_status().unwrap().unwrap();
    assert_eq!(rejected.retained_samples, full.retained_samples);
    assert_eq!(rejected.retained_bytes, full.retained_bytes);
    assert_eq!(rejected.retained_image_bytes, full.retained_image_bytes);
    assert_eq!(rejected.retention_rejections, 1);
    assert!(!rejected.installed);
    assert!(s.image_depth_transition().unwrap().is_none());
    assert_eq!(
        s.depth_status().unwrap().unwrap().counts.submitted,
        rejected_request
    );
    assert!(
        LOG.0.lock().unwrap().iter().any(|(level, message)| {
            *level == log::Level::Error
                && message.contains("operation=depth_transition_session_admit")
                && message.contains(&format!("request={rejected_request}"))
                && message.contains(&format!("source_id={}", motion_model::MAX_LAYERS))
                && message.contains(&cause)
        }),
        "capacity refusal must retain its cause/request/source at default Info"
    );
    s.cancel_depth().unwrap();
    s.request_image_depth(motion_model::MAX_LAYERS as u64, geometry(), model())
        .unwrap();
    wait_done(&mut s, motion_model::MAX_LAYERS as u64 + 2);
    assert!(s.poll_depth().unwrap().is_some());
    s.begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    s.request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(&mut s, motion_model::MAX_LAYERS as u64 + 3);
    assert!(s.image_depth_transition().unwrap().is_some());
    let recovered = s.depth_status().unwrap().unwrap();
    assert_eq!(recovered.retained_samples, 2);
    assert_eq!(
        recovered.retention_rejections, 1,
        "rejections are a documented lifetime counter"
    );
    assert!(recovered.installed);
}

#[test]
fn session_transition_seek_revokes_a_candidate_blocked_inside_its_actual_backend() {
    let root = tempfile::tempdir().unwrap();
    let mut s = session(root.path());
    let (release, blocked) = mpsc::channel();
    let seen = configure(&mut s, false, Some(blocked));
    // Release before Session drop, including during assertion unwinding.
    struct Release(Option<mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let mut guard = Release(Some(release));
    request_settled(&mut s);
    for _ in 0..3 {
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let flight = s.depth_status().unwrap().unwrap().in_flight.unwrap();
    assert_eq!(flight.phase, "backend");
    s.frame = 1.;
    assert!(s.image_depth_transition().unwrap().is_none());
    guard.0.take().unwrap().send(()).unwrap();
    wait_done(&mut s, 3);
    s.frame = 0.;
    assert!(s.image_depth_transition().unwrap().is_none());
    assert!(!s.depth_status().unwrap().unwrap().installed);
}

#[test]
fn session_transition_rejects_missing_reference_invalid_limits_and_backward_clock() {
    let root = tempfile::tempdir().unwrap();
    let mut s = session(root.path());
    assert!(s
        .begin_image_depth_transition(settled(), policy(), limits())
        .is_err());
    let (worker, _seen, _) = backend(false);
    s.configure_depth(worker).unwrap();
    assert!(s
        .begin_image_depth_transition(settled(), policy(), limits())
        .is_err());
    s.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut s, 1);
    s.poll_depth().unwrap().unwrap();
    let mut invalid = limits();
    invalid.scale_bounds = [0., 1.];
    assert!(s
        .begin_image_depth_transition(settled(), policy(), invalid)
        .is_err());
    // Invalid configuration did not replace the current raw admission.
    assert!(s.poll_depth().unwrap().is_some());
    s.begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    s.request_image_depth_transition(Activity::Interactive, Duration::from_secs(1))
        .unwrap()
        .unwrap();
    wait_done(&mut s, 2);
    assert!(s.poll_image_depth_transition().unwrap());
    assert!(s
        .request_image_depth_transition(Activity::Idle, Duration::ZERO)
        .unwrap_err()
        .to_string()
        .contains("clock moved backwards"));
    assert!(s.image_depth_transition().unwrap().is_some());
    assert_eq!(s.depth_status().unwrap().unwrap().counts.submitted, 2);
}

#[test]
fn session_transition_composes_saved_camera_z_and_inverse_calibration_without_changing_authoring_data(
) {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    for curve in [CalibrationCurve::CameraZ, CalibrationCurve::InverseCameraZ] {
        let root = tempfile::tempdir().unwrap();
        let mut s = calibrated_source_session(root.path());
        let (echo, _seen, _) = backend(false);
        s.configure_depth(Scaled {
            echo,
            blocked: None,
            fail: false,
        })
        .unwrap();
        s.request_image_depth(7, geometry(), model()).unwrap();
        wait_done(&mut s, 1);
        s.poll_depth().unwrap().unwrap();
        let command = s
            .depth_calibration_command(
                1,
                [
                    DepthAnchor {
                        value: 10.,
                        camera_z_meters: 8.,
                    },
                    DepthAnchor {
                        value: 90.,
                        camera_z_meters: 4.,
                    },
                ],
                curve,
                CalibrationExtrapolation::Reject,
                20.,
            )
            .unwrap();
        s.apply_command_batch(vec![command]).unwrap();
        s.request_image_depth(7, geometry(), model()).unwrap();
        wait_done(&mut s, 2);
        s.poll_depth().unwrap().unwrap();
        let authored = s.engine.project().layers[0]
            .depth_calibration
            .clone()
            .unwrap();
        s.begin_image_depth_transition(settled(), policy(), limits())
            .unwrap();
        s.request_image_depth_transition(Activity::Interactive, Duration::ZERO)
            .unwrap()
            .unwrap();
        wait_done(&mut s, 3);
        assert!(s.poll_image_depth_transition().unwrap());
        assert!(s
            .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
            .unwrap()
            .is_none());
        s.request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
            .unwrap()
            .unwrap();
        wait_done(&mut s, 4);
        let binding = s.bound_depth_calibration(1).unwrap().unwrap();
        assert_eq!(binding.descriptor().model, settled());
        assert_eq!(binding.anchors()[0].value, 3.5);
        assert_eq!(binding.anchors()[1].value, 43.5);
        let candidate = DepthResource::new(
            binding.descriptor().clone(),
            (0..9).map(|i| ((i + 1) as f32 * 10. - 3.) / 2.).collect(),
            None,
            None,
        )
        .unwrap();
        let calibrated = binding.bind(&candidate).unwrap();
        for i in 0..9 {
            let t = i as f64 / 8.;
            let expected = match curve {
                CalibrationCurve::CameraZ => 8. * (1. - t) + 4. * t,
                CalibrationCurve::InverseCameraZ => (0.125 * (1. - t) + 0.25 * t).recip(),
            };
            assert!((calibrated.camera_z_meters(i).unwrap().unwrap() - expected).abs() < 1e-10);
        }
        assert_eq!(
            s.engine.project().layers[0]
                .depth_calibration
                .as_ref()
                .unwrap(),
            &authored
        );
        assert!(
            authored
                .rebind(&candidate, s.depth_input_fingerprint().unwrap().unwrap())
                .is_err(),
            "saved authoring data must still refuse direct cross-model rebind"
        );
    }
}
