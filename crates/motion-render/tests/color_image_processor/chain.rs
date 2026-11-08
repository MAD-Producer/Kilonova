use super::*;
use motion_render::{ColorSpaceMismatch, RenderError};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct ChainReference {
    runtime: String,
    reference_sha256: String,
    absolute_tolerance: f32,
    relative_tolerance: f32,
    cases: Vec<ChainCase>,
}
#[derive(Deserialize)]
struct ChainCase {
    config: usize,
    stages: [usize; 2],
    intermediate: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn input_contract(p: &CompiledColorImageProcessor) -> ColorImageContract {
    ColorImageContract {
        range: motion_effects::color_processor::CodeValueRange::FULL,
        space: p.input_space(),
        alpha: p.input_alpha(),
        precision: ChannelPrecision::Float32,
    }
}
fn output_contract(p: &CompiledColorImageProcessor) -> ColorImageContract {
    ColorImageContract {
        range: motion_effects::color_processor::CodeValueRange::FULL,
        space: p.output_space(),
        alpha: p.output_alpha(),
        precision: ChannelPrecision::Float32,
    }
}
fn assert_pixels(got: &[[f32; 4]], want: &[[f32; 4]], abs: f32, rel: f32) -> f32 {
    assert_eq!(got.len(), 12);
    assert_eq!(got.len(), want.len());
    let mut max_fraction = 0.0_f32;
    for (pixel, (got, want)) in got.iter().zip(want).enumerate() {
        for channel in 0..4 {
            let tolerance = abs + rel * want[channel].abs();
            let error = (got[channel] - want[channel]).abs();
            assert!(
                got[channel].is_finite() && error <= tolerance,
                "pixel={pixel} channel={channel} got={got:?} want={want:?} tolerance={tolerance}"
            );
            max_fraction = max_fraction.max(error / tolerance);
        }
        assert_eq!(got[3].to_bits(), want[3].to_bits());
    }
    max_fraction
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn input_working_and_display_or_output_chain_matches_native_cpu_without_readback_between_passes() {
    let reference = reference();
    let configs = configs(&reference);
    let golden: ChainReference = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../motion-effects/tests/color_processor/chain_reference.json"
    )))
    .unwrap();
    assert_eq!(golden.runtime, "2.6.0");
    assert_eq!(
        golden.reference_sha256,
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../motion-effects/tests/color_processor/reference.json"
            )))
        )
    );
    assert_eq!(
        (golden.absolute_tolerance, golden.relative_tolerance),
        (0.0001, 0.0002)
    );
    assert_eq!(golden.cases.len(), 14);
    eprintln!("chain qualification phase=renderer_create");
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    eprintln!(
        "chain qualification phase=renderer_ready adapter={:?}",
        renderer.adapter_info
    );
    for (index, case) in golden.cases.iter().enumerate() {
        eprintln!("chain qualification case={index} phase=prepare");
        let (config, selection) = &configs[case.config];
        let upstream = &reference.cases[case.stages[0]];
        let downstream = &reference.cases[case.stages[1]];
        assert_eq!(upstream.config, case.config);
        assert_eq!(downstream.config, case.config);
        let first = upstream.processor.compile_image(config, selection).unwrap();
        let second = downstream
            .processor
            .compile_image(config, selection)
            .unwrap();
        assert_eq!(output_contract(&first), input_contract(&second));
        let source = renderer
            .color_image_target(3, 4, input_contract(&first))
            .unwrap();
        let working = renderer
            .color_image_target(3, 4, output_contract(&first))
            .unwrap();
        let target = renderer
            .color_image_target(3, 4, output_contract(&second))
            .unwrap();
        eprintln!("chain qualification case={index} phase=paint_source");
        paint(&renderer, &source, &upstream.input);
        eprintln!("chain qualification case={index} phase=first_pipeline");
        let first =
            ColorProcessorOutput::new(&renderer, &source, &first, working.contract()).unwrap();
        let second =
            ColorProcessorOutput::new(&renderer, &working, &second, target.contract()).unwrap();
        eprintln!("chain qualification case={index} phase=pipelines_ready");
        let admitted = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let mut max_fraction = 0.0_f32;
        for iteration in 0..3 {
            eprintln!("chain qualification case={index} iteration={iteration} phase=encode");
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            first.encode(&renderer, &mut encoder, &working).unwrap();
            second.encode(&renderer, &mut encoder, &target).unwrap();
            assert_eq!(
                renderer.allocation_budget().info().unwrap().admitted_bytes,
                admitted
            );
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(first.texture_resource_ledger());
                    ledger.merge(second.texture_resource_ledger());
                    ledger.merge(working.texture_resource_ledger());
                    ledger.merge(target.texture_resource_ledger());
                })
                .unwrap();
            eprintln!("chain qualification case={index} iteration={iteration} phase=readback");
            max_fraction = max_fraction.max(assert_pixels(
                &read(&renderer, &working),
                &case.intermediate,
                golden.absolute_tolerance,
                golden.relative_tolerance,
            ));
            max_fraction = max_fraction.max(assert_pixels(
                &read(&renderer, &target),
                &case.expected,
                golden.absolute_tolerance,
                golden.relative_tolerance,
            ));
            let captured = renderer.read_color_image_target(&target).unwrap();
            assert_eq!(captured.contract, second.output_contract());
            assert_eq!((captured.width, captured.height), (3, 4));
            assert_eq!(
                renderer.allocation_budget().info().unwrap().admitted_bytes,
                admitted
            );
        }
        println!(
            "chain case={index} config={} max_tolerance_fraction={max_fraction}",
            case.config
        );
    }
}
fn mismatch(
    error: RenderError,
    boundary: &str,
    expected: ColorSpaceIdentity,
    actual: ColorSpaceIdentity,
) {
    let RenderError::ColorSpaceMismatch(ColorSpaceMismatch {
        boundary: got,
        expected: want,
        actual: supplied,
        ..
    }) = error
    else {
        panic!("expected typed space mismatch, got {error:?}");
    };
    assert_eq!(got, boundary);
    assert_eq!(want, expected);
    assert_eq!(supplied, actual);
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn wrong_spaces_and_configuration_are_refused_before_allocation_or_destination_writes() {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    let p = case.processor.compile_image(config, selection).unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer
        .color_image_target(3, 4, input_contract(&p))
        .unwrap();
    let destination = renderer
        .color_image_target(3, 4, output_contract(&p))
        .unwrap();
    paint(&renderer, &source, &case.input);
    let binding =
        ColorProcessorOutput::new(&renderer, &source, &p, destination.contract()).unwrap();
    let mut spaces = vec![
        p.output_space(),
        config.display_identity(&selection.display).unwrap(),
    ];
    spaces.push(configs[1].0.space_identity(&configs[1].1.input).unwrap());
    for space in spaces {
        assert_ne!(space, p.input_space());
        let wrong = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    space,
                    ..input_contract(&p)
                },
            )
            .unwrap();
        let wrong_output = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: p.input_space(),
                    ..destination.contract()
                },
            )
            .unwrap();
        paint(&renderer, &wrong_output, &case.input);
        let before = read(&renderer, &wrong_output);
        let bytes = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let error = ColorProcessorOutput::new(&renderer, &wrong, &p, destination.contract())
            .err()
            .unwrap();
        mismatch(error, "source", p.input_space(), space);
        let declaration = ColorImageContract {
            range: motion_effects::color_processor::CodeValueRange::FULL,
            space: p.input_space(),
            ..destination.contract()
        };
        let error = ColorProcessorOutput::new(&renderer, &source, &p, declaration)
            .err()
            .unwrap();
        mismatch(
            error,
            "output-declaration",
            p.output_space(),
            p.input_space(),
        );
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        mismatch(
            binding
                .encode(&renderer, &mut encoder, &wrong_output)
                .unwrap_err(),
            "destination",
            p.output_space(),
            p.input_space(),
        );
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        assert_eq!(read(&renderer, &wrong_output), before);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            bytes
        );
    }
    submit(&renderer, &binding, &destination);
    assert_pixels(
        &read(&renderer, &destination),
        &case.expected,
        reference.absolute_tolerance,
        reference.relative_tolerance,
    );
}
