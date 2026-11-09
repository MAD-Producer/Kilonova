// Explicit application backend transport. Stdout remains empty. Only one
// command and one acknowledgment can exist; the owner polls with cancellation
// and reaps this direct child on cancellation/drop. No worker restart or retry.
fn service_ack(out: &Path, sequence: u64, execution: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(&json!({"schema":1,"sequence":sequence,
        "pid":std::process::id(),"execution":execution}))
    .map_err(|e| e.to_string())?;
    fs::write(out.join("ack.tmp"), bytes).map_err(|e| e.to_string())?;
    fs::rename(out.join("ack.tmp"), out.join("ack.json")).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn serve_graph(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    adapter_memory: &mut motion_render::AdapterMemoryPressure<'_>,
    graph: &Blueprint,
    compiled: &CompiledGraph,
    prepared: &PreparedGraph,
    program: &str,
    backend: &str,
    out: &Path,
    report: &mut Value,
) -> Result<()> {
    let workspace = out
        .parent()
        .ok_or("service workspace missing")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let execution = report["execution"].clone();
    report["service_mode"] = "retained_mailbox_v1".into();
    report["request"] = 0.into();
    report["operation"] = "retained_depth_service".into();
    phase(out, report, "retained_service_ready")?;
    service_ack(out, 0, &execution)?;
    let mut previous = 0u64;
    loop {
        let _idle = engine::enter(previous, "wait_retained_depth_request");
        report["request"] = json!(previous);
        report["operation"] = "wait_retained_depth_request".into();
        phase(out, report, "wait_retained_depth_request")?;
        let command_path = out.join("command.json");
        loop {
            match fs::metadata(&command_path) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => return Err(format!("read retained mailbox: {e}")),
            }
        }
        report["request"] = Value::Null;
        report["operation"] = "read_retained_command".into();
        phase(out, report, "read_retained_command")?;
        let command: service_admission::Command = serde_json::from_slice(&limited_read(
            &command_path,
            service_admission::COMMAND_BYTES,
        )?)
        .map_err(|e| format!("retained command schema: {e}"))?;
        let _admission = engine::enter(command.sequence, "admit_retained_request");
        report["request"] = json!(command.sequence);
        report["operation"] = "admit_retained_request".into();
        phase(out, report, "admit_retained_request")?;
        command.validate(previous, &workspace)?;
        fs::remove_file(&command_path).map_err(|e| e.to_string())?;
        let _flight = engine::enter(command.sequence, "retained_depth_inference");
        log::info!("start #{} retained_depth_inference", command.sequence);
        logging::flush();
        report["request"] = json!(command.sequence);
        report["operation"] = "retained_depth_inference".into();
        phase(out, report, "admit_retained_request")?;
        let raw = limited_read(&command.request, 16 * 1024)?;
        let request: GraphRequest = serde_json::from_slice(&raw)
            .map_err(|e| format!("retained graph request schema: {e}"))?;
        require(
            request.version == 1
                && request.program == program
                && request.program_sha256 == PROGRAM
                && request.requested_backend == backend
                && request.input.name == "rgb"
                && request.input.dtype == "float32"
                && request.input.shape == [1, 3, 518, 518],
            "retained request differs from prepared execution",
        )?;
        let root = command
            .request
            .parent()
            .ok_or("retained input root missing")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let input = tensor_bytes(&root, &request.input)?;
        fs::create_dir(&command.output).map_err(|e| e.to_string())?;
        let mut frame = json!({"status":"running","pid":std::process::id(),
            "request":command.sequence,"operation":"retained_depth_inference",
            "request_sha256":digest(&raw),"program_sha256":PROGRAM,
            "model_sha256":graph.model_sha256,"adapter":report["adapter"],
            "verified_resources":282,"execution":execution,
            "service_mode":"retained_mailbox_v1","context_preparations":1});
        let began = Instant::now();
        let memory_before = adapter_memory.snapshot();
        frame["adapter_memory_before_inference"] = json!(&memory_before);
        checked_gpu(device, "retained_depth_inference", || {
            execute_prepared_graph(
                device,
                queue,
                graph,
                compiled,
                prepared,
                &input,
                &command.output,
                &mut frame,
            )
        })?;
        frame["status"] =
            "actual_complete_gpu_output_written_independent_comparison_pending".into();
        frame["phase"] = "completed".into();
        frame["in_flight"] = Value::Null;
        frame["inference_process_memory"] =
            json!(motion_host::process_memory::ProcessPeak::current());
        frame["adapter_memory_samples"] = json!({
            "pid":std::process::id(),"before":memory_before,
            "after":adapter_memory.snapshot()});
        save(&command.output, &frame)?;
        report["completed_inputs"] = json!(command.sequence);
        phase(out, report, "retained_request_completed_before_reuse")?;
        service_ack(out, command.sequence, &execution)?;
        previous = command.sequence;
        log::debug!(
            "done #{} retained_depth_inference {}ms",
            previous,
            began.elapsed().as_millis()
        );
    }
}
