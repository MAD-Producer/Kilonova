//! Explicit optional depth worker. Model resources are installed separately.
mod admission;
mod convolution_admission;
mod graph_admission;
mod index_admission;
#[path = "../logging/sink.rs"]
mod logging;

use admission::{Result, Tensor};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Instant,
};
use wgpu::util::DeviceExt;

// Reuse the application's unchanged logger and panic hook with a real local
// operation context. The probe is single-threaded; no business locks are held.
mod engine {
    use std::{
        cell::RefCell,
        time::{Duration, Instant},
    };
    thread_local! { static CURRENT: RefCell<Option<(u64, &'static str, Instant)>> = const { RefCell::new(None) }; }
    pub fn current_command() -> Option<(u64, &'static str, Duration)> {
        CURRENT.with(|slot| {
            slot.try_borrow().ok().and_then(|v| {
                v.as_ref()
                    .map(|(id, op, start)| (*id, *op, start.elapsed()))
            })
        })
    }
    pub struct Flight {
        previous: Option<(u64, &'static str, Instant)>,
        entered: bool,
    }
    pub fn enter(id: u64, operation: &'static str) -> Flight {
        CURRENT.with(|slot| {
            if let Ok(mut v) = slot.try_borrow_mut() {
                Flight {
                    previous: v.replace((id, operation, Instant::now())),
                    entered: true,
                }
            } else {
                Flight {
                    previous: None,
                    entered: false,
                }
            }
        })
    }
    impl Drop for Flight {
        fn drop(&mut self) {
            if !self.entered {
                return;
            }
            CURRENT.with(|slot| {
                if let Ok(mut v) = slot.try_borrow_mut() {
                    *v = self.previous.take();
                }
            });
        }
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require(ok: bool, reason: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(reason.into()) }
}
fn limited_read(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    require(
        file.metadata().map_err(|e| e.to_string())?.len() <= maximum,
        "input exceeds declared byte bound",
    )?;
    let mut data = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    require(
        data.len() as u64 <= maximum,
        "input grew beyond declared byte bound",
    )?;
    Ok(data)
}
fn tensor_bytes(root: &Path, tensor: &Tensor) -> Result<Vec<u8>> {
    let relative = Path::new(&tensor.file);
    require(
        !relative.is_absolute()
            && relative
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
        "tensor path must stay within fixture root",
    )?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("tensor {}: {e}", tensor.name))?;
    require(
        path.starts_with(root),
        "tensor canonical path escapes fixture root",
    )?;
    let bytes = limited_read(&path, tensor.bytes()?)?;
    require(
        bytes.len() as u64 == tensor.bytes()? && digest(&bytes) == tensor.sha256,
        "tensor length or SHA-256 mismatch",
    )?;
    require(
        bytes
            .chunks_exact(4)
            .all(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]).is_finite()),
        "tensor contains nonfinite Float32 values",
    )?;
    Ok(bytes)
}
fn save(out: &Path, report: &Value) -> Result<()> {
    fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write observation receipt: {e}"))
}
fn phase(out: &Path, report: &mut Value, name: &str) -> Result<()> {
    report["phase"] = name.into();
    if let Some((request, operation, elapsed)) = engine::current_command() {
        report["in_flight"] =
            json!({"request":request,"operation":operation,"elapsed_ms":elapsed.as_millis()});
    }
    log::info!(
        "phase={name} request={} operation={}",
        report["request"],
        report["operation"]
    );
    logging::flush();
    save(out, report)
}
include!("graph_runtime_impl.rs");
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!(
            "usage: motion-depth-wgsl <absolute-request.json> <new-absolute-output-directory>"
        );
        std::process::exit(2);
    }
    let request = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    if !request.is_absolute() || !out.is_absolute() {
        eprintln!("absolute input/output paths required");
        std::process::exit(2);
    }
    if let Err(cause) = fs::create_dir(&out) {
        eprintln!(
            "create new observation directory {}: {cause}",
            out.display()
        );
        std::process::exit(2);
    }
    let _logger = match logging::init(out.join("logs")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("initialize file logger: {e}");
            std::process::exit(2);
        }
    };
    let mut report = json!({"status":"running","pid":std::process::id(),"request":0,"operation":"admission","phase":"read_request","encoded_operations":0,
        "limits":["Complete composed GPU research only; no product capability, temporal quality or performance qualification",
            "Software adapters and implicit backend/precision fallback refused",
            "Logical buffer admission is not measured process/device memory",
            "External fixture identities are verified, not leased against concurrent filesystem mutation"]});
    let _flight = engine::enter(0, "complete_model_probe");
    log::info!("start #0 complete_model_probe");
    logging::flush();
    let began = Instant::now();
    if let Err(cause) =
        phase(&out, &mut report, "read_request").and_then(|()| run(&request, &out, &mut report))
    {
        log::error!(
            "composed GPU research failed phase={} case={} elapsed_ms={} cause={cause}",
            report["phase"],
            report["case"],
            began.elapsed().as_millis()
        );
        report["status"] = "failed_original_cause_retained".into();
        report["failure"] = cause.into();
        report["elapsed_ms"] = json!(began.elapsed().as_millis());
        report["in_flight"] = Value::Null;
        if let Err(secondary) = save(&out, &report) {
            log::error!("failure receipt also unavailable: {secondary}");
        }
        logging::flush();
        std::process::exit(1);
    }
    logging::flush();
}
