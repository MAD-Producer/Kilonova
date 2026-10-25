//! Native opt-in execution, not a comparison of mocked compiler returns.
use motion_core::{Asset, Content, EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{
    Renderer, Scene,
    effect_plan::{FusionRetainReason, PlanBuilder},
};
use serde_json::json;
use std::time::Instant;

#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

const W: u32 = 65;
const H: u32 = 33;
const MODES: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];

fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let format = motion_render::image_formats::native_format(
        motion_render::image_formats::color_storage(precision),
    );
    // Explicitly request supported timestamp queries for this cost study.
    // Ordinary headless construction intentionally leaves them disabled.
    pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        None,
        format,
        true,
        Default::default(),
    ))
    .unwrap()
}

fn effect(id: u64, name: &str) -> EffectInstance {
    let p = motion_effects::builtin::package().unwrap();
    let d = p.manifest.effects.iter().find(|d| d.id == name).unwrap();
    EffectInstance::new(
        id,
        &p.manifest.id,
        &p.manifest.version,
        &p.hash,
        d,
        [W as f32, H as f32],
    )
}
fn scalar(e: &mut EffectInstance, name: &str, value: f32) {
    e.params.get_mut(name).unwrap().track.value[0] = value;
}
fn fixture(precision: ChannelPrecision) -> (Project, Vec<u8>) {
    let mut p = Project::new(W, H, 30, 60).unwrap();
    p.working_precision = precision;
    p.background = [0.; 4];
    p.assets.push(Asset {
        id: 1,
        path: "assets/pointwise-pattern.png".into(),
        width: W,
        height: H,
    });
    let mut layer = Layer::solid(
        1,
        "Pointwise proof",
        [W as f32, H as f32],
        [W as f32 / 2., H as f32 / 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 1 };
    let mut tint = effect(11, "tint");
    tint.params.get_mut("p0001").unwrap().track.value = [0.07, 0.19, 0.31, 1.];
    tint.params.get_mut("p0002").unwrap().track.value = [0.91, 0.65, 0.43, 1.];
    scalar(&mut tint, "p0003", 68.25);
    let mut invert = effect(12, "invert");
    scalar(&mut invert, "blend", 23.75);
    layer.effects = vec![tint, invert];
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    let pixels = (0..H)
        .flat_map(|y| {
            (0..W).flat_map(move |x| {
                [
                    (x * 255 / (W - 1)) as u8,
                    (y * 255 / (H - 1)) as u8,
                    ((x * 43 + y * 71) % 256) as u8,
                    match x % 5 {
                        0 => 0,
                        1 => 1,
                        2 => 63,
                        3 => 128,
                        _ => 255,
                    },
                ]
            })
        })
        .collect();
    (p, pixels)
}
fn scene(p: &Project) -> Scene {
    p.validate().unwrap();
    let mut scene = Scene::new(p).unwrap();
    scene.sample(p, 0., None, &ExpressionEvaluator).unwrap();
    scene
}
fn raw(r: &mut Renderer, p: &Project) -> (Vec<[f64; 4]>, u64) {
    let scene = scene(p);
    let t = r
        .image_capture_target(
            W,
            H,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                p.working_precision,
            ),
        )
        .unwrap();
    let began = Instant::now();
    r.draw(&scene, t.view(), W, H).unwrap();
    let image = r.read_image_target(&t).unwrap();
    (
        reference::linear(&image),
        began.elapsed().as_nanos().min(u64::MAX as u128) as u64,
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_fusion_preserves_four_storage_modes_alpha_channels_opacity_and_numeric_pipeline_reuse() {
    let mut report_rows = vec![];
    for precision in MODES {
        let (mut p, pixels) = fixture(precision);
        let mut unfused = renderer(precision);
        let mut fused = renderer(precision);
        for r in [&mut unfused, &mut fused] {
            r.upload_image(1, W, H, &pixels).unwrap();
        }
        let unfused_observer = unfused.start_command_profiling(8).unwrap();
        let fused_observer = fused.start_command_profiling(8).unwrap();
        fused.set_pointwise_fusion(true);
        let mut steady_compiles = None;
        for (case, (channel, opacity, reverse)) in [
            (1., 100., false),
            (2., 0., false),
            (3., 37.5, false),
            (4., 100., false),
            (5., 37.5, false),
            (5., 0., false),
            (1., 100., true),
            (5., 37.5, true),
        ]
        .into_iter()
        .enumerate()
        {
            p.layers[0]
                .effects
                .sort_by_key(|e| if reverse { u64::MAX - e.id } else { e.id });
            for e in &mut p.layers[0].effects {
                scalar(e, "effect_opacity", opacity);
                if e.effect == "invert" {
                    scalar(e, "channel", channel);
                }
            }
            let (expected, unfused_ns) = raw(&mut unfused, &p);
            let (actual, fused_ns) = raw(&mut fused, &p);
            let report = fused.pointwise_fusion_report();
            let integer_precision = matches!(
                precision,
                ChannelPrecision::Unorm8 | ChannelPrecision::Unorm16
            );
            let retained_alpha = integer_precision && channel == 5. && opacity != 0.;
            if opacity == 0. {
                // Existing planning omits zero-opacity effects entirely.
                // Keep this normal bypass quiet instead of forcing work to fuse.
                assert!(report.groups.is_empty());
                assert_eq!((report.original_passes, report.final_passes), (0, 0));
                assert!(report.retained.is_empty());
            } else if retained_alpha {
                assert!(report.groups.is_empty());
                assert_eq!((report.original_passes, report.final_passes), (7, 7));
                assert_eq!(
                    report
                        .retained
                        .get(&FusionRetainReason::QuantizedAlphaBoundary),
                    Some(&1)
                );
            } else {
                assert_eq!(
                    report.groups.len(),
                    1,
                    "{precision:?} case={case}: {report:?}"
                );
                assert_eq!((report.original_passes, report.final_passes), (7, 3));
                assert!(report.final_texture_traffic_bytes < report.original_texture_traffic_bytes);
                assert_eq!(report.groups[0].portable.static_loop_work, 1);
            }
            let worst = expected
                .iter()
                .zip(&actual)
                .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
                .fold(0_f64, f64::max);
            // Declared native linear-premultiplied absolute bounds include the
            // hardware's sRGB round trip and binary16 conversion boundaries.
            let tolerance = match precision {
                ChannelPrecision::Unorm8 => 0.012,
                ChannelPrecision::Unorm16 => 8. / 65535.,
                ChannelPrecision::Float16 => 0.005,
                ChannelPrecision::Float32 => 0.00002,
            };
            assert!(
                worst <= tolerance,
                "{precision:?} case={case} worst={worst} bound={tolerance}"
            );
            assert!(actual.iter().flatten().all(|v| v.is_finite()));
            if retained_alpha {
                assert_eq!(
                    actual, expected,
                    "retained alpha path must preserve every native pixel"
                );
            }
            let counters = (fused.effect_pipeline_builds(), report.portable_compiles);
            if case == 0 {
                steady_compiles = Some(counters);
            }
            if (1..=5).contains(&case) {
                assert_eq!(
                    counters.1,
                    steady_compiles.unwrap().1,
                    "numeric update recompiled portable group"
                );
                // The first retained chain can create the original native
                // pipelines that the fused path had not needed yet. Eligible
                // updates keep their execution policy and reuse native pipelines.
                if case <= 3 || !integer_precision {
                    assert_eq!(
                        Some(counters),
                        steady_compiles,
                        "eligible numeric update recompiled"
                    );
                }
            }
            if case >= 6 {
                assert_eq!(
                    report.portable_compiles, 2,
                    "only changed operation order specializes"
                );
            }
            report_rows.push(json!({"precision":precision,"case":case,"worst_linear_error":worst,
                "tolerance":tolerance,"fusion":report,
                "unfused_submit_and_readback_ns":unfused_ns,"fused_submit_and_readback_ns":fused_ns,
                "unfused_pipeline_builds":unfused.effect_pipeline_builds(),"fused_pipeline_builds":counters.0,
                "unfused_native_pipeline_build_ns":unfused.effect_pipeline_build_nanos(),
                "fused_native_pipeline_build_ns":fused.effect_pipeline_build_nanos(),
                "unfused_native_command_intervals":unfused_observer.as_ref().map(|o| o.snapshot()),
                "fused_native_command_intervals":fused_observer.as_ref().map(|o| o.snapshot()),
                "timing_scope":"command-encoder intervals include marker/scheduling overhead; null means unsupported/pending, not zero",
                "bandwidth_scope":"logical texture traffic upper bound; physical GPU bus counters unavailable"}));
        }
    }
    if let Ok(path) = std::env::var("MOTION_POINTWISE_ARTIFACTS") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            std::path::Path::new(&path).join("native-comparison.json"),
            serde_json::to_vec_pretty(&report_rows).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn neighborhood_and_unknown_implementations_keep_original_passes_with_a_reason() {
    for (boundary, position) in ["gaussian_blur", "turbulent_displace"]
        .into_iter()
        .flat_map(|name| [1, 2].map(|position| (name, position)))
    {
        let (mut p, _) = fixture(ChannelPrecision::Unorm8);
        p.layers[0].effects.insert(position, effect(13, boundary));
        p.rebuild_plugin_dependencies();
        let scene = scene(&p);
        let registry = motion_effects::Registry::new_with_builtins().unwrap();
        let mut original = PlanBuilder::new(registry.clone()).unwrap();
        let mut optimized = PlanBuilder::new(registry).unwrap();
        optimized.set_pointwise_fusion(true);
        let expected = original.build(&scene, &[1], W, H, true).unwrap();
        let expected_passes: Vec<_> = expected
            .passes
            .iter()
            .map(|p| (p.program, p.input, p.source, p.output, p.width, p.height))
            .collect();
        let actual = optimized.build(&scene, &[1], W, H, true).unwrap();
        assert_eq!(
            actual
                .passes
                .iter()
                .map(|p| (p.program, p.input, p.source, p.output, p.width, p.height))
                .collect::<Vec<_>>(),
            expected_passes
        );
        assert!(
            actual.diagnostics.is_empty(),
            "optimization refusal is not a render fault"
        );
        let report = optimized.pointwise_fusion_report();
        assert!(report.groups.is_empty());
        assert_eq!(
            report
                .retained
                .get(&FusionRetainReason::UnprovedImplementation),
            Some(&1)
        );
        if position == 2 {
            assert_eq!(
                report.retained.get(&FusionRetainReason::DownstreamBoundary),
                Some(&1)
            );
        }
    }
}

#[test]
fn actual_lifetimes_avoid_attachment_feedback_and_registry_replacement_retires_program_ids() {
    for precision in MODES {
        let (p, _) = fixture(precision);
        let scene = scene(&p);
        let registry = motion_effects::Registry::new_with_builtins().unwrap();
        let mut builder = PlanBuilder::new(registry.clone()).unwrap();
        builder.set_precision(precision);
        builder.set_pointwise_fusion(true);
        for replacement in [false, true] {
            if replacement {
                builder.set_registry(registry.clone());
            }
            let plan = builder.build(&scene, &[1], W, H, true).unwrap();
            assert_eq!(plan.passes.len(), 3);
            assert_eq!(plan.scratch_allocations.len(), 2);
            for binding in &plan.scratch_bindings {
                assert_ne!(binding.input, binding.output as i32);
                assert_ne!(binding.source, binding.output as i32);
            }
            let contract = motion_effects::pointwise::canonical_boundary(precision);
            assert!(
                plan.scratch_allocations
                    .iter()
                    .all(|a| a.class.format.storage() == contract.storage)
            );
            let report = builder.pointwise_fusion_report();
            assert_eq!(report.cache_entries, 1);
            assert_eq!(report.portable_compiles, if replacement { 2 } else { 1 });
            assert!(
                builder.programs[report.groups[0].program as usize]
                    .key
                    .starts_with("pointwise-v1")
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn hdr_neighbor_input_is_prepared_once_and_only_the_following_pointwise_chain_is_fused() {
    let package = reference::package();
    for precision in MODES {
        let (mut p, mut pixels) = fixture(precision);
        // The analytic glow produces its own varying alpha. Opaque input keeps
        // the neighborhood filter's peak genuinely HDR after the pointwise chain.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == "glow")
            .unwrap();
        let glow = EffectInstance::new(
            10,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [W as f32, H as f32],
        );
        scalar(&mut p.layers[0].effects[0], "p0003", 15.);
        scalar(&mut p.layers[0].effects[1], "channel", 4.);
        p.layers[0].effects.insert(0, glow);
        p.rebuild_plugin_dependencies();
        let mut registry = motion_effects::Registry::new_with_builtins().unwrap();
        registry.insert(package.clone()).unwrap();
        let mut original = renderer(precision);
        let mut optimized = renderer(precision);
        for r in [&mut original, &mut optimized] {
            r.set_effect_registry(registry.clone());
            r.upload_image(1, W, H, &pixels).unwrap();
        }
        optimized.set_pointwise_fusion(true);
        let (expected, _) = raw(&mut original, &p);
        let (actual, _) = raw(&mut optimized, &p);
        let report = optimized.pointwise_fusion_report();
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].effects, [11, 12]);
        assert_eq!(report.original_passes - report.final_passes, 4);
        assert_eq!(
            report.groups[0].portable.shader_texture_samples_upper_bound, 2,
            "upstream neighborhood sampling must not be expanded into the group"
        );
        assert_eq!(
            report
                .retained
                .get(&FusionRetainReason::UnprovedImplementation),
            Some(&1)
        );
        let worst = expected
            .iter()
            .zip(&actual)
            .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
            .fold(0_f64, f64::max);
        let bound = match precision {
            ChannelPrecision::Unorm8 => 0.012,
            ChannelPrecision::Unorm16 => 8. / 65535.,
            ChannelPrecision::Float16 => 0.008,
            ChannelPrecision::Float32 => 0.00004,
        };
        assert!(
            worst <= bound,
            "HDR {precision:?} worst={worst} bound={bound}"
        );
        if matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        ) {
            assert!(
                expected.iter().any(|p| p[0] > 1.1),
                "unfused fixture must exercise HDR: {precision:?}"
            );
            assert!(
                actual.iter().any(|p| p[0] > 1.1),
                "HDR must remain extended: {precision:?}"
            );
        }
    }
}
