use super::*;
use crate::transitions::{limits, policy, settled};
use motion_host::depth::transition::Activity;

struct UnitProfiles {
    physical: Physical,
    bias: f32,
}
impl Backend for UnitProfiles {
    fn infer(
        &mut self,
        input: &Input,
        canceled: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        let original = self.physical.infer(input, canceled)?;
        if input.key().model.processing != "settled" {
            return Ok(original);
        }
        let mut descriptor = original.resource.descriptor().clone();
        descriptor.measure = match descriptor.measure {
            DepthMeasure::Metric {
                meters_per_unit,
                axis,
            } => DepthMeasure::Metric {
                meters_per_unit: meters_per_unit * 0.5,
                axis,
            },
            DepthMeasure::InverseMetric {
                inverse_meters_per_unit,
                axis,
            } => DepthMeasure::InverseMetric {
                inverse_meters_per_unit: inverse_meters_per_unit * 0.5,
                axis,
            },
            _ => panic!("authored physical control required"),
        };
        Ok(Estimate {
            resource: DepthResource::new(
                descriptor,
                original
                    .resource
                    .values()
                    .iter()
                    .map(|v| v * 2. + self.bias)
                    .collect(),
                None,
                None,
            )?,
            provider: "authored-unit-profile-control".into(),
            costs: Default::default(),
        })
    }
}
fn project(root: &Path) -> Project {
    let initial = calibrated_source_session(root);
    let mut project = initial.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [16., 16., -120.])
        .unwrap();
    project.camera.position.set_animated(0, true).unwrap();
    project
        .camera
        .position
        .set_at(30, [40., 16., -120.])
        .unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let mut blue = motion_core::Layer::solid(
        2,
        "Physical transition occlusion oracle",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    project
}
fn prepare(
    session: &mut Session,
    root: &Path,
    inverse: bool,
    bias: f32,
) -> (mpsc::Receiver<Seen>, DepthSnapshot) {
    let (inner, seen, _) = backend(false);
    session
        .configure_depth_in(
            UnitProfiles {
                physical: Physical { inner, inverse },
                bias,
            },
            root.join("cache"),
        )
        .unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(session, 1);
    session.poll_depth().unwrap().unwrap();
    let command = session.physical_depth_placement_command(1, 20.).unwrap();
    session.apply_command_batch(vec![command]).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(session, 2);
    session.poll_depth().unwrap().unwrap();
    let raw = DepthSnapshot::capture(session).unwrap();
    session
        .begin_image_depth_transition(settled(), policy(), limits())
        .unwrap();
    assert!(DepthSnapshot::capture(session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    session
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    wait_done(session, 3);
    assert!(session.poll_image_depth_transition().unwrap());
    assert_eq!(DepthSnapshot::capture(session).unwrap().layer_count(), 1);
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap();
    session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .unwrap();
    assert!(DepthSnapshot::capture(session)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    wait_done(session, 4);
    (seen, raw)
}
// The persisted format's digest is an integrity checksum. Independently encode
// its typed tuple so the negative fixture reaches reconstruction of the actual
// plane proof; an old checksum would only test the earlier corruption guard.
#[derive(serde::Serialize, serde::Deserialize)]
struct WireSpec {
    alignment_version: u32,
    transition_version: u32,
    generation: u64,
    phase: motion_host::depth::transition::Phase,
    choices: motion_host::depth::transition::Choices,
    policy: motion_host::depth::transition::Policy,
    limits: motion_host::depth::alignment::Limits,
    coefficients: [f64; 2],
}
fn metadata_seal(value: &serde_json::Value) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let layer = &value["entries"][0];
    let aligned = &layer["alignment"];
    let asset: Asset = serde_json::from_value(layer["asset"].clone()).unwrap();
    let key: Key = serde_json::from_value(layer["key"].clone()).unwrap();
    let input: motion_model::depth::PreparedDepthInput =
        serde_json::from_value(layer["input"].clone()).unwrap();
    let saved: motion_model::depth::SavedDepthCalibration =
        serde_json::from_value(layer["calibration"].clone()).unwrap();
    let spec: WireSpec = serde_json::from_value(aligned["spec"].clone()).unwrap();
    let reference: [u8; 32] = serde_json::from_value(aligned["reference_digest"].clone()).unwrap();
    let candidate: [u8; 32] = serde_json::from_value(aligned["candidate_digest"].clone()).unwrap();
    let bytes = serde_json::to_vec(&(
        "motion-frozen-alignment-v1",
        layer["object"].as_u64().unwrap(),
        asset,
        key,
        input,
        saved,
        layer["resource"].as_u64().unwrap(),
        layer["request"].as_u64().unwrap(),
        layer["decoded_budget"].as_u64().unwrap(),
        aligned["reference"].as_u64().unwrap(),
        spec,
        reference,
        candidate,
    ))
    .unwrap();
    Sha256::digest(bytes).into()
}

fn alter_coefficients(bytes: &[u8]) -> Vec<u8> {
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
    let original: [u8; 32] =
        serde_json::from_value(value["entries"][0]["alignment"]["metadata_digest"].clone())
            .unwrap();
    assert_eq!(
        metadata_seal(&value),
        original,
        "fixture must reproduce the unmodified wire seal first"
    );
    value["entries"][0]["alignment"]["spec"]["coefficients"][0] = serde_json::json!(0.75);
    value["entries"][0]["alignment"]["metadata_digest"] = serde_json::json!(metadata_seal(&value));
    let metadata = serde_json::to_vec(&value).unwrap();
    let mut result = bytes[..8].to_vec();
    result.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    result.extend_from_slice(&metadata);
    result.extend_from_slice(&bytes[12 + size..]);
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn physical_profile_transition_and_frozen_export_use_each_declared_unit_once() {
    for inverse in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let copy = tempfile::tempdir().unwrap();
        let id =
            motion_host::session::open(root.path().into(), project(root.path()), Arc::new(NoMedia))
                .unwrap();
        struct Close(i64);
        impl Drop for Close {
            fn drop(&mut self) {
                motion_host::session::close(self.0);
            }
        }
        let close = Close(id);
        let (seen, raw, snapshot, project) = motion_host::session::with_session(id, |s| {
            let (seen, raw) = prepare(s, root.path(), inverse, 0.);
            assert!(s.poll_image_depth_transition()?);
            let saved = s.engine.project().layers[0]
                .depth_calibration
                .as_ref()
                .unwrap()
                .clone();
            let view = s.image_depth_transition()?.unwrap();
            view.validate_physical_placement(&saved)?;
            assert!(view.calibration(&saved).is_err());
            let unit = |measure| match measure {
                DepthMeasure::Metric {
                    meters_per_unit, ..
                } => meters_per_unit,
                DepthMeasure::InverseMetric {
                    inverse_meters_per_unit,
                    ..
                } => inverse_meters_per_unit,
                _ => panic!("physical measure required"),
            };
            assert_eq!(unit(view.descriptor().measure), 0.125);
            assert_eq!(unit(view.sample_measure()), 0.25);
            assert_eq!(view.descriptor().model.processing, "settled");
            assert_eq!(s.depth_status()?.unwrap().counts.submitted, 4);
            Ok((
                seen,
                raw,
                DepthSnapshot::capture(s)?,
                s.engine.project().clone(),
            ))
        })
        .unwrap();
        let result = motion_host::ops::export::capture(id).unwrap();
        let live = image::open(result["path"].as_str().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(live.get_pixel(16, 10).0, [0, 0, 255, 255]);
        let near = live.get_pixel(16, 22).0;
        assert!(near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255);
        let mut bytes = Vec::new();
        snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let metadata: serde_json::Value = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
        assert_eq!(metadata["resources"].as_array().unwrap().len(), 2);
        assert_eq!(
            metadata["entries"][0]["alignment"]["spec"]["coefficients"],
            serde_json::json!([0.5, 0.])
        );
        assert_eq!(
            metadata["entries"][0]["alignment"]["spec"]["phase"],
            "settled"
        );
        let tampered = alter_coefficients(&bytes);
        assert!(DepthSnapshot::read_from(&mut &tampered[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("coefficients differ"));
        let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
        restored.validate_project(&project).unwrap();
        std::fs::create_dir_all(copy.path().join("assets")).unwrap();
        std::fs::copy(
            root.path().join("assets/input.png"),
            copy.path().join("assets/input.png"),
        )
        .unwrap();
        // A fresh GPU cache must still use the worker's original paired colors.
        image::RgbaImage::from_pixel(3, 3, image::Rgba([0, 255, 0, 255]))
            .save(root.path().join("assets/input.png"))
            .unwrap();
        let changed = motion_host::ops::export::capture(id).unwrap();
        assert_eq!(
            image::open(changed["path"].as_str().unwrap())
                .unwrap()
                .to_rgba8(),
            live
        );
        drop(close);
        root.close().unwrap();
        let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        renderer.configure_assets(&project, copy.path()).unwrap();
        let target = renderer
            .sdr_capture_target(32, 32, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        let mut scene = motion_render::Scene::new(&project).unwrap();
        let mut frames = Vec::new();
        for frame in [0., 15., 30.] {
            let mut outputs = Vec::new();
            for frozen in [&raw, &snapshot, &restored] {
                scene
                    .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
                    .unwrap();
                frozen
                    .verify_sources(copy.path(), &|| Ok(()))
                    .unwrap()
                    .attach(&mut scene)
                    .unwrap();
                assert!(renderer
                    .prepare_scene_assets(
                        &scene,
                        motion_render::image_resources::Resolution::Full,
                        false
                    )
                    .unwrap());
                outputs.push(renderer.capture_sdr(&scene, &target).unwrap().0.pixels);
            }
            assert!(
                outputs.windows(2).all(|pair| pair[0] == pair[1]),
                "inverse={inverse} camera_frame={frame}"
            );
            if frame == 0. {
                assert_eq!(outputs[0], *live.as_raw());
            }
            frames.push(outputs.remove(0));
        }
        assert_ne!(
            frames.first(),
            frames.last(),
            "actual camera animation must change the result"
        );
        motion_host::session::with_session(id, |_| Ok(()))
            .expect_err("Session was closed before independent export");
        assert_eq!(
            seen.try_iter().count(),
            4,
            "freeze/restore/export must not infer or retry"
        );
        image::RgbaImage::from_pixel(3, 3, image::Rgba([201, 2, 3, 255]))
            .save(copy.path().join("assets/input.png"))
            .unwrap();
        assert!(restored
            .verify_sources(copy.path(), &|| Ok(()))
            .err()
            .unwrap()
            .to_string()
            .contains("fingerprint changed"));
    }
}

#[test]
fn biased_physical_profile_cannot_render_or_freeze_the_older_reference() {
    for inverse in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let id =
            motion_host::session::open(root.path().into(), project(root.path()), Arc::new(NoMedia))
                .unwrap();
        struct Close(i64);
        impl Drop for Close {
            fn drop(&mut self) {
                motion_host::session::close(self.0);
            }
        }
        let _close = Close(id);
        motion_host::session::with_session(id, |s| {
            let (seen, raw) = prepare(s, root.path(), inverse, 1.);
            let cause = s.poll_image_depth_transition().unwrap_err();
            assert!(cause.to_string().contains("residual"), "{cause}");
            for _ in 0..3 {
                assert!(!s.poll_image_depth_transition().unwrap());
                assert!(DepthSnapshot::capture(s)
                    .unwrap_err()
                    .to_string()
                    .contains("pending"));
            }
            let status = s.depth_status().unwrap().unwrap();
            assert_eq!(status.counts.submitted, 4);
            assert_eq!(status.counts.failed, 1);
            assert_eq!(seen.try_iter().count(), 4);
            raw.validate_project(s.engine.project()).unwrap();
            Ok(())
        })
        .unwrap();
        let cause = motion_host::ops::export::capture(id).unwrap_err();
        assert!(cause.to_string().contains("pending"), "{cause}");
        assert!(
            !root.path().join("exports/frame-000000.png").exists(),
            "a rejected physical profile must not create a flat or old-reference export"
        );
    }
}

#[test]
fn physical_restore_publishes_unit_verification_start_before_primary_cancellation() {
    init_log();
    let root = tempfile::tempdir().unwrap();
    let mut s = Session::new(project(root.path()), root.path().into(), Arc::new(NoMedia)).unwrap();
    let (seen, _raw) = prepare(&mut s, root.path(), false, 0.);
    assert!(s.poll_image_depth_transition().unwrap());
    let snapshot = DepthSnapshot::capture(&s).unwrap();
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
    LOG.0.lock().unwrap().clear();
    let observed = std::cell::Cell::new(false);
    let cause = DepthSnapshot::read_from(&mut &bytes[..], &|| {
        let entered = LOG.0.lock().unwrap().iter().any(|(level, line)| {
            *level == log::Level::Info
                && line.contains("start frozen_depth operation=restore_alignment")
                && line.contains("source_request=4 object=1 source_id=7")
                && line.contains("phase=verify_physical_units")
        });
        if entered {
            observed.set(true);
            Err("primary physical verification cancellation".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(
        observed.get(),
        "Info start must precede the physical plane comparison"
    );
    assert_eq!(
        cause.to_string(),
        "primary physical verification cancellation"
    );
    assert_eq!(seen.try_iter().count(), 4);
}
