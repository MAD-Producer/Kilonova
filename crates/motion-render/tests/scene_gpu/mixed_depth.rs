use super::*;
use motion_model::depth::{blend::*, *};
use motion_render::scene_gpu::{SceneDepthInput, ScenePass, ScenePassFrame, ScenePassInstance};

fn field(values: Vec<f32>, frame: i64) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size: [6, 1],
            source: DepthSource {
                asset: 76,
                revision: 1,
                pts: DepthTime {
                    ticks: frame,
                    timescale: 24,
                },
            },
            geometry: DepthGeometry {
                source_size: [6, 1],
                crop: [0, 0, 6, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "authored-native-blend-proof".into(),
                revision: "1".into(),
                sha256: [76; 32],
                processing: "camera-z-linear-v1".into(),
            },
            measure: DepthMeasure::Metric {
                meters_per_unit: 1.,
                axis: DepthAxis::CameraZ,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        values,
        None,
        None,
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn registered_masked_sequence_preserves_native_occlusion_without_accumulated_drift() {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 8;
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut pass = ScenePass::new(&renderer, 1).unwrap();
    let frame = ScenePassFrame::new(frame().camera);
    let projection = DepthProjection::new(frame.camera, 1.).unwrap();
    let resource = |shift: usize, noise: f32, tick: i64| {
        let mut descriptor = field(vec![4.; 6], tick).descriptor().clone();
        descriptor.size = [WIDTH as u32, HEIGHT as u32];
        descriptor.geometry.source_size = descriptor.size;
        descriptor.geometry.crop = [0, 0, WIDTH as u32, HEIGHT as u32];
        let values = (0..WIDTH * HEIGHT)
            .map(|index| {
                let (x, y) = (index % WIDTH, index / WIDTH);
                if (2 + shift..6 + shift).contains(&x) && (2..6).contains(&y) {
                    1. + noise
                } else {
                    4.
                }
            })
            .collect();
        DepthResource::new(descriptor, values, None, None).unwrap()
    };
    let reference = resource(0, 0., 0);
    let stale = DepthRegistration::same_sample(
        DepthImageFrame::from_depth(&reference),
        DepthImageFrame::from_depth(&reference),
    )
    .unwrap();
    let mut mesh = GpuScene::upload(
        &renderer,
        REQUEST,
        fixture(
            vec![emissive([1., 0., 0.], 1., "OPAQUE")],
            &[(0., 0)],
            None,
            false,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut first_cycle = Vec::new();
    let mut records = Vec::new();
    for tick in 0..48 {
        let phase = tick % 12;
        let shift = [0, 1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1][phase];
        let noise = [-0.2_f32, 0., 0.2, 0., -0.2, 0., 0.2, 0., -0.2, 0., 0.2, 0.][phase];
        let ai = resource(shift, noise, tick as i64);
        let registration = DepthRegistration::new(
            DepthImageFrame::from_depth(&ai),
            DepthImageFrame::from_depth(&reference),
            [[1., 0., -(shift as f64)], [0., 1., 0.], [0., 0., 1.]],
        )
        .unwrap();
        let mask = DepthWeightMask::new(
            ai.descriptor().source.clone(),
            ai.descriptor().size,
            ai.descriptor().geometry.clone(),
            (0..WIDTH * HEIGHT)
                .map(|i| [0., 0.5, 1.][i % WIDTH % 3])
                .collect(),
            None,
        )
        .unwrap();
        let mask_registration =
            DepthRegistration::same_sample(DepthImageFrame::from_depth(&ai), mask.frame().clone())
                .unwrap();
        let options = || DepthBlendOptions {
            weight: 0.5,
            output_model: ai.descriptor().model.clone(),
            byte_budget: WIDTH * HEIGHT * 5,
        };
        if tick != 0 {
            let error = blend_depth(
                DepthBlendInput::new(&ai, None, &projection, None).unwrap(),
                DepthBlendInput::new(&reference, None, &projection, None).unwrap(),
                &stale,
                Some((&mask, &mask_registration)),
                options(),
            )
            .err()
            .unwrap();
            assert!(error.to_string().contains("does not match source"));
        }
        let began = std::time::Instant::now();
        let mixed = blend_depth(
            DepthBlendInput::new(&ai, None, &projection, None).unwrap(),
            DepthBlendInput::new(&reference, None, &projection, None).unwrap(),
            &registration,
            Some((&mask, &mask_registration)),
            options(),
        )
        .unwrap();
        let blend_us = began.elapsed().as_micros();
        assert_eq!(mixed.metrics.output_pixels, (WIDTH * HEIGHT) as u64);
        assert_eq!(mixed.metrics.mask_reads, (WIDTH * HEIGHT) as u64);
        assert_eq!(
            mixed.metrics.reference_reads,
            ((WIDTH - shift) * HEIGHT) as u64
        );
        assert_eq!(mixed.metrics.ai_reads, (10 * HEIGHT) as u64);
        assert_eq!(mixed.metrics.invalid_pixels, (shift * HEIGHT) as u64);
        let expected_meters = |x: usize, y: usize| -> Option<f64> {
            if x < shift {
                return None;
            }
            if (2 + shift..6 + shift).contains(&x) && (2..6).contains(&y) {
                Some(1. + [0., 0.25, 0.5][x % 3] * f64::from(noise))
            } else {
                Some(4.)
            }
        };
        for index in 0..WIDTH * HEIGHT {
            let expected = expected_meters(index % WIDTH, index / WIDTH);
            assert_eq!(
                mixed.resource.validity().unwrap()[index],
                u8::from(expected.is_some())
            );
            if let Some(expected) = expected {
                assert!((f64::from(mixed.resource.values()[index]) - expected).abs() < 1e-6);
            }
        }
        let projected = ProjectedDepth::from_camera_z(
            &mixed.resource,
            projection.clone(),
            [WIDTH as u32, HEIGHT as u32],
        )
        .unwrap();
        let input = SceneDepthInput {
            field: &projected,
            source: &ai.descriptor().source,
            pixels_per_metre: 1.,
        };
        let mut instance = ScenePassInstance::new(0, REQUEST, 0, 1.);
        instance.lights = &[];
        instance.ambient = [0.; 3];
        let began = std::time::Instant::now();
        let stats = pass
            .draw_with_depth(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut mesh],
                &[instance],
                &frame,
                &input,
            )
            .unwrap();
        let depth = super::shared::depth(&renderer, pass.depth().unwrap());
        let image = pixels(&renderer, &target, ChannelPrecision::Unorm8);
        let readback_wall_us = began.elapsed().as_micros();
        assert_eq!(stats.depth_input_bytes, (WIDTH * HEIGHT * 8 + 16) as u64);
        let mut checked = 0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let uv = [
                    (f64::from(x) + 0.5) / f64::from(SIZE),
                    (f64::from(y) + 0.5) / f64::from(SIZE),
                ];
                let (px, py) = (2. * uv[0] - 1., 1. - 2. * uv[1]);
                let edges = [py + 0.9, px + (0.9 - py) * 0.5, (0.9 - py) * 0.5 - px];
                // Exact triangle-edge raster ownership is outside this scalar
                // depth oracle; all other pixels are checked on every frame.
                if edges.iter().any(|v| v.abs() < 1e-6) {
                    continue;
                }
                let inside = edges.iter().all(|v| *v > 0.);
                let z = expected_meters(
                    (uv[0] * WIDTH as f64).floor() as usize,
                    (uv[1] * HEIGHT as f64).floor() as usize,
                );
                let visible = inside && z.is_none_or(|z| z > 3.);
                let seed = z.map_or(1., |z| (z - 0.1) / 9.9);
                let expected = if inside {
                    seed.min((3. - 0.1) / 9.9)
                } else {
                    seed
                };
                let index = (y * SIZE + x) as usize;
                assert!(
                    (f64::from(depth[index]) - expected).abs() < 1e-6,
                    "tick={tick} pixel={x},{y}"
                );
                assert_eq!(
                    &image.pixels[index * 4..index * 4 + 4],
                    if visible {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 0, 0, 0]
                    },
                    "tick={tick} pixel={x},{y}"
                );
                checked += 1;
            }
        }
        assert!(checked > 4000);
        if tick < 12 {
            first_cycle.push((depth.clone(), image.pixels.clone()));
        } else {
            assert_eq!(
                (&depth, &image.pixels),
                (&first_cycle[phase].0, &first_cycle[phase].1),
                "complete repeated movement must not accumulate drift or retain prior occlusion"
            );
        }
        if let Some(root) = std::env::var_os("MOTION_DEPTH_BLEND_ARTIFACTS") {
            let root = std::path::PathBuf::from(root).join("sequence");
            std::fs::create_dir_all(&root).unwrap();
            image::save_buffer(
                root.join(format!("{tick:03}.png")),
                &image.pixels,
                SIZE,
                SIZE,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        records.push(json!({"frame":tick,"phase":phase,"shift":shift,"noise_m":noise,
            "blend_cpu_us":blend_us,"submit_and_readback_wall_us":readback_wall_us,
            "checked_pixels":checked,"samples":mixed.metrics,"native_input_bytes":stats.depth_input_bytes}));
    }
    if let Some(root) = std::env::var_os("MOTION_DEPTH_BLEND_ARTIFACTS") {
        std::fs::write(
            std::path::PathBuf::from(root).join("sequence-costs.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn registered_moving_depth_reaches_native_float_hardware_depth_and_mesh_occlusion() {
    let mut records = Vec::new();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let mut pass = ScenePass::new(&renderer, 1).unwrap();
        let frame = ScenePassFrame::new(frame().camera);
        let projection = DepthProjection::new(frame.camera, 1.).unwrap();
        let reference = field(vec![4., 4., 2., 4., 4., 4.], 0);
        let mut mesh = GpuScene::upload(
            &renderer,
            REQUEST,
            fixture(
                vec![emissive([1., 0., 0.], 1., "OPAQUE")],
                &[(0., 0)],
                None,
                false,
            ),
            SceneGpuLimits::default(),
        )
        .unwrap();
        for (tick, ai_z, expected_z) in [(0, 2.2, 2.1_f64), (1, 1.8, 1.9), (2, 2.1, 2.05)] {
            let mut values = vec![4.; 6];
            values[2 + tick] = ai_z;
            let ai = field(values, tick as i64);
            let registration = DepthRegistration::new(
                DepthImageFrame::from_depth(&ai),
                DepthImageFrame::from_depth(&reference),
                [[1., 0., -(tick as f64)], [0., 1., 0.], [0., 0., 1.]],
            )
            .unwrap();
            let began = std::time::Instant::now();
            let mixed = blend_depth(
                DepthBlendInput::new(&ai, None, &projection, None).unwrap(),
                DepthBlendInput::new(&reference, None, &projection, None).unwrap(),
                &registration,
                None,
                DepthBlendOptions {
                    weight: 0.5,
                    output_model: ai.descriptor().model.clone(),
                    byte_budget: 30,
                },
            )
            .unwrap();
            let blend_us = began.elapsed().as_micros();
            assert_eq!(
                (
                    mixed.metrics.ai_reads,
                    mixed.metrics.reference_reads,
                    mixed.metrics.invalid_pixels,
                    mixed.metrics.output_bytes
                ),
                (6, 6 - tick as u64, tick as u64, 30)
            );
            let began = std::time::Instant::now();
            let projected =
                ProjectedDepth::from_camera_z(&mixed.resource, projection.clone(), [6, 1]).unwrap();
            let projection_us = began.elapsed().as_micros();
            let input = SceneDepthInput {
                field: &projected,
                source: &ai.descriptor().source,
                pixels_per_metre: 1.,
            };
            let began = std::time::Instant::now();
            let stats = pass
                .draw_with_depth(&renderer, REQUEST, &target, &mut [], &[], &frame, &input)
                .unwrap();
            let submitted_us = began.elapsed().as_micros();
            assert_eq!(stats.depth_input_bytes, 64);
            let depth = super::shared::depth(&renderer, pass.depth().unwrap());
            // Independent orthographic near/far equation and source-grid map.
            // No call to the CPU projection or blend kernel supplies the oracle.
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let cell = ((f64::from(x) + 0.5) / f64::from(SIZE) * 6.).floor() as usize;
                    let expected = if cell < tick {
                        1.
                    } else {
                        ((if cell == 2 + tick { expected_z } else { 4. }) - 0.1) / 9.9
                    };
                    assert!(
                        (f64::from(depth[(y * SIZE + x) as usize]) - expected).abs() < 1e-6,
                        "precision={precision:?}, tick={tick}, pixel={x},{y}"
                    );
                }
            }
            let mut instance = ScenePassInstance::new(0, REQUEST, 0, 1.);
            instance.lights = &[];
            instance.ambient = [0.; 3];
            pass.draw_with_depth(
                &renderer,
                REQUEST,
                &target,
                &mut [&mut mesh],
                &[instance],
                &frame,
                &input,
            )
            .unwrap();
            let image = pixels(&renderer, &target, precision);
            // All three moving positions are well inside the triangle at y=45.
            let position = ((2.5 + tick as f64) / 6. * f64::from(SIZE)).floor() as usize;
            let bytes = usize::try_from(image.storage.bytes_per_pixel()).unwrap();
            let at = |x: usize| {
                let offset = (45 * SIZE as usize + x) * bytes;
                &image.pixels[offset..offset + bytes]
            };
            assert!(
                at(position).iter().all(|b| *b == 0),
                "foreground must occlude the mesh"
            );
            if tick != 0 {
                assert!(
                    at(27).iter().any(|b| *b != 0),
                    "old position must not retain a ghost"
                );
            }
            if precision == ChannelPrecision::Unorm8 {
                if let Some(root) = std::env::var_os("MOTION_DEPTH_BLEND_ARTIFACTS") {
                    let root = std::path::PathBuf::from(root);
                    std::fs::create_dir_all(&root).unwrap();
                    image::save_buffer(
                        root.join(format!("native-depth-{tick}.png")),
                        &image.pixels,
                        SIZE,
                        SIZE,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            records.push(json!({"precision":format!("{precision:?}"), "frame":tick,
                "expected_foreground_m":expected_z, "blend_us":blend_us,
                "projection_us":projection_us, "submission_cpu_us":submitted_us,
                "logical_samples":mixed.metrics, "depth_input_bytes":stats.depth_input_bytes,
                "limits":"Authored three-frame scalar motion, not real footage or GPU duration"}));
        }
    }
    if let Some(root) = std::env::var_os("MOTION_DEPTH_BLEND_ARTIFACTS") {
        std::fs::write(
            std::path::PathBuf::from(root).join("native-depth-costs.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}
