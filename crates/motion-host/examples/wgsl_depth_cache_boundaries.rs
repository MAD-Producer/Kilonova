//! Explicit real-model acceptance driver; not an ordinary model-free CI test.
//! Uses only owned resource/worker copies when checking corruption or identity.
//! Arguments: <owned-output-root> <python> <model.onnx> <copied-program.json>
//! <native-worker> <cold-proof-root> <copied-weight-file>.
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
        let identity = self.backend.execution_identity(input, cancel);
        if let Some((entered, release)) = self.blocked.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(60)).unwrap();
        }
        identity
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

    use depth::{
        alignment::Limits,
        transition::{Activity, Choices, Controller, Policy},
    };
    let proof = PathBuf::from(std::env::args_os().nth(6).unwrap());
    let resource = PathBuf::from(std::env::args_os().nth(7).unwrap());
    let owned_root = root.canonicalize().unwrap();
    assert!(program.canonicalize().unwrap().starts_with(&owned_root));
    assert!(resource.canonicalize().unwrap().starts_with(&owned_root));
    let config = depth::cache::Config {
        directory: root.join("cache"),
        byte_limit: 16 << 20,
        entry_limit: 8,
    };
    let mut jobs = Jobs::start_cached(create(None), config.clone()).unwrap();
    let mut controller = Controller::new(Policy {
        idle_delay: Duration::from_millis(100),
        minimum_interactive_residence: Duration::from_millis(300),
    })
    .unwrap();
    let choices = Choices {
        interactive: input.key().clone(),
        settled: input.key().clone(),
    };
    let first = controller
        .observe(choices.clone(), Activity::Idle, Duration::ZERO)
        .unwrap()
        .unwrap();
    controller
        .request(&first, &mut jobs, input.clone())
        .unwrap()
        .unwrap();
    wait(&jobs, 1);
    let reference = controller
        .take_ready(&first, &mut jobs, first.key())
        .unwrap()
        .unwrap();
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let raw = std::fs::read(proof.join("execution/actual-wgsl-depth.f32")).unwrap();
    assert_eq!(
        raw,
        reference
            .estimate()
            .resource
            .values()
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    );
    let ticket = controller
        .observe(choices, Activity::Idle, Duration::from_millis(300))
        .unwrap()
        .unwrap();
    let limits = Limits {
        minimum_fit_pairs: 3,
        minimum_validation_pairs: 3,
        minimum_pair_fraction: 1.,
        minimum_confidence: None,
        minimum_reference_stddev: 1e-8,
        scale_bounds: [0.1, 10.],
        maximum_normalized_rms: 1e-8,
        maximum_normalized_error: 1e-8,
    };
    controller
        .request_aligned(&ticket, &mut jobs, input.clone(), &reference, limits)
        .unwrap()
        .unwrap();
    wait(&jobs, 2);
    let admitted = controller
        .take_aligned_ready(&ticket, &mut jobs, ticket.key())
        .unwrap()
        .unwrap();
    let stats = admitted.statistics();
    {
        let view = admitted.view(&controller, ticket.key()).unwrap().unwrap();
        assert_eq!(view.costs().inference, Duration::ZERO);
        assert!(view.costs().alignment > Duration::ZERO);
        for i in 0..96 {
            assert!(
                (view.sample(i).unwrap().unwrap()
                    - reference.estimate().resource.sample(i).unwrap())
                .abs()
                    <= 1e-8
            );
        }
    }
    assert_eq!(jobs.counts().unwrap().cache_hits, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    controller.invalidate(Duration::from_millis(301)).unwrap();
    assert!(admitted.view(&controller, ticket.key()).unwrap().is_none());
    let original = std::fs::read(&resource).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    std::fs::write(&resource, changed).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 3);
    let cause = jobs.take_ready(input.key()).unwrap_err().to_string();
    assert!(
        cause.contains("length, digest or finite-value check failed"),
        "{cause}"
    );
    assert_eq!(jobs.counts().unwrap().failed, 1);
    assert_eq!(jobs.counts().unwrap().cache_hits, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    std::fs::write(&resource, original).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 4);
    let recovered = jobs.take_ready(input.key()).unwrap().unwrap();
    assert_eq!(recovered.costs.inference, Duration::ZERO);
    assert_eq!(jobs.counts().unwrap().cache_hits, 3);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(jobs);
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut canceled = Jobs::start_cached(create(Some((entered, blocked))), config).unwrap();
    canceled.request(input.clone()).unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap();
    assert_eq!(
        canceled.in_flight().unwrap().unwrap().phase,
        "cache_resolve"
    );
    canceled.cancel().unwrap();
    release.send(()).unwrap();
    wait(&canceled, 1);
    assert_eq!(canceled.counts().unwrap().canceled, 1);
    assert_eq!(canceled.counts().unwrap().failed, 0);
    assert_eq!(canceled.counts().unwrap().cache_hits, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(canceled.take_ready(input.key()).unwrap().is_none());
    assert!(canceled.in_flight().unwrap().is_none());
    assert_eq!(canceled.queue_depth().unwrap(), 0);
    drop(canceled);
    assert_eq!(std::fs::read_dir(root.join("scratch")).unwrap().count(), 0);
    let mut cpu = depth::onnx::Onnx::new(depth::onnx::Config {
        python,
        model,
        scratch: root.join("cpu-scratch"),
        provider: depth::onnx::Provider::Cpu,
    })
    .unwrap();
    let reference_cpu = cpu.infer(&input, &Cancellation::default()).unwrap();
    assert_eq!(reference_cpu.provider, "CPUExecutionProvider");
    let expected = reference_cpu.resource.values();
    let actual = reference.estimate().resource.values();
    assert_eq!(expected.len(), 96);
    assert_eq!(actual.len(), 96);
    let scale = expected
        .iter()
        .map(|v| f64::from(*v).abs())
        .fold(1e-6_f64, f64::max);
    let errors: Vec<f64> = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs() / scale)
        .collect();
    let max_error = errors.iter().copied().fold(0_f64, f64::max);
    let mean_error = errors.iter().sum::<f64>() / 96.;
    assert!(
        max_error <= 0.0002 && mean_error <= 0.00002,
        "max={max_error} mean={mean_error}"
    );
    std::fs::write(
        root.join("cpu-reference.f32"),
        expected
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(
        file.contains("stage=cache_resolve")
            && file.contains("length, digest or finite-value check failed")
            && file.contains(" INFO ")
    );
    println!(
        "{}",
        serde_json::json!({"status":"actual_cached_wgsl_alignment_generation_resource_refusal_recovery_and_cancel_passed","inferenceCalls":calls.load(Ordering::SeqCst),"cacheHits":3,"pixels":96,"alignmentValidationPairs":stats.validation_pairs,"normalizedMaximum":max_error,"normalizedMean":mean_error,"normalizationScale":scale,"originalMaximumBound":0.0002,"originalMeanBound":0.00002,"cancelScope":"after real hardware/resource/runtime resolution before cache lookup; controlled backend barrier; not live native-child cancellation"})
    );
}
