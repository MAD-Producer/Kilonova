//! Explicit native cost study; excluded from ordinary test execution. Run with
//! a new output directory. Measurements describe this adapter/workload only.
use motion_core::{Asset, Content, EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{Renderer, Scene};
use serde_json::json;
use std::{path::PathBuf, time::Instant};

#[allow(dead_code)]
#[path = "../tests/support/precision_golden.rs"]
mod reference;

const W: u32 = 1920;
const H: u32 = 1080;
const WARMUP: usize = 3;
const SAMPLES: usize = 12;

fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let format = motion_render::image_formats::native_format(
        motion_render::image_formats::color_storage(precision),
    );
    pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        None,
        format,
        true,
        Default::default(),
    ))
    .unwrap()
}
fn fixture(precision: ChannelPrecision) -> (Project, Vec<u8>) {
    let mut project = Project::new(W, H, 30, 60).unwrap();
    project.working_precision = precision;
    project.background = [0.; 4];
    project.assets.push(Asset {
        id: 1,
        path: "assets/fusion-cost.png".into(),
        width: W,
        height: H,
    });
    let mut layer = Layer::solid(
        47,
        "Fusion cost study",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 1 };
    let package = motion_effects::builtin::package().unwrap();
    for (id, name) in [(11, "tint"), (12, "invert")] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == name)
            .unwrap();
        let mut effect = EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [W as f32, H as f32],
        );
        if name == "tint" {
            effect.params.get_mut("p0001").unwrap().track.value = [0.07, 0.19, 0.31, 1.];
            effect.params.get_mut("p0002").unwrap().track.value = [0.91, 0.65, 0.43, 1.];
            effect.params.get_mut("p0003").unwrap().track.value[0] = 68.25;
        } else {
            effect.params.get_mut("blend").unwrap().track.value[0] = 23.75;
        }
        layer.effects.push(effect);
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let pixels = (0..H)
        .flat_map(|y| {
            (0..W).flat_map(move |x| {
                [
                    (x * 255 / (W - 1)) as u8,
                    (y * 255 / (H - 1)) as u8,
                    ((x * 43 + y * 71) % 256) as u8,
                    255,
                ]
            })
        })
        .collect();
    (project, pixels)
}
fn draw(r: &mut Renderer, s: &Scene, target: &motion_render::ImageCaptureTarget) -> u64 {
    let began = Instant::now();
    r.draw(s, target.view(), W, H).unwrap();
    r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    began.elapsed().as_nanos().try_into().unwrap()
}
fn median(values: &[u64]) -> u64 {
    let mut values = values.to_vec();
    values.sort_unstable();
    values[values.len() / 2]
}
fn main() {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("new output directory required"),
    );
    assert!(!output.exists(), "do not overwrite measurement evidence");
    std::fs::create_dir_all(&output).unwrap();
    let mut rows = vec![];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for reverse in [false, true] {
            let (mut project, pixels) = fixture(precision);
            let order = if reverse {
                "invert_tint"
            } else {
                "tint_invert"
            };
            if reverse {
                project.layers[0].effects.reverse();
            }
            let mut unfused = renderer(precision);
            let mut fused = renderer(precision);
            let adapter = fused.adapter.get_info();
            fused.set_pointwise_fusion(true);
            for r in [&mut unfused, &mut fused] {
                r.upload_image(1, W, H, &pixels).unwrap();
            }
            let contract = ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                precision,
            );
            let original_target = unfused.image_capture_target(W, H, contract).unwrap();
            let fused_target = fused.image_capture_target(W, H, contract).unwrap();
            let mut scene = Scene::new(&project).unwrap();
            scene
                .sample(&project, 0., None, &ExpressionEvaluator)
                .unwrap();
            let cold_original_ns = draw(&mut unfused, &scene, &original_target);
            let cold_fused_ns = draw(&mut fused, &scene, &fused_target);
            for _ in 0..WARMUP {
                draw(&mut unfused, &scene, &original_target);
                draw(&mut fused, &scene, &fused_target);
            }
            let original_builds = unfused.effect_pipeline_builds();
            let fused_builds = fused.effect_pipeline_builds();
            let portable_compiles = fused.pointwise_fusion_report().portable_compiles;
            let original_observer = unfused.start_command_profiling(8).unwrap();
            let fused_observer = fused.start_command_profiling(8).unwrap();
            for observer in [original_observer.as_ref(), fused_observer.as_ref()]
                .into_iter()
                .flatten()
            {
                observer.start_trace().unwrap();
            }
            let (mut original_ns, mut fused_ns) = (vec![], vec![]);
            for sample in 0..SAMPLES {
                project.layers[0]
                    .effects
                    .iter_mut()
                    .find(|e| e.effect == "tint")
                    .unwrap()
                    .params
                    .get_mut("p0003")
                    .unwrap()
                    .track
                    .value[0] = 60. + sample as f32;
                scene
                    .sample(&project, sample as f64, None, &ExpressionEvaluator)
                    .unwrap();
                if sample % 2 == 0 {
                    original_ns.push(draw(&mut unfused, &scene, &original_target));
                    fused_ns.push(draw(&mut fused, &scene, &fused_target));
                } else {
                    fused_ns.push(draw(&mut fused, &scene, &fused_target));
                    original_ns.push(draw(&mut unfused, &scene, &original_target));
                }
            }
            for observer in [original_observer.as_ref(), fused_observer.as_ref()]
                .into_iter()
                .flatten()
            {
                observer.stop_trace();
            }
            assert_eq!(unfused.effect_pipeline_builds(), original_builds);
            assert_eq!(fused.effect_pipeline_builds(), fused_builds);
            assert_eq!(
                fused.pointwise_fusion_report().portable_compiles,
                portable_compiles
            );
            let expected = reference::linear(&unfused.read_image_target(&original_target).unwrap());
            let actual = reference::linear(&fused.read_image_target(&fused_target).unwrap());
            let worst = expected
                .iter()
                .zip(&actual)
                .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
                .fold(0_f64, f64::max);
            let bound = match precision {
                ChannelPrecision::Unorm8 => 0.012,
                ChannelPrecision::Unorm16 => 8. / 65535.,
                ChannelPrecision::Float16 => 0.005,
                ChannelPrecision::Float32 => 0.00002,
            };
            if worst > bound {
                let mut differences = expected
                    .iter()
                    .zip(&actual)
                    .enumerate()
                    .map(|(index, (a, b))| {
                        let difference = (0..4).map(|i| (a[i] - b[i]).abs()).fold(0_f64, f64::max);
                        (difference, index, *a, *b)
                    })
                    .collect::<Vec<_>>();
                differences.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
                std::fs::write(
                    output.join(format!("failure-{precision:?}-{order}.json")),
                    serde_json::to_vec_pretty(&differences[..16]).unwrap(),
                )
                .unwrap();
            }
            let report = fused.pointwise_fusion_report();
            if precision == ChannelPrecision::Unorm16 && !reverse {
                assert_eq!((report.original_passes, report.final_passes), (7, 7));
                assert!(report.groups.is_empty());
                assert_eq!(
                    report.retained.get(
                        &motion_render::effect_plan::FusionRetainReason::QuantizedTransferBoundary
                    ),
                    Some(&1)
                );
                assert_eq!(
                    actual, expected,
                    "retained 1080p counterexample must match every pixel"
                );
            } else {
                assert_eq!((report.original_passes, report.final_passes), (7, 3));
                assert_eq!(report.groups.len(), 1);
            }
            rows.push(json!({"precision":precision,"order":order,"width":W,"height":H,"warmup":WARMUP,"samples":SAMPLES,
            "adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend),"driver":adapter.driver,
                "driverInfo":adapter.driver_info,"vendor":adapter.vendor,"device":adapter.device},
            "unfused_cold_submit_and_wait_ns":cold_original_ns,"fused_cold_submit_and_wait_ns":cold_fused_ns,
            "unfused_submit_and_wait_ns":original_ns,"fused_submit_and_wait_ns":fused_ns,
            "unfused_median_ns":median(&original_ns),"fused_median_ns":median(&fused_ns),
            "unfused_pipeline_builds":original_builds,"fused_pipeline_builds":fused_builds,
            "unfused_native_pipeline_build_ns":unfused.effect_pipeline_build_nanos(),
            "fused_native_pipeline_build_ns":fused.effect_pipeline_build_nanos(),"fusion":report,
            "unfused_gpu_trace":original_observer.as_ref().and_then(|o|o.trace_snapshot()),
            "fused_gpu_trace":fused_observer.as_ref().and_then(|o|o.trace_snapshot()),
            "worst_linear_error":worst,"tolerance":bound,
            "measurement_scope":"1920x1080 single image/two effects, numeric updates, alternating order; CPU submit+GPU completion wait excluding readback; native encoder intervals include markers/scheduling; cold CPU pipeline times include validation wait; logical traffic is not physical bus bandwidth; adapter-specific, no general speedup claim",
            "physical_gpu_bandwidth_available":false}));
            std::fs::write(
                output.join("costs.json"),
                serde_json::to_vec_pretty(&rows).unwrap(),
            )
            .unwrap();
        }
    }
    // Retain every mode's evidence before rejecting the complete study. This
    // preserves exactly the original per-mode tolerance assertions.
    for row in &rows {
        let worst = row["worst_linear_error"].as_f64().unwrap();
        let bound = row["tolerance"].as_f64().unwrap();
        assert!(worst <= bound, "{}: {worst}>{bound}", row["precision"]);
    }
}
