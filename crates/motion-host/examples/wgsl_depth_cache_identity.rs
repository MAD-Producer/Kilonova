//! Explicit real-model acceptance driver; not an ordinary model-free CI test.
//! Uses only owned resource/worker copies when checking corruption or identity.
//! Arguments: <owned-output-root> <python> <model.onnx> <program.json> <copied-worker>.
//! The worker must be an independently owned copy inside the output root.
use motion_host::depth::{self, Backend, Cancellation, Estimate, Input, Jobs, Key};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
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
    mutate_worker: Option<PathBuf>,
}
impl Backend for Actual {
    fn infer(&mut self, input: &Input, cancel: &Cancellation) -> depth::Result<Estimate> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(path) = self.mutate_worker.take() {
            use std::io::Write;
            let mut owned = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            owned
                .write_all(b"owned cache execution identity mutation fixture")
                .unwrap();
        }
        self.backend.infer(input, cancel)
    }
    fn execution_identity(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> depth::Result<Option<depth::cache::Execution>> {
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
    assert!(worker
        .canonicalize()
        .unwrap()
        .starts_with(root.canonicalize().unwrap()));
    let calls = Arc::new(AtomicUsize::new(0));
    let create = |mutate_worker| Actual {
        backend: depth::wgsl::Wgsl::new(depth::wgsl::Config {
            python: python.clone(),
            worker: worker.clone(),
            program: program.clone(),
            scratch: root.join("scratch"),
            hardware: depth::wgsl::Hardware::Vulkan,
        })
        .unwrap(),
        calls: calls.clone(),
        mutate_worker,
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
    // Only the independently owned worker copy supplied by the controller is changed.
    let original = std::fs::read(&worker).unwrap();
    let mut jobs = Jobs::start_cached(create(Some(worker.clone())), config.clone()).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 1);
    let cause = jobs.take_ready(input.key()).unwrap_err().to_string();
    assert!(
        cause.contains("WGSL worker changed since cache resolution"),
        "{cause}"
    );
    assert_eq!(jobs.counts().unwrap().failed, 1);
    assert_eq!(jobs.counts().unwrap().cache_misses, 1);
    assert_eq!(jobs.counts().unwrap().cache_hits, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(std::fs::read(&worker).unwrap().len() > original.len());
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
    assert_eq!(std::fs::read_dir(root.join("scratch")).unwrap().count(), 0);
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        file.contains(" INFO ")
            && file.contains("phase=verify_worker_identity start")
            && file.contains(" ERROR ")
            && file.contains(&cause)
    );
    assert!(!file.contains("phase=native_inference start"));
    std::fs::write(&worker, &original).unwrap();
    // Resolution after recovery must attest the restored executable rather than a retained identity.
    let mut recovered = create(None);
    let execution = recovered
        .execution_identity(&input, &Cancellation::default())
        .unwrap()
        .unwrap();
    assert!(execution.provider.starts_with("WGSL:Vulkan:"));
    assert_eq!(std::fs::read(&worker).unwrap(), original);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    println!(
        "{}",
        serde_json::json!({"status":"actual_product_worker_change_between_resolution_and_inference_refused_then_recovered","backendInferEntries":calls.load(Ordering::SeqCst),"cacheHits":0,"encodedModelInferenceStarted":false,"provider":execution.provider,"scope":"owned byte-for-byte executable copy changed after actual hardware/resource resolution; rejected before native inference child; restored bytes independently resolved again"})
    );
}
