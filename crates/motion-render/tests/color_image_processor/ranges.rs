use super::*;
use motion_render::{ColorRangeMismatch, RenderError};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct RangeReference {
    runtime: String,
    reference_sha256: String,
    absolute_tolerance: f32,
    relative_tolerance: f32,
    cases: Vec<RangeCase>,
}
#[derive(Deserialize)]
struct RangeCase {
    stage: usize,
    input_range: [f64; 2],
    output_range: [f64; 2],
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn golden() -> RangeReference {
    serde_json::from_str(include_str!(
        "../../../motion-effects/tests/color_processor/range_reference.json"
    ))
    .unwrap()
}
fn contract(p: &CompiledColorImageProcessor, input: bool) -> ColorImageContract {
    ColorImageContract {
        space: if input {
            p.input_space()
        } else {
            p.output_space()
        },
        range: if input {
            p.input_range()
        } else {
            p.output_range()
        },
        alpha: if input {
            p.input_alpha()
        } else {
            p.output_alpha()
        },
        precision: ChannelPrecision::Float32,
    }
}
fn differs(actual: &[[f32; 4]], expected: &[[f32; 4]]) -> bool {
    assert_eq!(actual.len(), expected.len());
    actual.iter().zip(expected).any(|(got, want)| {
        (0..3).any(|c| {
            !got[c].is_finite() || (got[c] - want[c]).abs() > 0.0001 + 0.0002 * want[c].abs()
        })
    })
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn limited_rgb_ranges_match_native_cpu_without_clipping_or_changing_alpha() {
    let reference = reference();
    let configs = configs(&reference);
    let golden = golden();
    assert_eq!(golden.runtime, "2.6.0");
    assert_eq!(
        golden.reference_sha256,
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../motion-effects/tests/color_processor/reference.json"
            ))
        )
    );
    assert_eq!(golden.absolute_tolerance, 0.0001);
    assert_eq!(golden.relative_tolerance, 0.0002);
    assert_eq!(golden.cases.len(), 60);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut decode_controls = 0;
    let mut encode_controls = 0;
    for (index, case) in golden.cases.iter().enumerate() {
        let stage = &reference.cases[case.stage];
        let (config, selection) = &configs[stage.config];
        let incoming = CodeValueRange::new(case.input_range[0], case.input_range[1]).unwrap();
        let outgoing = CodeValueRange::new(case.output_range[0], case.output_range[1]).unwrap();
        let p = stage
            .processor
            .compile_image_with_ranges(config, selection, incoming, outgoing)
            .unwrap();
        let source = renderer
            .upload_color_image_f32(3, 4, contract(&p, true), &case.input)
            .unwrap();
        let target = renderer
            .color_image_target(3, 4, contract(&p, false))
            .unwrap();
        assert_eq!(source.contract(), contract(&p, true));
        assert_eq!(target.contract(), contract(&p, false));
        assert_eq!(read(&renderer, &source), case.input);
        let binding = ColorProcessorOutput::new(&renderer, &source, &p, target.contract()).unwrap();
        for _ in 0..2 {
            submit(&renderer, &binding, &target);
            let actual = read(&renderer, &target);
            assert!(
                !differs(&actual, &case.expected),
                "range case={index} got={actual:?} want={:?}",
                case.expected
            );
            for (got, original) in actual.iter().zip(&case.input) {
                assert_eq!(got[3].to_bits(), original[3].to_bits());
            }
            assert_eq!(
                renderer.read_color_image_target(&target).unwrap().contract,
                target.contract()
            );
        }
        // Real GPU negative controls: deliberately interpret the same encoded
        // pixels as full range, or omit output coding. Both must be detectable
        // by the independent CPU oracle at the original tolerances.
        for omit_input in [true, false] {
            if (omit_input && incoming.is_full()) || (!omit_input && outgoing.is_full()) {
                continue;
            }
            let wrong = stage
                .processor
                .compile_image_with_ranges(
                    config,
                    selection,
                    if omit_input {
                        CodeValueRange::FULL
                    } else {
                        incoming
                    },
                    if omit_input {
                        outgoing
                    } else {
                        CodeValueRange::FULL
                    },
                )
                .unwrap();
            let wrong_source = renderer
                .upload_color_image_f32(3, 4, contract(&wrong, true), &case.input)
                .unwrap();
            let wrong_target = renderer
                .color_image_target(3, 4, contract(&wrong, false))
                .unwrap();
            let wrong_binding = ColorProcessorOutput::new(
                &renderer,
                &wrong_source,
                &wrong,
                wrong_target.contract(),
            )
            .unwrap();
            submit(&renderer, &wrong_binding, &wrong_target);
            let changed = differs(&read(&renderer, &wrong_target), &case.expected);
            assert!(
                changed,
                "ineffective range negative control case={index} omit_input={omit_input}"
            );
            if omit_input {
                decode_controls += 1;
            } else {
                encode_controls += 1;
            }
        }
        println!(
            "range case={index} stage={} input={incoming:?} output={outgoing:?} pixels=12",
            case.stage
        );
    }
    assert_eq!((decode_controls, encode_controls), (40, 40));
}

fn mismatch(error: RenderError, boundary: &str, expected: CodeValueRange, actual: CodeValueRange) {
    let RenderError::ColorRangeMismatch(ColorRangeMismatch {
        boundary: got,
        expected: want,
        actual: supplied,
        ..
    }) = error
    else {
        panic!("expected typed range mismatch, got {error:?}");
    };
    assert_eq!((got, want, supplied), (boundary, expected, actual));
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn wrong_code_ranges_refuse_before_allocating_tables_or_writing_destination() {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    let incoming = CodeValueRange::new(16. / 255., 235. / 255.).unwrap();
    let outgoing = CodeValueRange::new(64. / 1023., 940. / 1023.).unwrap();
    let p = case
        .processor
        .compile_image_with_ranges(config, selection, incoming, outgoing)
        .unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let input = contract(&p, true);
    let output = contract(&p, false);
    let source = renderer.color_image_target(3, 4, input).unwrap();
    let wrong_source = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: CodeValueRange::FULL,
                ..input
            },
        )
        .unwrap();
    let destination = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: CodeValueRange::FULL,
                ..output
            },
        )
        .unwrap();
    paint(&renderer, &destination, &case.input);
    let before = read(&renderer, &destination);
    let budget = renderer.allocation_budget();
    let bytes = budget.info().unwrap().admitted_bytes;
    mismatch(
        ColorProcessorOutput::new(&renderer, &wrong_source, &p, output)
            .err()
            .unwrap(),
        "source",
        incoming,
        CodeValueRange::FULL,
    );
    mismatch(
        ColorProcessorOutput::new(&renderer, &source, &p, destination.contract())
            .err()
            .unwrap(),
        "output-declaration",
        outgoing,
        CodeValueRange::FULL,
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, bytes);
    let binding = ColorProcessorOutput::new(&renderer, &source, &p, output).unwrap();
    let bytes = budget.info().unwrap().admitted_bytes;
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    mismatch(
        binding
            .encode(&renderer, &mut encoder, &destination)
            .unwrap_err(),
        "destination",
        outgoing,
        CodeValueRange::FULL,
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert_eq!(read(&renderer, &destination), before);
    assert_eq!(budget.info().unwrap().admitted_bytes, bytes);
}
