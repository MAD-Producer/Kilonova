//! Resolve both numerical procedures before lookup, then bind a cache miss to
//! exactly those observations. Calibration and resource ownership remain fresh.
use super::*;

#[derive(Clone)]
pub(super) struct Attestation {
    pub native: Execution,
    pub transform: Execution,
    pub worker_sha256: [u8; 32],
    pub native_memory: Option<crate::process_memory::ProcessPeak>,
    pub transform_memory: Option<crate::process_memory::ProcessPeak>,
}

fn program_digest() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        // The pinned, independently generated program is a source constant.
        *byte = u8::from_str_radix(&PROGRAM_SHA256[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Invalid("invalid pinned WGSL program digest"))?;
    }
    Ok(bytes)
}

fn transform_plan() -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"MotionStudio/depth/wgsl-transform-plan/v1\0");
    for source in [SHARED, TRANSFORM] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    hash.finalize().into()
}

impl Attestation {
    pub fn combined_retained(&self) -> Result<Execution> {
        let mut execution = self.combined()?;
        let mut hash = Sha256::new();
        hash.update(b"MotionStudio/depth/wgsl-retained-mailbox-runtime/v1\0");
        hash.update(execution.runtime_sha256);
        execution.runtime_sha256 = hash.finalize().into();
        execution.validate()?;
        Ok(execution)
    }
    pub fn combined(&self) -> Result<Execution> {
        self.native.validate()?;
        self.transform.validate()?;
        if self.native.backend != "wgsl"
            || self.native.precision != "float32"
            || !self.native.provider.starts_with("WGSL:")
            || self.native.plan_sha256 != program_digest()?
            || self.transform.backend != "wgsl-transform"
            || self.transform.provider != "NumPy/OpenCV"
            || self.transform.precision != "float32"
            || self.transform.plan_sha256 != transform_plan()
            || self.worker_sha256 == [0; 32]
        {
            return Err(Error::Invalid(
                "unqualified WGSL cache execution attestation",
            ));
        }
        let mut runtime = Sha256::new();
        runtime.update(b"MotionStudio/depth/wgsl-combined-runtime/v1\0");
        runtime.update(serde_json::to_vec(&(
            &self.native,
            &self.transform,
            self.worker_sha256,
        ))?);
        let mut plan = Sha256::new();
        plan.update(b"MotionStudio/depth/wgsl-combined-plan/v1\0");
        plan.update(self.native.plan_sha256);
        plan.update(self.transform.plan_sha256);
        let execution = Execution {
            backend: "wgsl".into(),
            provider: self.native.provider.clone(),
            precision: "float32".into(),
            runtime_sha256: runtime.finalize().into(),
            plan_sha256: plan.finalize().into(),
        };
        execution.validate()?;
        Ok(execution)
    }

    pub fn validate_native_result(&self, value: &serde_json::Value, report: &Native) -> Result<()> {
        let actual: Execution = serde_json::from_value(value.get("execution").cloned().ok_or(
            Error::Invalid("native WGSL inference omitted its execution attestation"),
        )?)?;
        actual.validate()?;
        if actual != self.native
            || value["verified_resources"] != 282
            || actual.provider != format!("WGSL:{}:{}", report.adapter.backend, report.adapter.name)
        {
            return Err(Error::Invalid(
                "native WGSL execution differs from cache resolution",
            ));
        }
        Ok(())
    }
}

pub(super) fn worker_digest(path: &Path, cancellation: &Cancellation) -> Result<[u8; 32]> {
    cancellation.check()?;
    let mut file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();
    if !metadata.is_file() || size == 0 || size > 512 << 20 {
        return Err(Error::Invalid(
            "WGSL worker identity requires a nonempty file up to 512 MiB",
        ));
    }
    let mut hash = Sha256::new();
    let mut remaining = size;
    let mut buffer = [0; 65536];
    while remaining > 0 {
        cancellation.check()?;
        let take = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..take])?;
        hash.update(&buffer[..take]);
        remaining -= take as u64;
    }
    if file.read(&mut buffer[..1])? != 0 {
        return Err(Error::Invalid("WGSL worker grew during identity read"));
    }
    cancellation.check()?;
    Ok(hash.finalize().into())
}

