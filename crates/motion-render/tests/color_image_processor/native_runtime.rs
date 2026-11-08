//! Execute the freshly built platform program, not only the saved reference
//! program. Pixel tolerances and all old reference bytes remain unchanged.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use std::path::{Path, PathBuf};

fn execute(
    renderer: &Renderer,
    config: &VerifiedColorConfig,
    selection: &ColorSelection,
    source: &ColorProcessorSource,
    case: &Case,
) -> Vec<[f32; 4]> {
    let p = source.compile_image(config, selection).unwrap();
    let contract = |space, alpha| ColorImageContract {
        space,
        alpha,
        range: CodeValueRange::FULL,
        precision: ChannelPrecision::Float32,
    };
    let input = renderer
        .color_image_target(3, 4, contract(p.input_space(), p.input_alpha()))
        .unwrap();
    let output = renderer
        .color_image_target(3, 4, contract(p.output_space(), p.output_alpha()))
        .unwrap();
    paint(renderer, &input, &case.input);
    let binding = ColorProcessorOutput::new(renderer, &input, &p, output.contract()).unwrap();
    submit(renderer, &binding, &output);
    read(renderer, &output)
}
fn compare(actual: &[[f32; 4]], case: &Case, reference: &Reference, context: &str) {
    assert_eq!(actual.len(), case.expected.len());
    for (i, (a, e)) in actual.iter().zip(&case.expected).enumerate() {
        for c in 0..4 {
            assert!(
                a[c].is_finite()
                    && (a[c] - e[c]).abs()
                        <= reference.absolute_tolerance + reference.relative_tolerance * e[c].abs(),
                "{context} pixel={i} channel={c} actual={a:?} expected={e:?}"
            );
        }
        assert_eq!(
            a[3].to_bits(),
            case.input[i][3].to_bits(),
            "{context} alpha is exact"
        );
    }
}

#[test]
#[ignore = "requires explicit pinned native OCIO prefix and an actual GPU"]
fn freshly_prepared_native_programs_match_original_cpu_pixels_on_the_gpu() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime = NativeColorRuntime::load(&PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
    ))
    .unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (index, case) in reference.cases.iter().enumerate() {
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
        let actual = execute(&renderer, config, selection, native.source(), case);
        // Save real platform output before any numerical assertion can fail.
        if let Some(root) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
            let root = Path::new(&root).join("native-programs");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                root.join(format!("case-{index}.json")),
                serde_json::to_vec(
                    &serde_json::json!({"processor":native.source(),"actual":actual}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        compare(
            &actual,
            case,
            &reference,
            &format!("fresh native case={index}"),
        );
        println!(
            "fresh native case={index} actual GPU pixels={} passed",
            actual.len()
        );
    }
    // Execute deliberately wrong GPU programs as negative controls for the
    // pixel gate. Structural comparison alone is not numerical qualification.
    let case = &reference.cases[8];
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
    for poison_table in [false, true] {
        let mut bad = native.source().clone();
        if poison_table {
            bad.tables[0].values.fill(0.);
        } else {
            assert!(bad.shader.contains("return outColor;"));
            bad.shader =
                bad.shader
                    .replacen("return outColor;", "return vec4(0.,0.,0.,inPixel.a);", 1);
        }
        let actual = execute(&renderer, config, selection, &bad, case);
        assert!(actual.iter().zip(&case.expected).any(|(a,e)|(0..3).any(|c|!a[c].is_finite() || (a[c]-e[c]).abs()>reference.absolute_tolerance+reference.relative_tolerance*e[c].abs())),
            "actual wrong GPU program poison_table={poison_table} must fail the unchanged pixel oracle");
    }
}

#[test]
#[ignore = "requires captured CI case-8 JSON programs and an actual GPU"]
fn captured_linux_and_macos_programs_match_original_cpu_pixels_on_the_gpu() {
    let reference = reference();
    let configs = configs(&reference);
    let root = PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_CAPTURED_CI").expect("captured immutable CI directory"),
    );
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let case = &reference.cases[8];
    let (config, selection) = &configs[case.config];
    for platform in ["linux", "macos"] {
        let raw = std::fs::read(root.join(format!("{platform}-actual-case-8.json"))).unwrap();
        let program: ColorProcessorSource = serde_json::from_slice(&raw).unwrap();
        let actual = execute(&renderer, config, selection, &program, case);
        compare(&actual, case, &reference, platform);
        println!("captured {platform} CI case=8 executed on this machine GPU: all {} pixels match unchanged CPU reference",actual.len());
    }
}
