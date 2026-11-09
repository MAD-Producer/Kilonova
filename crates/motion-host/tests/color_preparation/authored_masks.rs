//! Mask coverage is numeric data; only authored paints enter the OCIO input program.
use super::*;
use motion_core::{
    masks::{LayerMask, MaskMode},
    vector::{PathNode, VectorPath},
    Track,
};

pub(super) fn rectangle(id: u64, bounds: [f32; 4]) -> LayerMask {
    let [x, y, w, h] = bounds;
    LayerMask::new(
        id,
        VectorPath {
            id: 1,
            tangents: Default::default(),
            closed: true,
            nodes: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    )
}
fn coverage(mode: MaskMode, a: f32, b: f32) -> f32 {
    match mode {
        MaskMode::None => a,
        MaskMode::Add => a + b * (1. - a),
        MaskMode::Subtract => a * (1. - b),
        MaskMode::Intersect => a * b,
        MaskMode::Lighten => a.max(b),
        MaskMode::Darken => a.min(b),
        MaskMode::Difference => a * (1. - b) + b * (1. - a),
    }
}
fn encode_pixel(mut p: [f32; 4], alpha: AlphaMode) -> [f32; 4] {
    let factor = if alpha == AlphaMode::Premultiplied {
        p[3]
    } else {
        1.
    };
    for v in &mut p[..3] {
        *v = *v / 512. + factor / 1024.;
    }
    assert!(p.iter().all(|v| (0.0..=1.0).contains(v)));
    p
}
fn check_backend(renderer: &Renderer) {
    eprintln!("managed mask adapter={:?}", renderer.adapter_info);
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        _ => {}
    }
}

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn managed_masks_preserve_native_hdr_paints_and_numeric_coverage() {
    let reference = reference();
    let mut renderer = renderer();
    check_backend(&renderer);
    let modes = [
        MaskMode::Add,
        MaskMode::Subtract,
        MaskMode::Intersect,
        MaskMode::Lighten,
        MaskMode::Darken,
        MaskMode::Difference,
    ];
    let mut checked = 0;
    let mut hdr = 0;
    let mut negative = 0;
    for (ci, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let samples = [
            case.input
                .iter()
                .position(|c| c[3] > 0. && c[..3].iter().any(|v| *v > 1.))
                .unwrap(),
            case.input
                .iter()
                .position(|c| c[3] > 0. && c[..3].iter().any(|v| *v < 0.))
                .unwrap(),
        ];
        for sample in samples {
            let root = tempfile::tempdir().unwrap();
            let mut p = document(
                &reference,
                case.config,
                case.processor.input_alpha,
                root.path(),
            );
            p.width = 64;
            p.height = 64;
            settings(&mut p).authored_input.signal.range = SignalRange::Limited {
                black: 1. / 1024.,
                white: 3. / 1024.,
            };
            let paint = premult(case.expected[sample], case.processor.output_alpha);
            let back_sample = case
                .input
                .iter()
                .position(|c| c[3] > 0. && c[3] < 1.)
                .unwrap();
            let back = premult(case.expected[back_sample], case.processor.output_alpha);
            p.background = encode_pixel(case.input[back_sample], case.processor.input_alpha);
            p.layers.push(Layer::solid(
                1,
                "managed masked paint",
                [64.; 2],
                [32., 32., 0.],
                encode_pixel(case.input[sample], case.processor.input_alpha),
            ));
            let mut a = rectangle(1, [8., 8., 32., 48.]);
            a.opacity.value = 75.;
            let mut b = rectangle(2, [24., 16., 32., 32.]);
            b.opacity.value = 50.;
            p.layers[0].masks = vec![a, b];
            let mut prepared = prep::prepare_native_authored_project(
                &p,
                root.path(),
                &runtime(),
                &Observation::new(41000 + ci as u64),
            )
            .unwrap()
            .unwrap();
            for mode in modes {
                for invert in [false, true] {
                    p.layers[0].masks[1].mode = mode;
                    p.layers[0].masks[1].inverted = invert;
                    let before = serde_json::to_vec(&p).unwrap();
                    let frame = prepared
                        .authored
                        .sample(&p, 1.25, &ExpressionEvaluator)
                        .unwrap();
                    let target = renderer
                        .color_image_target(64, 64, frame.contract())
                        .unwrap();
                    if ci == 0 && sample == samples[0] && mode == MaskMode::Add && !invert {
                        let old = read(&renderer, &target);
                        let bytes = renderer.texture_bytes();
                        let policy = renderer.memory_policy();
                        renderer
                            .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
                                source_texture_budget_bytes: 4 + 64 * 64 * 4,
                                ..policy
                            })
                            .unwrap();
                        let error = renderer.draw_authored_scene(frame, &target).unwrap_err();
                        assert!(
                            error.to_string().contains("numeric mask coverage"),
                            "{error}"
                        );
                        assert_eq!(
                            renderer.texture_bytes(),
                            bytes,
                            "failed preflight allocated mask outputs"
                        );
                        assert_eq!(read(&renderer, &target), old);
                        renderer.set_memory_policy(policy).unwrap();
                    }
                    let reads = renderer.image_readback_info();
                    renderer.draw_authored_scene(frame, &target).unwrap();
                    assert_eq!(
                        renderer.image_readback_info(),
                        reads,
                        "mask composition cannot read back intermediates"
                    );
                    let pixels = read(&renderer, &target);
                    for y in 0..64 {
                        for x in 0..64 {
                            let a = if (8..40).contains(&x) && (8..56).contains(&y) {
                                0.75
                            } else {
                                0.
                            };
                            let inside = (24..56).contains(&x) && (16..48).contains(&y);
                            let b = if inside != invert { 0.5 } else { 0. };
                            let cov = coverage(mode, a, b);
                            let expected = over(paint.map(|v| v * cov), back);
                            let actual = pixels[(y * 64 + x) as usize];
                            assert!(close(actual,expected),"case={ci} mode={mode:?} invert={invert} x={x} y={y} actual={actual:?} expected={expected:?}");
                            checked += 1;
                            hdr += expected[..3].iter().filter(|v| **v > 1.).count();
                            negative += expected[..3].iter().filter(|v| **v < 0.).count();
                        }
                    }
                    let bytes = renderer.texture_bytes();
                    renderer.draw_authored_scene(frame, &target).unwrap();
                    assert_eq!(
                        read(&renderer, &target),
                        pixels,
                        "cached coverage changed pixels"
                    );
                    assert_eq!(renderer.texture_bytes(), bytes, "warm mask allocation grew");
                    assert_eq!(serde_json::to_vec(&p).unwrap(), before);
                    if ci == 0 && !invert && mode == MaskMode::Add {
                        let policy = renderer.memory_policy();
                        renderer
                            .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
                                source_texture_budget_bytes: 4,
                                ..policy
                            })
                            .unwrap();
                        let error = renderer.draw_authored_scene(frame, &target).unwrap_err();
                        assert!(error.to_string().contains("budget"), "{error}");
                        assert_eq!(
                            read(&renderer, &target),
                            pixels,
                            "failed coverage admission published pixels"
                        );
                        renderer.set_memory_policy(policy).unwrap();
                        renderer.draw_authored_scene(frame, &target).unwrap();
                        assert_eq!(read(&renderer, &target), pixels);
                    }
                }
            }
            p.layers[0].masks.clear();
            let frame = prepared
                .authored
                .sample(&p, 2., &ExpressionEvaluator)
                .unwrap();
            let target = renderer
                .color_image_target(64, 64, frame.contract())
                .unwrap();
            renderer.draw_authored_scene(frame, &target).unwrap();
            assert!(read(&renderer, &target)
                .iter()
                .all(|actual| close(*actual, over(paint, back))));
            assert_eq!(
                renderer.texture_bytes(),
                4,
                "removed masks must release coverage images"
            );
        }
    }
    assert!(
        checked >= 589824 && hdr > 0 && negative > 0,
        "missing colour or coverage cases: {checked} {hdr} {negative}"
    );
    eprintln!(
        "native managed mask pixels={checked}, negative channels={negative}, HDR channels={hdr}"
    );
}

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn managed_feather_expansion_and_initial_modes_match_independent_coverage() {
    let reference = reference();
    let case = reference
        .cases
        .iter()
        .find(|c| {
            c.processor.phase == ColorProcessorPhase::InputToWorking
                && c.processor.input_alpha == AlphaMode::Straight
        })
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut p = document(&reference, case.config, AlphaMode::Straight, root.path());
    p.width = 64;
    p.height = 64;
    settings(&mut p).authored_input.signal.range = SignalRange::Limited {
        black: 1. / 1024.,
        white: 3. / 1024.,
    };
    let sample = case
        .input
        .iter()
        .position(|c| c[3] > 0. && c[..3].iter().any(|v| *v > 1.))
        .unwrap();
    let paint = premult(case.expected[sample], case.processor.output_alpha);
    p.layers.push(Layer::solid(
        1,
        "numeric coverage",
        [64.; 2],
        [32., 32., 0.],
        encode_pixel(case.input[sample], AlphaMode::Straight),
    ));
    p.layers[0].masks.push(rectangle(1, [16., 8., 32., 48.]));
    p.layers[0].masks[0].opacity.value = 50.;
    let mut prepared = prep::prepare_native_authored_project(
        &p,
        root.path(),
        &runtime(),
        &Observation::new(42000),
    )
    .unwrap()
    .unwrap();
    let mut renderer = renderer();
    check_backend(&renderer);
    let modes = [
        MaskMode::None,
        MaskMode::Add,
        MaskMode::Subtract,
        MaskMode::Intersect,
        MaskMode::Lighten,
        MaskMode::Darken,
        MaskMode::Difference,
    ];
    let mut checked = 0;
    for mode in modes {
        for (feather, expansion) in [(0., 0.), (4., 0.), (0., 4.), (0., -4.)] {
            let m = &mut p.layers[0].masks[0];
            m.mode = mode;
            m.feather.value = [feather, 0.];
            m.expansion.value = expansion;
            let frame = prepared
                .authored
                .sample(&p, 0.5, &ExpressionEvaluator)
                .unwrap();
            for size in [64, 32] {
                let scale = size as f32 / 64.;
                let target = renderer
                    .color_image_target(size, size, frame.contract())
                    .unwrap();
                if size == 64 {
                    renderer.draw_authored_scene(frame, &target).unwrap();
                } else {
                    renderer
                        .draw_authored_preview(frame, &target, None)
                        .unwrap();
                }
                let pixels = read(&renderer, &target);
                // y is well inside both horizontal boundaries. Independent discrete
                // Gaussian reference uses scaled sigma and bounded support at pixel centres.
                for y in size / 4..3 * size / 4 {
                    for x in 0..size {
                        let inside = |x: f32| {
                            if x >= (16. - expansion) * scale && x < (48. + expansion) * scale {
                                1.
                            } else {
                                0.
                            }
                        };
                        let value = if feather > 0. {
                            let mut sum = 0.;
                            let mut weights = 0.;
                            let sigma = feather * 0.25 * scale;
                            for i in -16i32..=16 {
                                if (i as f32).abs() > 3. * sigma + 0.0001 {
                                    continue;
                                }
                                let w = (-0.5 * (i * i) as f32 / (sigma * sigma)).exp();
                                sum += inside(x as f32 + 0.5 + i as f32) * w;
                                weights += w;
                            }
                            sum / weights
                        } else {
                            inside(x as f32 + 0.5)
                        };
                        let initial = if matches!(
                            mode,
                            MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken
                        ) {
                            1.
                        } else {
                            0.
                        };
                        let cov = if mode == MaskMode::None {
                            1.
                        } else {
                            coverage(mode, initial, value * 0.5)
                        };
                        let expected = paint.map(|v| v * cov);
                        let actual = pixels[(y * size + x) as usize];
                        assert!(close(actual,expected),"mode={mode:?} size={size} feather={feather} expansion={expansion} x={x} y={y} actual={actual:?} expected={expected:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 71680);
    eprintln!("managed initial/feather/expansion pixels={checked}");
}