pub(super) fn worker_unchanged(
    path: &Path,
    input: &Input,
    cancellation: &Cancellation,
    expected: [u8; 32],
) -> Result<()> {
    log::info!(
        "operation=depth_wgsl asset={} phase=verify_worker_identity start",
        input.key().source.asset
    );
    log::logger().flush();
    if worker_digest(path, cancellation)? != expected {
        return Err(Error::Invalid("WGSL worker changed since cache resolution"));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransformResolution {
    process_memory: Option<crate::process_memory::ProcessPeak>,
    schema: u32,
    stage: String,
    processing: String,
    execution: Execution,
    encoded_operations: u64,
}
impl TransformResolution {
    fn validate(&self) -> Result<()> {
        if let Some(memory) = &self.process_memory {
            memory.validate().map_err(Error::Invalid)?;
        }
        self.execution.validate()?;
        if self.schema != 1
            || self.stage != "resolve"
            || self.processing != super::super::onnx::PROCESSING
            || self.encoded_operations != 0
            || self.execution.backend != "wgsl-transform"
            || self.execution.provider != "NumPy/OpenCV"
            || self.execution.precision != "float32"
            || self.execution.plan_sha256 != transform_plan()
        {
            return Err(Error::Invalid("unqualified WGSL transform resolution"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct NativeResolution {
    process_memory: Option<crate::process_memory::ProcessPeak>,
    status: String,
    phase: String,
    model_sha256: String,
    program_sha256: String,
    request_sha256: String,
    encoded_operations: u64,
    final_readbacks: u32,
    verified_resources: usize,
    execution: Execution,
    adapter: Adapter,
}
impl NativeResolution {
    fn validate(&self, hardware: Hardware, request: &str) -> Result<()> {
        if let Some(memory) = &self.process_memory {
            memory.validate().map_err(Error::Invalid)?;
        }
        self.execution.validate()?;
        if self.status != "actual_execution_identity_resolved_without_inference"
            || self.phase != "completed"
            || self.model_sha256 != MODEL_SHA256
            || self.program_sha256 != PROGRAM_SHA256
            || self.request_sha256 != request
            || self.encoded_operations != 0
            || self.final_readbacks != 0
            || self.verified_resources != 282
            || self.adapter.backend != hardware.actual()
            || !matches!(
                self.adapter.device_type.as_str(),
                "DiscreteGpu" | "IntegratedGpu"
            )
            || self.adapter.name.is_empty()
            || self.adapter.name.len() > 96
            || self.adapter.name.chars().any(char::is_control)
            || self.execution.backend != "wgsl"
            || self.execution.precision != "float32"
            || self.execution.plan_sha256 != program_digest()?
            || self.execution.provider
                != format!("WGSL:{}:{}", self.adapter.backend, self.adapter.name)
        {
            return Err(Error::Invalid(
                "native WGSL cache resolution differs from requested execution",
            ));
        }
        Ok(())
    }
}

impl Wgsl {
    pub(super) fn resolve_cache_identity(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
    ) -> Result<Attestation> {
        cancellation.check()?;
        if digest_model(input) != MODEL_SHA256
            || input.key().model.processing != super::super::onnx::PROCESSING
        {
            return Err(Error::Invalid(
                "WGSL backend does not implement the requested model/processing identity",
            ));
        }
        fs::create_dir_all(&self.config.scratch)?;
        let directory = tempfile::Builder::new()
            .prefix("motion-depth-wgsl-resolve-")
            .tempdir_in(&self.config.scratch)?;
        let result = self.resolve_in(directory.path(), input, cancellation);
        match (result, directory.close()) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(primary), Err(cleanup)) => Err(Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
            (Err(primary), Ok(())) => Err(primary),
            (Ok(_), Err(cleanup)) => Err(cleanup.into()),
        }
    }

    fn resolve_in(
        &mut self,
        root: &Path,
        input: &Input,
        cancellation: &Cancellation,
    ) -> Result<Attestation> {
        fs::write(root.join("depth_tensor.py"), TRANSFORM)?;
        fs::write(root.join("ai_depth_onnx.py"), SHARED)?;
        log::info!(
            "operation=depth_wgsl asset={} phase=resolve_worker_identity start",
            input.key().source.asset
        );
        log::logger().flush();
        let worker_sha256 = worker_digest(&self.config.worker, cancellation)?;
        let command = python_transform(&self.config.python, root, TransformInvocation::Resolve);
        let transform: TransformResolution = serde_json::from_slice(&self.child(
            command,
            input,
            "resolve_transform",
            cancellation,
        )?)?;
        transform.validate()?;
        let request = serde_json::to_vec(
            &serde_json::json!({"version":1,"program":self.config.program,
            "program_sha256":PROGRAM_SHA256,"requested_backend":self.config.hardware.requested()}),
        )?;
        let path = root.join("resolve-request.json");
        fs::write(&path, &request)?;
        let native_root = root.join("native-resolve");
        let mut command = Command::new(&self.config.worker);
        command
            .arg("--resolve")
            .arg(&path)
            .arg(&native_root)
            .env("TMP", root)
            .env("TEMP", root);
        let stdout = match self.child(command, input, "resolve_native", cancellation) {
            Ok(stdout) => stdout,
            Err(Error::Canceled) => {
                if let Some(cause) = completed_panic(&native_root) {
                    log::error!(
                        "operation=depth_wgsl asset={} phase=resolve_native_panic_during_cancellation cause={cause}",
                        input.key().source.asset
                    );
                }
                return Err(Error::Canceled);
            }
            Err(primary) => return Err(native_error(primary, &native_root)),
        };
        if let Some(cause) = completed_panic(&native_root) {
            log::error!(
                "operation=depth_wgsl asset={} phase=resolve_native_panic_after_completion first_observation={cause}",
                input.key().source.asset
            );
        }
        if !stdout.is_empty() {
            return Err(Error::Invalid("native WGSL worker wrote to stdout"));
        }
        let native: NativeResolution =
            serde_json::from_slice(&read_limited(&native_root.join("receipt.json"), 65536)?)?;
        native.validate(self.config.hardware, &digest(&request))?;
        worker_unchanged(&self.config.worker, input, cancellation, worker_sha256)?;
        let attestation = Attestation {
            native: native.execution,
            transform: transform.execution,
            worker_sha256,
            native_memory: native.process_memory,
            transform_memory: transform.process_memory,
        };
        attestation.combined()?;
        cancellation.check()?;
        Ok(attestation)
    }
}

fn digest_model(input: &Input) -> String {
    input
        .key()
        .model
        .sha256
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
