use motion_effects::{color_config::*, color_processor::*};
use motion_render::Renderer;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc},
};
use wgpu::util::DeviceExt;

#[derive(Deserialize)]
struct Reference {
    runtime: String,
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

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_ocio_float_programs_match_cpu_for_all_phases_versions_and_alpha() {
    let reference: Reference = serde_json::from_str(include_str!(
        "../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    assert_eq!(reference.runtime, "2.6.0");
    assert_eq!(reference.absolute_tolerance, 0.0001);
    assert_eq!(reference.relative_tolerance, 0.0002);
    assert_eq!(reference.configs.len(), 3);
    assert_eq!(reference.cases.len(), 20);
    let configs: Vec<_> = reference
        .configs
        .into_iter()
        .map(|config| {
            let verified = VerifiedColorConfig::new(
                config.manifest,
                Arc::from(config.config.as_bytes()),
                BTreeMap::new(),
            )
            .unwrap();
            (verified, config.selection)
        })
        .collect();
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (number, case) in reference.cases.iter().enumerate() {
        let (config, selection) = &configs[case.config];
        let compiled = case.processor.compile(config, selection).unwrap();
        assert_eq!(case.input.len(), 12);
        assert_eq!(case.expected.len(), 12);
        let actual = run(&renderer, &compiled, &case.input);
        let mut max_error = 0_f32;
        let mut max_tolerance_fraction = 0_f32;
        for (pixel, (got, expected)) in actual.iter().zip(&case.expected).enumerate() {
            for c in 0..4 {
                let error = (got[c] - expected[c]).abs();
                max_error = max_error.max(error);
                max_tolerance_fraction = max_tolerance_fraction.max(
                    error
                        / (reference.absolute_tolerance
                            + reference.relative_tolerance * expected[c].abs()),
                );
                assert!(got[c].is_finite() && error<=reference.absolute_tolerance+reference.relative_tolerance*expected[c].abs(),"case={number} {:?} {:?}->{:?} pixel={pixel} channel={c} got={got:?} expected={expected:?}",case.processor.phase,case.processor.input_alpha,case.processor.output_alpha);
            }
            assert_eq!(got[3].to_bits(), case.input[pixel][3].to_bits());
        }
        println!(
            "case={number} config={} phase={:?} alpha={:?}->{:?} max_abs_error={max_error} max_tolerance_fraction={max_tolerance_fraction}",
            case.config,
            case.processor.phase,
            case.processor.input_alpha,
            case.processor.output_alpha
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn numeric_tables_interpolate_fractional_coordinates_without_texture_filtering() {
    let mut reference: Reference = serde_json::from_str(include_str!(
        "../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    let fixture = reference.configs.remove(0);
    let config = VerifiedColorConfig::new(
        fixture.manifest,
        Arc::from(fixture.config.as_bytes()),
        BTreeMap::new(),
    )
    .unwrap();
    let mut source = reference.cases.remove(0).processor;
    // Independent 2x2 scalar table and exact arithmetic expectations. These are
    // sample-adapter probes, not OCIO reference transform semantics.
    source.shader="layout(set=0, binding = 1) uniform sampler2D motion_ocio_probeSampler;\nvec4 motion_ocio_transform(vec4 c) { return vec4(texture(motion_ocio_probeSampler, c.rg).rgb,c.a); }".into();
    source.tables = vec![ColorProcessorTable {
        texture_name: "motion_ocio_probe".into(),
        sampler_name: "motion_ocio_probeSampler".into(),
        binding: 1,
        width: 2,
        height: 2,
        channels: 1,
        interpolation: TableInterpolation::Linear,
        values: vec![0., 10., 20., 30.],
    }];
    let pixels = [
        [-1., -1., 0., 1.],
        [2., 2., 0., 0.5],
        [0.5, 0.5, 0., 0.001],
        [0.25, 0.75, 0., 1.],
        [0.375, 0.625, 0., 1.],
        [0.5, 0.25, 0., 1.],
    ];
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (interpolation, expected) in [
        (TableInterpolation::Linear, [0., 30., 15., 20., 17.5, 5.]),
        (TableInterpolation::Nearest, [0., 30., 30., 20., 20., 10.]),
    ] {
        source.tables[0].interpolation = interpolation;
        let compiled = source.compile(&config, &fixture.selection).unwrap();
        let actual = run(&renderer, &compiled, &pixels);
        for (i, (got, want)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (got[0] - want).abs() < 0.00001,
                "{interpolation:?} pixel={i} got={got:?} want={want}"
            );
            assert_eq!(got[1], 0.);
            assert_eq!(got[2], 0.);
            assert_eq!(got[3].to_bits(), pixels[i][3].to_bits());
        }
    }
}

fn buffer_layout(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(16),
        },
        count: None,
    }
}

fn run(
    renderer: &Renderer,
    compiled: &CompiledColorProcessor,
    pixels: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    let device = &renderer.device;
    let tables_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("OCIO numeric test tables"),
        entries: &compiled
            .tables()
            .iter()
            .enumerate()
            .map(|(i, _)| buffer_layout(i as u32, true))
            .collect::<Vec<_>>(),
    });
    let io_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("OCIO test input/output"),
        entries: &[buffer_layout(0, true), buffer_layout(1, false)],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&tables_layout), Some(&io_layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual compiled OCIO processor"),
        source: wgpu::ShaderSource::Wgsl(compiled.wgsl().into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("actual compiled OCIO processor"),
        layout: Some(&layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let tables: Vec<_> = compiled
        .tables()
        .iter()
        .map(|values| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("OCIO reference numeric table"),
                contents: bytemuck::cast_slice(values),
                usage: wgpu::BufferUsages::STORAGE,
            })
        })
        .collect();
    let table_entries: Vec<_> = tables
        .iter()
        .enumerate()
        .map(|(i, buffer)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    let tables = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &tables_layout,
        entries: &table_entries,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(pixels),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let sentinel = [-123.5_f32, 456.25, 789.5, -17.];
    let initial = vec![sentinel; pixels.len() + 2];
    let size = initial.len() as u64 * 16;
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&initial),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let io = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &io_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &tables, &[]);
        pass.set_bind_group(1, &io, &[]);
        pass.dispatch_workgroups(pixels.len() as u32 + 2, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, size);
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = read.slice(..).get_mapped_range().unwrap();
    let actual: Vec<[f32; 4]> = bytes
        .chunks_exact(16)
        .map(|b| {
            std::array::from_fn(|c| f32::from_le_bytes(b[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect();
    assert_eq!(
        &actual[pixels.len()..],
        &[sentinel, sentinel],
        "over-dispatch must leave the extra output rows unchanged"
    );

    actual
}
