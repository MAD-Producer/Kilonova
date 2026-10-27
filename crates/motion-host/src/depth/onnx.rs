//! Optional external ONNX Runtime host. The owned Python procedure is embedded;
//! Python, its packages and hash-identified models are installed separately.
//! Each cancellable child owns its runtime. There is no render-thread GPU access.
use super::{Backend, Cancellation, Costs, Error, Estimate, Input, Result};
use motion_model::depth::{DepthDescriptor, DepthMeasure, DepthResource, NearDirection};
use serde::Deserialize;
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
const WORKER: &str = include_str!("../../../../tools/ai_depth_onnx.py");

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
        })
    }

    fn infer_in(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
        directory: &std::path::Path,
    ) -> Result<Estimate> {
        let transport_began = Instant::now();
        cancellation.check()?;
        if input.key().model.processing != PROCESSING {
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
            "processing":PROCESSING,"width":width,"height":height,"provider":self.config.provider.name()});
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
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let input_transport = transport_began.elapsed();
        let (stdout, stderr) = capture(&mut command, cancellation)?;
        let output_transport_began = Instant::now();
        let report: Report = serde_json::from_slice(&stdout)?;
        if report.schema != 1
            || report.sha256 != digest
            || report.processing != PROCESSING
            || [report.width, report.height] != input.size()
            || report.resource_bytes != u64::from(width) * u64::from(height) * 4
        {
            return Err(Error::Invalid(
                "runtime report differs from requested resource identity",
            ));
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
                total: Duration::ZERO, // Filled after temporary cleanup in infer().
                host_transport: input_transport + output_transport_began.elapsed(),
                load: Duration::from_nanos(n.load),
                preprocess: Duration::from_nanos(n.preprocess),
                input_copy: Duration::from_nanos(n.input_copy),
                inference: Duration::from_nanos(n.inference),
                output_copy: Duration::from_nanos(n.output_copy),
                postprocess: Duration::from_nanos(n.postprocess),
                input_bytes: report.input_host_copy_bytes,
                output_bytes: report.output_host_copy_bytes,
            },
        })
    }
}

impl Backend for Onnx {
    fn infer(&mut self, input: &Input, cancellation: &Cancellation) -> Result<Estimate> {
        let began = Instant::now();
        cancellation.check()?;
        fs::create_dir_all(&self.config.scratch)?;
        let temporary = tempfile::Builder::new()
            .prefix("motion-depth-")
            .tempdir_in(&self.config.scratch)?;
        let result = self.infer_in(input, cancellation, temporary.path());
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
            if n >= limit {
                kept.clear();
                kept.extend_from_slice(&chunk[n - limit..n]);
                continue;
            }
            let excess = kept.len().saturating_add(n).saturating_sub(limit);
            if excess > 0 {
                kept.drain(..excess);
            }
            kept.extend_from_slice(&chunk[..n]);
        } else {
            let take = n.min(limit.saturating_sub(kept.len()));
            kept.extend_from_slice(&chunk[..take]);
        }
    }
    Ok(kept)
}

fn capture(command: &mut Command, cancellation: &Cancellation) -> Result<(Vec<u8>, String)> {
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
    let stderr = stderr
        .chars()
        .skip(stderr.chars().count().saturating_sub(512))
        .collect::<String>();
    cancellation.check()?;
    if overflow.load(Ordering::Acquire) {
        return Err(Error::Invalid("runtime stdout exceeds protocol budget"));
    }
    if !status.is_some_and(|status| status.success()) {
        return Err(Error::Backend(format!(
            "runtime exit={status:?} response={} stderr={stderr}",
            String::from_utf8_lossy(&stdout)
                .chars()
                .take(2048)
                .collect::<String>()
        )));
    }
    Ok((stdout, stderr))
}

#[cfg(test)]
mod tests;
