//! Independent proof obligations for the actual generated scalar kernels.
//! These do not replace native whole-chain alpha/consumer/compositing checks.
use motion_effects::{builtin, image_contract::ChannelPrecision, pointwise::*};
use motion_render::Renderer;
use wgpu::util::DeviceExt;

fn renderer() -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ))
    .unwrap()
}

fn helpers(precision: ChannelPrecision) -> String {
    let package = builtin::package().unwrap();
    let stages: Vec<_> = ["invert", "tint"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|e| e.id == name)
                .unwrap();
            let Eligibility::Proven(proof) = prove_builtin(&package, definition).unwrap() else {
                panic!("the original implementation must acquire its actual source/code proof");
            };
            let values: Vec<_> = proof.parameters().iter().map(|p| p.default).collect();
            PointwiseStage::new(
                proof,
                StageNamespace {
                    object: 1,
                    effect: index as u64 + 1,
                },
                &values,
            )
            .unwrap()
        })
        .collect();
    let source = PointwiseChain::new(precision, &stages)
        .unwrap()
        .generated_source()
        .unwrap();
    // Keep the actual generated operation/quantizer bodies, not copied test
    // implementations. Only the unused whole-chain entry point is excluded.
    format!(
        "{}\n{}",
        motion_effects::shader::HALF_STORE_WGSL,
        source.split_once("\nfn main_fx(").unwrap().0
    )
}

struct Probe {
    renderer: Renderer,
    pipeline: wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
    parameters: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    count: usize,
}
impl Probe {
    fn new(functions: &str, body: &str, inputs: &[[f32; 4]]) -> Self {
        let renderer = renderer();
        let device = &renderer.device;
        let source = format!(
            r#"
struct ProbeUniform {{ params: array<vec4<f32>,32> }};
@group(0) @binding(0) var<uniform> fx: ProbeUniform;
@group(0) @binding(1) var<storage,read> probe_input: array<vec4<f32>>;
@group(0) @binding(2) var<storage,read_write> probe_output: array<vec4<u32>>;
{functions}
@compute @workgroup_size(64) fn proof_kernel(@builtin(global_invocation_id) invocation:vec3<u32>) {{
    let index=invocation.x;if index>=arrayLength(&probe_input){{return;}}
    let pixel=probe_input[index];{body}
}}
"#
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("independent pointwise proof kernel"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("proof_kernel"),
            compilation_options: Default::default(),
            cache: None,
        });
        let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&[[0_f32; 4]; 32]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(inputs),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let size = (inputs.len() * 32) as u64;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: parameters.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        Self {
            renderer,
            pipeline,
            bind,
            parameters,
            output,
            readback,
            count: inputs.len(),
        }
    }
    fn run(&self, parameters: &[[f32; 4]; 32]) -> Vec<[[u32; 4]; 2]> {
        self.renderer
            .queue
            .write_buffer(&self.parameters, 0, bytemuck::bytes_of(parameters));
        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups((self.count as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.output, 0, &self.readback, 0, (self.count * 32) as u64);
        self.renderer.queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        self.renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        receive
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let pixels = {
            let bytes = self.readback.slice(..).get_mapped_range().unwrap();
            bytemuck::cast_slice::<u8, [[u32; 4]; 2]>(&bytes).to_vec()
        };
        self.readback.unmap();
        self.renderer.check_health().unwrap();
        pixels
    }
}

fn write(name: &str, value: serde_json::Value) {
    if let Some(dir) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join(name),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shader_operation_equivalence_uses_pinned_original_bodies_and_generated_gpu_functions() {
    let package = builtin::package().unwrap();
    let generated = helpers(ChannelPrecision::Float32);
    let values = [
        -4_f32,
        -0.0,
        0.0,
        f32::from_bits(1),
        2_f32.powi(-24),
        0.000001,
        1. / 255.,
        0.25,
        0.5,
        1.,
        2.75,
        16.,
    ];
    let mut inputs = Vec::new();
    for &r in &values {
        for &g in &values {
            for (i, &b) in values.iter().enumerate() {
                inputs.push([r, g, b, values[(i + 3) % values.len()]]);
            }
        }
    }
    let mut rows = Vec::new();
    for operation in ["invert", "tint"] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == operation)
            .unwrap();
        let original = std::str::from_utf8(&package.files[&definition.passes[0].shader]).unwrap();
        let start = original.rfind("fn main_fx(").unwrap();
        let body = original[start..].split_once('{').unwrap().1;
        assert_eq!(body.matches("let c=sample_input(p);").count(), 1);
        let original = format!(
            "{}fn original_operation(c:vec4<f32>)->vec4<f32>{{{}",
            &original[..start],
            body.replacen("let c=sample_input(p);", "", 1)
        );
        let probe = Probe::new(
            &format!("{generated}\n{original}"),
            &format!(
                "probe_output[index*2u]=bitcast<vec4<u32>>(original_operation(pixel));probe_output[index*2u+1u]=bitcast<vec4<u32>>(pointwise_{operation}(pixel,0u));"
            ),
            &inputs,
        );
        let cases: Vec<_> = if operation == "invert" {
            [1_f32, 2., 3., 4., 5.]
                .into_iter()
                .flat_map(|channel| {
                    [0_f32, 23.75, 50., 100.]
                        .into_iter()
                        .map(move |blend| (channel, blend))
                })
                .collect()
        } else {
            [0_f32, 15., 37.5, 68.25, 100.]
                .into_iter()
                .map(|amount| (0., amount))
                .collect()
        };
        for (first, amount) in cases {
            let mut parameters = [[0_f32; 4]; 32];
            if operation == "invert" {
                parameters[0][0] = first;
                parameters[1][0] = amount;
            } else {
                parameters[1] = [0.07, 0.19, 0.31, 1.];
                parameters[2] = [0.91, 0.65, 0.43, 1.];
                parameters[3][0] = amount;
            }
            let output = probe.run(&parameters);
            for (index, pair) in output.iter().enumerate() {
                assert_eq!(
                    pair[0], pair[1],
                    "operation={operation} parameter={first}/{amount} input={:?}",
                    inputs[index]
                );
            }
            rows.push(serde_json::json!({"operation":operation,"channel":first,"amount":amount,"pixels":inputs.len(),"rgba_bits_equal":true}));
        }
        // Replacing either proved implementation must prevent use of the
        // scalar theorem; recognized names and package hashes are insufficient.
        let mut changed = motion_effects::EffectPackage {
            manifest: package.manifest.clone(),
            hash: package.hash.clone(),
            bytes: vec![],
            files: [(
                definition.passes[0].shader.clone(),
                package.files[&definition.passes[0].shader].clone(),
            )]
            .into(),
            shaders: [(
                (operation.into(), 0),
                package.shaders[&(operation.into(), 0)].clone(),
            )]
            .into(),
        };
        changed
            .shaders
            .get_mut(&(operation.into(), 0))
            .unwrap()
            .wgsl
            .push_str("\n// changed implementation");
        assert!(matches!(
            prove_builtin(&changed, definition).unwrap(),
            Eligibility::Retain(RetainReason::CompiledImplementationChanged)
        ));
    }
    write(
        "proof-shader-operations.json",
        serde_json::json!({"scope":"pinned scalar operation bodies; SDK storage, alpha lifting and consumers have separate obligations","cases":rows}),
    );
}

