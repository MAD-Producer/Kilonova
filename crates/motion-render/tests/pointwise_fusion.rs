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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn registry_code_revalidation_preserves_native_and_portable_fusion_in_four_precisions() {
    let mut rows = vec![];
    for precision in MODES {
        let (mut project, pixels) = fixture(precision);
        // Invert -> Tint is proved in every mode, including UNORM16.
        project.layers[0].effects.reverse();
        let mut r = renderer(precision);
        r.upload_image(1, W, H, &pixels).unwrap();
        r.set_pointwise_fusion(true);
        let (before, _) = raw(&mut r, &project);
        let native = r.effect_pipeline_builds();
        let portable = r.pointwise_fusion_report().portable_compiles;
        assert_eq!(portable, 1);
        assert_eq!(r.pointwise_fusion_report().groups.len(), 1);
        let mut registry = motion_effects::Registry::new_with_builtins().unwrap();
        // Use independently parsed successful packages, not the pointer fast path.
        for package in registry.packages.values_mut() {
            *package = std::sync::Arc::new(
                motion_effects::EffectPackage::from_bytes(package.bytes.clone()).unwrap(),
            );
        }
        // Two updates before preparation must not erase the last prepared code.
        r.set_effect_registry(registry.clone());
        let key = registry.packages.keys().next().unwrap().clone();
        registry.disabled.insert(key.clone());
        r.set_effect_registry(registry.clone());
        registry.disabled.remove(&key);
        r.set_effect_registry(registry);
        let (after, _) = raw(&mut r, &project);
        assert_eq!(after, before);
        assert_eq!(
            r.effect_pipeline_builds(),
            native,
            "unchanged native fusion must survive program-ID reassignment"
        );
        let report = r.pointwise_fusion_report();
        assert_eq!(report.portable_compiles, portable);
        assert_eq!(report.portable_reuses, 1);
        assert_eq!(report.groups.len(), 1);
        assert!(report.retired_code_bytes <= report.retired_code_byte_limit);
        let cache = r.effect_pipeline_cache_info();
        assert!(cache.reuses > 0);
        rows.push(json!({"precision":precision,"pixels_equal":true,
            "new_native_builds":0,"new_portable_compiles":0,"fusion":report,"native_cache":cache}));
    }
    if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            std::path::PathBuf::from(out).join("structural-fusion-reuse.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unorm16_transfer_order_retains_native_stores_and_reverse_order_still_fuses() {
    let (mut project, mut pixels) = fixture(ChannelPrecision::Unorm16);
    // The color of the worst 1080p counterexample. Keep this fast native guard
    // test in the ordinary gate; the explicit cost example retains the complete
    // dense pattern and the failed pre-guard receipts retain its actual error.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 3, 15, 255]);
    }
    scalar(&mut project.layers[0].effects[0], "p0003", 71.);
    let mut original = renderer(ChannelPrecision::Unorm16);
    let mut optimized = renderer(ChannelPrecision::Unorm16);
    for r in [&mut original, &mut optimized] {
        r.upload_image(1, W, H, &pixels).unwrap();
    }
    optimized.set_pointwise_fusion(true);
    let (expected, _) = raw(&mut original, &project);
    let (actual, _) = raw(&mut optimized, &project);
    assert_eq!(actual, expected);
    let report = optimized.pointwise_fusion_report();
    assert_eq!((report.original_passes, report.final_passes), (7, 7));
    assert!(report.groups.is_empty());
    assert_eq!(report.portable_compiles, 0);
    assert_eq!(
        report
            .retained
            .get(&FusionRetainReason::QuantizedTransferBoundary),
        Some(&1)
    );
    project.layers[0].effects.reverse();
    let (expected, _) = raw(&mut original, &project);
    let (actual, _) = raw(&mut optimized, &project);
    let report = optimized.pointwise_fusion_report();
    assert_eq!((report.original_passes, report.final_passes), (7, 3));
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.portable_compiles, 1);
    let worst = expected
        .iter()
        .zip(&actual)
        .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
        .fold(0_f64, f64::max);
    assert!(worst <= 8. / 65535., "reverse UNORM16 error={worst}");
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
            let retained_alpha = channel == 5. && opacity != 0.;
            let retained_transfer = precision == ChannelPrecision::Unorm16
                && !reverse
                && opacity != 0.
                && !retained_alpha;
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
                    report.retained.get(&FusionRetainReason::AlphaBoundary),
                    Some(&1)
                );
            } else if retained_transfer {
                assert!(report.groups.is_empty());
                assert_eq!((report.original_passes, report.final_passes), (7, 7));
                assert_eq!(
                    report
                        .retained
                        .get(&FusionRetainReason::QuantizedTransferBoundary),
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
            if retained_alpha || retained_transfer {
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
                if case <= 3 {
                    assert_eq!(
                        Some(counters),
                        steady_compiles,
                        "eligible numeric update recompiled"
                    );
                }
            }
            if case >= 6 {
                assert_eq!(
                    report.portable_compiles,
                    if precision == ChannelPrecision::Unorm16 {
                        1
                    } else {
                        2
                    },
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
        let (mut p, _) = fixture(precision);
        // Keep a fused lifetime case in every mode; the forward UNORM16
        // transfer boundary is separately required to retain exact pixels.
        if precision == ChannelPrecision::Unorm16 {
            p.layers[0].effects.reverse();
        }
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
            assert_eq!(
                report.portable_compiles, 1,
                "registry-local IDs may change without recompiling identical proved code"
            );
            assert_eq!(report.portable_reuses, u64::from(replacement));
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
        if precision == ChannelPrecision::Unorm16 {
            assert!(report.groups.is_empty());
            assert_eq!(report.original_passes, report.final_passes);
            assert_eq!(
                report
                    .retained
                    .get(&FusionRetainReason::QuantizedTransferBoundary),
                Some(&1)
            );
            assert_eq!(
                actual, expected,
                "retained transfer stores preserve every native pixel"
            );
        } else {
            assert_eq!(report.groups.len(), 1);
            assert_eq!(report.groups[0].effects, [11, 12]);
            assert_eq!(report.original_passes - report.final_passes, 4);
            assert_eq!(
                report.groups[0].portable.shader_texture_samples_upper_bound, 2,
                "upstream neighborhood sampling must not be expanded into the group"
            );
        }
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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn effected_resource_and_matte_consumers_keep_the_producers_real_stores() {
    for precision in MODES {
        for matte in [false, true] {
            let (mut project, pixels) = fixture(precision);
            // Use an otherwise eligible producer even in UNORM16.
            if precision == ChannelPrecision::Unorm16 {
                project.layers[0].effects.reverse();
            }
            project.layers[0].visible = false;
            let mut receiver = Layer::solid(
                2,
                "Cross-layer receiver",
                [W as f32, H as f32],
                [W as f32 / 2., H as f32 / 2., 0.],
                [1.; 4],
            );
            receiver.content = Content::Image { asset: 1 };
            if matte {
                receiver.track_matte = Some(motion_core::compositing::TrackMatte {
                    source: 1,
                    mode: motion_core::compositing::MatteMode::Luma,
                    hide_source: true,
                });
            } else {
                let mut displacement = effect(21, "displacement_map");
                displacement.image_input = Some(motion_core::EffectImageInput::Layer {
                    layer: 1,
                    stage: motion_core::EffectImageStage::Effects,
                });
                receiver.effects.push(displacement);
            }
            project.layers.push(receiver);
            project.rebuild_plugin_dependencies();
            let mut original = renderer(precision);
            let mut optimized = renderer(precision);
            for r in [&mut original, &mut optimized] {
                r.upload_image(1, W, H, &pixels).unwrap();
            }
            optimized.set_pointwise_fusion(true);
            let (expected, _) = raw(&mut original, &project);
            let (actual, _) = raw(&mut optimized, &project);
            assert!(
                expected.iter().any(|p| p[3] > 0.1),
                "fixture rendered no receiver"
            );
            let report = optimized.pointwise_fusion_report();
            assert!(
                report.groups.is_empty(),
                "unproved cross-layer consumer must retain producer: {precision:?} matte={matte} {report:?}"
            );
            assert_eq!(report.original_passes, report.final_passes);
            assert_eq!(report.portable_compiles, 0);
            assert_eq!(
                report
                    .retained
                    .get(&FusionRetainReason::ExternalConsumerBoundary),
                Some(&1)
            );
            assert_eq!(
                actual, expected,
                "producer stores retained exactly: {precision:?} matte={matte}"
            );
            if !matte {
                project.layers[0].visible = true;
                project.layers[1].effects[0].image_input =
                    Some(motion_core::EffectImageInput::Layer {
                        layer: 1,
                        stage: motion_core::EffectImageStage::Source,
                    });
                let sampled = scene(&project);
                let mut builder =
                    PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap())
                        .unwrap();
                builder.set_precision(precision);
                builder.set_pointwise_fusion(true);
                builder.build(&sampled, &[1], W, H, true).unwrap();
                assert_eq!(
                    builder.pointwise_fusion_report().groups.len(),
                    1,
                    "raw-source readers do not consume the producer's effected output"
                );
                assert_eq!(builder.pointwise_fusion_report().groups[0].object, 1);
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn transfer_blends_and_adjustment_backdrops_retain_unproved_compositing_boundaries() {
    use motion_core::compositing::{BlendMode, BlendSpace, LayerBlend};
    for precision in MODES {
        for boundary in ["blend", "space", "adjustment"] {
            let (mut project, pixels) = fixture(precision);
            if precision == ChannelPrecision::Unorm16 {
                project.layers[0].effects.reverse();
            }
            let mut consumer = Layer::solid(
                2,
                "Compositing consumer",
                [W as f32, H as f32],
                [W as f32 / 2., H as f32 / 2., 0.],
                [0.4, 0.2, 0.3, 0.8],
            );
            match boundary {
                "blend" => {
                    consumer.blend = LayerBlend {
                        mode: BlendMode::ColorDodge,
                        space: BlendSpace::Linear,
                    }
                }
                "space" => {
                    consumer.blend = LayerBlend {
                        mode: BlendMode::Normal,
                        space: BlendSpace::Srgb,
                    }
                }
                _ => {
                    consumer.content = Content::Adjustment;
                    consumer.effects.push(effect(21, "invert"));
                }
            }
            project.layers.push(consumer);
            project.rebuild_plugin_dependencies();
            let mut original = renderer(precision);
            let mut optimized = renderer(precision);
            for r in [&mut original, &mut optimized] {
                r.upload_image(1, W, H, &pixels).unwrap();
            }
            optimized.set_pointwise_fusion(true);
            let (expected, _) = raw(&mut original, &project);
            let (actual, _) = raw(&mut optimized, &project);
            assert!(
                expected
                    .iter()
                    .chain(&actual)
                    .flatten()
                    .all(|v| v.is_finite())
            );
            assert!(expected.iter().any(|p| p[3] > 0.1));
            let report = optimized.pointwise_fusion_report();
            assert!(
                report.groups.is_empty(),
                "{precision:?} {boundary} {report:?}"
            );
            assert_eq!(report.original_passes, report.final_passes);
            assert_eq!(report.portable_compiles, 0);
            assert!(
                report
                    .retained
                    .get(&FusionRetainReason::CompositingBoundary)
                    .copied()
                    .unwrap_or(0)
                    >= 1
            );
            assert_eq!(
                actual, expected,
                "retained compositing: {precision:?} {boundary}"
            );
        }
    }
}
