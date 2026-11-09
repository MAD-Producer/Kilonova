use motion_effects::color_lut::{ColorLut, LutInterpolation, LutPurpose};
use motion_effects::image_contract::{ChannelPrecision, DataChannels, ImageContract, ImageMeaning};
use motion_effects::{AlphaMode, WorkingSpace};
use motion_render::{ImageCaptureTarget, LutOutput, Renderer};
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    tolerance: f32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    cube: String,
    interpolation: LutInterpolation,
    alpha: AlphaMode,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn contract(alpha: AlphaMode, precision: ChannelPrecision) -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha,
        },
        precision,
    )
}
fn target(renderer: &Renderer, width: u32, declaration: ImageContract) -> ImageCaptureTarget {
    renderer
        .image_capture_target(width, 1, declaration)
        .unwrap()
}
fn paint(renderer: &Renderer, target: &ImageCaptureTarget, pixels: &[[f32; 4]]) {
    let pixels = pixels
        .iter()
        .map(|v| format!("vec4<f32>({:?},{:?},{:?},{:?})", v[0], v[1], v[2], v[3]))
        .collect::<Vec<_>>()
        .join(",");
    let shader = renderer.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("independent LUT numerical input"), source: wgpu::ShaderSource::Wgsl(format!("
        @vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{
            let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(p[i],0.,1.);}}
        @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{
            let values=array({pixels});return values[u32(p.x)];}}").into()),
    });
    let pipeline = renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("independent LUT numerical input"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: motion_render::image_formats::native_format(target.storage()),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })
        .unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1. } else { -1. };
    let exponent = i32::from((bits >> 10) & 31);
    let mantissa = f32::from(bits & 1023);
    assert!(exponent < 31, "unexpected nonfinite half output {bits:x}");
    sign * if exponent == 0 {
        mantissa * 2_f32.powi(-24)
    } else {
        (1024. + mantissa) * 2_f32.powi(exponent - 25)
    }
}
fn read(renderer: &Renderer, target: &ImageCaptureTarget) -> Vec<[f32; 4]> {
    let bytes = renderer.read_image_target(target).unwrap().pixels;
    match target.contract().precision {
        ChannelPrecision::Float32 => bytes
            .chunks_exact(16)
            .map(|v| {
                std::array::from_fn(|c| f32::from_le_bytes(v[c * 4..c * 4 + 4].try_into().unwrap()))
            })
            .collect(),
        ChannelPrecision::Float16 => bytes
            .chunks_exact(8)
            .map(|v| {
                std::array::from_fn(|c| {
                    half(u16::from_le_bytes(v[c * 2..c * 2 + 2].try_into().unwrap()))
                })
            })
            .collect(),
        _ => panic!("floating fixture only"),
    }
}
fn submit(renderer: &Renderer, conversion: &LutOutput, output: &ImageCaptureTarget) {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    conversion.encode(renderer, &mut encoder, output).unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(conversion.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
        })
        .unwrap();
}
fn cube(name: &str) -> &'static str {
    match name {
        "one-d.cube" => include_str!("../../motion-effects/tests/color_lut/one-d.cube"),
        "three-d.cube" => include_str!("../../motion-effects/tests/color_lut/three-d.cube"),
        _ => panic!("unexpected reference"),
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_float_luts_match_all_declared_references_without_clamping_hdr_or_alpha() {
    let reference: Reference = serde_json::from_str(include_str!(
        "../../motion-effects/tests/color_lut/reference.json"
    ))
    .unwrap();
    assert_eq!(reference.cases.len(), 10);
    assert_eq!(reference.tolerance, 0.00001);
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for case in reference.cases {
        let lut =
            ColorLut::from_cube(cube(&case.cube), case.interpolation, LutPurpose::Look).unwrap();
        let source = target(
            &renderer,
            case.input.len() as u32,
            contract(case.alpha, ChannelPrecision::Float32),
        );
        paint(&renderer, &source, &case.input);
        for precision in [ChannelPrecision::Float32, ChannelPrecision::Float16] {
            let output = target(
                &renderer,
                case.input.len() as u32,
                contract(case.alpha, precision),
            );
            let binding = LutOutput::new(&renderer, &source, &lut, output.contract()).unwrap();
            assert_eq!(binding.key(), lut.key());
            assert_eq!(binding.output_contract(), output.contract());
            submit(&renderer, &binding, &output);
            let actual = read(&renderer, &output);
            assert_eq!(actual.len(), 12);
            for (index, (got, expected)) in actual.iter().zip(&case.expected).enumerate() {
                for c in 0..4 {
                    // Binary16 adds at most one half ULP relative rounding plus
                    // the unchanged Float32 transform tolerance. Float32 has no
                    // storage allowance. This is specified before measurement.
                    let storage_error = if precision == ChannelPrecision::Float16 {
                        expected[c].abs() * (1. / 1024.) + 2_f32.powi(-24)
                    } else {
                        0.
                    };
                    assert!(
                        (got[c] - expected[c]).abs() <= reference.tolerance + storage_error,
                        "{} {:?} {:?} {precision:?} pixel={index} channel={c} got={got:?} expected={expected:?}",
                        case.cube,
                        case.interpolation,
                        case.alpha
                    );
                }
                if precision == ChannelPrecision::Float32 {
                    assert_eq!(got[3].to_bits(), case.input[index][3].to_bits());
                }
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_lut_rejects_incompatible_targets_and_budget_without_changing_pixels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let declaration = contract(AlphaMode::Straight, ChannelPrecision::Float32);
    let source = target(&renderer, 2, declaration);
    let output = target(&renderer, 2, declaration);
    let original = [[0.125, -0.5, 2., 1.], [0.25, 0.5, 1., 0.3]];
    paint(&renderer, &source, &original);
    paint(&renderer, &output, &original);
    let lut = ColorLut::from_cube(
        cube("three-d.cube"),
        LutInterpolation::Tetrahedral,
        LutPurpose::Working,
    )
    .unwrap();
    let binding = LutOutput::new(&renderer, &source, &lut, declaration).unwrap();
    let before = read(&renderer, &output);
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut encoder, &source)
        .unwrap_err()
        .to_string()
        .contains("distinct"));
    let short = target(&renderer, 1, declaration);
    assert!(binding
        .encode(&renderer, &mut encoder, &short)
        .unwrap_err()
        .to_string()
        .contains("extents"));
    let premult = target(
        &renderer,
        2,
        contract(AlphaMode::Premultiplied, ChannelPrecision::Float32),
    );
    assert!(binding
        .encode(&renderer, &mut encoder, &premult)
        .unwrap_err()
        .to_string()
        .contains("contract"));
    drop(encoder);
    assert_eq!(read(&renderer, &output), before);
    for meaning in [
        ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
        ImageMeaning::Coverage {},
        ImageMeaning::RelativeDepth {},
        ImageMeaning::LinearDepth {},
        ImageMeaning::Normal {},
    ] {
        let data_contract = ImageContract::new(meaning, ChannelPrecision::Float32);
        let data = target(&renderer, 2, data_contract);
        assert!(LutOutput::new(&renderer, &data, &lut, declaration).is_err());
        assert!(LutOutput::new(&renderer, &source, &lut, data_contract).is_err());
    }
    assert!(LutOutput::new(
        &renderer,
        &source,
        &lut,
        contract(AlphaMode::Straight, ChannelPrecision::Unorm8)
    )
    .is_err());
    let other = pollster::block_on(Renderer::headless()).unwrap();
    assert!(LutOutput::new(&other, &source, &lut, declaration).is_err());
    let mut foreign_encoder = other
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut foreign_encoder, &output)
        .is_err());
    assert!(binding
        .encode(&other, &mut foreign_encoder, &output)
        .is_err());
    drop(foreign_encoder);
    let budget = renderer.allocation_budget();
    let baseline = budget.info().unwrap();
    budget.set_limit(baseline.admitted_bytes + 64).unwrap();
    let refusal = match LutOutput::new(&renderer, &source, &lut, declaration) {
        Err(error) => error,
        Ok(_) => panic!("table must exceed budget"),
    };
    assert!(refusal.resource_rejection().is_some());
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        baseline.admitted_bytes
    );
    budget.set_limit(baseline.budget_bytes).unwrap();
    submit(&renderer, &binding, &output);
    let mut expected = original;
    lut.apply_rgba(&mut expected, AlphaMode::Straight).unwrap();
    for (got, expected) in read(&renderer, &output).iter().zip(expected) {
        for c in 0..4 {
            assert!((got[c] - expected[c]).abs() < 0.00001);
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn lut_bindings_reuse_buffers_and_pending_commands_retain_their_capacity() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let declaration = contract(AlphaMode::Premultiplied, ChannelPrecision::Float32);
    let source = target(&renderer, 2, declaration);
    let output = target(&renderer, 2, declaration);
    paint(
        &renderer,
        &source,
        &[[0.25, 0.5, 1., 1.], [0.1, 0.2, 0.3, 0.5]],
    );
    let lut = ColorLut::from_cube(
        cube("one-d.cube"),
        LutInterpolation::Linear,
        LutPurpose::Look,
    )
    .unwrap();
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let binding = LutOutput::new(&renderer, &source, &lut, declaration).unwrap();
    let owned = renderer.allocation_budget().info().unwrap().admitted_bytes;
    assert_eq!(owned - baseline, 64 + lut.values().len() as u64 * 16);
    for _ in 0..3 {
        submit(&renderer, &binding, &output);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            owned
        );
    }
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    binding.encode(&renderer, &mut encoder, &output).unwrap();
    let commands = encoder.finish();
    drop(binding);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        owned
    );
    renderer
        .submit_allocated(commands, |ledger| {
            ledger.merge(source.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
        })
        .unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
    assert_ne!(read(&renderer, &output), read(&renderer, &source));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn floating_sources_and_explicit_alpha_conversions_preserve_the_declared_meaning() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let lut = ColorLut::from_cube(
        cube("three-d.cube"),
        LutInterpolation::Tetrahedral,
        LutPurpose::Output,
    )
    .unwrap();
    for input_precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        for input_alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let source = target(&renderer, 3, contract(input_alpha, input_precision));
            // Exactly representable binary inputs isolate alpha conversion from
            // input quantization. The middle pixel explicitly exercises alpha 0.
            let input = [
                [0.25, -0.125, 0.5, 0.5],
                [0.125, 0.25, 0.5, 0.],
                [1., 0.5, 2., 1.],
            ];
            paint(&renderer, &source, &input);
            assert_eq!(read(&renderer, &source), input);
            let mut straight = input;
            for pixel in &mut straight {
                if input_alpha == AlphaMode::Premultiplied {
                    for c in 0..3 {
                        pixel[c] = if pixel[3] == 0. {
                            0.
                        } else {
                            pixel[c] / pixel[3]
                        };
                    }
                }
            }
            lut.apply_rgba(&mut straight, AlphaMode::Straight).unwrap();
            for output_alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
                for output_precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
                    let output = target(&renderer, 3, contract(output_alpha, output_precision));
                    let binding =
                        LutOutput::new(&renderer, &source, &lut, output.contract()).unwrap();
                    submit(&renderer, &binding, &output);
                    let mut expected = straight;
                    if output_alpha == AlphaMode::Premultiplied {
                        for pixel in &mut expected {
                            for c in 0..3 {
                                pixel[c] *= pixel[3];
                            }
                        }
                    }
                    for (got, want) in read(&renderer, &output).iter().zip(expected) {
                        for c in 0..4 {
                            let quantization = if output_precision == ChannelPrecision::Float16 {
                                want[c].abs() / 1024. + 2_f32.powi(-24)
                            } else {
                                0.
                            };
                            assert!(
                                (got[c] - want[c]).abs() <= 0.00001 + quantization,
                                "{input_alpha:?} {input_precision:?} -> {output_alpha:?} {output_precision:?}: {got:?} {want:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    let large = ColorLut::from_cube(
        "LUT_1D_SIZE 2\n0 0 0\n70000 70000 70000\n",
        LutInterpolation::Linear,
        LutPurpose::Look,
    )
    .unwrap();
    let source = target(
        &renderer,
        1,
        contract(AlphaMode::Straight, ChannelPrecision::Float32),
    );
    assert!(LutOutput::new(
        &renderer,
        &source,
        &large,
        contract(AlphaMode::Straight, ChannelPrecision::Float16)
    )
    .is_err());
    assert!(LutOutput::new(&renderer, &source, &large, source.contract()).is_ok());
}
