use motion_core::{
    compositing::*,
    render_sampling::{ExposureClock, Shutter},
    Content, Ease, EffectImageInput, EffectImageStage, EffectInstance, ExpressionEvaluator, Layer,
    Project,
};
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes},
    LayerRaster, Renderer, ResourceBudgetScope, ResourceRejection, Scene,
};
use std::{sync::Arc, time::Instant};

fn project() -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    p
}
fn layer(id: u64, color: [f32; 4]) -> Layer {
    Layer::solid(id, "phase test", [32.; 2], [32., 32., 0.], color)
}
fn effect(name: &str, id: u64) -> EffectInstance {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == name)
        .unwrap();
    EffectInstance::new(
        id,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    )
}
fn adjustment(id: u64, name: &str) -> Layer {
    let mut a = layer(id, [1.; 4]);
    a.size = [64.; 2];
    a.content = Content::Adjustment;
    a.effects.push(effect(name, 1));
    a
}
fn sample(p: &mut Project, frame: f64) -> Scene {
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    scene(p, frame)
}
fn scene(p: &Project, frame: f64) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &ExpressionEvaluator).unwrap();
    s
}
fn retained(r: &mut Renderer, s: &Scene, w: u32, h: u32, preview: bool) -> Vec<LayerRaster> {
    s.layers
        .iter()
        .filter(|l| !l.adjustment)
        .map(|l| {
            r.materialize_layer(s, l.id, w, h, preview)
                .unwrap()
                .unwrap()
                .0
        })
        .collect()
}
fn reference(r: &mut Renderer, s: &Scene, w: u32, h: u32, preview: bool) -> Vec<u8> {
    let target = r.capture_target(w, h).unwrap();
    if preview {
        let mut encoder = r.device.create_command_encoder(&Default::default());
        r.encode_preview(s, &target.view, w, h, &mut encoder, None)
            .unwrap();
        r.submit_tracked(encoder.finish(), |_| {});
        r.read_target(&target).unwrap()
    } else {
        r.capture(s, &target).unwrap().0
    }
}
fn draw(
    r: &mut Renderer,
    s: &Scene,
    rasters: &[LayerRaster],
    w: u32,
    h: u32,
    preview: bool,
) -> Vec<u8> {
    let target = r.capture_target(w, h).unwrap();
    let refs: Vec<_> = rasters.iter().rev().collect();
    r.draw_exposure_phase(s, &refs, &target.view, w, h, preview)
        .unwrap();
    r.read_target(&target).unwrap()
}
fn equal(a: &[u8], b: &[u8], context: &str) {
    assert_eq!(a.len(), b.len());
    let (i, d) = a
        .iter()
        .zip(b)
        .enumerate()
        .map(|(i, (a, b))| (i, a.abs_diff(*b)))
        .max_by_key(|(_, d)| *d)
        .unwrap();
    assert!(
        d <= 1,
        "{context}: difference={d} at byte={i}, actual={} expected={}",
        a[i],
        b[i]
    );
}
fn pixel(p: &[u8], x: usize, y: usize) -> &[u8] {
    &p[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn static_adjustments_match_original_opacity_background_upper_and_letterboxing() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    p.background = [0., 0., 1., 1.];
    let mut lower = layer(1, [1., 0., 0., 0.5]);
    lower.transform.position.value[0] = 20.;
    let a = adjustment(2, "tint");
    let mut upper = layer(3, [0., 1., 0., 1.]);
    upper.size = [8.; 2];
    upper.transform.position.value[0] = 48.;
    p.layers = vec![lower, a, upper];
    for opacity in [1., 0., 0.5] {
        p.layers[1].transform.opacity.value = opacity;
        let s = sample(&mut p, 5.);
        for (w, h) in [(64, 64), (80, 60), (96, 96)] {
            for preview in [false, true] {
                let expected = reference(&mut r, &s, w, h, preview);
                let images = retained(&mut r, &s, w, h, preview);
                let actual = draw(&mut r, &s, &images, w, h, preview);
                equal(&actual, &expected, "static opacity/quality/letterbox");
                if (w, h) == (64, 64) {
                    assert_eq!(pixel(&actual, 48, 32), [0, 255, 0, 255]);
                    assert_eq!(pixel(&actual, 60, 60), [0, 0, 255, 255]);
                }
            }
        }
    }
    assert_eq!(r.exposure_texture_bytes(), 0);
    r.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn stacked_parented_adjustments_preserve_crossing_planes_mattes_blends_and_bgra() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut r = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ))
    .unwrap();
    let mut rgba = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    let mut matte = layer(1, [1., 1., 1., 0.7]);
    matte.visible = false;
    matte.size = [22., 38.];
    p.layers.push(matte);
    for (id, color, angle) in [(2, [1., 0., 0., 0.6], 35.), (3, [0., 0.4, 1., 0.5], -35.)] {
        let mut l = layer(id, color);
        l.size = [56.; 2];
        l.three_d = true;
        l.transform.rotation.value[1] = angle;
        if id == 2 {
            l.track_matte = Some(TrackMatte {
                source: 1,
                mode: MatteMode::AlphaInverted,
                hide_source: false,
            });
        }
        if id == 3 {
            l.blend = LayerBlend {
                mode: BlendMode::Multiply,
                space: BlendSpace::Srgb,
            };
        }
        p.layers.push(l);
    }
    let mut parent = layer(4, [1.; 4]);
    parent.content = Content::Null;
    parent.transform.rotation.value[2] = 20.;
    p.layers.push(parent);
    let mut first = adjustment(5, "tint");
    first.parent = Some(motion_core::ParentLink {
        object: Some(4),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    first.size = [24., 18.];
    first.transform.rotation.value[2] = 15.;
    first.transform.opacity.value = 0.5;
    first.effects[0]
        .params
        .get_mut("p0002")
        .unwrap()
        .track
        .value = [1., 0., 0., 1.];
    let mut second = adjustment(6, "gaussian_blur");
    second.effects[0]
        .params
        .get_mut("p0001")
        .unwrap()
        .track
        .value[0] = 2.;
    p.layers.extend([first, second]);
    for swap in [false, true] {
        if swap {
            p.layers.swap(4, 5);
        }
        let s = sample(&mut p, 0.);
        for (w, h) in [(64, 64), (80, 60)] {
            let expected = reference(&mut r, &s, w, h, false);
            equal(
                &expected,
                &reference(&mut rgba, &s, w, h, false),
                "BGRA/RGBA channel agreement",
            );
            assert!(expected.chunks_exact(4).any(|v| v[3] > 0));
            let images = retained(&mut r, &s, w, h, false);
            equal(
                &draw(&mut r, &s, &images, w, h, false),
                &expected,
                "BGRA parent/3D/matte/blend/stack",
            );
        }
    }
    r.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn receiver_exposure_keeps_adjustment_parameters_and_effect_inputs_at_center_time() {
    let mut p = project();
    let shutter = Shutter {
        frames: 1.,
        samples: 8,
    };
    let source_shutter = Shutter {
        frames: 1.,
        samples: 3,
    };
    let mut moving = layer(1, [1., 0., 0., 1.]);
    moving.size = [8.; 2];
    moving.sampling.motion_blur = Some(shutter);
    let mut sharp = layer(4, [0., 1., 0., 1.]);
    sharp.size = [8.; 2];
    for (l, y) in [(&mut moving, 16.), (&mut sharp, 48.)] {
        for (f, x) in [(0, 14.), (1, 50.)] {
            l.transform
                .position
                .upsert(f, [x, y, 0.], Ease::Linear)
                .unwrap();
        }
    }
    let mut a = adjustment(2, "gaussian_blur");
    a.size = [64., 32.];
    a.transform.position.value[1] = 16.;
    for (f, value) in [(0, 1.), (1, 3.)] {
        a.effects[0]
            .params
            .get_mut("p0001")
            .unwrap()
            .track
            .upsert(f, [value, 0., 0., 0.], Ease::Linear)
            .unwrap();
    }
    let mut displacement = effect("displacement_map", 2);
    displacement.image_input = Some(EffectImageInput::Layer {
        layer: 3,
        stage: EffectImageStage::Effects,
    });
    displacement.params.get_mut("amount").unwrap().track.value = [4., 0., 0., 0.];
    a.effects.push(displacement);
    let mut source = layer(3, [0.8, 0.2, 0.1, 1.]);
    source.visible = false;
    source.size = [64.; 2];
    source.sampling.motion_blur = Some(source_shutter);
    source.effects.push(effect("brightness_contrast", 1));
    for (f, value) in [(0, -20.), (1, 20.)] {
        source.effects[0]
            .params
            .get_mut("p0001")
            .unwrap()
            .track
            .upsert(f, [value, 0., 0., 0.], Ease::Linear)
            .unwrap();
    }
    p.layers = vec![moving, a, source, sharp];
    let center = sample(&mut p, 0.5);
    let clock = ExposureClock::new(0.5, &[(1, shutter), (3, source_shutter)], false).unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|phase| phase.weight).collect();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let original = reference(&mut r, &center, 64, 64, false);
    let mut expected_target = r.exposure_target(64, 64, &weights).unwrap();
    let mut wrong_target = r.exposure_target(64, 64, &weights).unwrap();
    for i in 0..weights.len() {
        let s = scene(&p, clock.layer_frame(1, i).unwrap());
        let mut correct = center.clone();
        // Independent ordinary-renderer oracle: only the receiver plane moves;
        // adjustment parameters and its hidden dependency stay at the center.
        *correct.layers.iter_mut().find(|l| l.id == 1).unwrap() =
            s.layers.iter().find(|l| l.id == 1).unwrap().clone();
        r.accumulate_exposure(&mut expected_target, Some(&correct), false)
            .unwrap();
        r.accumulate_exposure(&mut wrong_target, Some(&s), false)
            .unwrap();
    }
    let expected = r.capture_exposure(&expected_target).unwrap();
    let wrong = r.capture_exposure(&wrong_target).unwrap();
    assert_ne!(
        expected, wrong,
        "oracle must distinguish whole-scene averaging"
    );
    drop(expected_target);
    drop(wrong_target);
    let document = ExposureDocument::new(Arc::new(p)).unwrap();
    let root = document.composition_id.clone();
    let mut task =
        ExposureScenes::new(document, &root, 0.5, false, None, ExpressionEvaluator).unwrap();
    let mut target = r.exposure_target(64, 64, &weights).unwrap();
    let mut images = Vec::new();
    let mut peak = 0;
    let mut draws = 0;
    let started = Instant::now();
    loop {
        match task.current().unwrap() {
            ExposureSceneStep::Receiver(receiver) => {
                if !receiver.layer().is_some_and(|l| l.adjustment) {
                    if let Some(s) = receiver.scene {
                        if let Some((image, stats)) = r
                            .materialize_layer(s, receiver.object, 64, 64, false)
                            .unwrap()
                        {
                            assert_eq!(image.frame(), receiver.frame);
                            draws += stats.draw_calls;
                            images.push(image);
                        }
                    }
                }
                peak = peak.max(r.exposure_texture_bytes());
                task.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { index, .. } => {
                let refs: Vec<_> = images.iter().collect();
                draws += r
                    .accumulate_exposure_phase(&mut target, &center, &refs, false)
                    .unwrap()
                    .draw_calls;
                assert_eq!(target.phase_index(), index + 1);
                images.clear();
                task.finish_phase().unwrap();
            }
            ExposureSceneStep::Complete => break,
        }
    }
    let actual = r.capture_exposure(&target).unwrap();
    let capture_us = started.elapsed().as_micros();
    equal(
        &actual,
        &expected,
        "actual receiver schedule with center adjustment and input",
    );
    equal(
        &actual[40 * 64 * 4..],
        &original[40 * 64 * 4..],
        "unblurred animated upper",
    );
    let support = |v: &[u8]| (0..64).filter(|x| v[(16 * 64 + x) * 4 + 3] > 1).count();
    assert!(support(&actual) > support(&original) * 2);
    let area = |v: &[u8]| {
        v[..32 * 64 * 4]
            .chunks_exact(4)
            .map(|v| u64::from(v[3]))
            .sum::<u64>()
    };
    assert!((area(&actual) as f64 / area(&original) as f64 - 1.).abs() < 0.03);
    if let Ok(path) = std::env::var("MOTION_SAMPLING_ARTIFACT_DIR") {
        std::fs::create_dir_all(&path).unwrap();
        for (name, pixels) in [
            ("adjustment-sharp.png", &original),
            ("adjustment-exposure.png", &actual),
        ] {
            image::save_buffer(
                std::path::Path::new(&path).join(name),
                pixels,
                64,
                64,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        std::fs::write(std::path::Path::new(&path).join("adjustment-cost.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"adapter":format!("{:?}",r.adapter_info),
                "phases":target.phase_count(),"capture_us":capture_us,"peak_temporal_texture_bytes":peak,
                "effect_and_plane_draws":draws,"output":[64,64]})).unwrap()).unwrap();
    }
    drop(target);
    assert_eq!(r.exposure_texture_bytes(), 0);
    r.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn empty_backdrop_and_inactive_adjustment_do_not_paint_background() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    p.background = [0.2, 0.4, 0.8, 0.6];
    p.layers.push(adjustment(1, "tint"));
    for active in [true, false] {
        p.layers[0].effects[0].enabled = active;
        let s = sample(&mut p, 2.);
        let expected = reference(&mut r, &s, 64, 64, false);
        equal(
            &draw(&mut r, &s, &[], 64, 64, false),
            &expected,
            "empty backdrop",
        );
        assert!(expected.chunks_exact(4).all(|v| v == &expected[..4]));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn preparation_failures_preserve_original_cause_phase_and_existing_images() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    p.layers = vec![layer(1, [0.7, 0.2, 0.3, 1.]), adjustment(2, "tint")];
    let s = sample(&mut p, 6.);
    let expected = reference(&mut r, &s, 64, 64, false);
    let mut bad = s.clone();
    bad.effects[0].effect = "controlled_missing_adjustment".into();
    for preview in [true, false] {
        let images = retained(&mut r, &s, 64, 64, preview);
        let refs: Vec<_> = images.iter().collect();
        let mut target = r.exposure_target(64, 64, &[1.]).unwrap();
        let mismatch = r
            .accumulate_exposure_phase(&mut target, &s, &refs, !preview)
            .unwrap_err()
            .to_string();
        assert!(mismatch.contains("preview policy"), "{mismatch}");
        assert_eq!(target.phase_index(), 0);
        let error = r
            .accumulate_exposure_phase(&mut target, &bad, &refs, preview)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("controlled_missing_adjustment")
                && error.contains("adjustment_prepare")
                && error.contains("sample_frame=6"),
            "{error}"
        );
        assert_eq!(target.phase_index(), 0);
        assert!(r.capture_exposure(&target).is_err());
        r.accumulate_exposure_phase(&mut target, &s, &refs, preview)
            .unwrap();
        equal(
            &r.capture_exposure(&target).unwrap(),
            &expected,
            "explicit recovery after failure",
        );
    }
    assert_eq!(r.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn receiver_missing_at_center_can_enable_phase_blending_without_losing_adjustment_bindings() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    let mut bottom = layer(1, [0.1, 0.3, 0.8, 0.7]);
    bottom.size = [48.; 2];
    let mut later = layer(2, [1., 0.2, 0.1, 0.8]);
    later.timeline = Some(motion_core::LayerTimeline {
        in_frame: 1,
        out_frame: 20,
        offset_frame: 0,
    });
    later.blend = LayerBlend {
        mode: BlendMode::Screen,
        space: BlendSpace::Srgb,
    };
    let shutter = Shutter {
        frames: 1.,
        samples: 8,
    };
    later.sampling.motion_blur = Some(shutter);
    let mut a = adjustment(3, "gaussian_blur");
    a.effects[0].params.get_mut("p0001").unwrap().track.value[0] = 3.;
    p.layers = vec![bottom, later, a];
    let center = sample(&mut p, 0.75);
    assert!(!center.layers.iter().any(|l| l.id == 2));
    let clock = ExposureClock::new(0.75, &[(2, shutter)], false).unwrap();
    let weights: Vec<_> = clock.phases().iter().map(|v| v.weight).collect();
    let mut expected_target = r.exposure_target(64, 64, &weights).unwrap();
    for i in 0..weights.len() {
        let at_time = scene(&p, clock.layer_frame(2, i).unwrap());
        let mut phase = center.clone();
        if let Some(l) = at_time.layers.iter().find(|l| l.id == 2) {
            phase.layers.push(l.clone());
        }
        phase.layers.sort_by_key(|l| l.order);
        r.accumulate_exposure(&mut expected_target, Some(&phase), false)
            .unwrap();
    }
    let expected = r.capture_exposure(&expected_target).unwrap();
    drop(expected_target);
    let mut target = r.exposure_target(64, 64, &weights).unwrap();
    let mut present = 0;
    for i in 0..weights.len() {
        let at_time = scene(&p, clock.layer_frame(2, i).unwrap());
        let mut images = retained(&mut r, &center, 64, 64, false);
        if let Some((image, _)) = r.materialize_layer(&at_time, 2, 64, 64, false).unwrap() {
            present += 1;
            images.push(image);
        }
        let refs: Vec<_> = images.iter().collect();
        r.accumulate_exposure_phase(&mut target, &center, &refs, false)
            .unwrap();
    }
    assert!(present > 0 && present < weights.len());
    let actual = r.capture_exposure(&target).unwrap();
    equal(
        &actual,
        &expected,
        "center-missing receiver changes transfer allocation",
    );
    assert_ne!(actual, reference(&mut r, &center, 64, 64, false));
    r.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn nested_dependency_failure_is_not_bypassed_in_preview_and_valid_child_recovers() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    let mut child = layer(10, [0.8, 0.2, 0.3, 1.]);
    child.effects.push(effect("tint", 1));
    p.compositions.push(motion_core::Composition {
        id: "comp-adjustment-input".into(),
        name: "center dependency".into(),
        width: 64,
        height: 64,
        fps: 30,
        frames: 120,
        work_area: motion_core::WorkArea::full(120),
        guides: Vec::new(),
        background: [0.; 4],
        camera: motion_core::Camera::new(64, 64),
        layers: vec![child],
        expressions: vec![],
    });
    let mut reference_layer = layer(1, [1.; 4]);
    reference_layer.content = Content::Composition {
        clip: motion_core::CompositionClip::new("comp-adjustment-input".into()),
    };
    p.layers = vec![reference_layer, adjustment(2, "tint")];
    let center = sample(&mut p, 3.);
    let mut bad = center.clone();
    std::sync::Arc::make_mut(&mut bad.nested[0].scene).effects[0].effect =
        "controlled_nested_effect_failure".into();
    for preview in [true, false] {
        let expected = reference(&mut r, &center, 64, 64, preview);
        let images = retained(&mut r, &center, 64, 64, preview);
        let refs: Vec<_> = images.iter().collect();
        let mut target = r.exposure_target(64, 64, &[1.]).unwrap();
        let error = r
            .accumulate_exposure_phase(&mut target, &bad, &refs, preview)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("comp-adjustment-input")
                && error.contains("controlled_nested_effect_failure")
                && error.contains("adjustment_prepare"),
            "{error}"
        );
        assert_eq!(target.phase_index(), 0);
        assert!(r.capture_exposure(&target).is_err());
        r.accumulate_exposure_phase(&mut target, &center, &refs, preview)
            .unwrap();
        equal(
            &r.capture_exposure(&target).unwrap(),
            &expected,
            "valid nested dependency after refusal",
        );
    }
    r.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn adjustment_accumulator_budget_is_rejected_before_allocation() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = project();
    p.layers = vec![layer(1, [1., 0., 0., 1.]), adjustment(2, "tint")];
    let s = sample(&mut p, 0.);
    let images = retained(&mut r, &s, 64, 64, false);
    let refs: Vec<_> = images.iter().collect();
    let original = r.memory_policy();
    let mut reduced = original;
    reduced.source_texture_budget_bytes = 16384;
    r.set_memory_policy(reduced).unwrap();
    let before = r.texture_resource_ledger().total_known_bytes();
    let mut target = r.exposure_target(64, 64, &[1.]).unwrap();
    let error = r
        .accumulate_exposure_phase(&mut target, &s, &refs, false)
        .unwrap_err();
    assert!(
        matches!(
            error.resource_rejection(),
            Some(ResourceRejection::Budget {
                scope: ResourceBudgetScope::LayerCompositing,
                required_bytes: 32768,
                budget_bytes: 16384
            })
        ),
        "{error}"
    );
    assert_eq!(r.texture_resource_ledger().total_known_bytes(), before);
    assert_eq!(target.phase_index(), 0);
    r.set_memory_policy(original).unwrap();
    let expected = reference(&mut r, &s, 64, 64, false);
    r.accumulate_exposure_phase(&mut target, &s, &refs, false)
        .unwrap();
    equal(
        &r.capture_exposure(&target).unwrap(),
        &expected,
        "budget recovery",
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dependency_plane_and_sum_submissions_keep_retained_inputs_and_output_in_ledger() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    r.set_submission_tracking(true);
    let mut p = project();
    p.layers = vec![
        layer(1, [0.3, 0.7, 0.2, 1.]),
        adjustment(2, "gaussian_blur"),
    ];
    p.layers[1].effects[0]
        .params
        .get_mut("p0001")
        .unwrap()
        .track
        .value[0] = 3.;
    let s = sample(&mut p, 0.);
    let images = retained(&mut r, &s, 64, 64, false);
    let mut target = r.exposure_target(64, 64, &[1.]).unwrap();
    r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mut current = r.texture_resource_ledger();
    current.merge(target.texture_resource_ledger());
    current.merge(images[0].texture_resource_ledger());
    let before = r.submission_memory_info(&current);
    r.accumulate_exposure_phase(&mut target, &s, &[&images[0]], false)
        .unwrap();
    r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let after = r.submission_memory_info(&current);
    assert_eq!(after.submitted, before.submitted + 3);
    assert_eq!(after.completed, after.submitted);
    assert_eq!(after.pending_submissions, 0);
    assert!(after.peak_known_pending_bytes >= current.total_known_bytes());
    assert_eq!(after.known_retired_bytes, 0);
    assert_eq!(after.skipped_snapshots, 0);
    assert_eq!(target.phase_index(), 1);
}
