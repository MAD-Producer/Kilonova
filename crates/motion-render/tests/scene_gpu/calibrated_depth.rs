use super::*;
use motion_model::depth::*;
use motion_render::scene_gpu::{SceneDepthInput, ScenePass, ScenePassFrame, ScenePassInstance};

struct DepthLog(std::sync::Mutex<Vec<String>>);
static DEPTH_LOG: DepthLog = DepthLog(std::sync::Mutex::new(Vec::new()));
impl log::Log for DepthLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = record.args().to_string();
        if line.starts_with("scene_shared_pass #740087 ") {
            assert_eq!(record.level(), log::Level::Error);
            let mut lines = self.0.lock().unwrap();
            assert!(lines.len() < 3, "sustained failure must be suppressed");
            lines.push(line);
        }
    }
    fn flush(&self) {}
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_depth_failure_observation_keeps_sample_context_and_recovers_at_default_level() {
    const CHILD: &str = "MOTION_TEST_CALIBRATED_DEPTH_OBSERVER_CHILD";
    // Isolate the process-global logger from the existing shared-pass observer.
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "calibrated_depth::calibrated_depth_failure_observation_keeps_sample_context_and_recovers_at_default_level", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    log::set_logger(&DEPTH_LOG).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let frame = ScenePassFrame::new(frame().camera);
    let field = field(frame.camera, 1., vec![2.], None);
    let mut pass = ScenePass::new(&renderer, 1).unwrap();
    let request = SceneRequest {
        request: 740087,
        source: 7400,
    };
    let mut stale = field.source().source.clone();
    stale.pts.ticks += 1;
    let bad = SceneDepthInput {
        field: &field,
        source: &stale,
        pixels_per_metre: 1.,
    };
    for occurrence in 1..=65 {
        assert!(pass
            .draw_with_depth(&renderer, request, &target, &mut [], &[], &frame, &bad)
            .is_err());
        assert_eq!(
            DEPTH_LOG.0.lock().unwrap().len(),
            if occurrence < 60 { 1 } else { 2 }
        );
    }
    pass.draw_with_depth(
        &renderer,
        request,
        &target,
        &mut [],
        &[],
        &frame,
        &input(&field),
    )
    .unwrap();
    assert!(pass
        .draw_with_depth(&renderer, request, &target, &mut [], &[], &frame, &bad)
        .is_err());
    let lines = DEPTH_LOG.0.lock().unwrap();
    assert_eq!(lines.len(), 3);
    for line in lines.iter() {
        assert!(line.len() < 1300);
        for part in [
            "#740087",
            "source=7400",
            "elapsed_ms=",
            "phase=calibrated_depth",
            "asset: 74",
            "revision: 2",
            "ticks: 12",
            "ticks: 13",
            "timescale: 24",
            "stale depth source/revision/PTS",
        ] {
            assert!(line.contains(part), "missing {part}: {line}");
        }
    }
}

