use super::*;
use motion_core::{Asset, Content};
use motion_render::AuthoredImageSet;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
struct Original {
    samples: Vec<[u16; 4]>,
    cases: Vec<OriginalCase>,
}
#[derive(Deserialize)]
struct OriginalCase {
    config: usize,
    stages: [usize; 2],
    intermediate: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn original() -> Original {
    serde_json::from_str(include_str!("source_reference.json")).unwrap()
}
fn image_document(reference: &Reference, root: &Path, config: usize, alpha: AlphaMode) -> Project {
    let mut p = document(reference, config, alpha, root);
    p.width = 3;
    p.height = 4;
    p.assets.push(Asset {
        id: 1,
        path: "assets/1.png".into(),
        width: 3,
        height: 4,
    });
    let mut layer = Layer::solid(1, "original 16-bit image", [3., 4.], [1.5, 2., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 1 };
    p.layers.push(layer);
    p.validate().unwrap();
    p
}
fn write_image(root: &Path, pixels: &[[u16; 4]]) {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(
        3,
        4,
        pixels.iter().flatten().copied().collect::<Vec<_>>(),
    )
    .unwrap()
    .save(root.join("assets/1.png"))
    .unwrap();
}
fn prepare(
    p: &Project,
    root: &Path,
    renderer: &mut Renderer,
) -> (prep::PreparedNativeAuthoredProject, AuthoredImageSet) {
    let observation = Observation::new(8900);
    let prepared = prep::prepare_native_authored_project(p, root, &runtime(), &observation)
        .unwrap()
        .unwrap();
    let images = prepared
        .prepare_images(renderer, p, root, &AtomicBool::new(false), &observation)
        .unwrap();
    assert!(!observation.running());
    (prepared, images)
}
fn compare(actual: &[[f32; 4]], expected: &[[f32; 4]], label: &str) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            close(*actual, *expected),
            "{label} pixel={index} actual={actual:?} expected={expected:?}"
        );
    }
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn original_images_compose_in_working_space_and_reuse_bindings_without_file_io_or_readback() {
    let reference = reference();
    let original = original();
    let mut renderer = renderer();
    let mut checked = 0;
    let mut omitted_conversion_detected = 0;
    let mut double_conversion_detected = 0;
    for case in &original.cases {
        let root = tempfile::tempdir().unwrap();
        let alpha = reference.cases[case.stages[0]].processor.input_alpha;
        let mut p = image_document(&reference, root.path(), case.config, alpha);
        let endpoint_alpha = reference.cases[case.stages[1]].processor.output_alpha;
        settings(&mut p).display.alpha = endpoint_alpha;
        settings(&mut p).output.alpha = endpoint_alpha;
        p.validate().unwrap();
        let before = serde_json::to_value(&p).unwrap();
        write_image(root.path(), &original.samples);
        let (mut prepared, images) = prepare(&p, root.path(), &mut renderer);
        let target = renderer
            .color_image_target(3, 4, prepared.authored.contract())
            .unwrap();
        let reads = renderer.image_readback_info();
        // Neither scene sampling nor the retained GPU bindings may reopen these.
        std::fs::remove_file(root.path().join("assets/1.png")).unwrap();
        for resource in p.color_management.resources().unwrap() {
            std::fs::remove_file(root.path().join(resource.path)).unwrap();
        }
        let expected: Vec<_> = case
            .intermediate
            .iter()
            .map(|p| premult(*p, alpha))
            .collect();
        for frame in [0.25, 5.5, 10.75] {
            let frame = prepared
                .authored
                .sample(&p, frame, &ExpressionEvaluator)
                .unwrap();
            renderer
                .draw_authored_scene_with_images(frame, &target, &images)
                .unwrap();
        }
        let end_processor = match reference.cases[case.stages[1]].processor.phase {
            ColorProcessorPhase::WorkingToDisplay => prepared.colors.display(),
            ColorProcessorPhase::WorkingToOutput => prepared.colors.output(),
            _ => panic!("independent reference endpoint phase"),
        };
        let display = endpoint(&mut renderer, &target, end_processor);
        assert_eq!(renderer.image_readback_info(), reads);
        let pixels = read(&renderer, &target);
        compare(&pixels, &expected, "working composition");
        let display_pixels = read(&renderer, &display);
        for (actual, expected) in display_pixels.iter().zip(&case.expected) {
            if expected[3] == 0. && endpoint_alpha == AlphaMode::Straight {
                // The independent direct straight-input oracle retains hidden
                // RGB. Premultiplied composition necessarily discards it. Check
                // finite output and exact zero coverage; all nontransparent
                // samples keep the original unweighted CPU pixel tolerance.
                assert!(actual.iter().all(|v| v.is_finite()));
                assert_eq!(actual[3], 0.);
                assert_eq!(premult(*actual, AlphaMode::Straight), [0.; 4]);
            } else {
                assert!(
                    close(*actual, *expected),
                    "composition plus display actual={actual:?} expected={expected:?}"
                );
            }
        }
        assert_ne!(
            pixels[0], pixels[1],
            "adjacent 16-bit codes survive actual composition"
        );

        // Real faulty GPU draws, using the same plane compositor and geometry.
        // Raw encoded pixels omit input conversion; encoded working pixels run
        // the default input transform twice. Both retain truthful test intent:
        // these deliberately false producer contracts are never product paths.
        let raw: Vec<[f32; 4]> = original
            .samples
            .iter()
            .map(|p| premult(p.map(|v| v as f32 / 65535.), alpha))
            .collect();
        for (wrong, double) in [(&raw, false), (&expected, true)] {
            let wrong = if double {
                let encoded = renderer
                    .upload_color_image_f32(
                        3,
                        4,
                        ColorImageContract {
                            space: prepared.colors.default_input().input_space(),
                            range: prepared.colors.default_input().input_range(),
                            alpha: AlphaMode::Premultiplied,
                            precision: ChannelPrecision::Float32,
                        },
                        wrong,
                    )
                    .unwrap();
                // The default processor's input alpha may be straight. Feed its
                // declared alpha exactly, so the wrong operation is conversion.
                if alpha == AlphaMode::Premultiplied {
                    endpoint(&mut renderer, &encoded, prepared.colors.default_input())
                } else {
                    let straight = renderer
                        .upload_color_image_f32(
                            3,
                            4,
                            ColorImageContract {
                                alpha: AlphaMode::Straight,
                                ..encoded.contract()
                            },
                            wrong,
                        )
                        .unwrap();
                    endpoint(&mut renderer, &straight, prepared.colors.default_input())
                }
            } else {
                renderer
                    .upload_color_image_f32(3, 4, target.contract(), wrong)
                    .unwrap()
            };
            let wrong_set = renderer
                .prepare_authored_images(
                    target.contract(),
                    [(p.assets[0].clone(), Arc::new(wrong))],
                )
                .unwrap();
            renderer
                .draw_authored_scene_with_images(
                    prepared.authored.current().unwrap(),
                    &target,
                    &wrong_set,
                )
                .unwrap();
            if read(&renderer, &target)
                .iter()
                .zip(&expected)
                .any(|(a, e)| !close(*a, *e))
            {
                if double {
                    double_conversion_detected += 1;
                } else {
                    omitted_conversion_detected += 1;
                }
            }
        }
        assert_eq!(serde_json::to_value(&p).unwrap(), before);
        checked += expected.len();
    }
    assert_eq!(checked, 168);
    assert!(
        omitted_conversion_detected >= 8,
        "identity configs need not reject a no-op conversion"
    );
    assert!(double_conversion_detected >= 8);
    println!("working image CPU pixels={checked}, omitted conversion GPU controls={omitted_conversion_detected}, double conversion GPU controls={double_conversion_detected}");
}

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn image_opacity_and_binding_failures_preserve_pixels_and_preparation_can_recover() {
    let reference = reference();
    let original = original();
    let case = &original.cases[0];
    let alpha = reference.cases[case.stages[0]].processor.input_alpha;
    let root = tempfile::tempdir().unwrap();
    let mut p = image_document(&reference, root.path(), case.config, alpha);
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
    write_image(root.path(), &original.samples);
    let mut renderer = renderer();
    let (mut prepared, images) = prepare(&p, root.path(), &mut renderer);
    let frame = prepared
        .authored
        .sample(&p, 5.5, &ExpressionEvaluator)
        .unwrap();
    let target = renderer.color_image_target(3, 4, frame.contract()).unwrap();
    let stats = renderer
        .draw_authored_scene_with_images(frame, &target, &images)
        .unwrap();
    let expected: Vec<_> = case
        .intermediate
        .iter()
        .map(|p| premult(*p, alpha).map(|v| v * 0.55))
        .collect();
    compare(
        &read(&renderer, &target),
        &expected,
        "fractional image opacity",
    );
    let original_pixels = read(&renderer, &target);
    assert!(renderer.draw_authored_scene(frame, &target).is_err());
    assert_eq!(read(&renderer, &target), original_pixels);
    let source = prepared
        .colors
        .prepare_source_image(
            &mut renderer,
            prep::SourceImageRequest {
                project: &p,
                root: root.path(),
                asset: 1,
                cancel: &AtomicBool::new(false),
            },
            &Observation::new(8901),
        )
        .unwrap()
        .unwrap();
    let source = Arc::new(source.into_working());
    let mut stale = p.assets[0].clone();
    stale.path = "assets/replaced.png".into();
    let stale_set = renderer
        .prepare_authored_images(frame.contract(), [(stale, source.clone())])
        .unwrap();
    assert!(renderer
        .draw_authored_scene_with_images(frame, &target, &stale_set)
        .is_err());
    assert_eq!(read(&renderer, &target), original_pixels);
    let alias_set = renderer
        .prepare_authored_images(frame.contract(), [(p.assets[0].clone(), source.clone())])
        .unwrap();
    assert!(
        renderer
            .draw_authored_scene_with_images(frame, &source, &alias_set)
            .is_err(),
        "read/write alias must fail"
    );
    let mut wrong_size = p.assets[0].clone();
    let mut alias_declaration = p.assets[0].clone();
    alias_declaration.id = 2;
    let shared = renderer
        .prepare_authored_images(
            frame.contract(),
            [
                (p.assets[0].clone(), source.clone()),
                (alias_declaration, source.clone()),
            ],
        )
        .unwrap();
    assert_eq!(
        shared.texture_resource_ledger().total_known_bytes(),
        3 * 4 * 16,
        "two descriptors sharing the same native allocation count once"
    );
    wrong_size.width += 1;
    assert!(renderer
        .prepare_authored_images(frame.contract(), [(wrong_size, source.clone())])
        .is_err());
    assert!(renderer
        .prepare_authored_images(
            frame.contract(),
            [
                (p.assets[0].clone(), source.clone()),
                (p.assets[0].clone(), source.clone())
            ]
        )
        .is_err());
    let other = super::renderer();
    assert!(other
        .prepare_authored_images(frame.contract(), [(p.assets[0].clone(), source.clone())])
        .is_err());
    let wrong = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                alpha: AlphaMode::Straight,
                ..frame.contract()
            },
        )
        .unwrap();
    assert!(renderer
        .prepare_authored_images(frame.contract(), [(p.assets[0].clone(), Arc::new(wrong))])
        .is_err());
    let policy = renderer.memory_policy();
    renderer
        .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
            source_texture_budget_bytes: stats.texture_bytes - 1,
            ..policy
        })
        .unwrap();
    assert!(renderer
        .draw_authored_scene_with_images(frame, &target, &images)
        .is_err());
    assert_eq!(read(&renderer, &target), original_pixels);
    renderer.set_memory_policy(policy).unwrap();
    renderer
        .draw_authored_scene_with_images(frame, &target, &images)
        .unwrap();
    let mut changed = p.clone();
    changed.assets[0].path = "assets/replaced.png".into();
    assert!(prepared
        .authored
        .sample(&changed, 6.5, &ExpressionEvaluator)
        .is_err());
    assert_eq!(prepared.authored.current().unwrap().frame(), 5.5);
    let observation = Observation::new(8902);
    assert!(prepared
        .prepare_images(
            &mut renderer,
            &p,
            root.path(),
            &AtomicBool::new(true),
            &observation
        )
        .is_err());
    assert!(!observation.running());
    let recovered = prepared
        .prepare_images(
            &mut renderer,
            &p,
            root.path(),
            &AtomicBool::new(false),
            &observation,
        )
        .unwrap();
    renderer
        .draw_authored_scene_with_images(prepared.authored.current().unwrap(), &target, &recovered)
        .unwrap();
    compare(
        &read(&renderer, &target),
        &expected,
        "recovery after cancelled preparation",
    );
    settings(&mut changed).sources.push(SourceColorAssignment {
        asset: 1,
        interpretation: SourceInterpretation::Data {
            meaning: ImageMeaning::LinearDepth {},
        },
    });
    let error =
        prep::prepare_native_authored_project(&changed, root.path(), &runtime(), &observation)
            .err()
            .unwrap();
    let prep::Error::Provider { source, .. } = error else {
        panic!("typed numeric consumer refusal")
    };
    assert!(matches!(
        *source,
        motion_host::Error::Render(motion_render::RenderError::AuthoredScene(
            AuthoredSceneError::Admission {
                phase: "source_binding",
                object: Some(1),
                reason: "numeric data cannot enter the color image compositor"
            }
        ))
    ));
    assert!(!observation.running());
}
