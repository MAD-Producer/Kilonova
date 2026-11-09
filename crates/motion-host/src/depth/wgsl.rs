//! Explicit optional WGSL backend for the existing bounded Jobs contract.
//! All three child phases are owned/reaped directly by this backend. The render
//! session and its device are never passed to the inference worker. Quality and
//! model processing identity stay independent of the execution backend.
use super::{Backend, Cancellation, Costs, Error, Estimate, Input, Key, Result, cache::Execution};
use motion_model::depth::{DepthDescriptor, DepthMeasure, DepthResource, NearDirection};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

const TRANSFORM: &str = include_str!("../../../../tools/depth_tensor.py");
const SHARED: &str = include_str!("../../../../tools/ai_depth_onnx.py");
pub const PROGRAM_SHA256: &str = "2ec1d8012905198cfd5af1430f88d1770edbf752531fe817dfcc65e1477fef6a";
pub const MODEL_SHA256: &str = "274368034c0fcef08f9b197d33b606bae9c578acca861ef72feb268143b21434";
const SIDE: u64 = 518;
const TENSOR_BYTES: u64 = 3 * SIDE * SIDE * 4;
const DEPTH_BYTES: u64 = SIDE * SIDE * 4;
const BUFFER_BYTES: u64 = 227_968_156;
mod identity;
mod service;
use identity::Attestation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hardware {
    Vulkan,
    Dx12,
    Metal,
}
impl Hardware {
    fn requested(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::Dx12 => "dx12",
            Self::Metal => "metal",
        }
    }
    fn actual(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan",
            Self::Dx12 => "Dx12",
            Self::Metal => "Metal",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    /// Separately installed Python with NumPy/OpenCV, no ONNX runtime needed.
    pub python: PathBuf,
    /// Explicit native motion-depth-wgsl executable; never auto-downloaded.
    pub worker: PathBuf,
    /// Exact pinned static program with separately installed hash-checked weights.
    pub program: PathBuf,
    pub scratch: PathBuf,
    pub hardware: Hardware,
}
pub struct Wgsl {
    config: Config,
    last_diagnostic: Option<String>,
    last_provider: Option<String>,
    attestation: Option<Attestation>,
    retained_gpu_buffers: Option<(Key, u64)>,
    retained: bool,
    service: Option<service::Service>,
    service_attestation: Option<Attestation>,
    service_failed: bool,
}
impl Wgsl {
    pub fn new(config: Config) -> Result<Self> {
        if [
            &config.python,
            &config.worker,
            &config.program,
            &config.scratch,
        ]
        .iter()
        .any(|p| !p.is_absolute())
        {
            return Err(Error::Invalid(
                "WGSL Python, worker, program and scratch paths must be absolute",
            ));
        }
        Ok(Self {
            config,
            last_diagnostic: None,
            last_provider: None,
            attestation: None,
            retained_gpu_buffers: None,
            retained: false,
            service: None,
            service_attestation: None,
            service_failed: false,
        })
    }
    /// Explicit retained-process mode. Cancellation or a protocol/GPU failure
    /// closes this backend permanently; construct a new backend deliberately
    /// to start another process. No automatic restart, retry or fallback.
    pub fn new_retained(config: Config) -> Result<Self> {
        let mut backend = Self::new(config)?;
        backend.retained = true;
        Ok(backend)
    }
    fn ensure_service(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
    ) -> Result<Attestation> {
        if self.service_failed {
            return Err(Error::Invalid(
                "retained WGSL backend has closed; no implicit restart",
            ));
        }
        if let Some(attestation) = self.service_attestation.clone() {
            identity::worker_unchanged(
                &self.config.worker,
                input,
                cancellation,
                attestation.worker_sha256,
            )?;
            return Ok(attestation);
        }
        let attestation = self.resolve_cache_identity(input, cancellation)?;
        let service = service::Service::start(&self.config, &attestation, input, cancellation)?;
        self.service = Some(service);
        self.service_attestation = Some(attestation.clone());
        Ok(attestation)
    }
    fn close_failed_service(&mut self, primary: Error) -> Error {
        self.service_failed = true;
        self.service_attestation = None;
        match self.service.take() {
            Some(mut service) => service.failure(primary),
            None => primary,
        }
    }
    fn child(
        &mut self,
        mut command: Command,
        input: &Input,
        phase: &str,
        cancellation: &Cancellation,
    ) -> Result<Vec<u8>> {
        cancellation.check()?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        log::info!(
            "operation=depth_wgsl asset={} phase={phase} start",
            input.key().source.asset
        );
        log::logger().flush();
        let began = Instant::now();
        let (stdout, stderr) =
            super::onnx::capture(&mut command, cancellation).map_err(|error| {
                if matches!(error, Error::Canceled) {
                    error
                } else {
                    Error::Backend(format!(
                        "WGSL phase={phase} elapsed_ms={} cause={error}",
                        began.elapsed().as_millis()
                    ))
                }
            })?;
        let panic = last_panic(&stderr);
        if let Some(cause) = panic {
            log::error!(
                "operation=depth_wgsl asset={} phase={phase} native_panic={cause}",
                input.key().source.asset
            );
            self.last_diagnostic = Some(stderr);
        } else if !stderr.is_empty() && self.last_diagnostic.as_ref() != Some(&stderr) {
            log::warn!(
                "operation=depth_wgsl asset={} phase={phase} runtime_diagnostic={stderr}",
                input.key().source.asset
            );
            self.last_diagnostic = Some(stderr);
        }
        log::debug!(
            "operation=depth_wgsl asset={} phase={phase} done elapsed_ms={}",
            input.key().source.asset,
            began.elapsed().as_millis()
        );
        Ok(stdout)
    }
    #[allow(clippy::too_many_arguments)]
    fn transform(
        &mut self,
        root: &Path,
        input: &Input,
        stage: &str,
        rgb: &str,
        depth: Option<&str>,
        cancellation: &Cancellation,
        expected: Option<&Execution>,
    ) -> Result<Transform> {
        let [width, height] = input.size();
        let request = serde_json::json!({"schema":1,"stage":stage,"processing":super::onnx::PROCESSING,
            "width":width,"height":height,"rgb_sha256":rgb,"depth_sha256":depth});
        fs::write(root.join("transform.json"), serde_json::to_vec(&request)?)?;
        // Isolated Python deliberately omits the script directory from sys.path.
        // The tiny -c loader admits exactly our generated, owned module root.
        let mode = if expected.is_some() {
            TransformInvocation::Attest
        } else {
            TransformInvocation::Plain
        };
        let command = python_transform(&self.config.python, root, mode);
        let stdout = self.child(command, input, stage, cancellation)?;
        let report = Transform::decode(&stdout, expected)?;
        report.validate(input, stage, rgb, depth)?;
        Ok(report)
    }
    fn infer_in(
        &mut self,
        root: &Path,
        input: &Input,
        cancellation: &Cancellation,
        attestation: Option<&Attestation>,
    ) -> Result<(Estimate, u64)> {
        cancellation.check()?;
        let model = input
            .key()
            .model
            .sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if model != MODEL_SHA256 || input.key().model.processing != super::onnx::PROCESSING {
            return Err(Error::Invalid(
                "WGSL backend does not implement the requested model/processing identity",
            ));
        }
        fs::write(root.join("depth_tensor.py"), TRANSFORM)?;
        fs::write(root.join("ai_depth_onnx.py"), SHARED)?;
        if let Some(expected) = attestation {
            fs::write(
                root.join("transform-execution.json"),
                serde_json::to_vec(&expected.transform)?,
            )?;
        }
        fs::write(root.join("input.rgb"), input.rgb())?;
        let rgb = digest(input.rgb());
        let pre_began = Instant::now();
        let prepared = self.transform(
            root,
            input,
            "prepare",
            &rgb,
            None,
            cancellation,
            attestation.map(|a| &a.transform),
        )?;
        let pre = pre_began.elapsed();
        let tensor = read_owned(
            &root.join("tensor.f32"),
            TENSOR_BYTES,
            &prepared.output_sha256,
        )?;
        drop(tensor);
        let native_request = serde_json::to_vec(
            &serde_json::json!({"version":1,"program":self.config.program,
            "program_sha256":PROGRAM_SHA256,"requested_backend":self.config.hardware.requested(),
            "input":{"name":"rgb","dtype":"float32","shape":[1,3,518,518],"file":"tensor.f32","sha256":prepared.output_sha256}}),
        )?;
        let native_path = root.join("native-request.json");
        fs::write(&native_path, &native_request)?;
        let native_root = root.join("native");
        let mut command = Command::new(&self.config.worker);
        if let Some(expected) = attestation {
            identity::worker_unchanged(
                &self.config.worker,
                input,
                cancellation,
                expected.worker_sha256,
            )?;
            let path = root.join("native-execution.json");
            fs::write(&path, serde_json::to_vec(&expected.native)?)?;
            command.arg("--attest").arg(path);
        }
        command
            .arg(&native_path)
            .arg(&native_root)
            .env("TMP", root)
            .env("TEMP", root);
        let gpu_began = Instant::now();
        let invocation = if self.retained {
            self.service
                .as_mut()
                .ok_or(Error::Invalid("retained WGSL service missing"))?
                .infer(&native_path, &native_root, cancellation)
                .map(|()| Vec::new())
        } else {
            self.child(command, input, "native_inference", cancellation)
        };
        let stdout = match invocation {
            Ok(result) => result,
            Err(Error::Canceled) => {
                if let Some(cause) = completed_panic(&native_root) {
                    log::error!(
                        "operation=depth_wgsl asset={} phase=native_panic_during_cancellation cause={cause}",
                        input.key().source.asset
                    );
                }
                return Err(Error::Canceled);
            }
            Err(primary) => {
                // Preserve the bounded primary receipt before temporary cleanup.
                // A long panic backtrace can otherwise displace its cause from
                // the captured stderr tail. Observation failure stays secondary.
                return Err(native_error(primary, &native_root));
            }
        };
        let gpu = gpu_began.elapsed();
        if let Some(cause) = completed_panic(&native_root) {
            // An internal catch_unwind must not erase an observed panic. Keep
            // its cause before cleanup without replacing a successful result.
            log::error!(
                "operation=depth_wgsl asset={} phase=native_panic_after_completion first_observation={cause}",
                input.key().source.asset
            );
        }
        if !stdout.is_empty() {
            return Err(Error::Invalid("native WGSL worker wrote to stdout"));
        }
        let receipt = read_limited(&native_root.join("receipt.json"), 65536)?;
        let report: Native = serde_json::from_slice(&receipt)?;
        report.validate(self.config.hardware, &digest(&native_request))?;
        if let Some(expected) = attestation {
            expected.validate_native_result(&serde_json::from_slice(&receipt)?, &report)?;
        }
        let depth = read_owned(
            &native_root.join("relative_inverse_depth.f32"),
            DEPTH_BYTES,
            &report.output_sha256,
        )?;
        fs::write(root.join("depth518.f32"), &depth)?;
        drop(depth);
        let post_began = Instant::now();
        let remapped = self.transform(
            root,
            input,
            "remap",
            &rgb,
            Some(&report.output_sha256),
            cancellation,
            attestation.map(|a| &a.transform),
        )?;
        let post = post_began.elapsed();
        let bytes = read_owned(
            &root.join("output.f32"),
            u64::from(input.size()[0]) * u64::from(input.size()[1]) * 4,
            &remapped.output_sha256,
        )?;
        cancellation.check()?;
        let values = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let key = input.key();
        let resource = DepthResource::new(
            DepthDescriptor {
                size: input.size(),
                source: key.source.clone(),
                geometry: key.geometry.clone(),
                model: key.model.clone(),
                measure: DepthMeasure::Relative {
                    near: NearDirection::Increasing,
                },
                scale: 1.,
                offset: 0.,
                valid_range: None,
            },
            values,
            None,
            None,
        )?;
        let provider = format!("WGSL:{}:{}", report.adapter.backend, report.adapter.name);
        if self.last_provider.as_ref() != Some(&provider) {
            log::info!(
                "operation=depth_wgsl asset={} requested_backend={} resolved_backend={} adapter={} model={} retained_buffer_bytes={BUFFER_BYTES}",
                key.source.asset,
                self.config.hardware.requested(),
                report.adapter.backend,
                report.adapter.name,
                model
            );
            self.last_provider = Some(provider.clone());
        }
        Ok((
            Estimate {
                resource,
                provider,
                costs: Costs {
                    preprocess: pre,
                    inference: gpu,
                    postprocess: post,
                    input_bytes: TENSOR_BYTES,
                    output_bytes: DEPTH_BYTES,
                    child_process_memory: Some(if self.retained {
                        crate::process_memory::Children::WgslRetained {
                            resolution_transform: attestation
                                .and_then(|a| a.transform_memory.clone()),
                            resolution_native: attestation.and_then(|a| a.native_memory.clone()),
                            prepare: prepared.process_memory,
                            inference: report.inference_process_memory.clone(),
                            remap: remapped.process_memory,
                        }
                    } else {
                        crate::process_memory::Children::Wgsl {
                            resolution_transform: attestation
                                .and_then(|a| a.transform_memory.clone()),
                            resolution_native: attestation.and_then(|a| a.native_memory.clone()),
                            prepare: prepared.process_memory,
                            inference: report.inference_process_memory.clone(),
                            remap: remapped.process_memory,
                        }
                    }),
                    inference_process_memory: report.inference_process_memory,
                    ..Costs::default()
                },
            },
            report.actual_retained_buffer_capacity_bytes,
        ))
    }
}
impl Backend for Wgsl {
    fn retained_gpu_buffer_capacity(&self, input: &Input) -> Option<u64> {
        self.retained_gpu_buffers
            .as_ref()
            .filter(|(key, _)| key.same_sample(input.key()))
            .map(|(_, capacity)| *capacity)
    }
    fn execution_identity(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
    ) -> Result<Option<Execution>> {
        self.attestation = None;
        self.retained_gpu_buffers = None;
        let attestation = if self.retained {
            match self.ensure_service(input, cancellation) {
                Ok(value) => value,
                Err(error) => return Err(self.close_failed_service(error)),
            }
        } else {
            self.resolve_cache_identity(input, cancellation)?
        };
        let execution = if self.retained {
            attestation.combined_retained()?
        } else {
            attestation.combined()?
        };
        self.attestation = Some(attestation);
        Ok(Some(execution))
    }
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate> {
        self.retained_gpu_buffers = None;
        let began = Instant::now();
        cancellation.check()?;
        fs::create_dir_all(&self.config.scratch)?;
        if self.retained {
            match self.ensure_service(input, cancellation) {
                Ok(attestation) => self.attestation = Some(attestation),
                Err(error) => return Err(self.close_failed_service(error)),
            }
        }
        let scratch = self
            .service
            .as_ref()
            .map(|s| s.workspace())
            .unwrap_or(&self.config.scratch);
        let directory = tempfile::Builder::new()
            .prefix("motion-depth-wgsl-")
            .tempdir_in(scratch)?;
        let attestation = self.attestation.take();
        let result = self.infer_in(directory.path(), input, cancellation, attestation.as_ref());
        // Stop/reap the worker first, but retain its owning workspace until the
        // nested request directory is closed. Dropping Service here would
        // delete the nested directory and turn cancellation into Cleanup error.
        let mut failed_service = if self.retained && result.is_err() {
            self.service_failed = true;
            self.service_attestation = None;
            self.service.take()
        } else {
            None
        };
        let result = result.map_err(|primary| match failed_service.as_mut() {
            Some(service) => service.failure(primary),
            None => primary,
        });
        let result = match (result, directory.close()) {
            (Ok((mut estimate, capacity)), Ok(())) => {
                estimate.costs.total = began.elapsed();
                estimate.costs.host_transport = estimate.costs.total.saturating_sub(
                    estimate.costs.preprocess
                        + estimate.costs.inference
                        + estimate.costs.postprocess,
                );
                self.retained_gpu_buffers = Some((input.key().clone(), capacity));
                Ok(estimate)
            }
            (Err(primary), Err(cleanup)) => Err(Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(error)) => Err(error.into()),
        };
        drop(failed_service);
        result
    }
}
enum TransformInvocation {
    Plain,
    Resolve,
    Attest,
}
fn python_transform(python: &Path, root: &Path, mode: TransformInvocation) -> Command {
    let mut command = Command::new(python);
    command.args(["-I","-B","-X","faulthandler","-c","import runpy,sys; root=sys.argv.pop(1); sys.path.insert(0,root); sys.argv[0]=root+'/depth_tensor.py'; runpy.run_path(sys.argv[0],run_name='__main__')"])
        .arg(root).env("TMP",root).env("TEMP",root).env("PYTHONDONTWRITEBYTECODE","1");
    match mode {
        TransformInvocation::Plain => {
            command.arg(root);
        }
        TransformInvocation::Resolve => {
            command.arg("--resolve");
        }
        TransformInvocation::Attest => {
            command.arg("--attest").arg(root);
        }
    }
    command
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn native_failure(root: &Path) -> String {
    let cause = (|| -> Result<String> {
        let native: Result<serde_json::Value> = read_limited(&root.join("receipt.json"), 65536)
            .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?));
        if let Ok(value) = &native {
            if value["failure"].as_str().is_some() {
                // A model failure after an internally caught panic is still
                // the primary failure of the operation that actually exited.
                return Ok(receipt_failure(value));
            }
        }
        let panic = root.join("panic.json");
        match fs::File::open(&panic) {
            Ok(_) => {
                let value: serde_json::Value =
                    serde_json::from_slice(&read_limited(&panic, 16384)?)?;
                if value["schema"] != 1 || value["status"] != "native_panic_original_cause_retained"
                {
                    return Err(Error::Invalid("native panic receipt identity differs"));
                }
                let cause = value["cause"]
                    .as_str()
                    .ok_or(Error::Invalid("native panic cause absent"))?;
                let cause = cause
                    .chars()
                    .take(384)
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>();
                let location = value["location"]
                    .as_str()
                    .unwrap_or("unavailable")
                    .chars()
                    .rev()
                    .take(192)
                    .collect::<String>();
                let location = location
                    .chars()
                    .rev()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>();
                return Ok(format!("native_panic cause={cause} location={location}"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(receipt_failure(&native?))
    })();
    match cause {
        Ok(cause) => cause,
        Err(error) => format!(
            "native_receipt unavailable: {}",
            error.to_string().chars().take(512).collect::<String>()
        ),
    }
}
fn receipt_failure(value: &serde_json::Value) -> String {
    let bounded = |field: &str, limit| {
        value[field]
            .as_str()
            .unwrap_or("unavailable")
            .chars()
            .take(limit)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
    };
    format!(
        "native_receipt status={} phase={} cause={}",
        bounded("status", 128),
        bounded("phase", 128),
        bounded("failure", 1536)
    )
}
fn completed_panic(root: &Path) -> Option<String> {
    match fs::symlink_metadata(root.join("panic.json")) {
        Ok(_) => Some(native_failure(root)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(format!(
            "native_panic receipt lookup unavailable: {}",
            error.to_string().chars().take(384).collect::<String>()
        )),
    }
}
fn native_error(primary: Error, root: &Path) -> Error {
    // Jobs bounds its default Error record. Put the model's retained primary
    // cause before a possible long native backtrace tail; keep both failures.
    let retained = native_failure(root);
    let child = primary.to_string();
    if !retained.starts_with("native_receipt status=failed_original_cause_retained ") {
        if let Some(final_panic) = last_panic(&child) {
            // The first panic may have been caught internally. The final hook
            // summary retains the later fatal cause and its actual location.
            return Error::Backend(format!(
                "native_panic final_cause={final_panic}; first_observation={retained}; child_failure={child}"
            ));
        }
    }
    Error::Backend(format!("{retained}; child_failure={child}"))
}
fn last_panic(message: &str) -> Option<String> {
    const PREFIX: &str = "ERROR motion_depth_wgsl: panic cause=";
    message.lines().rev().find_map(|line| {
        line.strip_prefix(PREFIX)
            .or_else(|| {
                line.split_once(" stderr=")
                    .and_then(|(_, value)| value.strip_prefix(PREFIX))
            })
            .map(|value| value.chars().take(512).collect())
    })
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(Error::Invalid("WGSL output exceeds byte bound"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Invalid("WGSL output grew beyond byte bound"));
    }
    Ok(bytes)
}
fn read_owned(path: &Path, length: u64, hash: &str) -> Result<Vec<u8>> {
    let bytes = read_limited(path, length)?;
    if bytes.len() as u64 != length
        || !hex(hash)
        || digest(&bytes) != hash
        || bytes
            .chunks_exact(4)
            .any(|b| !f32::from_le_bytes([b[0], b[1], b[2], b[3]]).is_finite())
    {
        return Err(Error::Invalid(
            "WGSL output length, SHA-256 or finite-value check failed",
        ));
    }
    Ok(bytes)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transform {
    process_memory: Option<crate::process_memory::ProcessPeak>,
    schema: u32,
    stage: String,
    processing: String,
    width: u32,
    height: u32,
    rgb_sha256: String,
    depth_sha256: Option<String>,
    output_sha256: String,
    output_bytes: u64,
    elapsed_ns: u64,
}
impl Transform {
    fn decode(bytes: &[u8], expected: Option<&Execution>) -> Result<Self> {
        if let Some(expected) = expected {
            let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
            let actual = value
                .as_object_mut()
                .and_then(|v| v.remove("execution"))
                .ok_or(Error::Invalid(
                    "WGSL transform omitted its execution attestation",
                ))?;
            let actual: Execution = serde_json::from_value(actual)?;
            actual.validate()?;
            if &actual != expected {
                return Err(Error::Invalid(
                    "WGSL transform execution differs from cache resolution",
                ));
            }
            Ok(serde_json::from_value(value)?)
        } else {
            Ok(serde_json::from_slice(bytes)?)
        }
    }
    fn validate(&self, input: &Input, stage: &str, rgb: &str, depth: Option<&str>) -> Result<()> {
        if let Some(memory) = &self.process_memory {
            memory.validate().map_err(Error::Invalid)?;
        }
        let bytes = if stage == "prepare" {
            TENSOR_BYTES
        } else {
            u64::from(input.size()[0]) * u64::from(input.size()[1]) * 4
        };
        if self.schema != 1
            || self.stage != stage
            || self.processing != super::onnx::PROCESSING
            || [self.width, self.height] != input.size()
            || self.rgb_sha256 != rgb
            || self.depth_sha256.as_deref() != depth
            || self.output_bytes != bytes
            || !hex(&self.output_sha256)
        {
            return Err(Error::Invalid(
                "WGSL transform report differs from request identity",
            ));
        }
        let _measured_child_ns = self.elapsed_ns;
        Ok(())
    }
}
#[derive(Deserialize)]
struct Native {
    inference_process_memory: Option<super::capability::memory::ProcessPeak>,
    status: String,
    phase: String,
    model_sha256: String,
    program_sha256: String,
    request_sha256: String,
    encoded_operations: u64,
    final_readbacks: u32,
    per_node_cpu_injection: bool,
    pool_retained_through_completion: bool,
    actual_retained_buffer_capacity_bytes: u64,
    output_bytes: u64,
    output_sha256: String,
    adapter: Adapter,
}
#[derive(Deserialize)]
struct Adapter {
    backend: String,
    name: String,
    device_type: String,
}
impl Native {
    fn validate(&self, hardware: Hardware, request: &str) -> Result<()> {
        if let Some(memory) = &self.inference_process_memory {
            memory.validate().map_err(Error::Invalid)?;
        }
        if self.status != "actual_complete_gpu_output_written_independent_comparison_pending"
            || self.phase != "completed"
            || self.model_sha256 != MODEL_SHA256
            || self.program_sha256 != PROGRAM_SHA256
            || self.request_sha256 != request
            || self.encoded_operations != 429
            || self.final_readbacks != 1
            || self.per_node_cpu_injection
            || !self.pool_retained_through_completion
            || self.actual_retained_buffer_capacity_bytes != BUFFER_BYTES
            || self.output_bytes != DEPTH_BYTES
            || !hex(&self.output_sha256)
            || self.adapter.backend != hardware.actual()
            || !matches!(
                self.adapter.device_type.as_str(),
                "DiscreteGpu" | "IntegratedGpu"
            )
            || self.adapter.name.is_empty()
            || self.adapter.name.len() > 256
            || self.adapter.name.chars().any(char::is_control)
        {
            return Err(Error::Invalid(
                "native WGSL result differs from requested model/backend/ownership contract",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
