//! Explicit measured inference over the fixed sensor-reference input cohort.
//! Outputs remain private evaluation inputs, not quality certificates or assets.
use motion_host::depth::{self, Backend, Input, Jobs, Key};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
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
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const RGB_BYTES: usize = 640 * 480 * 3;

#[derive(Deserialize)]
struct Rgb {
    filename: String,
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
struct Spec {
    index: usize,
    rgb_ns: u64,
    split: String,
    rgb_input: Rgb,
}

fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    if path.metadata()?.len() > limit as u64 {
        return Err("reference input exceeds byte budget".into());
    }
    // The second bound also detects a source growing between metadata and read.
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("reference input grew beyond byte budget".into());
    }
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn decode_digest(text: &str) -> Result<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return Err("reference digest requires 64 hexadecimal ASCII characters".into());
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16)?;
    }
    Ok(bytes)
}
fn validate_plan(plan: &[Spec]) -> Result<()> {
    if plan.len() != 32 {
        return Err("reference cohort requires exactly32 pairs".into());
    }
    for (index, row) in plan.iter().enumerate() {
        if row.index != index
            || row.split
                != if index < 8 {
                    "calibration"
                } else {
                    "evaluation"
                }
            || row.rgb_input.filename != format!("{index:02}.rgb")
            || row.rgb_input.bytes != RGB_BYTES
            || i64::try_from(row.rgb_ns).is_err()
            || (index > 0 && row.rgb_ns <= plan[index - 1].rgb_ns)
        {
            return Err("reference cohort order, role, extent or filename differs".into());
        }
        decode_digest(&row.rgb_input.sha256)?;
    }
    Ok(())
}
fn read_rgb(root: &Path, row: &Spec) -> Result<Vec<u8>> {
    let rgb = bounded_read(&root.join(&row.rgb_input.filename), RGB_BYTES)?;
    if rgb.len() != RGB_BYTES
        || <[u8; 32]>::from(Sha256::digest(&rgb)) != decode_digest(&row.rgb_input.sha256)?
    {
        return Err("reference RGB differs from frozen preparation identity".into());
    }
    Ok(rgb)
}
fn write_report(
    root: &Path,
    status: &str,
    rows: &[serde_json::Value],
    summary: Option<&serde_json::Value>,
    cause: Option<&str>,
) -> Result<()> {
    let value = serde_json::json!({
        "schema": 1, "status":status, "samples":rows, "summary":summary, "cause":cause,
        "scope":"Fixed32 predecoded RGB8 inputs; actual measured Jobs; raw relative float outputs; no sensor fit, quality decision, full memory or thermal claim"
    });
    let pending = root.join("measurements.pending.json");
    fs::write(&pending, serde_json::to_vec_pretty(&value)?)?;
    fs::rename(pending, root.join("measurements.json"))?;
    Ok(())
}

