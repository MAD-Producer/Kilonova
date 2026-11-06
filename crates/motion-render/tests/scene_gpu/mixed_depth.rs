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
