//! Opaque desktop output must matte working RGB before the nonlinear view.
use super::*;
use motion_effects::native_color::NativeColorRuntime;

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn opaque_display_mattes_before_native_view_and_keeps_alpha_endpoint_separate() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime = NativeColorRuntime::load(&std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
    ))
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut compared = 0;
    let mut sdr_compared = 0;
    let mut wrong_rgb = 0;
    for (index, case) in reference.cases.iter().enumerate().filter(|(_, c)| {
        c.processor.phase == ColorProcessorPhase::WorkingToDisplay
            && c.processor.input_alpha == AlphaMode::Premultiplied
    }) {
        let (config, selection) = &configs[case.config];
        let native = runtime
            .prepare(
                config,
                selection,
                case.processor.phase,
                case.processor.input_alpha,
                case.processor.output_alpha,
            )
            .unwrap();
        let source_before = serde_json::to_vec(native.source()).unwrap();
        let ordinary = native.source().compile_image(config, selection).unwrap();
        let opaque = native
            .source()
            .compile_display_over_black(config, selection)
            .unwrap();
        assert!(!ordinary.is_opaque_display());
        assert!(opaque.is_opaque_display());
        assert_ne!(
            ordinary.key(),
            opaque.key(),
            "opaque matte must not alias an alpha-preserving cache entry"
        );
        assert_eq!(serde_json::to_vec(native.source()).unwrap(), source_before);
        let mut input = case.input.clone();
        // Undefined RGB behind zero alpha cannot become visible on the window.
        input[0] = [0.75, 0.25, 0.5, 0.];
        let expected: Vec<[f32; 4]> = input
            .iter()
            .map(|p| {
                let rgb = if p[3] > 0. {
                    [p[0], p[1], p[2]]
                } else {
                    [0.; 3]
                };
                let rgb = native.apply_rgb(rgb).unwrap();
                [rgb[0], rgb[1], rgb[2], 1.]
            })
            .collect();
        let input = renderer
            .upload_color_image_f32(
                3,
                4,
                ColorImageContract {
                    space: opaque.input_space(),
                    range: CodeValueRange::FULL,
                    alpha: opaque.input_alpha(),
                    precision: ChannelPrecision::Float32,
                },
                &input,
            )
            .unwrap();
        let output = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    space: opaque.output_space(),
                    range: CodeValueRange::FULL,
                    alpha: opaque.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let pass =
            ColorProcessorOutput::new(&renderer, &input, &opaque, output.contract()).unwrap();
        submit(&renderer, &pass, &output);
        let actual = read(&renderer, &output);
        assert_eq!(actual.len(), expected.len());
        for (pixel, (a, e)) in actual.iter().zip(&expected).enumerate() {
            for channel in 0..4 {
                assert!(
                    a[channel].is_finite()
                        && (a[channel] - e[channel]).abs()
                            <= reference.absolute_tolerance
                                + reference.relative_tolerance * e[channel].abs(),
                    "case={index} pixel={pixel} channel={channel} actual={a:?} CPU={e:?}"
                );
            }
            assert_eq!(a[3].to_bits(), 1_f32.to_bits());
        }
        let display = motion_render::ManagedDisplayTarget::new(&renderer, &input, &opaque).unwrap();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let presenter =
                motion_render::ManagedDisplayPresenter::new(&renderer, &display, format).unwrap();
            let output = managed_display::surface(&renderer, format);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            display.encode(&renderer, &mut encoder).unwrap();
            presenter.encode(&renderer, &mut encoder, &output).unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    display.collect_texture_resources(ledger);
                    presenter.collect_texture_resources(ledger);
                })
                .unwrap();
            let actual = managed_display::pixels(&renderer, &output);
            assert_eq!(
                managed_display::errors(&actual, &expected, format),
                0,
                "opaque surface case={index} format={format:?}"
            );
            assert!(actual.chunks_exact(4).all(|p| p[3] == 255));
            sdr_compared += expected.len();
        }
        let wrong =
            ColorProcessorOutput::new(&renderer, &input, &ordinary, output.contract()).unwrap();
        submit(&renderer, &wrong, &output);
        let wrong = read(&renderer, &output);
        wrong_rgb += wrong
            .iter()
            .zip(&expected)
            .filter(|(a, e)| {
                (0..3).any(|c| {
                    (a[c] - e[c]).abs()
                        > reference.absolute_tolerance + reference.relative_tolerance * e[c].abs()
                })
            })
            .count();
        compared += expected.len();
    }
    assert_eq!(compared, 48);
    assert_eq!(sdr_compared, 192);
    assert!(
        wrong_rgb > 0,
        "actual alpha-preserving GPU output cannot substitute for pre-view working matte"
    );
    for case in reference.cases.iter().filter(|c| {
        c.processor.phase != ColorProcessorPhase::WorkingToDisplay
            || c.processor.input_alpha != AlphaMode::Premultiplied
    }) {
        let (config, selection) = &configs[case.config];
        assert!(case
            .processor
            .compile_display_over_black(config, selection)
            .is_err());
    }
    renderer.check_health().unwrap();
    println!("opaque native display CPU pixels={compared}, SDR pixels={sdr_compared}; actual wrong RGB pixels={wrong_rgb}");
}
