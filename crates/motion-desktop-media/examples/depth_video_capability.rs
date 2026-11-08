//! Explicit sequential original-video sampling, using actual libav and Jobs.
//! This preserves measured source preparation; it does not certify video quality.
use motion_host::{
    depth::{self, Backend, Jobs, VideoInput, VideoKey},
    Platform,
};
use motion_model::depth::{DepthGeometry, DepthModel};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[path = "../../../desktop/app/src/logging/sink.rs"]
mod logging;
mod engine {
    pub fn current_command() -> Option<(u64, &'static str, std::time::Duration)> {
        None
    }
}

fn read(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("bounded video probe metadata exceeded".into());
    }
    Ok(bytes)
}
fn hash(path: &Path) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut file = File::open(path)?.take(motion_core::storage::MAX_MEDIA_ASSET + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 << 10];
    let mut total = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > motion_core::storage::MAX_MEDIA_ASSET {
            return Err("video hashing exceeds source byte bound".into());
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

struct Video {
    root: PathBuf,
    asset: motion_core::VideoAsset,
    targets: Vec<u64>,
    model: DepthModel,
}
fn measure(
    backend: impl Backend,
    video: Video,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let platform = motion_desktop_media::platform();
    let geometry = DepthGeometry {
        source_size: [video.asset.display_width, video.asset.display_height],
        crop: [0, 0, video.asset.display_width, video.asset.display_height],
        quarter_turns: 0,
        mirror_x: false,
    };
    let mut jobs = Jobs::start_measured(backend)?;
    let mut profile = depth::capability::Profile::new(video.targets.len())?;
    let mut rows = Vec::with_capacity(video.targets.len());
    let mut written = 0u64;
    for (index, &target) in video.targets.iter().enumerate() {
        let key = VideoKey {
            object: 75,
            asset: video.asset.id,
            revision: 1,
            target_us: target,
            geometry: geometry.clone(),
            model: video.model.clone(),
        };
        let request = jobs.request_video_sample(VideoInput {
            key: key.clone(),
            root: video.root.clone(),
            asset: video.asset.clone(),
            platform: platform.clone(),
            budget: 128 << 20,
        })?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let ready = loop {
            if let Some(ready) = jobs.take_video_sample(&key)? {
                break ready;
            }
            if Instant::now() >= deadline {
                return Err(format!("video probe request={request} target_us={target} did not complete within 60 seconds").into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let descriptor = ready.estimate().resource.descriptor();
        let measurement = ready
            .measurement()
            .ok_or("uncached video measurement missing")?;
        let end = ready
            .video_end_us()
            .ok_or("verified source interval missing")?;
        if descriptor.source.pts.timescale != 1_000_000
            || descriptor.source.pts.ticks != target as i64
            || end <= target
            || measurement.request() != request
            || measurement.execution().provider != ready.estimate().provider
        {
            return Err(
                "video probe differs from actual managed-index source interval or execution".into(),
            );
        }
        profile.record(&ready)?;
        let values = ready.estimate().resource.values();
        written = written
            .checked_add(values.len() as u64 * 4)
            .ok_or("depth output byte count overflow")?;
        if written > 128 << 20 {
            return Err("video probe depth output exceeds 128 MiB bound".into());
        }
        let name = format!("depth-{index:04}.f32le");
        let mut file = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(video.root.join(&name))?,
        );
        let mut digest = Sha256::new();
        for value in values {
            let bytes = value.to_le_bytes();
            file.write_all(&bytes)?;
            digest.update(bytes);
        }
        file.flush()?;
        let costs = &ready.estimate().costs;
        rows.push(serde_json::json!({"request": request, "target_us": target, "end_us": end,
            "descriptor": descriptor, "input": ready.input(), "execution": measurement.execution(),
            "worker_total_ns": measurement.total().as_nanos(), "resolve_ns": measurement.resolve().as_nanos(),
            "source_prepare_ns": costs.source_prepare.as_nanos(), "source_rgb_bytes": costs.source_rgb_bytes,
            "load_ns": costs.load.as_nanos(), "preprocess_ns": costs.preprocess.as_nanos(),
            "inference_ns": costs.inference.as_nanos(), "host_transport_ns": costs.host_transport.as_nanos(),
            "input_copy_ns": costs.input_copy.as_nanos(), "output_copy_ns": costs.output_copy.as_nanos(),
            "postprocess_ns": costs.postprocess.as_nanos(), "input_tensor_bytes": costs.input_bytes,
            "output_tensor_bytes": costs.output_bytes, "retained_gpu_buffer_capacity_bytes": measurement.retained_gpu_buffer_capacity(),
            "depth_file": name, "depth_values": values.len(), "depth_sha256": <[u8;32]>::from(digest.finalize())}));
    }
    let counts = jobs.counts()?;
    if counts.completed != video.targets.len() as u64
        || counts.failed != 0
        || counts.canceled != 0
        || counts.cache_hits != 0
        || counts.restored != 0
        || jobs.in_flight()?.is_some()
        || jobs.queue_depth()? != 0
    {
        return Err("video probe reused, failed, canceled or retained work".into());
    }
    let summary = profile.summary()?;
    Ok(serde_json::json!({"samples": rows, "asset": video.asset,
        "profile": {"samples": summary.samples(), "fastest_ns": summary.fastest().as_nanos(),
            "median_ns": summary.median().as_nanos(), "slowest_ns": summary.slowest().as_nanos(),
            "retained_gpu_buffer_capacity_bytes": summary.retained_gpu_buffer_capacity()},
        "cache_hits": counts.cache_hits, "restored": counts.restored, "depth_output_bytes": written}))
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 9 {
        return Err("usage: depth_video_capability <new-output-root> <onnx|wgsl> <python> <model.onnx> <program.json|-> <native-worker|-> <source-video> <start-index> <count:1..64>".into());
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
    let original = PathBuf::from(&args[6]);
    if [&python, &model, &original]
        .iter()
        .any(|p| !p.is_absolute())
    {
        return Err("absolute source and runtime paths required".into());
    }
    let start: usize = args[7]
        .to_str()
        .ok_or("start index must be UTF-8")?
        .parse()?;
    let count: usize = args[8].to_str().ok_or("count must be UTF-8")?.parse()?;
    if !(1..=64).contains(&count) {
        return Err("video probe count outside 1..64".into());
    }
    log::info!("start depth_video_capability phase=source_registration");
    log::logger().flush();
    let registration = Instant::now();
    let bytes = original.metadata()?.len();
    if bytes == 0 || bytes > motion_core::storage::MAX_MEDIA_ASSET {
        return Err("source video exceeds owned media byte limit".into());
    }
    let original_sha = hash(&original)?;
    let assets = root.join("assets");
    fs::create_dir(&assets)?;
    let source = assets.join("source.mp4");
    if fs::copy(&original, &source)? != bytes || hash(&source)? != original_sha {
        return Err("copied original video differs from source bytes".into());
    }
    log::info!("start depth_video_capability phase=source_probe");
    log::logger().flush();
    let mut probe =
        motion_desktop_media::platform().probe_video(&source, None, Some(u32::MAX), &|| Ok(()))?;
    probe.asset.id = 75;
    probe.asset.path = "assets/source.mp4".into();
    probe.asset.validate()?;
    let end = start
        .checked_add(count)
        .ok_or("video probe index overflow")?;
    let targets = probe
        .timestamps
        .get(start..end)
        .ok_or("video probe exceeds original indexed frame count")?
        .to_vec();
    let index = motion_media::video_cache_path(&root, &probe.asset)?;
    fs::create_dir_all(index.parent().ok_or("managed index parent missing")?)?;
    motion_media::save_index(&index, &probe.asset, &probe.timestamps)?;
    let asset = probe.asset.clone();
    drop(probe);
    let source_registration_ns = registration.elapsed().as_nanos();
    let metadata: serde_json::Value =
        serde_json::from_slice(&read(&model.with_file_name("model.json"), 64 << 10)?)?;
    let hex = metadata["sha256"].as_str().ok_or("model digest missing")?;
    if hex.len() != 64 || !hex.is_ascii() {
        return Err("model digest must be 64 ASCII hex characters".into());
    }
    let mut digest = [0; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    let field = |name: &str| -> Result<String, Box<dyn std::error::Error>> {
        Ok(metadata[name]
            .as_str()
            .ok_or("model identity missing")?
            .into())
    };
    let video = Video {
        root: root.clone(),
        asset,
        targets,
        model: DepthModel {
            id: field("id")?,
            revision: field("revision")?,
            sha256: digest,
            processing: field("processing")?,
        },
    };
    let measured = match kind {
        "onnx" => measure(
            depth::onnx::Onnx::new(depth::onnx::Config {
                python,
                model,
                scratch: root.join("scratch"),
                provider: depth::onnx::Provider::Cpu,
            })?,
            video,
        )?,
        "wgsl" if program.is_absolute() && worker.is_absolute() => measure(
            depth::wgsl::Wgsl::new(depth::wgsl::Config {
                python,
                program,
                worker,
                scratch: root.join("scratch"),
                hardware: depth::wgsl::Hardware::Vulkan,
            })?,
            video,
        )?,
        _ => {
            return Err(
                "explicit onnx or wgsl with absolute program and worker paths required".into(),
            )
        }
    };
    let report = serde_json::json!({"schema": 1, "source_sha256": original_sha, "source_bytes": bytes,
        "source_registration_ns": source_registration_ns, "start_index": start, "sample_count": count,
        "scope": "consecutive original indexed SDR video frames; worker includes actual decode, transfer conversion, crop, RGB fingerprint, runtime resolution and inference; registration, queue wait, output archival, alignment, renderer, quality, peak memory and thermal stability are separate",
        "measured": measured});
    fs::write(
        root.join("measurements.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
fn main() {
    if let Err(cause) = run() {
        let cause: String = cause.to_string().chars().take(2048).collect();
        if log::log_enabled!(log::Level::Error) {
            log::error!(
                "operation=depth_video_capability phase=driver cause={}",
                cause
            );
        } else {
            eprintln!("depth video capability failed: {cause}");
        }
        std::process::exit(1);
    }
}
