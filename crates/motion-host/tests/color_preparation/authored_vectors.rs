//! Original CPU references through sampled fills/strokes, groups and raster GPU.
use super::*;
use motion_core::{
    vector::{groups::VectorGroup, LineCap, LineJoin, ShapeKind, Stroke, VectorContent},
    Content, Track,
};

fn vector_layer(id: u64, center: [f32; 3], rgba: [f32; 4], stroke: bool, group: bool) -> Layer {
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    vector.fill = (!stroke).then(|| Track::constant(rgba));
    vector.stroke = stroke.then(|| Stroke {
        color: Track::constant(rgba),
        width: Track::constant(6.),
        cap: LineCap::Butt,
        join: LineJoin::Miter,
        miter_limit: 4.,
        dashes: None,
    });
    if group {
        vector = VectorGroup::wrap(vector, [8., 48.]);
    }
    let mut layer = Layer::solid(id, "OCIO vector reference tile", [8., 48.], center, [1.; 4]);
    layer.content = Content::Vector { vector };
    layer
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn native_vector_fills_strokes_and_grouped_paints_match_original_cpu_pixels() {
    run_references(false);
}
#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn finite_vector_codes_preserve_original_negative_hdr_and_alpha_references() {
    run_references(true);
}
fn run_references(finite_range: bool) {
    let reference = reference();
    let mut renderer = renderer();
    let mut checked = 0;
    let (mut negative, mut hdr) = (0, 0);
    for (index, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let mut p = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        let mut input = case.input.clone();
        if finite_range {
            // Exact power-of-two coding puts original [-0.3, 256] samples
            // inside valid vector controls. The CPU oracle remains unchanged.
            let black = 1. / 1024.;
            settings(&mut p).authored_input.signal.range = SignalRange::Limited {
                black,
                white: 3. / 1024.,
            };
            for pixel in &mut input {
                let factor = if case.processor.input_alpha == AlphaMode::Premultiplied {
                    pixel[3]
                } else {
                    1.
                };
                for value in &mut pixel[..3] {
                    *value = *value / 512. + black as f32 * factor;
                }
                assert!(pixel.iter().all(|value| (0.0..=1.0).contains(value)));
            }
        }
        let mut probes = Vec::new();
        for (sample, rgba) in input
            .iter()
            .enumerate()
            .filter(|(_, p)| p.iter().all(|v| (0.0..=1.0).contains(v)))
        {
            for group in [false, true] {
                for stroke in [false, true] {
                    let tile = p.layers.len() as u32;
                    let x = tile * 12 + 6;
                    p.layers.push(vector_layer(
                        tile as u64 + 1,
                        [x as f32, 32., 0.],
                        *rgba,
                        stroke,
                        group,
                    ));
                    probes.push((
                        sample,
                        x,
                        if stroke { 9 } else { 32 },
                        premult(case.expected[sample], case.processor.output_alpha),
                    ));
                }
            }
        }
        p.width = p.layers.len() as u32 * 12;
        p.validate().unwrap();
        let before = serde_json::to_value(&p).unwrap();
        let mut prepared = prep::prepare_native_authored_project(
            &p,
            root.path(),
            &runtime(),
            &Observation::new(9000 + index as u64),
        )
        .unwrap()
        .unwrap();
        let frame = prepared
            .authored
            .sample(&p, 0.25, &ExpressionEvaluator)
            .unwrap();
        let target = renderer
            .color_image_target(p.width, p.height, frame.contract())
            .unwrap();
        let readbacks = renderer.image_readback_info();
        renderer.draw_authored_scene(frame, &target).unwrap();
        assert_eq!(
            renderer.image_readback_info(),
            readbacks,
            "vector composition must not read back intermediate images"
        );
        let pixels = read(&renderer, &target);
        for (sample, x, y, expected) in &probes {
            let actual = pixels[(y * p.width + x) as usize];
            assert!(
                close(actual, *expected),
                "case={index} sample={sample} x={x} y={y} actual={actual:?} expected={expected:?}"
            );
            checked += 1;
            negative += expected[..3].iter().filter(|value| **value < 0.).count();
            hdr += expected[..3].iter().filter(|value| **value > 1.).count();
        }
        let warm = renderer
            .vector_upload_info()
            .unwrap()
            .encoded_upload_bytes_total;
        renderer.draw_authored_scene(frame, &target).unwrap();
        assert_eq!(
            renderer
                .vector_upload_info()
                .unwrap()
                .encoded_upload_bytes_total,
            warm,
            "same frame reuses vector rasters"
        );
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        // Native programs and sampled paint colors are retained; the next frame
        // needs neither configuration IO nor another runtime preparation.
        let resource = &p.color_management.resources().unwrap()[0];
        std::fs::remove_file(root.path().join(&resource.path)).unwrap();
        for time in [0.5, 0.75] {
            let frame = prepared
                .authored
                .sample(&p, time, &ExpressionEvaluator)
                .unwrap();
            renderer.draw_authored_scene(frame, &target).unwrap();
            let again = read(&renderer, &target);
            for (_, x, y, expected) in &probes {
                assert!(close(again[(y * p.width + x) as usize], *expected));
            }
            assert_eq!(
                renderer
                    .vector_upload_info()
                    .unwrap()
                    .encoded_upload_bytes_total,
                warm
            );
        }
    }
    println!("native vector CPU/GPU reference pixels={checked} finite_range={finite_range} negative_channels={negative} hdr_channels={hdr} fill_stroke=both plain_grouped=both rasters=reused");
    assert_eq!(checked, if finite_range { 288 } else { 192 });
    if finite_range {
        assert!(negative > 0 && hdr > 0);
    }
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn animated_vector_paints_preserve_opacity_and_reject_actual_wrong_gpu_routes() {
    use motion_core::vector::{groups::VectorGroup, VectorSource};
    let reference = reference();
    let case = &reference.cases[0];
    let mut renderer = renderer();
    let mut controls = 0;
    for grouped in [false, true] {
        for stroke in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut p = document(&reference, 0, AlphaMode::Straight, root.path());
            let mut layer = vector_layer(1, [32., 32., 0.], case.input[10], stroke, false);
            let Content::Vector { vector } = &mut layer.content else {
                unreachable!()
            };
            let paint = if stroke {
                &mut vector.stroke.as_mut().unwrap().color
            } else {
                vector.fill.as_mut().unwrap()
            };
            paint.upsert(0, case.input[10], Ease::Hold).unwrap();
            paint.upsert(10, case.input[3], Ease::Hold).unwrap();
            if grouped {
                *vector = VectorGroup::wrap(vector.clone(), [8., 48.]);
                let VectorSource::Group { group } = &mut vector.source else {
                    unreachable!()
                };
                group.transform.opacity.value = 50.;
            }
            layer.transform.opacity.upsert(0, 0., Ease::Linear).unwrap();
            layer
                .transform
                .opacity
                .upsert(10, 1., Ease::Linear)
                .unwrap();
            p.layers.push(layer);
            p.validate().unwrap();
            let before = serde_json::to_value(&p).unwrap();
            let mut prepared = prep::prepare_native_authored_project(
                &p,
                root.path(),
                &runtime(),
                &Observation::new(9100),
            )
            .unwrap()
            .unwrap();
            let target = renderer
                .color_image_target(64, 64, prepared.authored.contract())
                .unwrap();
            let probe = if stroke { 9 * 64 + 32 } else { 32 * 64 + 32 };
            for (time, sample, opacity) in [(5.5, 10, 0.55), (10., 3, 1.), (5.5, 10, 0.55)] {
                let frame = prepared
                    .authored
                    .sample(&p, time, &ExpressionEvaluator)
                    .unwrap();
                let expected = premult(case.expected[sample], AlphaMode::Straight)
                    .map(|value| value * opacity * if grouped { 0.5 } else { 1. });
                renderer.draw_authored_scene(frame, &target).unwrap();
                let actual = read(&renderer, &target)[probe];
                assert!(close(actual, expected), "grouped={grouped} stroke={stroke} time={time} actual={actual:?} expected={expected:?}");
                for double_decode in [false, true] {
                    // These real GPU negative controls intentionally use the
                    // legacy route; the managed product route never does so.
                    let mut legacy = p.clone();
                    legacy.color_management = Arc::new(ProjectColorManagement::Unmanaged {});
                    let mut scene = motion_render::Scene::new(&legacy).unwrap();
                    scene
                        .sample(&legacy, time, None, &ExpressionEvaluator)
                        .unwrap();
                    if double_decode {
                        let vector = Arc::make_mut(scene.layers[0].vector.as_mut().unwrap());
                        fn replace(
                            vector: &mut motion_core::vector::SampledVector,
                            working: [f32; 4],
                        ) {
                            if let Some(fill) = &mut vector.fill {
                                *fill = working;
                            }
                            if let Some(stroke) = &mut vector.stroke {
                                stroke.0 = working;
                            }
                            if let Some(batches) = &mut vector.batches {
                                for batch in batches {
                                    replace(&mut batch.vector, working);
                                }
                            }
                        }
                        replace(vector, case.expected[sample]);
                    }
                    renderer
                        .draw(&scene, target.image().view(), 64, 64)
                        .unwrap();
                    assert!(!close(read(&renderer, &target)[probe], expected), "wrong route must fail original CPU pixel oracle grouped={grouped} stroke={stroke} time={time} double_decode={double_decode}");
                    controls += 1;
                    renderer.draw_authored_scene(frame, &target).unwrap();
                    assert!(
                        close(read(&renderer, &target)[probe], expected),
                        "legacy/working cache switches must rerasterize correctly"
                    );
                }
                let pixels = read(&renderer, &target);
                let policy = renderer.memory_policy();
                renderer
                    .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
                        source_texture_budget_bytes: 4,
                        ..policy
                    })
                    .unwrap();
                let error = renderer.draw_authored_scene(frame, &target).unwrap_err();
                assert!(
                    error.to_string().contains("budget"),
                    "original budget cause: {error}"
                );
                assert_eq!(
                    read(&renderer, &target),
                    pixels,
                    "failed admission cannot publish pixels"
                );
                renderer.set_memory_policy(policy).unwrap();
                renderer.draw_authored_scene(frame, &target).unwrap();
                assert!(close(read(&renderer, &target)[probe], expected));
            }
            let mut stale = p.clone();
            settings(&mut stale).display.alpha = AlphaMode::Premultiplied;
            assert!(matches!(
                prepared.authored.sample(&stale, 6., &ExpressionEvaluator),
                Err(AuthoredSceneError::Admission {
                    phase: "binding",
                    ..
                })
            ));
            assert_eq!(prepared.authored.current().unwrap().frame(), 5.5);
            assert_eq!(serde_json::to_value(&p).unwrap(), before);
        }
    }
    println!("animated vector GPU negative controls={controls} hold_keys=preserved group_layer_opacity=preserved source_budget=rejected_then_explicitly_recovered");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn mixed_images_and_vectors_share_input_budget_before_the_first_raster() {
    use std::sync::atomic::AtomicBool;
    let reference = reference();
    let case = &reference.cases[0];
    for grouped in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut p = document(&reference, 0, AlphaMode::Straight, root.path());
        p.layers.push(vector_layer(
            1,
            [32., 32., 0.],
            case.input[4],
            false,
            grouped,
        ));
        p.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/red.png".into(),
            width: 1,
            height: 1,
        });
        let mut image_layer =
            Layer::solid(2, "original red image", [64.; 2], [32., 32., 0.], [1.; 4]);
        image_layer.content = Content::Image { asset: 1 };
        p.layers.insert(0, image_layer);
        std::fs::create_dir_all(root.path().join("assets")).unwrap();
        image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(1, 1, vec![65535, 0, 0, 65535])
            .unwrap()
            .save(root.path().join("assets/red.png"))
            .unwrap();
        p.validate().unwrap();
        let expected = over(
            premult(case.expected[4], AlphaMode::Straight),
            case.expected[3],
        );
        let mut combined = 0;
        for constrained in [false, true] {
            let mut prepared = prep::prepare_native_authored_project(
                &p,
                root.path(),
                &runtime(),
                &Observation::new(9200),
            )
            .unwrap()
            .unwrap();
            let mut renderer = renderer();
            let images = prepared
                .prepare_images(
                    &mut renderer,
                    &p,
                    root.path(),
                    &AtomicBool::new(false),
                    &Observation::new(9201),
                )
                .unwrap();
            let frame = prepared
                .authored
                .sample(&p, 0., &ExpressionEvaluator)
                .unwrap();
            let target = renderer
                .color_image_target(64, 64, frame.contract())
                .unwrap();
            if constrained {
                let policy = renderer.memory_policy();
                renderer
                    .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
                        source_texture_budget_bytes: combined - 1,
                        ..policy
                    })
                    .unwrap();
                let before = read(&renderer, &target);
                assert!(renderer.draw_authored_scene_with_images(frame, &target, &images).is_err(), "first raster must account for both working images and vectors grouped={grouped}");
                assert_eq!(read(&renderer, &target), before);
                renderer.set_memory_policy(policy).unwrap();
                renderer
                    .draw_authored_scene_with_images(frame, &target, &images)
                    .unwrap();
            } else {
                combined = renderer
                    .draw_authored_scene_with_images(frame, &target, &images)
                    .unwrap()
                    .texture_bytes;
                assert!(combined > images.texture_resource_ledger().total_known_bytes());
            }
            let actual = read(&renderer, &target)[32 * 64 + 32];
            assert!(close(actual, expected), "mixed paints grouped={grouped} constrained={constrained} actual={actual:?} expected={expected:?}");
        }
    }
}