fn field(
    camera: CameraPose,
    units: f64,
    values: Vec<f32>,
    validity: Option<Vec<u8>>,
) -> ProjectedDepth {
    let width = values.len() as u32;
    let descriptor = DepthDescriptor {
        size: [width, 1],
        source: DepthSource {
            asset: 74,
            revision: 2,
            pts: DepthTime {
                ticks: 12,
                timescale: 24,
            },
        },
        geometry: DepthGeometry {
            source_size: [width, 1],
            crop: [0, 0, width, 1],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "independent-native-field".into(),
            revision: "1".into(),
            sha256: [4; 32],
            processing: "native-field-v1".into(),
        },
        measure: DepthMeasure::Relative {
            near: NearDirection::Increasing,
        },
        scale: 1.,
        offset: 0.,
        valid_range: None,
    };
    let calibration = RelativeDepthCalibration::new(
        descriptor.clone(),
        [
            DepthAnchor {
                value: 1.,
                camera_z_meters: 4.,
            },
            DepthAnchor {
                value: 2.,
                camera_z_meters: 2.,
            },
        ],
        CalibrationCurve::CameraZ,
        CalibrationExtrapolation::Reject,
    )
    .unwrap();
    let resource = DepthResource::new(descriptor, values, validity, None).unwrap();
    ProjectedDepth::from_calibrated(
        &calibration.bind(&resource).unwrap(),
        DepthProjection::new(camera, units).unwrap(),
        [width, 1],
    )
    .unwrap()
}
fn input(field: &ProjectedDepth) -> SceneDepthInput<'_> {
    SceneDepthInput {
        field,
        source: &field.source().source,
        pixels_per_metre: field.projection().pixels_per_meter(),
    }
}
fn instance(units: f32) -> ScenePassInstance<'static> {
    let mut item = ScenePassInstance::new(0, REQUEST, 0, units);
    item.lights = &[];
    item.ambient = [0.; 3];
    item
}
fn red(renderer: &Renderer) -> GpuScene {
    GpuScene::upload(
        renderer,
        REQUEST,
        fixture(
            vec![emissive([1., 0., 0.], 1., "OPAQUE")],
            &[(0., 0)],
            None,
            false,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_depth_occludes_real_meshes_in_four_precisions_and_both_sample_counts() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let reference = renderer.render_target(SIZE, SIZE).unwrap();
        let mut gpu = red(&renderer);
        let mut pass = ScenePass::new(&renderer, 8).unwrap();
        let mut frame = ScenePassFrame::new(frame().camera);
        let near = field(frame.camera, 1., vec![2.], None);
        let far = field(frame.camera, 1., vec![1.], None);
        for samples in [1, 4] {
            frame.samples = samples;
            pass.draw(
                &renderer,
                REQUEST,
                &reference,
                &mut [&mut gpu],
                &[instance(1.)],
                &frame,
            )
            .unwrap();
            let expected = pixels(&renderer, &reference, precision);
            assert!(center_linear(&expected)[0] > 0.99);
            let stats = pass
                .draw_with_depth(
                    &renderer,
                    REQUEST,
                    &target,
                    &mut [&mut gpu],
                    &[instance(1.)],
                    &frame,
                    &input(&near),
                )
                .unwrap();
            assert_eq!(stats.depth_input_bytes, 24);
            assert_eq!(
                center_linear(&pixels(&renderer, &target, precision)),
                [0.; 4]
            );
            if samples == 1 {
                let actual = super::shared::depth(&renderer, pass.depth().unwrap());
                assert!(
                    (actual[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (2. - 0.1) / (10. - 0.1))
                        .abs()
                        < 1e-6
                );
            }
            pass.draw_with_depth(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut gpu],
                &[instance(1.)],
                &frame,
                &input(&far),
            )
            .unwrap();
            assert_eq!(
                pixels(&renderer, &target, precision).pixels,
                expected.pixels,
                "depth seeding must not alter color or MSAA coverage"
            );
            if samples == 1 {
                let actual = super::shared::depth(&renderer, pass.depth().unwrap());
                assert!(
                    (actual[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (3. - 0.1) / (10. - 0.1))
                        .abs()
                        < 1e-6
                );
                assert!((actual[0] - (4. - 0.1) / (10. - 0.1)).abs() < 1e-6);
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_native_validity_and_letterbox_leave_color_and_uncovered_depth_intact() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut pass = ScenePass::new(&renderer, 1).unwrap();
    let mut frame = ScenePassFrame::new(frame().camera);
    frame.viewport = Some([0., 16.25, SIZE as f32, 32.5]);
    frame.background = [0.2, 0.4, 0.6, 0.5];
    let field = field(
        frame.camera,
        1.,
        vec![2., 1., f32::NAN],
        Some(vec![1, 1, 0]),
    );
    pass.draw(&renderer, REQUEST, &target, &mut [], &[], &frame)
        .unwrap();
    let background = pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels;
    pass.draw_with_depth(
        &renderer,
        REQUEST,
        &target,
        &mut [],
        &[],
        &frame,
        &input(&field),
    )
    .unwrap();
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        background
    );
    let actual = super::shared::depth(&renderer, pass.depth().unwrap());
    for y in 0..SIZE {
        for x in 0..SIZE {
            let expected = if (y as f32 + 0.5) < 16.25
                || (y as f32 + 0.5) >= 48.75
                || (x as f32 + 0.5) >= SIZE as f32 * 2. / 3.
            {
                1.
            } else if (x as f32 + 0.5) < SIZE as f32 / 3. {
                (2. - 0.1) / (10. - 0.1)
            } else {
                (4. - 0.1) / (10. - 0.1)
            };
            assert!(
                (actual[(y * SIZE + x) as usize] - expected).abs() < 1e-6,
                "pixel {x},{y}"
            );
        }
    }
    assert_eq!(
        pass.texture_resource_ledger().total_known_bytes(),
        u64::from(SIZE * SIZE * 4) + 24
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_depth_preserves_global_blending_and_alpha_mask_depth_rules() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut source = GpuScene::upload(
        &renderer,
        REQUEST,
        fixture(
            vec![
                emissive([1., 0., 0.], 0.5, "BLEND"),
                emissive([0., 1., 0.], 1., "OPAQUE"),
                emissive([0., 0., 1.], 0., "MASK"),
            ],
            &[(0., 0), (-0.5, 1), (1., 2)],
            None,
            false,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let frame = ScenePassFrame::new(frame().camera);
    let field = field(frame.camera, 1., vec![1.375], None);
    let mut pass = ScenePass::new(&renderer, 4).unwrap();
    let stats = pass
        .draw_with_depth(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut source],
            &[instance(1.)],
            &frame,
            &input(&field),
        )
        .unwrap();
    assert_eq!((stats.draws, stats.blended_draws), (3, 1));
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0.5, 0., 0., 0.5],
    );
    let actual = super::shared::depth(&renderer, pass.depth().unwrap());
    assert!(
        (actual[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (3.25 - 0.1) / (10. - 0.1)).abs() < 1e-6,
        "discarded mask and blended foreground must leave the calibrated hardware depth intact"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn stale_calibrated_source_camera_and_world_units_refuse_before_target_recording() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = red(&renderer);
    let mut pass = ScenePass::new(&renderer, 4).unwrap();
    let mut frame = ScenePassFrame::new(frame().camera);
    let field = field(frame.camera, 1., vec![1.], None);
    pass.draw_with_depth(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut gpu],
        &[instance(1.)],
        &frame,
        &input(&field),
    )
    .unwrap();
    let original = pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels;
    let depth = pass.depth().unwrap().clone();
    drain(&renderer);
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    for site in 0..6 {
        let mut expected = field.source().source.clone();
        let mut units = 1.;
        match site {
            0 => expected.asset += 1,
            1 => expected.revision += 1,
            2 => expected.pts.ticks += 1,
            3 => frame.camera.eye.x += 1.,
            4 => units = 2.,
            _ => units = f64::NAN,
        }
        let bad = SceneDepthInput {
            field: &field,
            source: &expected,
            pixels_per_metre: units,
        };
        let cause = pass
            .draw_with_depth(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut gpu],
                &[instance(1.)],
                &frame,
                &bad,
            )
            .unwrap_err();
        assert!(matches!(cause, SceneGpuError::DepthInput { .. }), "{cause}");
        assert!(cause.to_string().contains("asset: 74"));
        frame.camera = field.projection().camera();
        assert_eq!(&**pass.depth().unwrap(), &*depth);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
        assert_eq!(
            pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
            original
        );
    }
    // Equivalent rational PTS is the same source sample.
    let mut expected = field.source().source.clone();
    expected.pts.ticks *= 2;
    expected.pts.timescale *= 2;
    let valid = SceneDepthInput {
        field: &field,
        source: &expected,
        pixels_per_metre: 1.,
    };
    pass.draw_with_depth(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut gpu],
        &[instance(1.)],
        &frame,
        &valid,
    )
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_depth_admission_rolls_back_and_pending_frames_keep_immutable_inputs() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let frame = ScenePassFrame::new(frame().camera);
    let near = field(frame.camera, 1., vec![2.], None);
    let far = field(frame.camera, 1., vec![1.], None);
    let mut pass = ScenePass::new(&renderer, 4).unwrap();
    pass.draw_with_depth(&renderer, REQUEST, &a, &mut [], &[], &frame, &input(&near))
        .unwrap();
    let original = pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels;
    drain(&renderer);
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    renderer.allocation_budget().set_limit(before + 23).unwrap();
    let cause = pass
        .draw_with_depth(&renderer, REQUEST, &a, &mut [], &[], &frame, &input(&far))
        .unwrap_err();
    assert!(cause.resource_rejection().is_some(), "{cause}");
    assert!(matches!(cause, SceneGpuError::DepthInput { .. }));
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    renderer.allocation_budget().set_limit(128 << 20).unwrap();
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        original
    );
    // Admit the new immutable input and 4x depth, then refuse the replacement
    // MSAA color by exactly one byte. No upload may have been queued yet.
    let old_depth = pass.depth().unwrap().clone();
    let mut four = ScenePassFrame::new(frame.camera);
    four.samples = 4;
    renderer
        .allocation_budget()
        .set_limit(before + 24 + u64::from(SIZE * SIZE * 4 * 4 * 2) - 1)
        .unwrap();
    let cause = pass
        .draw_with_depth(&renderer, REQUEST, &a, &mut [], &[], &four, &input(&far))
        .unwrap_err();
    assert!(cause.resource_rejection().is_some(), "{cause}");
    assert_eq!(&**pass.depth().unwrap(), &*old_depth);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before,
        "post-input attachment refusal must roll back without a pending queue-upload lease"
    );
    drop(old_depth);
    renderer.allocation_budget().set_limit(128 << 20).unwrap();
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        original
    );
    let mut gpu = red(&renderer);
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pass.encode_with_depth(
        &renderer,
        REQUEST,
        &mut first,
        &a,
        &mut [&mut gpu],
        &[instance(1.)],
        &frame,
        &input(&near),
    )
    .unwrap();
    let mut second = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pass.encode_with_depth(
        &renderer,
        REQUEST,
        &mut second,
        &b,
        &mut [&mut gpu],
        &[instance(1.)],
        &frame,
        &input(&far),
    )
    .unwrap();
    drop((pass, gpu));
    renderer.submit_allocated(second.finish(), |_| {}).unwrap();
    near8(
        center8(&pixels(&renderer, &b, ChannelPrecision::Unorm8)),
        [1., 0., 0., 1.],
    );
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(
        center8(&pixels(&renderer, &a, ChannelPrecision::Unorm8)),
        [0; 4]
    );
    drop((a, b));
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
    // Abandon an encode with a queued immutable upload, then retire the upload
    // through the existing allocated submission path. No premature lease release.
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut pass = ScenePass::new(&renderer, 1).unwrap();
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pass.encode_with_depth(
        &renderer,
        REQUEST,
        &mut abandoned,
        &target,
        &mut [],
        &[],
        &frame,
        &input(&near),
    )
    .unwrap();
    drop((abandoned, pass, target));
    let empty = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(empty.finish(), |_| {}).unwrap();
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_perspective_depth_uses_actual_finite_and_infinite_projection_in_world_units() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = red(&renderer);
    let mut pass = ScenePass::new(&renderer, 4).unwrap();
    for infinite in [false, true] {
        let eye = Vec3::new(0., 0., 60.);
        let projection = if infinite {
            Mat4::perspective_infinite_rh(1., 1., 2.)
        } else {
            Mat4::perspective_rh(1., 1., 2., 200.)
        };
        let camera = CameraPose {
            eye,
            target: Vec3::ZERO,
            projection: ProjectionKind::Perspective,
            view_projection: projection * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
        };
        let frame = ScenePassFrame::new(camera);
        let near = field(camera, 20., vec![2.], None);
        let far = field(camera, 20., vec![1.], None);
        for (field, visible, z) in [(&near, false, 40.), (&far, true, 60.)] {
            pass.draw_with_depth(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut gpu],
                &[instance(20.)],
                &frame,
                &input(field),
            )
            .unwrap();
            assert_eq!(
                center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
                if visible { [255, 0, 0, 255] } else { [0; 4] }
            );
            let expected = if infinite {
                1. - 2. / z
            } else {
                200. / 198. - 400. / (198. * z)
            };
            let actual = super::shared::depth(&renderer, pass.depth().unwrap());
            assert!((actual[(SIZE / 2 * SIZE + SIZE / 2) as usize] - expected).abs() < 1e-6);
        }
    }
}
