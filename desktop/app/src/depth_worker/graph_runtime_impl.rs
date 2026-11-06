// Standalone composed execution of an independently pinned static program.
// No product capability, graph optimization, CPU output injection or fallback.
use graph_admission::{Blueprint, CompiledGraph, Shader};
const PROGRAM: &str = "2ec1d8012905198cfd5af1430f88d1770edbf752531fe817dfcc65e1477fef6a";
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphRequest {
    version: u32,
    program: String,
    program_sha256: String,
    requested_backend: String,
    input: Tensor,
}
fn finite(bytes: &[u8]) -> bool {
    bytes.len() % 4 == 0
        && bytes
            .chunks_exact(4)
            .all(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]).is_finite())
}
fn bind(buffer: &wgpu::Buffer, offset: u64, size: u64) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset,
        size: std::num::NonZeroU64::new(size),
    })
}
// The single staging allocation is retained and reused only after its preceding
// copy submission has completed. Queue::write_buffer does not hide extra copies.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    staging: &wgpu::Buffer,
    target: &wgpu::Buffer,
    bytes: &[u8],
    chunk_size: usize,
) -> Result<()> {
    for (chunk, part) in bytes.chunks(chunk_size).enumerate() {
        let range = staging.slice(..part.len() as u64);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        range.map_async(wgpu::MapMode::Write, move |r| {
            let _ = send.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("upload poll: {e}"))?;
        receive
            .recv()
            .map_err(|e| format!("upload callback closed: {e}"))?
            .map_err(|e| format!("upload map: {e}"))?;
        let mut view = range
            .get_mapped_range_mut()
            .map_err(|e| format!("upload view: {e}"))?;
        view.copy_from_slice(part);
        drop(view);
        staging.unmap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("depth bounded resource upload"),
        });
        encoder.copy_buffer_to_buffer(
            staging,
            0,
            target,
            (chunk * chunk_size) as u64,
            part.len() as u64,
        );
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("upload completion: {e}"))?;
    }
    Ok(())
}
fn execute_graph(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    graph: &Blueprint,
    compiled: &CompiledGraph,
    root: &Path,
    input: &[u8],
    out: &Path,
    report: &mut Value,
) -> Result<()> {
    phase(out, report, "allocate_fixed_pool_and_upload_resources")?;
    let mut buffers = Vec::new();
    let storage =
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
    for name in &compiled.persistent_names {
        buffers.push(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size: graph.persistent_capacities[name],
            usage: storage,
            mapped_at_creation: false,
        }));
    }
    for &size in &graph.pool_slot_capacities {
        buffers.push(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth fixed reusable pool slot"),
            size,
            usage: storage,
            mapped_at_creation: false,
        }));
    }
    for _ in 0..3 {
        buffers.push(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("depth unused binding"),
                contents: &[0; 4],
                usage: storage,
            }),
        );
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("depth bounded upload staging"),
        size: graph.upload_staging_bytes,
        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("depth final-only readback"),
        size: graph.readback_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut uniform_words = vec![0u32; (graph.uniform_ring_bytes / 4) as usize];
    for node in &compiled.nodes {
        let offset = (node.uniform_offset / 4) as usize;
        uniform_words[offset..offset + node.parameters.len()].copy_from_slice(&node.parameters);
    }
    let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("depth whole-program uniform ring"),
        contents: bytemuck::cast_slice(&uniform_words),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let actual_capacity = buffers.iter().map(wgpu::Buffer::size).sum::<u64>()
        + staging.size()
        + readback.size()
        + uniforms.size();
    require(
        actual_capacity == compiled.retained_buffer_capacity_bytes
            && actual_capacity <= graph.capacity_budget_bytes,
        "actual allocated buffer capacities differ from the independently validated budget",
    )?;
    report["actual_retained_buffer_capacity_bytes"] = json!(actual_capacity);
    report["buffer_count"] = json!(buffers.len() + 3);
    for (index, name) in compiled.persistent_names.iter().enumerate() {
        let _flight = engine::enter(index as u64 + 1, "upload_model_resource");
        let resource = &graph.resources[name];
        log::info!(
            "start #{} upload_model_resource name={} bytes={}",
            index + 1,
            name,
            resource.bytes
        );
        logging::flush();
        report["resource"] = name.clone().into();
        report["request"] = json!(index + 1);
        report["operation"] = "upload_model_resource".into();
        phase(out, report, "read_hash_finite_upload_resource")?;
        let path = root
            .join(&resource.file)
            .canonicalize()
            .map_err(|e| format!("resource {name}: {e}"))?;
        require(
            path.starts_with(root),
            "canonical model resource escapes program root",
        )?;
        let data = limited_read(&path, resource.bytes)?;
        require(
            data.len() as u64 == resource.bytes
                && digest(&data) == resource.sha256
                && finite(&data),
            &format!("resource {name} length, digest or finite-value check failed"),
        )?;
        upload(
            device,
            queue,
            &staging,
            &buffers[index],
            &data,
            graph.upload_staging_bytes as usize,
        )?;
    }
    report["resource"] = Value::Null;
    upload(
        device,
        queue,
        &staging,
        &buffers[compiled.pool_buffer_base + graph.owner_slots["rgb"]],
        input,
        graph.upload_staging_bytes as usize,
    )?;
    report["request"] = 0.into();
    report["operation"] = "complete_model_probe".into();
    phase(out, report, "compile_required_wgsl_pipelines")?;
    let entries: Vec<_> = (0..5)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: if binding == 4 {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage {
                        read_only: binding != 3,
                    }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        })
        .collect();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("depth whole graph layout"),
        entries: &entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("depth whole graph pipelines"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let mut pipelines = HashMap::new();
    for node in &compiled.nodes {
        if pipelines.contains_key(node.entry) {
            continue;
        }
        let _flight = engine::enter(u64::from(node.index), "compile_graph_pipeline");
        log::info!(
            "start #{} compile_graph_pipeline operation={}",
            node.index,
            node.entry
        );
        logging::flush();
        report["request"] = json!(node.index);
        report["operation"] = "compile_graph_pipeline".into();
        report["node"] = node.name.clone().into();
        phase(out, report, "compile_graph_pipeline")?;
        let source = match node.shader {
            Shader::Numerical => include_str!("operators.wgsl"),
            Shader::Convolution => include_str!("convolution.wgsl"),
            Shader::Erf => include_str!("erf.wgsl"),
            Shader::Index => include_str!("index.wgsl"),
        };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(node.entry),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(node.entry),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(node.entry),
            compilation_options: Default::default(),
            cache: None,
        });
        pipelines.insert(node.entry, pipeline);
    }
    report["request"] = 0.into();
    report["operation"] = "complete_model_probe".into();
    report["node"] = Value::Null;
    phase(out, report, "bind_validated_logical_ranges")?;
    let mut bindings = Vec::new();
    for node in &compiled.nodes {
        bindings.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&node.name),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: bind(&buffers[node.input_buffers[0]], 0, node.input_bytes[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bind(&buffers[node.input_buffers[1]], 0, node.input_bytes[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bind(&buffers[node.input_buffers[2]], 0, node.input_bytes[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: bind(&buffers[node.output_buffer], 0, node.output_bytes),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bind(&uniforms, node.uniform_offset, 256),
                },
            ],
        }));
    }
    report["request"] = 0.into();
    report["operation"] = "complete_model_probe".into();
    phase(out, report, "encode_separate_ordered_compute_passes")?;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("depth entire learned graph"),
    });
    for (ordinal, node) in compiled.nodes.iter().enumerate() {
        let _flight = engine::enter(u64::from(node.index), node.entry);
        log::info!(
            "start #{} {} node={} phase=encode_ordered_pass",
            node.index,
            node.entry,
            node.name
        );
        // This is command encoding, not per-node GPU completion. The following
        // submission remains one observable in-flight full-model operation.
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&node.name),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipelines[node.entry]);
        pass.set_bind_group(0, &bindings[ordinal], &[]);
        pass.dispatch_workgroups(node.dispatch[0], node.dispatch[1], node.dispatch[2]);
    }
    encoder.copy_buffer_to_buffer(
        &buffers[compiled.output_buffer],
        0,
        &readback,
        0,
        graph.readback_bytes,
    );
    phase(out, report, "submit_complete_model_and_wait_final_readback")?;
    let began = Instant::now();
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = send.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("whole-model completion poll: {e}"))?;
    receive
        .recv()
        .map_err(|e| format!("final readback callback closed: {e}"))?
        .map_err(|e| format!("final readback map: {e}"))?;
    let view = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|e| format!("final mapped view: {e}"))?;
    let data = view.to_vec();
    drop(view);
    readback.unmap();
    report["submit_through_final_map_seconds"] = json!(began.elapsed().as_secs_f64());
    require(
        data.len() as u64 == graph.readback_bytes && finite(&data),
        "whole-model output size or finite-value failure",
    )?;
    phase(out, report, "write_only_actual_final_gpu_depth")?;
    fs::write(out.join("relative_inverse_depth.f32"), &data)
        .map_err(|e| format!("write GPU depth: {e}"))?;
    report["output_sha256"] = digest(&data).into();
    report["output_bytes"] = json!(data.len());
    report["encoded_operations"] = json!(compiled.nodes.len());
    report["final_readbacks"] = 1.into();
    report["per_node_cpu_injection"] = false.into();
    report["pool_retained_through_completion"] = true.into();
    Ok(())
}
fn run(request_path: &Path, out: &Path, report: &mut Value) -> Result<()> {
    require(
        cfg!(target_endian = "little"),
        "explicit little endian F32 required",
    )?;
    let raw = limited_read(request_path, 16 * 1024)?;
    let request: GraphRequest =
        serde_json::from_slice(&raw).map_err(|e| format!("graph request schema: {e}"))?;
    require(
        request.version == 1 && request.program_sha256 == PROGRAM,
        "unqualified graph request identity/version",
    )?;
    require(
        request.input.name == "rgb" && request.input.shape == [1, 3, 518, 518],
        "graph input name/shape differs",
    )?;
    report["request_sha256"] = digest(&raw).into();
    let program_path = Path::new(&request.program);
    require(program_path.is_absolute(), "absolute program path required")?;
    let program_raw = limited_read(program_path, 4 << 20)?;
    require(
        digest(&program_raw) == PROGRAM,
        "pinned static program digest differs",
    )?;
    let graph: Blueprint =
        serde_json::from_slice(&program_raw).map_err(|e| format!("blueprint schema: {e}"))?;
    let root = program_path
        .parent()
        .ok_or("program parent missing")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let request_root = request_path
        .parent()
        .ok_or("request root missing")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let input = tensor_bytes(&request_root, &request.input)?;
    let backend = match request.requested_backend.as_str() {
        "vulkan" => wgpu::Backends::VULKAN,
        "dx12" => wgpu::Backends::DX12,
        "metal" => wgpu::Backends::METAL,
        _ => return Err("explicit supported graph backend required".into()),
    };
    phase(out, report, "request_explicit_hardware_adapter")?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: backend,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .map_err(|e| format!("graph adapter: {e}"))?;
    let info = adapter.get_info();
    require(
        info.device_type != wgpu::DeviceType::Cpu && wgpu::Backends::from(info.backend) == backend,
        "implicit backend or software adapter refused",
    )?;
    report["adapter"] = json!({"name":info.name,"backend":format!("{:?}",info.backend),"device_type":format!("{:?}",info.device_type),"driver":info.driver,"driver_info":info.driver_info});
    phase(out, report, "validate_blueprint_and_request_device")?;
    let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
    let compiled = graph.compile_for_research(&limits)?;
    require(
        compiled.nodes.len() == 429 && compiled.persistent_names.len() == 282,
        "pinned complete graph count differs",
    )?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("composed depth research"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .map_err(|e| format!("graph device: {e}"))?;
    report["program_sha256"] = PROGRAM.into();
    report["model_sha256"] = graph.model_sha256.clone().into();
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let executed = execute_graph(
        &device, &queue, &graph, &compiled, &root, &input, out, report,
    );
    let validation_cause = pollster::block_on(validation.pop());
    let oom_cause = pollster::block_on(oom.pop());
    match executed {
        Err(primary) => {
            return Err(format!(
                "{primary}; validation={validation_cause:?}; allocation={oom_cause:?}"
            ));
        }
        Ok(()) => require(
            validation_cause.is_none() && oom_cause.is_none(),
            &format!("whole graph validation={validation_cause:?}; allocation={oom_cause:?}"),
        )?,
    }
    report["status"] = "actual_complete_gpu_output_written_independent_comparison_pending".into();
    report["phase"] = "completed".into();
    report["in_flight"] = Value::Null;
    if let Some((_, _, elapsed)) = engine::current_command() {
        report["elapsed_ms"] = json!(elapsed.as_millis());
        log::debug!("done #0 complete_model_probe {}ms", elapsed.as_millis());
    }
    save(out, report)
}
