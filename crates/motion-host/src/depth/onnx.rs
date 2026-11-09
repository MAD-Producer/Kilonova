//! Optional external ONNX Runtime host. The owned Python procedure is embedded;
//! Python, its packages and hash-identified models are installed separately.
//! Each cancellable child owns its runtime. There is no render-thread GPU access.
use super::{Backend, Cancellation, Costs, Error, Estimate, Input, Result};
use motion_model::depth::{DepthDescriptor, DepthMeasure, DepthResource, NearDirection};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const PROCESSING: &str = "dav2-small-rgb8-letterbox518-align-corners-relative-v1";
/// Explicit processing profiles, independent of the requested provider.
pub fn processing_side(processing: &str) -> Option<u32> {
    match processing {
        "dav2-small-rgb8-letterbox266-align-corners-relative-v1" => Some(266),
        "dav2-small-rgb8-letterbox392-align-corners-relative-v1" => Some(392),
        PROCESSING => Some(518),
        _ => None,
    }
}
const WORKER: &str = include_str!("../../../../tools/ai_depth_onnx.py");

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn processing_resolution_is_explicit_and_not_a_provider_choice() {
        for side in [266, 392, 518] {
            assert_eq!(
                processing_side(&format!(
                    "dav2-small-rgb8-letterbox{side}-align-corners-relative-v1"
                )),
                Some(side)
            );
        }
        assert_eq!(processing_side(PROCESSING), Some(518));
        for value in [
            "",
            "CPUExecutionProvider",
            "dav2-small-rgb8-letterbox517-align-corners-relative-v1",
            "dav2-base-rgb8-letterbox518-align-corners-relative-v1",
            "dav2-small-rgb8-letterbox518-align-corners-relative-v2",
            "dav2-small-rgb8-letterbox518-align-corners-relative-v1 ",
        ] {
            assert_eq!(processing_side(value), None, "{value}");
        }
    }

    fn report(processing: &str, input_bytes: u64, output_bytes: u64) -> Report {
        serde_json::from_value(serde_json::json!({
            "schema": 1, "sha256": "a".repeat(64), "processing": processing,
            "width": 12, "height": 8, "resource_bytes": 384,
            "providers": ["CPUExecutionProvider"], "cpu_fallback": false,
            "runtime_version": "control", "input_host_copy_bytes": input_bytes,
            "output_host_copy_bytes": output_bytes,
            "costs_ns": {"load":1,"preprocess":1,"input_copy":1,
                "inference":1,"output_copy":1,"postprocess":1}
        }))
        .unwrap()
    }

    #[test]
    fn memory_report_retains_unavailable_and_rejects_inconsistent_counters() {
        use super::super::capability::memory::{Method, ProcessPeak};
        let mut observed = report(PROCESSING, 3_219_888, 1_073_296);
        observed.inference_process_memory = Some(ProcessPeak::Unavailable {
            pid: 75, cause: "original OS query failure".into(),
        });
        let digest = "a".repeat(64);
        observed.validate_identity(PROCESSING, &digest, [12, 8]).unwrap();
        observed.inference_process_memory = Some(ProcessPeak::Observed {
            pid: 75, method: Method::GetrusageLinux,
            peak_resident_bytes: 4096, peak_private_commit_bytes: Some(4096),
        });
        assert!(observed.validate_identity(PROCESSING, &digest, [12, 8]).is_err());
    }

    #[test]
    fn report_requires_the_exact_requested_profile_and_tensor_byte_counts() {
        // Independent byte counts for RGB Float32 and scalar Float32 tensors.
        for (side, input_bytes, output_bytes) in [
            (266, 849_072, 283_024),
            (392, 1_843_968, 614_656),
            (518, 3_219_888, 1_073_296),
        ] {
            let processing = format!("dav2-small-rgb8-letterbox{side}-align-corners-relative-v1");
            let digest = "a".repeat(64);
            let original = report(&processing, input_bytes, output_bytes);
            assert!(original
                .validate_identity(&processing, &digest, [12, 8])
                .is_ok());
            assert!(original
                .validate_identity("unknown", &digest, [12, 8])
                .is_err());
            let other = if side == 518 { 266 } else { 518 };
            let other_processing =
                format!("dav2-small-rgb8-letterbox{other}-align-corners-relative-v1");
            assert!(original
                .validate_identity(&other_processing, &digest, [12, 8])
                .is_err());
            for field in 0..7 {
                let mut altered = report(&processing, input_bytes, output_bytes);
                match field {
                    0 => altered.schema = 2,
                    1 => altered.sha256 = "b".repeat(64),
                    2 => altered.width = 13,
                    3 => altered.height = 9,
                    4 => altered.resource_bytes += 4,
                    5 => altered.input_host_copy_bytes += 4,
                    _ => altered.output_host_copy_bytes += 4,
                }
                assert!(
                    altered
                        .validate_identity(&processing, &digest, [12, 8])
                        .is_err(),
                    "side={side} field={field}"
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Provider {
    Cpu,
    Cuda,
    DirectMl,
    CoreMl,
}
impl Provider {
    fn name(self) -> &'static str {
        match self {
            Self::Cpu => "CPUExecutionProvider",
            Self::Cuda => "CUDAExecutionProvider",
            Self::DirectMl => "DmlExecutionProvider",
            Self::CoreMl => "CoreMLExecutionProvider",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub python: PathBuf,
    pub model: PathBuf,
    /// Explicit writable scratch root; never use the process's implicit temp.
    pub scratch: PathBuf,
    pub provider: Provider,
}

pub struct Onnx {
    config: Config,
    last_provider: Option<String>,
    last_diagnostic: Option<String>,
    execution: Option<super::cache::Execution>,
    resolution_memory: Option<crate::process_memory::ProcessPeak>,
}
impl Onnx {
    /// Construction does not load a model or runtime on the caller's thread.
    pub fn new(config: Config) -> Result<Self> {
        if [&config.python, &config.model, &config.scratch]
            .iter()
            .any(|p| !p.is_absolute())
        {
            return Err(Error::Invalid(
                "Python, model and scratch paths must be absolute",
            ));
        }
        Ok(Self {
            config,
            last_provider: None,
            last_diagnostic: None,
            execution: None,
            resolution_memory: None,
        })
    }

    fn infer_in(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
        directory: &std::path::Path,
        execution: Option<&super::cache::Execution>,
    ) -> Result<Estimate> {
        let transport_began = Instant::now();
        cancellation.check()?;
        let processing = &input.key().model.processing;
        if processing_side(processing).is_none() {
            return Err(Error::Invalid(
                "ONNX backend does not implement the requested preprocessing identity",
            ));
        }
        let digest: String = input
            .key()
            .model
            .sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        fs::write(directory.join("worker.py"), WORKER)?;
        fs::write(directory.join("input.rgb"), input.rgb())?;
        let [width, height] = input.size();
        let request = serde_json::json!({"schema":1,"model":self.config.model,"sha256":digest,
            "processing":processing,"width":width,"height":height,"provider":self.config.provider.name()});
        fs::write(
            directory.join("request.json"),
            serde_json::to_vec(&request)?,
        )?;
        cancellation.check()?;
        let mut command = Command::new(&self.config.python);
        command
            .arg("-I")
            .arg("-B")
            .arg(directory.join("worker.py"))
            .arg(directory)
            .env("TMP", directory)
            .env("TEMP", directory)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if execution.is_some() { command.arg("--attest"); }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let input_transport = transport_began.elapsed();
        let (stdout, stderr) = capture(&mut command, cancellation)?;
        let output_transport_began = Instant::now();
        let report: Report = serde_json::from_slice(&stdout)?;
        report.validate_identity(processing, &digest, input.size())?;
        if let Some(expected) = execution {
            if report.execution.as_ref() != Some(expected) {
                return Err(Error::Invalid("executing runtime differs from pre-lookup cache identity"));
            }
        }
        if report.providers.is_empty()
            || report.providers.len() > 4
            || report
                .providers
                .iter()
                .any(|p| p.is_empty() || p.len() > 64 || p.chars().any(char::is_control))
        {
            return Err(Error::Invalid(
                "runtime did not report bounded actual node providers",
            ));
        }
        let provider = report.providers.join(",");
        if self.last_provider.as_ref() != Some(&provider) {
            log::info!("operation=depth_runtime asset={} actual_provider={} requested_provider={} cpu_fallback={} runtime={}",
                input.key().source.asset, provider, self.config.provider.name(), report.cpu_fallback,
                report.runtime_version.chars().take(64).collect::<String>());
            self.last_provider = Some(provider.clone());
        }
        if !stderr.is_empty() && self.last_diagnostic.as_ref() != Some(&stderr) {
            log::warn!(
                "operation=depth_runtime asset={} phase=runtime_diagnostic cause={}",
                input.key().source.asset,
                stderr
            );
            self.last_diagnostic = Some(stderr);
        }
        let path = directory.join("output.f32");
        if path.metadata()?.len() != report.resource_bytes {
            return Err(Error::Invalid(
                "depth output bytes differ from declared dimensions",
            ));
        }
        let bytes = fs::read(path)?;
        let values = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        cancellation.check()?;
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
        let n = report.costs_ns;
        Ok(Estimate {
            resource,
            provider,
            costs: Costs {
                adapter_memory_samples: None,
                source_prepare: Duration::ZERO,
                source_rgb_bytes: 0,
                total: Duration::ZERO, // Filled after temporary cleanup in infer().
                host_transport: input_transport + output_transport_began.elapsed(),
                load: Duration::from_nanos(n.load),
                preprocess: Duration::from_nanos(n.preprocess),
                input_copy: Duration::from_nanos(n.input_copy),
                inference: Duration::from_nanos(n.inference),
                output_copy: Duration::from_nanos(n.output_copy),
                postprocess: Duration::from_nanos(n.postprocess),
                alignment: Duration::ZERO,
                input_bytes: report.input_host_copy_bytes,
                output_bytes: report.output_host_copy_bytes,
                child_process_memory: Some(crate::process_memory::Children::Onnx {
                    resolution: if execution.is_some() { self.resolution_memory.take() } else { None },
                    inference: report.inference_process_memory.clone(),
                }),
                inference_process_memory: report.inference_process_memory,
            },
        })
    }
}

impl Backend for Onnx {
    fn execution_identity(&mut self, input: &Input, cancellation: &Cancellation)
        -> Result<Option<super::cache::Execution>> {
        self.execution = None;
        self.resolution_memory = None;
        cancellation.check()?;
        if !matches!(self.config.provider, Provider::Cpu) { return Ok(None); }
        fs::create_dir_all(&self.config.scratch)?;
        let temporary = tempfile::Builder::new().prefix("motion-depth-resolve-").tempdir_in(&self.config.scratch)?;
        let result = (|| {
            let directory = temporary.path();
            fs::write(directory.join("worker.py"), WORKER)?;
            let digest: String = input.key().model.sha256.iter().map(|b| format!("{b:02x}")).collect();
            let [width, height] = input.size();
            fs::write(directory.join("request.json"), serde_json::to_vec(&serde_json::json!({
                "schema":1,"model":self.config.model,"sha256":digest,"processing":input.key().model.processing,
                "width":width,"height":height,"provider":self.config.provider.name()}))?)?;
            let mut command = Command::new(&self.config.python);
            command.arg("-I").arg("-B").arg(directory.join("worker.py")).arg(directory).arg("--resolve")
                .env("TMP",directory).env("TEMP",directory).env("PYTHONDONTWRITEBYTECODE","1")
                .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(windows)] {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let (stdout, stderr) = capture(&mut command, cancellation)?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Resolved { schema: u32, execution: super::cache::Execution, process_memory: Option<crate::process_memory::ProcessPeak> }
            let resolved: Resolved = serde_json::from_slice(&stdout)?;
            if let Some(memory) = &resolved.process_memory { memory.validate().map_err(Error::Invalid)?; }
            resolved.execution.validate()?;
            if resolved.schema != 1 || resolved.execution.backend != "onnx"
                || resolved.execution.provider != "CPUExecutionProvider" || resolved.execution.precision != "float32"
                || resolved.execution.plan_sha256 != <[u8;32]>::from(Sha256::digest(WORKER.as_bytes())) {
                return Err(Error::Invalid("unqualified CPU cache execution attestation"));
            }
            if !stderr.is_empty() && self.last_diagnostic.as_ref() != Some(&stderr) {
                log::warn!("operation=depth_runtime asset={} phase=cache_resolve cause={}", input.key().source.asset, stderr);
                self.last_diagnostic = Some(stderr);
            }
            cancellation.check()?;
            Ok((resolved.execution, resolved.process_memory))
        })();
        match (result, temporary.close()) {
            (Ok((execution, memory)), Ok(())) => { self.execution = Some(execution.clone()); self.resolution_memory = memory; Ok(Some(execution)) },
            (Err(primary), Ok(())) => Err(primary),
            (Err(primary), Err(cleanup)) => Err(Error::Cleanup { primary: Box::new(primary), cleanup }),
            (Ok(_), Err(cleanup)) => Err(cleanup.into()),
        }
    }
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate> {
        let began = Instant::now();
        cancellation.check()?;
        fs::create_dir_all(&self.config.scratch)?;
        let temporary = tempfile::Builder::new()
            .prefix("motion-depth-")
            .tempdir_in(&self.config.scratch)?;
        let execution = self.execution.take();
        let result = self.infer_in(input, cancellation, temporary.path(), execution.as_ref());
        match (result, temporary.close()) {
            (Ok(mut estimate), Ok(())) => {
                estimate.costs.total = began.elapsed();
                Ok(estimate)
            }
            (Err(error), Ok(())) => Err(error),
            (Err(primary), Err(cleanup)) => Err(Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
            (Ok(_), Err(cleanup)) => Err(Error::Io(cleanup)),
        }
    }
}

#[derive(Deserialize)]
struct CostsNs {
    load: u64,
    preprocess: u64,
    input_copy: u64,
    inference: u64,
    output_copy: u64,
    postprocess: u64,
}
#[derive(Deserialize)]
struct Report {
    inference_process_memory: Option<super::capability::memory::ProcessPeak>,
    execution: Option<super::cache::Execution>,
    schema: u32,
    sha256: String,
    processing: String,
    width: u32,
    height: u32,
    providers: Vec<String>,
    cpu_fallback: bool,
    runtime_version: String,
    costs_ns: CostsNs,
    input_host_copy_bytes: u64,
    output_host_copy_bytes: u64,
    resource_bytes: u64,
}

impl Report {
    fn validate_identity(&self, processing: &str, digest: &str, size: [u32; 2]) -> Result<()> {
        if let Some(memory) = &self.inference_process_memory { memory.validate().map_err(Error::Invalid)?; }
        let side = processing_side(processing).map(u64::from);
        if self.schema != 1
            || self.sha256 != digest
            || self.processing != processing
            || [self.width, self.height] != size
            || self.resource_bytes != u64::from(size[0]) * u64::from(size[1]) * 4
            || side.is_none_or(|n| {
                self.input_host_copy_bytes != n * n * 12 || self.output_host_copy_bytes != n * n * 4
            })
        {
            return Err(Error::Invalid(
                "runtime report differs from requested resource identity",
            ));
        }
        Ok(())
    }
}

struct ChildOwner {
    child: Child,
    reaped: bool,
}
impl ChildOwner {
    fn terminate(&mut self) -> std::io::Result<()> {
        if self.reaped {
            return Ok(());
        }
        if self.child.try_wait()?.is_none() {
            if let Err(error) = self.child.kill() {
                if self.child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            self.child.wait()?;
        }
        self.reaped = true;
        Ok(())
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            log::error!(
                "operation=depth_runtime phase=child_cleanup pid={} cause={error}",
                self.child.id()
            );
        }
    }
}

fn drain(
    mut input: impl Read,
    limit: usize,
    tail: bool,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::with_capacity(limit);
    let mut head = Vec::new();
    let mut chunk = [0u8; 2048];
    let mut total = 0usize;
    loop {
        let n = input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n);
        if total > limit && !tail {
            overflow.store(true, Ordering::Release);
        }
        if tail {
            // Keep the fatal-stack header and the final original cause. A long
            // extension-module list/backtrace must not displace either end.
            let prefix = n.min((limit / 2).saturating_sub(head.len()));
            head.extend_from_slice(&chunk[..prefix]);
            let part = &chunk[prefix..n];
            let tail_limit = limit - limit / 2;
            if tail_limit == 0 {
                continue;
            }
            if part.len() >= tail_limit {
                kept.clear();
                kept.extend_from_slice(&part[part.len() - tail_limit..]);
                continue;
            }
            let excess = kept
                .len()
                .saturating_add(part.len())
                .saturating_sub(tail_limit);
            if excess > 0 {
                kept.drain(..excess);
            }
            kept.extend_from_slice(part);
        } else {
            let take = n.min(limit.saturating_sub(kept.len()));
            kept.extend_from_slice(&chunk[..take]);
        }
    }
    if tail {
        if total > limit {
            const OMITTED: &[u8] = b"\n[stderr middle omitted]\n";
            let marker = &OMITTED[..OMITTED.len().min(head.len())];
            head.truncate(head.len() - marker.len());
            head.extend_from_slice(marker);
        }
        head.extend_from_slice(&kept);
        Ok(head)
    } else {
        Ok(kept)
    }
}

fn diagnostic_summary(text: &str) -> String {
    const LIMIT: usize = 512;
    const OMITTED: &str = "\n[stderr middle omitted]\n";
    if text.chars().count() <= LIMIT {
        return text.to_owned();
    }
    let prefix = LIMIT / 2;
    let suffix = LIMIT - prefix - OMITTED.chars().count();
    let head = text.chars().take(prefix).collect::<String>();
    let tail = text
        .chars()
        .rev()
        .take(suffix)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{head}{OMITTED}{tail}")
}

pub(super) fn capture(
    command: &mut Command,
    cancellation: &Cancellation,
) -> Result<(Vec<u8>, String)> {
    let mut owner = ChildOwner {
        child: command.spawn()?,
        reaped: false,
    };
    let stdout = owner
        .child
        .stdout
        .take()
        .ok_or(Error::Invalid("missing runtime stdout pipe"))?;
    let stderr = owner
        .child
        .stderr
        .take()
        .ok_or(Error::Invalid("missing runtime stderr pipe"))?;
    let overflow = Arc::new(AtomicBool::new(false));
    let out_limit = overflow.clone();
    let stdout = thread::Builder::new()
        .name("depth-stdout".into())
        .spawn(move || drain(stdout, 16384, false, out_limit))?;
    let stderr = thread::Builder::new()
        .name("depth-stderr".into())
        .spawn(move || drain(stderr, 8192, true, Arc::new(AtomicBool::new(false))))?;
    let status = loop {
        if cancellation.is_canceled() {
            owner.terminate().map_err(|cleanup| Error::Cleanup {
                primary: Box::new(Error::Canceled),
                cleanup,
            })?;
            break None;
        }
        if overflow.load(Ordering::Acquire) {
            owner.terminate().map_err(|cleanup| Error::Cleanup {
                primary: Box::new(Error::Invalid("runtime stdout exceeds protocol budget")),
                cleanup,
            })?;
            break None;
        }
        if let Some(status) = owner.child.try_wait()? {
            owner.reaped = true;
            break Some(status);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout
        .join()
        .map_err(|p| Error::Panic(super::panic_cause(p.as_ref())))??;
    let stderr = stderr
        .join()
        .map_err(|p| Error::Panic(super::panic_cause(p.as_ref())))??;
    let stderr = String::from_utf8_lossy(&stderr);
    cancellation.check()?;
    if overflow.load(Ordering::Acquire) {
        return Err(Error::Invalid("runtime stdout exceeds protocol budget"));
    }
    if !status.is_some_and(|status| status.success()) {
        // Retain the bounded stack before temporary directories disappear.
        // Stdout remains the protocol channel; it is never used for logging.
        if !stderr.is_empty() {
            log::error!(
                "operation=depth_runtime phase=child_exit pid={} status={status:?} stderr={stderr}",
                owner.child.id()
            );
            log::logger().flush();
        }
        return Err(Error::Backend(format!(
            "runtime exit={status:?} response={} stderr={}",
            String::from_utf8_lossy(&stdout)
                .chars()
                .take(2048)
                .collect::<String>(),
            diagnostic_summary(&stderr)
        )));
    }
    Ok((stdout, diagnostic_summary(&stderr)))
}

#[cfg(test)]
mod tests;
