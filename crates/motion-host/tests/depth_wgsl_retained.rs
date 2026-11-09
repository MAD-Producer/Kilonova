#![cfg(feature = "ai-depth")]
//! Explicit real-hardware control, never an implicit model/runtime download.
use motion_host::depth::{
    Backend, Cancellation, Estimate, Input, Jobs, Key,
    wgsl::{Config, Hardware, Wgsl},
};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
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

struct Observed {
    backend: Wgsl,
    calls: Arc<std::sync::atomic::AtomicUsize>,
    identities: Arc<std::sync::Mutex<Vec<motion_host::depth::cache::Execution>>>,
}
impl Backend for Observed {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.backend.infer(input, cancel)
    }
    fn retained_gpu_buffer_capacity(&self, input: &Input) -> Option<u64> {
        self.backend.retained_gpu_buffer_capacity(input)
    }
    fn execution_identity(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Option<motion_host::depth::cache::Execution>> {
        let actual = self.backend.execution_identity(input, cancel)?;
        if let Some(value) = &actual {
            let mut identities = self.identities.lock().unwrap();
            assert!(
                identities.len() < 8,
                "bounded explicit control observations"
            );
            identities.push(value.clone());
        }
        Ok(actual)
    }
}

#[test]
#[ignore = "requires explicit installed model, pinned GPU worker, Python and real RGB fixture"]
fn actual_retained_cache_reopens_without_inference_and_isolates_cold_execution() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let path = std::env::var_os("MOTION_DEPTH_RETAINED_TEST_FIXTURE")
        .expect("explicit real fixture required");
    let fixture: Fixture = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let out = fixture.evidence.parent().unwrap();
    let _logger = logging::init(out.join("logs")).unwrap();
    assert!(fixture.scratch.is_absolute() && !fixture.scratch.exists());
    fs::create_dir(&fixture.scratch).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let identities = Arc::new(Mutex::new(Vec::new()));
    let create = |retained| {
        let config = Config {
            python: fixture.python.clone(),
            worker: fixture.worker.clone(),
            program: fixture.program.clone(),
            scratch: fixture.scratch.clone(),
            hardware: Hardware::Vulkan,
        };
        Observed {
            backend: if retained {
                Wgsl::new_retained(config)
            } else {
                Wgsl::new(config)
            }
            .unwrap(),
            calls: calls.clone(),
            identities: identities.clone(),
        }
    };
    let cache = motion_host::depth::cache::Config {
        directory: out.join("cache"),
        byte_limit: 16 << 20,
        entry_limit: 8,
    };
    let sample = input(&fixture, 0, 0);
    let check = |result: &Estimate| {
        let bytes: Vec<_> = result
            .resource
            .values()
            .iter()
            .flat_map(|n| n.to_le_bytes())
            .collect();
        assert_eq!(sha(&bytes), fixture.expected_output_sha256[0]);
        assert!(result.resource.values().iter().all(|n| n.is_finite()));
    };
    let mut jobs = Jobs::start_cached(create(true), cache.clone()).unwrap();
    jobs.request(sample.clone()).unwrap();
    let original = wait(|| jobs.take_ready(sample.key()).unwrap());
    check(&original);
    assert_eq!(jobs.counts().unwrap().cache_misses, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let first_root = service_root(&fixture.scratch).unwrap();
    jobs.request(sample.clone()).unwrap();
    let hit = wait(|| jobs.take_ready(sample.key()).unwrap());
    check(&hit);
    assert_eq!(hit.costs.inference, Duration::ZERO);
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let first_state = wait(|| receipt(&first_root.join("service/receipt.json")));
    assert_eq!(first_state["completed_inputs"], 1);
    let retained_cache_path = fs::read_dir(&cache.directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|s| s == "depth"))
        .unwrap();
    drop(jobs);
    assert!(!first_root.exists());

    let mut jobs = Jobs::start_cached(create(true), cache.clone()).unwrap();
    jobs.request(sample.clone()).unwrap();
    let reopened = wait(|| jobs.take_ready(sample.key()).unwrap());
    check(&reopened);
    assert_eq!(reopened.costs.inference, Duration::ZERO);
    assert_ne!(
        original.resource.cache_identity(),
        reopened.resource.cache_identity()
    );
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    assert_eq!(jobs.counts().unwrap().cache_misses, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let reopened_root = service_root(&fixture.scratch).unwrap();
    assert_ne!(reopened_root, first_root);
    let reopened_state = wait(|| receipt(&reopened_root.join("service/receipt.json")));
    assert_eq!(reopened_state["context_preparations"], 1);
    assert_eq!(reopened_state["completed_inputs"], 0);
    drop(jobs);
    assert!(!reopened_root.exists());

    let mut cold = Jobs::start_cached(create(false), cache.clone()).unwrap();
    cold.request(sample.clone()).unwrap();
    let cold_result = wait(|| cold.take_ready(sample.key()).unwrap());
    check(&cold_result);
    assert_eq!(cold.counts().unwrap().cache_hits, 0);
    assert_eq!(cold.counts().unwrap().cache_misses, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(service_root(&fixture.scratch).is_none());
    drop(cold);
    let entries = fs::read_dir(&cache.directory)
        .unwrap()
        .filter_map(|e| {
            let path = e.unwrap().path();
            path.extension()
                .is_some_and(|s| s == "depth")
                .then_some(path)
        })
        .count();
    assert_eq!(
        entries, 2,
        "execution modes must produce separate cache entries"
    );

    // Corrupt only this control's retained-mode cache entry. A checksum fault
    // must preserve its cause and must not fall through to inference or retry.
    let mut bytes = fs::read(&retained_cache_path).unwrap();
    let offset = bytes.len() - 33;
    bytes[offset] ^= 1;
    fs::write(&retained_cache_path, bytes).unwrap();
    let mut jobs = Jobs::start_cached(create(true), cache).unwrap();
    jobs.request(sample.clone()).unwrap();
    let cause = wait(|| match jobs.take_ready(sample.key()) {
        Ok(None) => None,
        Err(error) => Some(error.to_string()),
        Ok(Some(_)) => panic!("corrupt cache must not be restored or silently reinferred"),
    });
    assert!(cause.contains("content SHA-256 differs"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let corrupt_root = service_root(&fixture.scratch).unwrap();
    let corrupt_state = wait(|| receipt(&corrupt_root.join("service/receipt.json")));
    assert_eq!(corrupt_state["completed_inputs"], 0);
    drop(jobs);
    assert!(!corrupt_root.exists());
    let actual = identities.lock().unwrap().clone();
    assert_eq!(actual.len(), 5);
    assert_eq!(actual[0], actual[1]);
    assert_eq!(actual[0], actual[2]);
    assert_eq!(actual[0], actual[4]);
    assert_eq!(actual[0].provider, actual[3].provider);
    assert_eq!(actual[0].plan_sha256, actual[3].plan_sha256);
    assert_ne!(actual[0].runtime_sha256, actual[3].runtime_sha256);
    let log = fs::read_to_string(out.join("logs/motion-studio.log")).unwrap();
    assert!(log.contains("stage=cache_read") && log.contains("content SHA-256 differs"));
    fs::write(&fixture.evidence, serde_json::to_vec_pretty(&json!({
        "status":"actual_cache_reopen_mode_isolation_and_corruption_refusal_passed",
        "inference_calls":calls.load(Ordering::SeqCst),"cache_entries":entries,
        "output_sha256":fixture.expected_output_sha256[0],"executions":actual,
        "first_service":first_state,"reopened_service":reopened_state,"corrupt_service":corrupt_state,
        "original_corruption_cause":cause,"all_owned_service_directories_removed":true
    })).unwrap()).unwrap();
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    python: PathBuf,
    worker: PathBuf,
    program: PathBuf,
    scratch: PathBuf,
    rgb: [PathBuf; 2],
    rgb_sha256: [String; 2],
    expected_output_sha256: [String; 2],
    evidence: PathBuf,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn input(fixture: &Fixture, index: usize, tick: u64) -> Input {
    let rgb = fs::read(&fixture.rgb[index]).unwrap();
    assert_eq!(sha(&rgb), fixture.rgb_sha256[index]);
    let mut model = [0; 32];
    for (n, b) in model.iter_mut().enumerate() {
        *b = u8::from_str_radix(
            &motion_host::depth::wgsl::MODEL_SHA256[n * 2..n * 2 + 2],
            16,
        )
        .unwrap();
    }
    Input::new(
        Key {
            source: DepthSource {
                asset: 75,
                revision: 1,
                pts: DepthTime {
                    ticks: tick as i64,
                    timescale: 30,
                },
            },
            geometry: DepthGeometry {
                source_size: [640, 480],
                crop: [0, 0, 640, 480],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "dav2-small".into(),
                revision: "pinned".into(),
                sha256: model,
                processing: motion_host::depth::onnx::PROCESSING.into(),
            },
        },
        [640, 480],
        Arc::from(rgb),
    )
    .unwrap()
}
fn wait<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    let began = Instant::now();
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "real backend control deadline exceeded"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn receipt(path: &Path) -> Option<Value> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
fn service_root(scratch: &Path) -> Option<PathBuf> {
    let mut roots = fs::read_dir(scratch).ok()?.filter_map(|entry| {
        let path = entry.ok()?.path();
        path.join("service/receipt.json").is_file().then_some(path)
    });
    let root = roots.next()?;
    assert!(
        roots.next().is_none(),
        "a retained backend must own exactly one service"
    );
    Some(root)
}

#[test]
#[ignore = "requires explicit installed model, pinned GPU worker, Python and real RGB fixture"]
fn actual_jobs_reuse_one_gpu_context_and_reap_it_on_cancel_without_restart() {
    let path = std::env::var_os("MOTION_DEPTH_RETAINED_TEST_FIXTURE")
        .expect("explicit real fixture required");
    let fixture: Fixture = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let _logger = logging::init(fixture.evidence.parent().unwrap().join("logs")).unwrap();
    assert!(fixture.scratch.is_absolute() && !fixture.scratch.exists());
    fs::create_dir(&fixture.scratch).unwrap();
    let backend = Wgsl::new_retained(Config {
        python: fixture.python.clone(),
        worker: fixture.worker.clone(),
        program: fixture.program.clone(),
        scratch: fixture.scratch.clone(),
        hardware: Hardware::Vulkan,
    })
    .unwrap();
    let mut jobs = Jobs::start_measured(backend).unwrap();
    let mut outputs = Vec::new();
    let mut pid = None;
    let mut execution = None;
    for (ordinal, index) in [0, 1, 0].into_iter().enumerate() {
        let input = input(&fixture, index, ordinal as u64);
        let key = input.key().clone();
        jobs.request(input).unwrap();
        let result = wait(|| jobs.take_prepared_ready(&key).unwrap());
        let bytes: Vec<_> = result
            .estimate()
            .resource
            .values()
            .iter()
            .flat_map(|n| n.to_le_bytes())
            .collect();
        assert_eq!(sha(&bytes), fixture.expected_output_sha256[index]);
        assert!(
            result
                .estimate()
                .resource
                .values()
                .iter()
                .all(|n| n.is_finite())
        );
        let measurement = result.measurement().unwrap();
        assert_eq!(measurement.retained_gpu_buffer_capacity(), Some(227968156));
        assert!(
            measurement
                .process_memory()
                .unwrap()
                .observed_resident_sum()
                .is_none()
        );
        if let Some(prior) = &execution {
            assert_eq!(prior, measurement.execution());
        }
        execution = Some(measurement.execution().clone());
        let root = service_root(&fixture.scratch).unwrap();
        let state = wait(|| receipt(&root.join("service/receipt.json")));
        let actual_pid = state["pid"].as_u64().unwrap();
        if let Some(prior) = pid {
            assert_eq!(prior, actual_pid);
        }
        pid = Some(actual_pid);
        assert_eq!(state["context_preparations"], 1);
        assert_eq!(state["completed_inputs"], ordinal + 1);
        outputs.push(json!({"index":index,"sha256":sha(&bytes),"service":state,
            "execution":measurement.execution(),"memory":measurement.process_memory()}));
        fs::write(
            &fixture.evidence,
            serde_json::to_vec_pretty(&json!({
                "status":"reuse_complete_cancellation_pending","outputs":outputs,"pid":pid
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(jobs.counts().unwrap().completed, 3);
    let root = service_root(&fixture.scratch).unwrap();
    let canceled = input(&fixture, 1, 3);
    let canceled_key = canceled.key().clone();
    jobs.request(canceled).unwrap();
    // Observe actual submitted native work before canceling, instead of relying
    // on a sleep or canceling only a pending/not-yet-running request.
    let submitted = wait(|| {
        fs::read_dir(&root)
            .ok()?
            .filter_map(|entry| {
                let path = entry.ok()?.path().join("native/receipt.json");
                let value = receipt(&path)?;
                (value["request"] == 4
                    && value["phase"] == "submit_complete_model_and_wait_final_readback")
                    .then_some(value)
            })
            .next()
    });
    let native_log = root.join("service/logs/motion-studio.log");
    let log = fs::read_to_string(&native_log).unwrap();
    assert!(log.contains("start #4 retained_depth_inference"));
    assert!(log.contains("phase=submit_complete_model_and_wait_final_readback"));
    fs::write(
        fixture
            .evidence
            .parent()
            .unwrap()
            .join("native-service.log"),
        log,
    )
    .unwrap();
    assert!(jobs.in_flight().unwrap().is_some());
    jobs.cancel().unwrap();
    wait(|| (jobs.counts().unwrap().canceled >= 1).then_some(()));
    assert!(jobs.take_prepared_ready(&canceled_key).unwrap().is_none());
    let next = input(&fixture, 0, 4);
    let next_key = next.key().clone();
    jobs.request(next).unwrap();
    let error = wait(|| match jobs.take_prepared_ready(&next_key) {
        Ok(None) => None,
        Err(error) => Some(error),
        Ok(Some(_)) => panic!("canceled backend must not silently restart"),
    });
    assert!(error.to_string().contains("no implicit restart"));
    let counts = jobs.counts().unwrap();
    assert_eq!(counts.completed, 3);
    assert_eq!(counts.failed, 1);
    drop(jobs);
    assert!(
        !root.exists(),
        "owned service scratch must be removed after reap"
    );
    fs::write(
        &fixture.evidence,
        serde_json::to_vec_pretty(&json!({"outputs":outputs,
        "submitted_before_cancel":submitted,"pid":pid,"canceled":counts.canceled,
        "failed":counts.failed,"completed":counts.completed,
        "post_cancel_error":error.to_string(),"service_directory_removed":true}))
        .unwrap(),
    )
    .unwrap();
}