fn samples(backend: impl Backend, model: DepthModel, input_root: &Path, root: &Path) -> Result<()> {
    let plan: Vec<Spec> =
        serde_json::from_slice(&bounded_read(&input_root.join("inputs.json"), 64 << 10)?)?;
    validate_plan(&plan)?;
    let mut jobs = Jobs::start_measured(backend)?;
    let mut profile = depth::capability::Profile::new(32)?;
    let mut rows = Vec::with_capacity(32);
    write_report(root, "running", &rows, None, None)?;
    let result: Result<serde_json::Value> = (|| {
        for row in &plan {
            let loading = Instant::now();
            let rgb = read_rgb(input_root, row)?;
            let caller_rgb_load = loading.elapsed();
            let key = Key {
                source: DepthSource {
                    asset: 750_000 + row.index as u64,
                    revision: 1,
                    pts: DepthTime {
                        ticks: row.rgb_ns.try_into()?,
                        timescale: 1_000_000_000,
                    },
                },
                geometry: DepthGeometry {
                    source_size: [640, 480],
                    crop: [0, 0, 640, 480],
                    quarter_turns: 0,
                    mirror_x: false,
                },
                model: model.clone(),
            };
            let request = jobs.request(Input::new(key.clone(), [640, 480], Arc::from(rgb))?)?;
            let wait = Instant::now();
            let ready = loop {
                if let Some(ready) = jobs.take_prepared_ready(&key)? {
                    break ready;
                }
                if wait.elapsed() >= Duration::from_secs(60) {
                    log::error!("operation=depth_reference request={request} asset={} phase=wait elapsed_ms={} cause=request did not complete within60seconds", key.source.asset, wait.elapsed().as_millis());
                    return Err("reference request did not complete within60seconds".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let measurement = ready
                .measurement()
                .ok_or("reference worker omitted measurement")?;
            let estimate = ready.estimate();
            if measurement.request() != request
                || measurement.execution().provider != estimate.provider
                || measurement.total() != estimate.costs.total
                || measurement.total() < measurement.resolve()
                || ready.input().rgb_sha256 != decode_digest(&row.rgb_input.sha256)?
                || estimate.resource.values().len() != 640 * 480
            {
                return Err(
                    "reference worker changed source, extent or measurement identity".into(),
                );
            }
            profile.record(&ready)?;
            let writing = Instant::now();
            let values: Vec<u8> = estimate
                .resource
                .values()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            let filename = format!("{:02}.relative.f32le", row.index);
            use std::io::Write;
            fs::File::create_new(root.join(&filename))?.write_all(&values)?;
            let output_sha256 = hex(&Sha256::digest(&values));
            let caller_output = writing.elapsed();
            let c = &estimate.costs;
            rows.push(serde_json::json!({
                "index":row.index, "split":row.split, "key":key, "request":request,
                "input":ready.input(), "execution":measurement.execution(),
                "worker_total_ns":measurement.total().as_nanos(), "resolve_ns":measurement.resolve().as_nanos(),
                "retained_gpu_buffer_capacity_bytes":measurement.retained_gpu_buffer_capacity(),
                "caller_rgb_load_ns":caller_rgb_load.as_nanos(), "caller_output_ns":caller_output.as_nanos(),
                "source_prepare_ns":c.source_prepare.as_nanos(), "source_rgb_bytes":c.source_rgb_bytes,
                "load_ns":c.load.as_nanos(), "preprocess_ns":c.preprocess.as_nanos(), "inference_ns":c.inference.as_nanos(),
                "postprocess_ns":c.postprocess.as_nanos(), "host_transport_ns":c.host_transport.as_nanos(),
                "input_copy_ns":c.input_copy.as_nanos(), "output_copy_ns":c.output_copy.as_nanos(),
                "input_tensor_bytes":c.input_bytes, "output_tensor_bytes":c.output_bytes,
                "output":{"filename":filename,"bytes":values.len(),"sha256":output_sha256}
            }));
            write_report(root, "running", &rows, None, None)?;
        }
        let counts = jobs.counts()?;
        if counts.submitted != 32
            || counts.completed != 32
            || counts.failed != 0
            || counts.canceled != 0
            || counts.cache_hits != 0
            || counts.cache_misses != 0
            || counts.restored != 0
            || jobs.in_flight()?.is_some()
            || jobs.queue_depth()? != 0
        {
            return Err("reference cohort reused, discarded or retained work".into());
        }
        let summary = profile.summary()?;
        Ok(
            serde_json::json!({ "samples":summary.samples(), "model":summary.model(),
            "execution":summary.execution(), "fastest_ns":summary.fastest().as_nanos(),
            "median_ns":summary.median().as_nanos(), "slowest_ns":summary.slowest().as_nanos(),
            "submitted":counts.submitted,"completed":counts.completed,"failed":counts.failed,
            "canceled":counts.canceled,"cache_hits":counts.cache_hits,"cache_misses":counts.cache_misses,"restored":counts.restored }),
        )
    })();
    match result {
        Ok(summary) => write_report(root, "completed", &rows, Some(&summary), None),
        Err(primary) => {
            let cause: String = primary.to_string().chars().take(2048).collect();
            if let Err(secondary) = write_report(root, "failed", &rows, None, Some(&cause)) {
                log::error!("operation=depth_reference phase=failed_report primary={cause} secondary={secondary}");
            }
            Err(primary)
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 7 {
        return Err("usage: depth_reference onnx|wgsl PYTHON MODEL PROGRAM WORKER INPUT_DIRECTORY OUTPUT_DIRECTORY".into());
    }
    let kind = args[0].to_str().ok_or("backend must be ASCII")?;
    let paths: Vec<_> = args[1..].iter().map(PathBuf::from).collect();
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err("absolute reference paths required".into());
    }
    let (python, model_path, program, worker, input_root, root) = (
        &paths[0], &paths[1], &paths[2], &paths[3], &paths[4], &paths[5],
    );
    fs::create_dir(root)?;
    let _guard = logging::init(root.join("logs"))?;
    log::info!("start operation=depth_reference phase=source_plan");
    log::logger().flush();
    let manifest: serde_json::Value = serde_json::from_slice(&bounded_read(
        &model_path.with_file_name("model.json"),
        64 << 10,
    )?)?;
    let field = |name: &str| -> Result<&str> {
        Ok(manifest[name]
            .as_str()
            .ok_or("model manifest field missing")?)
    };
    let model = DepthModel {
        id: field("id")?.into(),
        revision: field("revision")?.into(),
        sha256: decode_digest(field("sha256")?)?,
        processing: field("processing")?.into(),
    };
    model.validate()?;
    match kind {
        "onnx" => samples(
            depth::onnx::Onnx::new(depth::onnx::Config {
                python: python.clone(),
                model: model_path.clone(),
                scratch: root.join("scratch"),
                provider: depth::onnx::Provider::Cpu,
            })?,
            model,
            input_root,
            root,
        ),
        "wgsl" => samples(
            depth::wgsl::Wgsl::new(depth::wgsl::Config {
                python: python.clone(),
                worker: worker.clone(),
                program: program.clone(),
                scratch: root.join("scratch"),
                hardware: depth::wgsl::Hardware::Vulkan,
            })?,
            model,
            input_root,
            root,
        ),
        _ => Err("explicit onnx or wgsl backend required".into()),
    }
}
fn main() {
    if let Err(cause) = run() {
        let cause: String = cause.to_string().chars().take(2048).collect();
        if log::log_enabled!(log::Level::Error) {
            log::error!("operation=depth_reference phase=driver cause={cause}");
        } else {
            eprintln!("depth reference probe failed: {cause}");
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cohort() -> Vec<Spec> {
        (0..32)
            .map(|index| Spec {
                index,
                rgb_ns: (index as u64 + 1) * 1_000_000_000,
                split: if index < 8 {
                    "calibration"
                } else {
                    "evaluation"
                }
                .into(),
                rgb_input: Rgb {
                    filename: format!("{index:02}.rgb"),
                    bytes: RGB_BYTES,
                    sha256: "0c".repeat(32),
                },
            })
            .collect()
    }

    #[test]
    fn cohort_rejects_changed_role_order_extent_path_or_digest() {
        validate_plan(&cohort()).unwrap();
        for field in 0..9 {
            let mut plan = cohort();
            match field {
                0 => plan[7].split = "evaluation".into(),
                1 => plan[8].split = "calibration".into(),
                2 => plan[20].rgb_ns = plan[19].rgb_ns,
                3 => plan[5].rgb_input.filename = "../05.rgb".into(),
                4 => plan[2].rgb_input.sha256 = "g".repeat(64),
                5 => plan[31].rgb_ns = u64::MAX,
                6 => plan[2].rgb_input.bytes -= 1,
                7 => plan[3].index = 4,
                _ => {
                    plan.pop();
                }
            }
            assert!(validate_plan(&plan).is_err());
        }
    }

    #[test]
    fn source_byte_replacement_at_the_same_path_cannot_enter_the_cohort() {
        let root = tempfile::tempdir().unwrap();
        let mut rgb = vec![127u8; RGB_BYTES];
        let mut row = cohort().remove(0);
        row.rgb_input.sha256 = hex(&Sha256::digest(&rgb));
        let path = root.path().join(&row.rgb_input.filename);
        fs::write(&path, &rgb).unwrap();
        assert_eq!(read_rgb(root.path(), &row).unwrap(), rgb);
        rgb[0] = 153;
        fs::write(&path, rgb).unwrap();
        assert!(read_rgb(root.path(), &row)
            .unwrap_err()
            .to_string()
            .contains("differs from frozen preparation identity"));
    }
}
