#![cfg(feature = "ai-depth")]
//! Explicit real-hardware control, never an implicit model/runtime download.
use motion_host::depth::{
    Input, Jobs, Key,
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
        let state = receipt(&root.join("service/receipt.json")).unwrap();
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
