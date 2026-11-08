//! Explicit real-model cache acceptance driver; ordinary CI needs no model.
//! Run with absolute paths: <new-output-root> <python> <model.onnx> <program.json> <native-worker>.
//! Builds in the real workspace graph; no alternate backend or assertion budget.
use motion_host::depth::{self, Backend, Cancellation, Estimate, Input, Jobs, Key};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
#[path = "../../../desktop/app/src/logging/sink.rs"]
mod logging;
mod engine {
    pub fn current_command() -> Option<(u64, &'static str, std::time::Duration)> {
        None
    }
}
struct Actual {
    backend: depth::wgsl::Wgsl,
    calls: Arc<AtomicUsize>,
    blocked: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl Backend for Actual {
    fn infer(&mut self, input: &Input, cancel: &Cancellation) -> depth::Result<Estimate> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.backend.infer(input, cancel)
    }
    fn execution_identity(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> depth::Result<Option<depth::cache::Execution>> {
        if let Some((entered, release)) = self.blocked.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(60)).unwrap();
        }
        self.backend.execution_identity(input, cancel)
    }
}
fn wait(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let c = jobs.counts().unwrap();
        if c.completed + c.failed + c.canceled >= count {
            return;
        }
        assert!(
            Instant::now() < until,
            "actual WGSL cache worker did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn main() {
    assert!(std::env::var_os("MOTION_LOG").is_none());
    let root = PathBuf::from(std::env::args_os().nth(1).unwrap());
    assert!(
        root.is_absolute() && !root.exists(),
        "new absolute output root required"
    );
    let python = PathBuf::from(std::env::args_os().nth(2).unwrap());
    let model = PathBuf::from(std::env::args_os().nth(3).unwrap());
    let program = PathBuf::from(std::env::args_os().nth(4).unwrap());
    let worker = PathBuf::from(std::env::args_os().nth(5).unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(model.with_file_name("model.json")).unwrap())
            .unwrap();
    let digest = manifest["sha256"].as_str().unwrap();
    assert_eq!(digest, depth::wgsl::MODEL_SHA256);
    let mut hash = [0; 32];
    for i in 0..32 {
        hash[i] = u8::from_str_radix(&digest[i * 2..i * 2 + 2], 16).unwrap();
    }
    let _guard = logging::init(root.join("logs")).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let create = |blocked| Actual {
        backend: depth::wgsl::Wgsl::new(depth::wgsl::Config {
            python: python.clone(),
            worker: worker.clone(),
            program: program.clone(),
            scratch: root.join("scratch"),
            hardware: depth::wgsl::Hardware::Vulkan,
        })
        .unwrap(),
        calls: calls.clone(),
        blocked,
    };
    let input = Input::new(
        Key {
            source: DepthSource {
                asset: 7,
                revision: 1,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: [12, 8],
                crop: [0, 0, 12, 8],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: manifest["id"].as_str().unwrap().into(),
                revision: manifest["revision"].as_str().unwrap().into(),
                sha256: hash,
                processing: manifest["processing"].as_str().unwrap().into(),
            },
        },
        [12, 8],
        Arc::from(
            (0..288)
                .map(|n| ((n * 31 + n / 36) % 256) as u8)
                .collect::<Vec<_>>(),
        ),
    )
    .unwrap();
    let config = depth::cache::Config {
        directory: root.join("cache"),
        byte_limit: 16 << 20,
        entry_limit: 8,
    };
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut jobs = Jobs::start_cached(create(Some((entered, blocked))), config.clone()).unwrap();
    jobs.request(input.clone()).unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap();
    assert_eq!(jobs.in_flight().unwrap().unwrap().phase, "cache_resolve");
    // No fixture flush: start must already be persisted by the production worker.
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        file.contains(" INFO ")
            && file.contains("start #1 depth_cache asset=7 phase=cache_resolve")
    );
    let cold_began = Instant::now();
    release.send(()).unwrap();
    wait(&jobs, 1);
    let cold_wall = cold_began.elapsed();
    let original = jobs.take_ready(input.key()).unwrap().unwrap();
    assert_eq!(jobs.counts().unwrap().cache_misses, 1);
    assert!(original.costs.inference > Duration::ZERO);
    assert!(original.provider.starts_with("WGSL:Vulkan:"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let raw: Vec<u8> = original
        .resource
        .values()
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    std::fs::write(root.join("actual-wgsl-depth.f32"), raw).unwrap();
    std::fs::write(root.join("input.rgb"), input.rgb()).unwrap();
    drop(jobs);
    let hit_began = Instant::now();
    let mut jobs = Jobs::start_cached(create(None), config.clone()).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 1);
    let restored = jobs.take_ready(input.key()).unwrap().unwrap();
    let hit_wall = hit_began.elapsed();
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    assert_eq!(restored.costs.inference, Duration::ZERO);
    assert_eq!(
        original
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        restored
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    assert_ne!(
        original.resource.cache_identity(),
        restored.resource.cache_identity()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let path = std::fs::read_dir(&config.directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|s| s == "depth"))
        .unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let n = bytes.len();
    bytes[n - 33] ^= 1;
    std::fs::write(path, bytes).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 2);
    let cause = jobs.take_ready(input.key()).unwrap_err().to_string();
    assert!(cause.contains("content SHA-256 differs"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        file.contains(" ERROR ")
            && file.contains("stage=cache_read")
            && file.contains("content SHA-256 differs")
    );
    println!(
        "{}",
        serde_json::json!({"status":"actual_wgsl_cache_reopened_without_inference_and_default_error_channels_passed","provider":original.provider,"inferenceCalls":calls.load(Ordering::SeqCst),"firstInferenceMs":original.costs.inference.as_secs_f64()*1000.,"pixels":restored.resource.values().len(),"coldFullPathMs":cold_wall.as_secs_f64()*1000.,"newWorkerHitFullPathMs":hit_wall.as_secs_f64()*1000.,"measurementScope":"single synthetic input; cold excludes controlled publication pause; hit includes new worker and actual hardware/resource/runtime resolution; not sustained or quality qualification"})
    );
}
