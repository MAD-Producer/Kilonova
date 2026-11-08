//! Explicit real-model probe. See depth75-measured-capability-current.md for
//! timing scope and limits; these observations establish no quality ranking.
use motion_host::depth::{self, Backend, Input, Jobs, Key};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "../../../desktop/app/src/logging/sink.rs"]
mod logging;
mod engine {
    pub fn current_command() -> Option<(u64, &'static str, std::time::Duration)> {
        None
    }
}

fn samples(
    backend: impl Backend,
    input: Input,
    count: usize,
    latency_budget: Duration,
) -> depth::Result<serde_json::Value> {
    let mut jobs = Jobs::start_measured(backend)?;
    let mut rows = Vec::with_capacity(count);
    let mut profile = depth::capability::Profile::new(count)?;
    for _ in 0..count {
        let request = jobs.request(input.clone())?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let ready = loop {
            if let Some(ready) = jobs.take_prepared_ready(input.key())? {
                break ready;
            }
            if Instant::now() >= deadline {
                return Err(depth::Error::Backend(format!(
                    "capability probe request={request} did not complete within 60 seconds"
                )));
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let measured = ready
            .measurement()
            .ok_or(depth::Error::Invalid("worker omitted probe measurement"))?;
        if measured.request() != request
            || measured.execution().provider != ready.estimate().provider
            || measured.total() != ready.estimate().costs.total
            || measured.total() < measured.resolve()
        {
            return Err(depth::Error::Invalid(
                "inconsistent measured capability result",
            ));
        }
        let c = &ready.estimate().costs;
        profile.record(&ready)?;
        rows.push(serde_json::json!({
            "request": request, "execution": measured.execution(), "input": ready.input(),
            "worker_total_ns": measured.total().as_nanos(), "resolve_ns": measured.resolve().as_nanos(),
            "retained_gpu_buffer_capacity_bytes": measured.retained_gpu_buffer_capacity(),
            "inference_process_memory": measured.inference_process_memory(),
            "source_prepare_ns": c.source_prepare.as_nanos(), "source_rgb_bytes": c.source_rgb_bytes,
            "load_ns": c.load.as_nanos(), "preprocess_ns": c.preprocess.as_nanos(),
            "inference_ns": c.inference.as_nanos(), "postprocess_ns": c.postprocess.as_nanos(),
            "host_transport_ns": c.host_transport.as_nanos(), "input_copy_ns": c.input_copy.as_nanos(),
            "output_copy_ns": c.output_copy.as_nanos(), "input_tensor_bytes": c.input_bytes,
            "output_tensor_bytes": c.output_bytes, "output_values": ready.estimate().resource.values().len()
        }));
    }
    let counts = jobs.counts()?;
    let summary = profile.summary()?;
    let decision = depth::capability::latency::select(
        depth::capability::latency::Request {
            model: input.key().model.clone(),
            geometry: input.key().geometry.clone(),
            backend: Some(summary.execution().backend.clone()),
            maximum_worker_latency: latency_budget,
            minimum_samples: 2,
        },
        &[&profile],
    )?;
    let resolved = decision.resolved().map(|s| {
        serde_json::json!({
            "model": s.model(), "execution": s.execution(), "samples": s.samples(),
            "slowest_ns": s.slowest().as_nanos(),
        })
    });
    if counts.completed != count as u64
        || counts.failed != 0
        || counts.canceled != 0
        || counts.cache_hits != 0
        || counts.restored != 0
        || jobs.in_flight()?.is_some()
        || jobs.queue_depth()? != 0
    {
        return Err(depth::Error::Invalid(
            "probe unexpectedly reused, discarded or retained work",
        ));
    }
    Ok(serde_json::json!({
        "schema": 1, "key": input.key(), "samples": rows,
        "scope": "predecoded opaque RGB8 sRGB; worker preparation, resolution, inference and output checks; no queue wait, caller decode, alignment, renderer, quality, peak memory or thermal claim",
        "cache_hits": counts.cache_hits, "restored": counts.restored,
        "latency_selection": {
            "scope": "worker-latency admission only; quality, memory and sustained admission remain unqualified",
            "requested": {"model": decision.requested().model, "geometry": decision.requested().geometry,
                "backend": decision.requested().backend, "maximum_worker_latency_ns": latency_budget.as_nanos(),
                "minimum_samples": decision.requested().minimum_samples},
            "disposition": decision.disposition(), "resolved": resolved,
            "candidate_admission": decision.candidates()[0].admission(),
        },
        "profile": {"samples": summary.samples(), "fastest_ns": summary.fastest().as_nanos(),
            "median_ns": summary.median().as_nanos(), "slowest_ns": summary.slowest().as_nanos(),
            "resolve_slowest_ns": summary.resolve_slowest().as_nanos(),
            "retained_gpu_buffer_capacity_bytes": summary.retained_gpu_buffer_capacity()}
    }))
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("probe input exceeds its explicit bound".into());
    }
    Ok(bytes)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 10 {
        return Err("usage: depth_capability <new-output-root> <onnx|wgsl> <python> <model.onnx> <program.json|-> <worker|-> <rgb-file> <width>x<height> <samples:1..16> <maximum-worker-latency-ms:1..60000>".into());
    }
    if std::env::var_os("MOTION_LOG").is_some() {
        return Err("unset MOTION_LOG for default-level qualification".into());
    }
    let root = PathBuf::from(&args[0]);
    if !root.is_absolute() || root.exists() {
        return Err("new absolute output root required".into());
    }
    let _guard = logging::init(root.join("logs"))?;
    let kind = args[1].to_str().ok_or("backend must be UTF-8")?;
    let python = PathBuf::from(&args[2]);
    let model = PathBuf::from(&args[3]);
    let program = PathBuf::from(&args[4]);
    let worker = PathBuf::from(&args[5]);
    let rgb_path = PathBuf::from(&args[6]);
    let dimensions = args[7].to_str().ok_or("dimensions must be UTF-8")?;
    let (w, h) = dimensions
        .split_once('x')
        .ok_or("dimensions must be WIDTHxHEIGHT")?;
    let size: [u32; 2] = [w.parse()?, h.parse()?];
    if size.contains(&0) || size.iter().any(|n| *n > 4096) {
        return Err("dimensions outside 1..4096".into());
    }
    let count: usize = args[8]
        .to_str()
        .ok_or("sample count must be UTF-8")?
        .parse()?;
    if !(1..=16).contains(&count) {
        return Err("sample count outside 1..16".into());
    }
    let budget_ms: u64 = args[9]
        .to_str()
        .ok_or("latency budget must be UTF-8")?
        .parse()?;
    if !(1..=60_000).contains(&budget_ms) {
        return Err("latency budget outside 1..60000 milliseconds".into());
    }
    let latency_budget = Duration::from_millis(budget_ms);
    if [&python, &model, &rgb_path]
        .iter()
        .any(|p| !p.is_absolute())
    {
        return Err("absolute input paths required".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bounded_read(
        &model.with_file_name("model.json"),
        64 << 10,
    )?)?;
    let digest = manifest["sha256"].as_str().ok_or("model digest missing")?;
    if digest.len() != 64 || !digest.is_ascii() {
        return Err("model digest must be 64 hexadecimal ASCII characters".into());
    }
    let mut hash = [0; 32];
    for (i, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest[i * 2..i * 2 + 2], 16)?;
    }
    let field = |name: &str| -> Result<String, Box<dyn std::error::Error>> {
        Ok(manifest[name]
            .as_str()
            .ok_or("model manifest field missing")?
            .to_owned())
    };
    let input = Input::new(
        Key {
            source: DepthSource {
                asset: 75,
                revision: 1,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: size,
                crop: [0, 0, size[0], size[1]],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: field("id")?,
                revision: field("revision")?,
                sha256: hash,
                processing: field("processing")?,
            },
        },
        size,
        Arc::from(bounded_read(
            &rgb_path,
            u64::from(size[0]) * u64::from(size[1]) * 3,
        )?),
    )?;
    let report = match kind {
        "onnx" => samples(
            depth::onnx::Onnx::new(depth::onnx::Config {
                python,
                model,
                scratch: root.join("scratch"),
                provider: depth::onnx::Provider::Cpu,
            })?,
            input,
            count,
            latency_budget,
        )?,
        "wgsl" if program.is_absolute() && worker.is_absolute() => samples(
            depth::wgsl::Wgsl::new(depth::wgsl::Config {
                python,
                worker,
                program,
                scratch: root.join("scratch"),
                hardware: depth::wgsl::Hardware::Vulkan,
            })?,
            input,
            count,
            latency_budget,
        )?,
        _ => {
            return Err("explicit onnx or wgsl with absolute program/worker paths required".into())
        }
    };
    let bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(root.join("measurements.json"), bytes)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn main() {
    if let Err(cause) = run() {
        let cause: String = cause.to_string().chars().take(2048).collect();
        if log::log_enabled!(log::Level::Error) {
            log::error!("operation=depth_capability phase=driver cause={}", cause);
        } else {
            // Argument errors before logger initialization still reach stderr.
            eprintln!("depth capability probe failed: {cause}");
        }
        std::process::exit(1);
    }
}
