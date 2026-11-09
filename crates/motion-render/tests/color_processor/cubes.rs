//! CPU results and shaders come from the freshly built pinned native fixture,
//! through the exact production C ABI; these are not sampled by our adapter.
use super::*;
use motion_effects::AlphaMode;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{ColorImageContract, ColorProcessorOutput};

#[derive(Deserialize)]
struct NativeCubes {
    version: u32,
    runtime: String,
    cases: Vec<Cube>,
}
#[derive(Deserialize)]
struct Cube {
    edge: u32,
    tetrahedral: bool,
    mixed: bool,
    shader: String,
    tables: Vec<ColorProcessorTable>,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}

pub(super) fn image(
    renderer: &mut Renderer,
    source: &ColorProcessorSource,
    config: &VerifiedColorConfig,
    selection: &ColorSelection,
    pixels: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    let program = source.compile_image(config, selection).unwrap();
    let contract = |space, alpha| ColorImageContract {
        space,
        alpha,
        range: CodeValueRange::FULL,
        precision: ChannelPrecision::Float32,
    };
    let input = renderer
        .upload_color_image_f32(
            3,
            4,
            contract(program.input_space(), program.input_alpha()),
            pixels,
        )
        .unwrap();
    let output = renderer
        .color_image_target(
            3,
            4,
            contract(program.output_space(), program.output_alpha()),
        )
        .unwrap();
    let binding = ColorProcessorOutput::new(renderer, &input, &program, output.contract()).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("native cube image qualification"),
        })
        .unwrap();
    binding.encode(renderer, &mut encoder, &output).unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(input.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
            ledger.merge(binding.texture_resource_ledger());
        })
        .unwrap();
    renderer
        .read_color_image_target(&output)
        .unwrap()
        .pixels
        .chunks_exact(16)
        .map(|pixel| {
            std::array::from_fn(|c| f32::from_le_bytes(pixel[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect()
}

#[test]
#[ignore = "requires fresh pinned native OCIO cube fixture and actual GPU"]
fn native_cube_buffers_match_cpu_for_linear_tetrahedral_and_mixed_tables() {
    let path = std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_FAILURE_LIBRARY_DIR")
            .expect("explicit native qualification directory"),
    )
    .join("native-cubes.json");
    let fixture: NativeCubes = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(fixture.version, 1);
    assert_eq!(fixture.runtime, "2.6.0");
    assert_eq!(fixture.cases.len(), 8);
    let mut reference: Reference = serde_json::from_str(include_str!(
        "../../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    let item = reference.configs.remove(0);
    let config = VerifiedColorConfig::new(
        item.manifest,
        Arc::from(item.config.as_bytes()),
        BTreeMap::new(),
    )
    .unwrap();
    let mut template = reference.cases.remove(0).processor;
    // The config/selection below identify this test adapter invocation only.
    // Inline 3D transforms are independent fixture programs, not a claim that
    // the resource-free project config itself contains a FileTransform.
    template.config_key = config.key();
    template.selection_key = config.resolve(&item.selection).unwrap().key;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    println!("native cube adapter={:?}", renderer.adapter_info);
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        _ => {}
    }
    let mut pixels_checked = 0;
    for case in fixture.cases {
        assert_eq!(case.input.len(), 12);
        assert_eq!(case.expected.len(), 12);
        assert_eq!(case.tables.len(), if case.mixed { 2 } else { 1 });
        let cube = case.tables.iter().find(|table| table.depth > 1).unwrap();
        assert_eq!(cube.depth, case.edge);
        assert_eq!(
            cube.interpolation,
            if case.tetrahedral {
                TableInterpolation::Nearest
            } else {
                TableInterpolation::Linear
            }
        );
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let mut source = template.clone();
            source.shader = case.shader.clone();
            source.tables = case.tables.clone();
            source.input_alpha = alpha;
            source.output_alpha = alpha;
            let pixels: Vec<_> = case
                .input
                .iter()
                .map(|pixel| {
                    let mut pixel = *pixel;
                    if alpha == AlphaMode::Premultiplied {
                        for c in 0..3 {
                            pixel[c] *= pixel[3];
                        }
                    }
                    pixel
                })
                .collect();
            let compiled = source.compile(&config, &item.selection).unwrap();
            assert!(!compiled.wgsl().contains("textureSample"));
            let actual = run(&renderer, &compiled, &pixels);
            let image = image(&mut renderer, &source, &config, &item.selection, &pixels);
            assert_eq!(image.len(), 12);
            for (path, values) in [("compute", &actual[..12]), ("image", image.as_slice())] {
                for (index, (got, want)) in values.iter().zip(&case.expected).enumerate() {
                    // Zero associated alpha carries zero RGB into the transform.
                    // The fixture's zero-alpha input is already black.
                    for c in 0..3 {
                        let want = want[c]
                            * if alpha == AlphaMode::Premultiplied {
                                want[3]
                            } else {
                                1.
                            };
                        let tolerance = reference.absolute_tolerance
                            + reference.relative_tolerance * want.abs();
                        assert!(
                            (got[c] - want).abs() <= tolerance,
                            "path={path} edge={} tetra={} mixed={} alpha={alpha:?} pixel={index} c={c} actual={} CPU={want} tolerance={tolerance}",
                            case.edge,
                            case.tetrahedral,
                            case.mixed,
                            got[c]
                        );
                    }
                    assert_eq!(got[3].to_bits(), case.input[index][3].to_bits());
                    pixels_checked += 1;
                }
            }
            // Deliberate table-axis corruption must be caught by these CPU
            // references, even when vertex endpoints happen to coincide.
            let mut wrong = source.clone();
            let table = wrong
                .tables
                .iter_mut()
                .find(|table| table.depth > 1)
                .unwrap();
            let edge = table.depth as usize;
            let values = table.values.clone();
            for z in 0..edge {
                for y in 0..edge {
                    for x in 0..edge {
                        for c in 0..3 {
                            table.values[((z * edge + y) * edge + x) * 3 + c] =
                                values[((x * edge + y) * edge + z) * 3 + c];
                        }
                    }
                }
            }
            let wrong = run(
                &renderer,
                &wrong.compile(&config, &item.selection).unwrap(),
                &pixels,
            );
            assert!(
                wrong
                    .iter()
                    .zip(&actual)
                    .any(|(a, b)| (0..3).any(|c| (a[c] - b[c]).abs() > 0.01)),
                "axis negative control was invisible"
            );
        }
    }
    assert_eq!(pixels_checked, 384);
    println!(
        "native cube CPU/GPU pixels={pixels_checked}; 8 cases; 2 alpha modes; axis negative controls and over-dispatch sentinels passed"
    );
}
