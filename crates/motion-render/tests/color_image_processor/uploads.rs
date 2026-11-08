use super::*;
use motion_render::{ColorImageUploadError, RenderError};

fn contract(processor: &CompiledColorImageProcessor, input: bool) -> ColorImageContract {
    ColorImageContract {
        space: if input {
            processor.input_space()
        } else {
            processor.output_space()
        },
        alpha: if input {
            processor.input_alpha()
        } else {
            processor.output_alpha()
        },
        precision: ChannelPrecision::Float32,
    }
}

fn program() -> CompiledColorImageProcessor {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    case.processor.compile_image(config, selection).unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn uploaded_float32_color_keeps_exact_input_bits_and_matches_native_ocio_reference() {
    let reference = reference();
    let configs = configs(&reference);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (index, case) in reference.cases.iter().enumerate() {
        let (config, selection) = &configs[case.config];
        let processor = case.processor.compile_image(config, selection).unwrap();
        let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let source = renderer
            .upload_color_image_f32(3, 4, contract(&processor, true), &case.input)
            .unwrap();
        assert_eq!(source.image().readback_bytes(), 0);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes - before,
            3 * 4 * 16
        );
        let exact = read(&renderer, &source);
        for (actual, expected) in exact.iter().zip(&case.input) {
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
        }
        let output = renderer
            .color_image_target(3, 4, contract(&processor, false))
            .unwrap();
        assert_eq!(output.image().readback_bytes(), 0);
        let binding =
            ColorProcessorOutput::new(&renderer, &source, &processor, output.contract()).unwrap();
        let before_encode = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let before_read = renderer.image_readback_info();
        submit(&renderer, &binding, &output);
        assert_eq!(
            renderer.image_readback_info(),
            before_read,
            "encode must not read pixels back"
        );
        let actual = read(&renderer, &output);
        for (pixel, (got, want)) in actual.iter().zip(&case.expected).enumerate() {
            for channel in 0..4 {
                let tolerance = reference.absolute_tolerance
                    + reference.relative_tolerance * want[channel].abs();
                assert!(
                    got[channel].is_finite() && (got[channel] - want[channel]).abs() <= tolerance,
                    "case={index} pixel={pixel} channel={channel} got={got:?} want={want:?}"
                );
            }
            assert_eq!(got[3].to_bits(), want[3].to_bits());
        }
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before_encode,
            "temporary readback staging must retire before return"
        );
        assert_eq!(output.image().readback_bytes(), 0);
        drop(binding);
        drop(source);
        drop(output);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn color_target_needs_only_texture_budget_and_explicit_readback_refusal_recovers() {
    let processor = program();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let initial = budget.info().unwrap();
    let texture_bytes = 17 * 2 * 16;
    budget
        .set_limit(initial.admitted_bytes + texture_bytes)
        .unwrap();
    let target = renderer
        .color_image_target(17, 2, contract(&processor, true))
        .unwrap();
    assert_eq!(target.image().texture_bytes(), texture_bytes);
    assert_eq!(target.image().readback_bytes(), 0);
    let readback = renderer.image_readback_info();
    let error = renderer.read_color_image_target(&target).unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error:?}");
    let failed = renderer.image_readback_info();
    assert_eq!(failed.failed_calls, readback.failed_calls + 1);
    assert_eq!(failed.in_flight_calls, 0);
    assert_eq!(failed.submitted_calls, readback.submitted_calls);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        initial.admitted_bytes + texture_bytes
    );
    budget.set_limit(initial.budget_bytes).unwrap();
    let pixels = renderer.read_color_image_target(&target).unwrap();
    assert_eq!(pixels.pixels, vec![0; texture_bytes as usize]);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        initial.admitted_bytes + texture_bytes
    );
    assert_eq!(target.image().readback_bytes(), 0);
    drop(target);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        initial.admitted_bytes
    );
    let capture = renderer
        .image_capture_target(
            17,
            2,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: processor.input_alpha(),
                },
                ChannelPrecision::Float32,
            ),
        )
        .unwrap();
    assert_eq!(
        capture.readback_bytes(),
        512 * 2,
        "explicit capture targets retain their reusable staging"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_float_uploads_are_typed_refusals_before_allocation_and_valid_upload_recovers() {
    use std::error::Error;
    let processor = program();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let input = contract(&processor, true);
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    for (width, height, pixels) in [(0, 1, vec![]), (1, 1, vec![]), (1, 1, vec![[0.; 4]; 2])] {
        assert!(matches!(
            renderer.upload_color_image_f32(width, height, input, &pixels),
            Err(RenderError::ColorImageUpload(
                ColorImageUploadError::Count { .. }
            ))
        ));
    }
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
    ] {
        assert!(
            matches!(renderer.upload_color_image_f32(1, 1, ColorImageContract { precision, ..input }, &[[0.; 4]]),
            Err(RenderError::ColorImageUpload(ColorImageUploadError::Precision(got))) if got == precision)
        );
    }
    for channel in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut pixels = [[0., 0., 0., 1.]; 2];
            pixels[1][channel] = value;
            let error = renderer
                .upload_color_image_f32(2, 1, input, &pixels)
                .err()
                .unwrap();
            assert!(
                matches!(&error, RenderError::ColorImageUpload(ColorImageUploadError::Pixel { pixel: 1, channel: got }) if *got == channel)
            );
            assert!(error
                .source()
                .unwrap()
                .downcast_ref::<ColorImageUploadError>()
                .is_some());
        }
    }
    for alpha in [-0.01, 1.01] {
        assert!(matches!(
            renderer.upload_color_image_f32(1, 1, input, &[[0., 0., 0., alpha]]),
            Err(RenderError::ColorImageUpload(
                ColorImageUploadError::Pixel {
                    pixel: 0,
                    channel: 3
                }
            ))
        ));
    }
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    assert_eq!(renderer.image_readback_info().requested_calls, 0);
    let values = [
        [-1.25, 16.125, -0.0, 0.],
        [0.125, 65536., f32::MIN_POSITIVE, 1.],
    ];
    let target = renderer
        .upload_color_image_f32(2, 1, input, &values)
        .unwrap();
    for (got, want) in read(&renderer, &target).iter().zip(values) {
        assert_eq!(got.map(f32::to_bits), want.map(f32::to_bits));
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn pending_float_upload_retains_its_reservation_after_target_drop_until_submission_finishes() {
    let processor = program();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap().admitted_bytes;
    let target = renderer
        .upload_color_image_f32(3, 4, contract(&processor, true), &[[0., 0., 0., 1.]; 12])
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before + 3 * 4 * 16);
    drop(target);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before + 3 * 4 * 16,
        "queue-owned unsubmitted upload must retain its destination allowance"
    );
    assert_eq!(renderer.image_readback_info().requested_calls, 0);
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
}
