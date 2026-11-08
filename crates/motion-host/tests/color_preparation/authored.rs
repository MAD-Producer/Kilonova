//! Actual managed documents, retained native CPU programs and GPU composition.
//! Existing OCIO CPU goldens stay immutable; no legacy unmanaged scene is used.
use super::*;
use motion_core::{Ease, ExpressionEvaluator, Layer};
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{
    AuthoredSceneError, ColorImageContract, ColorImageTarget, ColorProcessorOutput, Renderer,
};
#[path = "authored_images.rs"]
mod image_composition;
#[path = "authored_vectors.rs"]
mod vector_composition;
#[path = "authored_transfer.rs"]
mod transfer;

fn runtime() -> std::path::PathBuf {
    std::env::var_os("MOTION_TEST_OCIO_RUNTIME")
        .expect("explicit pinned native runtime")
        .into()
}
fn document(reference: &Reference, config: usize, alpha: AlphaMode, root: &Path) -> Project {
    let mut p = project(&reference.configs[config], alpha, root);
    p.assets.clear();
    p.video_assets.clear();
    p.background = [0.; 4];
    p.working_precision = ChannelPrecision::Float32;
    settings(&mut p).working.alpha = AlphaMode::Premultiplied;
    p
}
fn renderer() -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Float32,
        Default::default(),
    ))
    .unwrap()
}
fn read(renderer: &Renderer, target: &ColorImageTarget) -> Vec<[f32; 4]> {
    renderer
        .read_color_image_target(target)
        .unwrap()
        .pixels
        .chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|c| f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect()
}
fn close(actual: [f32; 4], expected: [f32; 4]) -> bool {
    (0..4).all(|c| {
        actual[c].is_finite()
            && (actual[c] - expected[c]).abs() <= 0.0001 + 0.0002 * expected[c].abs()
    })
}
fn premult(mut p: [f32; 4], alpha: AlphaMode) -> [f32; 4] {
    if alpha == AlphaMode::Straight {
        for c in 0..3 {
            p[c] *= p[3];
        }
    }
    p
}
fn over(front: [f32; 4], back: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|c| front[c] + back[c] * (1. - front[3]))
}
fn endpoint(
    renderer: &mut Renderer,
    input: &ColorImageTarget,
    processor: &CompiledColorImageProcessor,
) -> ColorImageTarget {
    let output = renderer
        .color_image_target(
            input.image().width(),
            input.image().height(),
            ColorImageContract {
                space: processor.output_space(),
                range: processor.output_range(),
                alpha: processor.output_alpha(),
                precision: ChannelPrecision::Float32,
            },
        )
        .unwrap();
    let binding = ColorProcessorOutput::new(renderer, input, processor, output.contract()).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    binding.encode(renderer, &mut encoder, &output).unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(input.texture_resource_ledger());
            ledger.merge(binding.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
        })
        .unwrap();
    output
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn native_authored_documents_match_independent_cpu_tiles_and_display_output_endpoints() {
    let reference = reference();
    let chains: serde_json::Value = serde_json::from_str(include_str!(
        "../../../motion-effects/tests/color_processor/chain_reference.json"
    ))
    .unwrap();
    let mut renderer = renderer();
    let mut checked = 0;
    for (stage, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let alpha = case.processor.input_alpha;
        let mut p = document(&reference, case.config, alpha, root.path());
        let indices: Vec<_> = case
            .input
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel.iter().all(|v| (0.0..=1.0).contains(v)))
            .map(|(i, _)| i)
            .collect();
        p.width = indices.len() as u32 * 8;
        for (tile, index) in indices.iter().enumerate() {
            p.layers.push(Layer::solid(
                tile as u64 + 1,
                "authored CPU reference tile",
                [8., 64.],
                [tile as f32 * 8. + 4., 32., 0.],
                case.input[*index],
            ));
        }
        p.validate().unwrap();
        let before = serde_json::to_value(&p).unwrap();
        let observation = Observation::new(8500 + stage as u64);
        let mut prepared =
            prep::prepare_native_authored_project(&p, root.path(), &runtime(), &observation)
                .unwrap()
                .unwrap();
        assert!(!observation.running());
        assert_eq!(
            prepared.colors.program_count(),
            3,
            "authored program can share default input's exact key"
        );
        assert!(prepared.authored.current().is_none());
        let frame = prepared
            .authored
            .sample(&p, 0.25, &ExpressionEvaluator)
            .unwrap();
        assert_eq!(frame.frame(), 0.25);
        let working = renderer
            .color_image_target(p.width, p.height, frame.contract())
            .unwrap();
        let no_readback = renderer.image_readback_info();
        renderer.draw_authored_scene(frame, &working).unwrap();
        let display = endpoint(&mut renderer, &working, prepared.colors.display());
        let output = endpoint(&mut renderer, &working, prepared.colors.output());
        assert_eq!(
            renderer.image_readback_info(),
            no_readback,
            "product composition and endpoints must not read back"
        );
        let pixels = read(&renderer, &working);
        for (tile, index) in indices.iter().enumerate() {
            let actual = pixels[(32 * p.width) as usize + tile * 8 + 4];
            let expected = premult(case.expected[*index], alpha);
            assert!(
                close(actual, expected),
                "stage={stage} tile={tile} actual={actual:?} expected={expected:?}"
            );
            checked += 1;
        }
        for (target, phase) in [
            (&display, ColorProcessorPhase::WorkingToDisplay),
            (&output, ColorProcessorPhase::WorkingToOutput),
        ] {
            let endstage = reference
                .cases
                .iter()
                .position(|c| {
                    c.config == case.config
                        && c.processor.phase == phase
                        && c.processor.input_alpha == alpha
                        && c.processor.output_alpha == alpha
                })
                .unwrap();
            let chain = chains["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["stages"] == serde_json::json!([stage, endstage]))
                .unwrap();
            let expected: Vec<[f32; 4]> =
                serde_json::from_value(chain["expected"].clone()).unwrap();
            let actual = read(&renderer, target);
            for (tile, index) in indices.iter().enumerate() {
                let pixel = actual[(32 * p.width) as usize + tile * 8 + 4];
                assert!(
                    close(pixel, expected[*index]),
                    "stage={stage} endpoint={phase:?} tile={tile} actual={pixel:?} expected={:?}",
                    expected[*index]
                );
            }
        }
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
    }
    assert!(checked >= 40);
    println!("actual authored CPU/GPU tile checks={checked}; all display/output endpoints passed");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn native_authored_composites_fractional_opacity_and_refuses_stale_consumers_before_writes() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut p = document(&reference, 0, AlphaMode::Straight, root.path());
    let case = &reference.cases[0];
    p.background = case.input[2];
    p.background[3] = 0.5;
    p.layers.push(Layer::solid(
        1,
        "foreground",
        [64.; 2],
        [32., 32., 0.],
        case.input[4],
    ));
    p.layers[0]
        .transform
        .opacity
        .upsert(0, 0., Ease::Linear)
        .unwrap();
    p.layers[0]
        .transform
        .opacity
        .upsert(10, 1., Ease::Linear)
        .unwrap();
    p.validate().unwrap();
    let observation = Observation::new(8600);
    let before = serde_json::to_value(&p).unwrap();
    let mut prepared =
        prep::prepare_native_authored_project(&p, root.path(), &runtime(), &observation)
            .unwrap()
            .unwrap();
    let mut renderer = renderer();
    let frame = prepared
        .authored
        .sample(&p, 5.5, &ExpressionEvaluator)
        .unwrap();
    let target = renderer
        .color_image_target(64, 64, frame.contract())
        .unwrap();
    renderer.draw_authored_scene(frame, &target).unwrap();
    let back = premult(
        [
            case.expected[2][0],
            case.expected[2][1],
            case.expected[2][2],
            0.5,
        ],
        AlphaMode::Straight,
    );
    let front = premult(
        [
            case.expected[4][0],
            case.expected[4][1],
            case.expected[4][2],
            0.5 * 0.55,
        ],
        AlphaMode::Straight,
    );
    let expected = over(front, back);
    let pixels = read(&renderer, &target);
    assert!(
        pixels.iter().all(|p| close(*p, expected)),
        "actual={:?} expected={expected:?}",
        pixels[2000]
    );
    // Real wrong GPU pipelines, only in this negative-control fixture. The
    // product path above never disables management or round-trips through sRGB.
    let mut legacy = p.clone();
    legacy.color_management = Arc::new(ProjectColorManagement::Unmanaged {});
    for double_decode in [false, true] {
        if double_decode {
            legacy.background[..3].copy_from_slice(&case.expected[2][..3]);
            let motion_core::Content::Solid { color } = &mut legacy.layers[0].content else {
                unreachable!()
            };
            color[..3].copy_from_slice(&case.expected[4][..3]);
        }
        let mut scene = motion_render::Scene::new(&legacy).unwrap();
        scene
            .sample(&legacy, 5.5, None, &ExpressionEvaluator)
            .unwrap();
        let wrong = renderer
            .color_image_target(64, 64, frame.contract())
            .unwrap();
        renderer.draw(&scene, wrong.image().view(), 64, 64).unwrap();
        assert!(read(&renderer,&wrong).iter().any(|pixel| !close(*pixel,expected)),
            "actual GPU negative control double_decode={double_decode} must fail the independent composition oracle");
    }
    let old_contract = frame.contract();
    let mut bad_contract = old_contract;
    bad_contract.alpha = AlphaMode::Straight;
    let wrong_target = renderer.color_image_target(64, 64, bad_contract).unwrap();
    let wrong_before = read(&renderer, &wrong_target);
    let bytes = renderer.allocation_budget().info().unwrap().admitted_bytes;
    assert!(matches!(
        renderer.draw_authored_scene(frame, &wrong_target),
        Err(motion_render::RenderError::AuthoredScene(
            AuthoredSceneError::Destination { .. }
        ))
    ));
    assert_eq!(read(&renderer, &wrong_target), wrong_before);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        bytes
    );
    let short = renderer.color_image_target(32, 64, old_contract).unwrap();
    let short_before = read(&renderer, &short);
    assert!(matches!(
        renderer.draw_authored_scene(frame, &short),
        Err(motion_render::RenderError::AuthoredScene(
            AuthoredSceneError::Extent { .. }
        ))
    ));
    assert_eq!(read(&renderer, &short), short_before);
    let mut stale = p.clone();
    settings(&mut stale).display.alpha = AlphaMode::Premultiplied;
    assert!(matches!(
        prepared.authored.sample(&stale, 6., &ExpressionEvaluator),
        Err(AuthoredSceneError::Admission {
            phase: "binding",
            ..
        })
    ));
    let mut unsupported = p.clone();
    unsupported.layers[0].three_d = true;
    assert!(matches!(
        prepared
            .authored
            .sample(&unsupported, 6., &ExpressionEvaluator),
        Err(AuthoredSceneError::Admission {
            object: Some(1),
            ..
        })
    ));
    assert!(matches!(
        prepared.authored.sample(&p, f64::NAN, &ExpressionEvaluator),
        Err(AuthoredSceneError::Model { .. })
    ));
    assert_eq!(
        prepared.authored.current().unwrap().frame(),
        5.5,
        "failed samples must not publish or silently choose another frame"
    );
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    assert!(prep::prepare_native_authored_project(
        &unsupported,
        root.path(),
        &runtime(),
        &observation
    )
    .is_err());
    assert!(!observation.running());
    let unmanaged = Project::new(64, 64, 30, 120).unwrap();
    assert!(prep::prepare_native_authored_project(
        &unmanaged,
        root.path(),
        Path::new("missing-runtime"),
        &observation
    )
    .unwrap()
    .is_none());
    assert!(!observation.running());
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn native_authored_limited_ranges_preserve_alpha_and_need_no_files_during_sampling() {
    let reference = reference();
    let ranges: serde_json::Value = serde_json::from_str(include_str!(
        "../../../motion-effects/tests/color_processor/range_reference.json"
    ))
    .unwrap();
    let mut renderer = renderer();
    let mut checked = 0;
    for (stage, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let range_case = ranges["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["stage"] == stage && c["output_range"] == serde_json::json!([0., 1.]))
            .unwrap();
        let input: Vec<[f32; 4]> = serde_json::from_value(range_case["input"].clone()).unwrap();
        let expected: Vec<[f32; 4]> =
            serde_json::from_value(range_case["expected"].clone()).unwrap();
        let indices: Vec<_> = input
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel.iter().all(|v| (0.0..=1.0).contains(v)))
            .map(|(i, _)| i)
            .collect();
        let root = tempfile::tempdir().unwrap();
        let mut p = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut p).authored_input.signal.range = SignalRange::Limited {
            black: 16. / 255.,
            white: 235. / 255.,
        };
        p.width = indices.len() as u32 * 8;
        for (tile, index) in indices.iter().enumerate() {
            p.layers.push(Layer::solid(
                tile as u64 + 1,
                "limited authored tile",
                [8., 64.],
                [tile as f32 * 8. + 4., 32., 0.],
                input[*index],
            ));
        }
        p.validate().unwrap();
        let observation = Observation::new(8700 + stage as u64);
        let mut prepared =
            prep::prepare_native_authored_project(&p, root.path(), &runtime(), &observation)
                .unwrap()
                .unwrap();
        assert_eq!(
            prepared.colors.program_count(),
            4,
            "finite authored range differs from full-range default input"
        );
        // Deleting the configuration after preparation would expose any hidden
        // per-frame runtime/processor rebuild, including the second spare scene.
        for resource in p.color_management.resources().unwrap() {
            std::fs::remove_file(root.path().join(resource.path)).unwrap();
        }
        for time in [0.25, 1.5, 2.75] {
            let frame = prepared
                .authored
                .sample(&p, time, &ExpressionEvaluator)
                .unwrap();
            let target = renderer
                .color_image_target(p.width, p.height, frame.contract())
                .unwrap();
            renderer.draw_authored_scene(frame, &target).unwrap();
            let actual = read(&renderer, &target);
            for (tile, index) in indices.iter().enumerate() {
                let got = actual[(32 * p.width) as usize + tile * 8 + 4];
                let want = premult(expected[*index], case.processor.output_alpha);
                assert!(
                    close(got, want),
                    "stage={stage} time={time} tile={tile} actual={got:?} expected={want:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 120);
    println!("finite-range actual authored tile checks={checked}, including zero alpha; no configuration file during sampling");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn finite_authored_codes_reach_negative_and_hdr_working_pixels_without_clipping() {
    let reference = reference();
    let root = tempfile::tempdir().unwrap();
    let mut renderer = renderer();
    let mut negative = 0;
    let mut hdr = 0;
    for case in reference
        .cases
        .iter()
        .filter(|c| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let mut p = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        // Binary-exact finite code interval represents the original oracle's
        // negative/HDR inputs while every stored authored channel stays [0,1].
        settings(&mut p).authored_input.signal.range = SignalRange::Limited {
            black: 0.25,
            white: 0.251953125,
        };
        p.width = case.input.len() as u32 * 8;
        for (tile, input) in case.input.iter().enumerate() {
            let mut coded = *input;
            for c in 0..3 {
                let straight = if case.processor.input_alpha == AlphaMode::Premultiplied {
                    if input[3] > 0. {
                        input[c] / input[3]
                    } else {
                        0.
                    }
                } else {
                    input[c]
                };
                coded[c] = straight * (1. / 512.) + 0.25;
                if case.processor.input_alpha == AlphaMode::Premultiplied {
                    coded[c] *= input[3];
                }
            }
            p.layers.push(Layer::solid(
                tile as u64 + 1,
                "encoded negative/HDR oracle",
                [8., 64.],
                [tile as f32 * 8. + 4., 32., 0.],
                coded,
            ));
        }
        p.validate().unwrap();
        let mut prepared = prep::prepare_native_authored_project(
            &p,
            root.path(),
            &runtime(),
            &Observation::new(8750),
        )
        .unwrap()
        .unwrap();
        let frame = prepared
            .authored
            .sample(&p, 0.5, &ExpressionEvaluator)
            .unwrap();
        let target = renderer
            .color_image_target(p.width, p.height, frame.contract())
            .unwrap();
        renderer.draw_authored_scene(frame, &target).unwrap();
        let actual = read(&renderer, &target);
        for (tile, expected) in case.expected.iter().enumerate() {
            let got = actual[(32 * p.width) as usize + tile * 8 + 4];
            let want = premult(*expected, case.processor.output_alpha);
            assert!(
                close(got, want),
                "config={} alpha={:?} tile={tile} actual={got:?} expected={want:?}",
                case.config,
                case.processor.input_alpha
            );
            negative += got[..3].iter().filter(|v| **v < 0.).count();
            hdr += got[..3].iter().filter(|v| **v > 1.).count();
        }
    }
    assert!(negative > 0 && hdr > 0, "negative={negative} HDR={hdr}");
    println!("actual authored channels preserved: negative={negative} HDR={hdr}; unchanged CPU oracle and tolerance");
}
