//! Explicit real-model acceptance driver, never part of ordinary model-free CI.
#[path = "../../../desktop/app/src/logging/sink.rs"]
mod logging;
mod engine {
    pub fn current_command() -> Option<(u64, &'static str, std::time::Duration)> {
        None
    }
}
use motion_host::depth::{
    self,
    wgsl::{Config, Hardware, Wgsl},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    key: depth::Key,
    size: [u32; 2],
    rgb: String,
}
fn run(args: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(&args[5]);
    let request_path = PathBuf::from(&args[4]);
    let raw = fs::read(&request_path)?;
    if raw.len() > 65536 {
        return Err("acceptance request exceeds 64KiB".into());
    }
    let request: Request = serde_json::from_slice(&raw)?;
    let relative = Path::new(&request.rgb);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("RGB path must remain within request root".into());
    }
    let root = request_path
        .parent()
        .ok_or("missing request root")?
        .canonicalize()?;
    let path = root.join(relative).canonicalize()?;
    if !path.starts_with(&root) {
        return Err("RGB escapes request root".into());
    }
    let mut rgb = Vec::new();
    fs::File::open(path)?
        .take(4096 * 4096 * 3 + 1)
        .read_to_end(&mut rgb)?;
    let input = depth::Input::new(request.key.clone(), request.size, Arc::from(rgb))?;
    let backend = Wgsl::new(Config {
        python: PathBuf::from(&args[0]),
        worker: PathBuf::from(&args[1]),
        program: PathBuf::from(&args[2]),
        scratch: PathBuf::from(&args[3]),
        hardware: Hardware::Vulkan,
    })?;
    let mut jobs = depth::Jobs::start(backend)?;
    let began = Instant::now();
    let id = jobs.request(input)?;
    let mut flight = None;
    let mode = args
        .get(6)
        .and_then(|arg| arg.to_str())
        .unwrap_or("complete");
    let mut cancel_sent = false;
    let estimate = loop {
        if flight.is_none() {
            if let Some(current) = jobs.in_flight()? {
                flight = Some(
                    serde_json::json!({"request":current.request,"asset":current.asset,"phase":current.phase}),
                );
                fs::write(
                    out.join("in-flight.json"),
                    serde_json::to_vec_pretty(&flight)?,
                )?;
            }
        }
        if mode == "cancel" && !cancel_sent && out.join("cancel").is_file() {
            jobs.cancel()?;
            cancel_sent = true;
        }
        if cancel_sent && jobs.in_flight()?.is_none() && jobs.queue_depth()? == 0 {
            let counts = jobs.counts()?;
            if counts.canceled != 1
                || counts.failed != 0
                || jobs.take_ready(&request.key)?.is_some()
            {
                return Err("actual cancellation did not discard and reap its result".into());
            }
            fs::write(
                out.join("receipt.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                "status":"actual_shared_jobs_cancellation_completed","canceled":counts.canceled,"failed":counts.failed,
                "in_flight_clear":true,"queue_depth":0,"in_flight_observed":flight}))?,
            )?;
            return Ok(());
        }
        if mode == "stale" && jobs.counts()?.completed == 1 {
            let mut current = request.key.clone();
            current.source.revision += 1;
            if jobs.take_ready(&current)?.is_some() || jobs.take_ready(&request.key)?.is_some() {
                return Err("old GPU result escaped the live source identity guard".into());
            }
            let counts = jobs.counts()?;
            if counts.stale != 1 || jobs.in_flight()?.is_some() {
                return Err("stale result accounting mismatch".into());
            }
            fs::write(
                out.join("receipt.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                "status":"actual_old_gpu_result_discarded","stale":counts.stale,"completed":counts.completed,
                "in_flight_clear":true,"queue_depth":jobs.queue_depth()?,"in_flight_observed":flight}))?,
            )?;
            return Ok(());
        }
        if mode == "stale" {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        if let Some(estimate) = jobs.take_prepared_ready(&request.key)? {
            break estimate;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if flight.is_none() {
        return Err("real worker did not publish in-flight state".into());
    }
    let prepared = estimate.input();
    let estimate = estimate.into_parts().0;
    let bytes = estimate
        .resource
        .values()
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    fs::write(out.join("depth.f32"), &bytes)?;
    let counts = jobs.counts()?;
    let report = serde_json::json!({"status":"actual_shared_jobs_wgsl_output_written_independent_comparison_pending",
        "request":id,"key":request.key,"descriptor":estimate.resource.descriptor(),"prepared":prepared,
        "provider":estimate.provider,"output_sha256":format!("{:x}",Sha256::digest(&bytes)),"output_bytes":bytes.len(),
        "in_flight_observed":flight,"queue_depth":jobs.queue_depth()?,"in_flight_clear":jobs.in_flight()?.is_none(),
        "counts":{"submitted":counts.submitted,"completed":counts.completed,"failed":counts.failed,"stale":counts.stale},
        "costs_ns":{"total":estimate.costs.total.as_nanos(),"preprocess":estimate.costs.preprocess.as_nanos(),
            "native_child":estimate.costs.inference.as_nanos(),"postprocess":estimate.costs.postprocess.as_nanos(),
            "host_transport":estimate.costs.host_transport.as_nanos()},"elapsed_ms":began.elapsed().as_millis()});
    fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(6..=7).contains(&args.len())
        || args
            .get(6)
            .is_some_and(|arg| arg != "cancel" && arg != "stale")
    {
        return Err("usage: wgsl_depth <python> <native-worker> <program> <scratch> <input.json> <new-output-root> [cancel|stale]".into());
    }
    if args.iter().take(6).any(|arg| !Path::new(arg).is_absolute()) {
        return Err("absolute paths required".into());
    }
    let out = PathBuf::from(&args[5]);
    fs::create_dir(&out)?;
    let _logging = logging::init(out.join("logs"))?;
    let result = run(&args);
    if let Err(error) = &result {
        log::error!("operation=wgsl_depth_acceptance cause={error}");
    }
    result
}