// Independent binary16 center construction; no production half conversion.
fn half_center(bits: u32) -> f32 {
    let exponent = (bits >> 10) & 31;
    let fraction = bits & 1023;
    if exponent == 31 {
        f32::INFINITY
    } else if exponent == 0 {
        (fraction as f64 * 2_f64.powi(-24)) as f32
    } else {
        ((1. + fraction as f64 / 1024.) * 2_f64.powi(exponent as i32 - 15)) as f32
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn quantization_half_gpu_function_checks_every_finite_cell_and_ties_against_independent_centers() {
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    for sign in [0_u32, 0x80000000] {
        for bits in 0..=0x7bff_u32 {
            let center = half_center(bits);
            // The overflow cell ends at 65536; the midpoint 65520 rounds to Inf.
            let next = if bits == 0x7bff {
                65536.
            } else {
                half_center(bits + 1)
            };
            let step = next - center;
            let pixel = [
                center,
                center + step * 0.25,
                center + step * 0.5,
                center + step * 0.75,
            ]
            .map(|v| f32::from_bits(v.to_bits() | sign));
            let rounded = [
                center,
                center,
                half_center(bits + u32::from(bits & 1 != 0)),
                half_center(bits + 1),
            ]
            .map(|v| v.to_bits() | sign);
            inputs.push(pixel);
            expected.push(rounded);
        }
    }
    let probe = Probe::new(
        &helpers(ChannelPrecision::Float16),
        // The second output deliberately omits quantization. It must differ,
        // so comparing a copied input or a mocked return cannot pass this gate.
        "probe_output[index*2u]=bitcast<vec4<u32>>(pointwise_work_store(pixel));probe_output[index*2u+1u]=bitcast<vec4<u32>>(pixel+fx.params[0]);",
        &inputs,
    );
    let output = probe.run(&[[0_f32; 4]; 32]);
    let mut negative_control_differences = 0;
    for (index, (pair, expected)) in output.iter().zip(&expected).enumerate() {
        assert_eq!(
            &pair[0], expected,
            "cell={index} inputs={:?}",
            inputs[index]
        );
        negative_control_differences += (0..4).filter(|&c| pair[1][c] != expected[c]).count();
    }
    assert!(
        negative_control_differences >= inputs.len() * 3,
        "eliding the stores must be detected in every finite interval"
    );
    write(
        "proof-half-quantization.json",
        serde_json::json!({"finite_cells":inputs.len(),"scalar_probes":inputs.len()*4,"signed_zero":true,"ties_to_even":true,"overflow_ties":true,"negative_control_differences":negative_control_differences,"scope":"generated software round-to-binary16 kernel; native attachment/backend and full-chain bounds remain separate"}),
    );
}
