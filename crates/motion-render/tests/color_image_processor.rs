use motion_effects::{
    color_config::*,
    color_processor::*,
    image_contract::{ChannelPrecision, DataChannels, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_render::{ColorImageContract, ColorImageTarget, ColorProcessorOutput, Renderer};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};
#[path = "color_image_processor/chain.rs"]
mod chain;
#[path = "color_image_processor/native_scene.rs"]
mod native_scene;
#[path = "color_image_processor/ranges.rs"]
mod ranges;
#[path = "color_image_processor/uploads.rs"]
mod uploads;
#[path = "color_image_processor/native_runtime.rs"]
mod native_runtime;
#[derive(Deserialize)]
struct Reference {
    absolute_tolerance: f32,
    relative_tolerance: f32,
    configs: Vec<Config>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Config {
    manifest: ColorConfigManifest,
    config: String,
    selection: ColorSelection,
}
#[derive(Deserialize)]
struct Case {
    config: usize,
    processor: ColorProcessorSource,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn reference() -> Reference {
    serde_json::from_str(include_str!(
        "../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap()
}
fn configs(reference: &Reference) -> Vec<(VerifiedColorConfig, ColorSelection)> {
    reference
        .configs
        .iter()
        .map(|c| {
            (
                VerifiedColorConfig::new(
                    c.manifest.clone(),
                    Arc::from(c.config.as_bytes()),
                    BTreeMap::new(),
                )
                .unwrap(),
                c.selection.clone(),
            )
        })
        .collect()
}
fn paint(renderer: &Renderer, target: &ColorImageTarget, pixels: &[[f32; 4]]) {
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
            let values=array({pixels});return values[u32(p.y)*3u+u32(p.x)];}}").into()),
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
                    format: motion_render::image_formats::native_format(target.image().storage()),
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
                view: target.image().view(),
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
fn read(renderer: &Renderer, target: &ColorImageTarget) -> Vec<[f32; 4]> {
    renderer
        .read_color_image_target(target)
        .unwrap()
        .pixels
        .chunks_exact(16)
        .map(|v| {
            std::array::from_fn(|c| f32::from_le_bytes(v[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect()
}
fn submit(renderer: &Renderer, binding: &ColorProcessorOutput, output: &ColorImageTarget) {
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    binding.encode(renderer, &mut encoder, output).unwrap();
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before,
        "encoding must not allocate image buffers or tables"
    );
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(binding.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
        })
        .unwrap();
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_ocio_images_match_cpu_across_rows_phases_versions_and_alpha() {
    let reference = reference();
    let configs = configs(&reference);
    assert_eq!(reference.absolute_tolerance, 0.0001);
    assert_eq!(reference.relative_tolerance, 0.0002);
    assert_eq!(reference.cases.len(), 20);
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (n, case) in reference.cases.iter().enumerate() {
        let (config, selection) = &configs[case.config];
        let p = case.processor.compile_image(config, selection).unwrap();
        let source = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: p.input_space(),
                    alpha: p.input_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let output = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: p.output_space(),
                    alpha: p.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        assert_eq!(case.input.len(), 12);
        assert_eq!(case.expected.len(), 12);
        paint(&renderer, &source, &case.input);
        assert_eq!(
            read(&renderer, &source),
            case.input,
            "fixture upload must preserve exact Float32 values and row order"
        );
        let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
        let binding = ColorProcessorOutput::new(&renderer, &source, &p, output.contract()).unwrap();
        let owned = renderer.allocation_budget().info().unwrap().admitted_bytes;
        assert_eq!(
            owned - before,
            p.tables().iter().map(|t| t.len() as u64 * 16).sum::<u64>()
        );
        let mut max_fraction = 0.0_f32;
        for iteration in 0..3 {
            submit(&renderer, &binding, &output);
            let actual = read(&renderer, &output);
            assert_eq!(actual.len(), case.expected.len());
            for (i, (got, want)) in actual.iter().zip(&case.expected).enumerate() {
                for c in 0..4 {
                    let error = (got[c] - want[c]).abs();
                    let tolerance =
                        reference.absolute_tolerance + reference.relative_tolerance * want[c].abs();
                    max_fraction = max_fraction.max(error / tolerance);
                    assert!(got[c].is_finite() && error<=tolerance,"case={n} iteration={iteration} pixel={i} channel={c} got={got:?} want={want:?}");
                }
                assert_eq!(got[3].to_bits(), case.input[i][3].to_bits());
            }
            assert_eq!(
                renderer.allocation_budget().info().unwrap().admitted_bytes,
                owned
            );
        }
        println!(
            "image case={n} phase={:?} alpha={:?}->{:?} max_tolerance_fraction={max_fraction}",
            p.phase(),
            p.input_alpha(),
            p.output_alpha()
        );
        drop(binding);
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before
        );
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_transform_rejects_data_alpha_extent_ownership_and_alias_without_writes() {
    let reference = reference();
    let configs = configs(&reference);
    let case = &reference.cases[0];
    let (config, selection) = &configs[case.config];
    let p = case.processor.compile_image(config, selection).unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: motion_effects::color_processor::CodeValueRange::FULL,
                space: p.input_space(),
                alpha: p.input_alpha(),
                precision: ChannelPrecision::Float32,
            },
        )
        .unwrap();
    let output = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: motion_effects::color_processor::CodeValueRange::FULL,
                space: p.output_space(),
                alpha: p.output_alpha(),
                precision: ChannelPrecision::Float32,
            },
        )
        .unwrap();
    paint(&renderer, &source, &case.input);
    paint(&renderer, &output, &case.input);
    let before = read(&renderer, &output);
    let binding = ColorProcessorOutput::new(&renderer, &source, &p, output.contract()).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut encoder, &source)
        .unwrap_err()
        .to_string()
        .contains("distinct"));
    let short = renderer
        .color_image_target(3, 3, output.contract())
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut encoder, &short)
        .unwrap_err()
        .to_string()
        .contains("extents"));
    let other_alpha = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: motion_effects::color_processor::CodeValueRange::FULL,
                space: p.output_space(),
                alpha: AlphaMode::Premultiplied,
                precision: ChannelPrecision::Float32,
            },
        )
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut encoder, &other_alpha)
        .unwrap_err()
        .to_string()
        .contains("contract"));
    let other_source_alpha = renderer
        .color_image_target(
            3,
            4,
            ColorImageContract {
                range: motion_effects::color_processor::CodeValueRange::FULL,
                space: p.input_space(),
                alpha: AlphaMode::Premultiplied,
                precision: ChannelPrecision::Float32,
            },
        )
        .unwrap();
    assert!(
        ColorProcessorOutput::new(&renderer, &other_source_alpha, &p, output.contract()).is_err()
    );
    assert!(ColorProcessorOutput::new(&renderer, &source, &p, other_alpha.contract()).is_err());
    drop(encoder);
    assert_eq!(read(&renderer, &output), before);
    for meaning in [
        ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
        ImageMeaning::Coverage {},
        ImageMeaning::Weight { normalized: false },
        ImageMeaning::RelativeDepth {},
        ImageMeaning::LinearDepth {},
        ImageMeaning::Normal {},
    ] {
        let data_contract = ImageContract::new(meaning, ChannelPrecision::Float32);
        let data = renderer.image_capture_target(3, 4, data_contract).unwrap();
        assert!(ColorImageTarget::from_image(data, p.input_space()).is_err());
    }
    for precision in [ChannelPrecision::Float16] {
        let mut declaration = source.contract();
        declaration.precision = precision;
        let lower = renderer.color_image_target(3, 4, declaration).unwrap();
        assert!(ColorProcessorOutput::new(&renderer, &lower, &p, output.contract()).is_err());
        assert!(ColorProcessorOutput::new(&renderer, &source, &p, declaration).is_err());
    }
    let mut integer = source.contract();
    integer.precision = ChannelPrecision::Unorm8;
    assert!(renderer.color_image_target(3, 4, integer).is_err());
    for precision in [ChannelPrecision::Unorm8, ChannelPrecision::Float32] {
        let legacy = renderer
            .image_capture_target(
                3,
                4,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Srgb,
                        alpha: p.input_alpha(),
                    },
                    precision,
                ),
            )
            .unwrap();
        assert!(ColorImageTarget::from_image(legacy, p.input_space()).is_err());
    }
    let foreign = pollster::block_on(Renderer::headless()).unwrap();
    let mut foreign_encoder = foreign
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(ColorProcessorOutput::new(&foreign, &source, &p, output.contract()).is_err());
    assert!(binding
        .encode(&foreign, &mut foreign_encoder, &output)
        .is_err());
    assert!(binding
        .encode(&renderer, &mut foreign_encoder, &output)
        .is_err());
    let foreign_target = foreign.color_image_target(3, 4, output.contract()).unwrap();
    let mut local_encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(binding
        .encode(&renderer, &mut local_encoder, &foreign_target)
        .is_err());
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_transform_resources_survive_encoding_and_submission_then_retire() {
    let reference = reference();
    let configs = configs(&reference);
    let case = reference
        .cases
        .iter()
        .find(|c| !c.processor.tables.is_empty())
        .unwrap();
    let (config, selection) = &configs[case.config];
    let p = case.processor.compile_image(config, selection).unwrap();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let baseline = budget.info().unwrap().admitted_bytes;
    for submit_work in [false, true] {
        let source = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: p.input_space(),
                    alpha: p.input_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let output = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: p.output_space(),
                    alpha: p.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        paint(&renderer, &source, &case.input);
        let binding = ColorProcessorOutput::new(&renderer, &source, &p, output.contract()).unwrap();
        let owned = budget.info().unwrap().admitted_bytes;
        assert!(owned > baseline);
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        binding.encode(&renderer, &mut encoder, &output).unwrap();
        let mut ledger = binding.texture_resource_ledger();
        ledger.merge(output.texture_resource_ledger());
        drop(binding);
        drop(source);
        drop(output);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            owned,
            "unsubmitted commands must retain all resources"
        );
        let command = encoder.finish();
        if submit_work {
            renderer
                .submit_allocated(command, |out| out.merge(ledger))
                .unwrap();
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
        } else {
            drop(command);
            drop(ledger);
        }
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            baseline,
            "abandon or completion must release leases"
        );
    }
}
