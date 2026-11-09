use super::*;
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use serde_json::json;
use std::sync::Arc;

fn input() -> Input {
    let mut model = [0; 32];
    for (n, byte) in model.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&MODEL_SHA256[n * 2..n * 2 + 2], 16).unwrap();
    }
    Input::new(
        crate::depth::Key {
            source: DepthSource {
                asset: 7,
                revision: 2,
                pts: DepthTime {
                    ticks: 13,
                    timescale: 30,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 3],
                crop: [0, 0, 2, 3],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "dav2-small".into(),
                revision: "pinned".into(),
                sha256: model,
                processing: crate::depth::onnx::PROCESSING.into(),
            },
        },
        [2, 3],
        Arc::from([17u8; 18]),
    )
    .unwrap()
}

fn attestation() -> Attestation {
    Attestation {
        native_memory: None,
        transform_memory: None,
        native: Execution {
            backend: "wgsl".into(),
            provider: "WGSL:Vulkan:authored GPU".into(),
            precision: "float32".into(),
            runtime_sha256: [1; 32],
            plan_sha256: program_digest().unwrap(),
        },
        transform: Execution {
            backend: "wgsl-transform".into(),
            provider: "NumPy/OpenCV".into(),
            precision: "float32".into(),
            runtime_sha256: [2; 32],
            plan_sha256: transform_plan(),
        },
        worker_sha256: [3; 32],
    }
}

#[test]
fn retained_mode_cannot_reuse_the_cold_process_execution_identity() {
    let observation = attestation();
    let cold = observation.combined().unwrap();
    let retained = observation.combined_retained().unwrap();
    assert_ne!(cold.runtime_sha256, retained.runtime_sha256);
    assert_eq!(cold.plan_sha256, retained.plan_sha256);
    assert_eq!(cold.provider, retained.provider);
    assert_eq!(retained, observation.combined_retained().unwrap());
    let mut changed = observation;
    changed.native.runtime_sha256[0] ^= 1;
    assert_ne!(retained, changed.combined_retained().unwrap());
}

#[test]
fn phase_memory_observations_do_not_change_the_runtime_identity() {
    let mut observed = attestation();
    let execution = observed.combined().unwrap();
    observed.native_memory = Some(crate::process_memory::ProcessPeak::Unavailable {
        pid:75,cause:"original native OS query failure".into(),
    });
    observed.transform_memory = Some(crate::process_memory::ProcessPeak::Observed {
        pid:76,method:crate::process_memory::Method::GetrusageLinux,
        peak_resident_bytes:123456,peak_private_commit_bytes:None,
    });
    assert_eq!(observed.combined().unwrap(),execution);
}

fn resolution() -> serde_json::Value {
    json!({"status":"actual_execution_identity_resolved_without_inference","phase":"completed",
        "model_sha256":MODEL_SHA256,"program_sha256":PROGRAM_SHA256,"request_sha256":"a".repeat(64),
        "encoded_operations":0,"final_readbacks":0,"verified_resources":282,"execution":attestation().native,
        "adapter":{"backend":"Vulkan","name":"authored GPU","device_type":"DiscreteGpu"}})
}

#[test]
fn combined_identity_tracks_native_transform_and_actual_worker_independently() {
    let a = attestation();
    let first = a.combined().unwrap();
    assert_eq!(first, a.combined().unwrap());
    for which in 0..3 {
        let mut changed = a.clone();
        match which {
            0 => changed.native.runtime_sha256[0] ^= 1,
            1 => changed.transform.runtime_sha256[0] ^= 1,
            _ => changed.worker_sha256[0] ^= 1,
        }
        let next = changed.combined().unwrap();
        assert_ne!(first.runtime_sha256, next.runtime_sha256);
        assert_eq!(first.plan_sha256, next.plan_sha256);
    }
}

#[test]
fn combined_identity_refuses_wrong_backend_precision_and_embedded_procedure() {
    for which in 0..7 {
        let mut bad = attestation();
        match which {
            0 => bad.native.backend = "onnx".into(),
            1 => bad.native.precision = "float16".into(),
            2 => bad.native.plan_sha256[0] ^= 1,
            3 => bad.transform.provider = "unrequested transform".into(),
            4 => bad.transform.plan_sha256[0] ^= 1,
            5 => bad.worker_sha256 = [0; 32],
            _ => bad.transform.runtime_sha256 = [0; 32],
        }
        assert!(bad.combined().is_err(), "case {which}");
    }
}

#[test]
fn input_free_native_resolution_refuses_partial_work_other_hardware_and_stale_requests() {
    let valid = resolution();
    let check = |value| {
        serde_json::from_value::<NativeResolution>(value)
            .unwrap()
            .validate(Hardware::Vulkan, &"a".repeat(64))
    };
    check(valid.clone()).unwrap();
    for (field, value) in [
        ("encoded_operations", json!(1)),
        ("final_readbacks", json!(1)),
        ("verified_resources", json!(281)),
        ("request_sha256", json!("b".repeat(64))),
        ("phase", json!("running")),
        ("program_sha256", json!("b".repeat(64))),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert!(check(bad).is_err(), "{field}");
    }
    for (field, value) in [
        ("backend", "Dx12"),
        ("device_type", "Cpu"),
        ("name", "wrong GPU"),
    ] {
        let mut bad = valid.clone();
        bad["adapter"][field] = value.into();
        assert!(check(bad).is_err(), "{field}");
    }
}

#[test]
fn transform_resolution_is_source_bound_and_contains_no_learned_operations() {
    let valid = json!({"schema":1,"stage":"resolve","processing":crate::depth::onnx::PROCESSING,
        "execution":attestation().transform,"encoded_operations":0});
    serde_json::from_value::<TransformResolution>(valid.clone())
        .unwrap()
        .validate()
        .unwrap();
    let mut bad = valid.clone();
    bad["encoded_operations"] = 1.into();
    assert!(serde_json::from_value::<TransformResolution>(bad)
        .unwrap()
        .validate()
        .is_err());
    let mut bad = valid;
    bad["execution"]["plan_sha256"] = json!(vec![3; 32]);
    assert!(serde_json::from_value::<TransformResolution>(bad)
        .unwrap()
        .validate()
        .is_err());
}

#[test]
fn actual_transform_receipt_requires_the_identity_when_attestation_was_requested() {
    let expected = attestation().transform;
    let ordinary = json!({"schema":1,"stage":"prepare","processing":crate::depth::onnx::PROCESSING,
        "width":2,"height":3,"rgb_sha256":"a".repeat(64),"depth_sha256":null,
        "output_sha256":"b".repeat(64),"output_bytes":TENSOR_BYTES,"elapsed_ns":1});
    let bytes = serde_json::to_vec(&ordinary).unwrap();
    Transform::decode(&bytes, None).unwrap();
    assert!(Transform::decode(&bytes, Some(&expected)).is_err());
    let mut attested = ordinary;
    attested["execution"] = json!(expected);
    let bytes = serde_json::to_vec(&attested).unwrap();
    Transform::decode(&bytes, Some(&expected))
        .unwrap()
        .validate(&input(), "prepare", &"a".repeat(64), None)
        .unwrap();
    assert!(Transform::decode(&bytes, None).is_err());
    attested["execution"]["runtime_sha256"] = json!(vec![4; 32]);
    assert!(Transform::decode(&serde_json::to_vec(&attested).unwrap(), Some(&expected)).is_err());
}

#[test]
fn actual_native_receipt_requires_verified_weights_and_the_same_execution() {
    let a = attestation();
    let value = json!({"status":"actual_complete_gpu_output_written_independent_comparison_pending","phase":"completed",
        "model_sha256":MODEL_SHA256,"program_sha256":PROGRAM_SHA256,"request_sha256":"a".repeat(64),
        "encoded_operations":429,"final_readbacks":1,"per_node_cpu_injection":false,"pool_retained_through_completion":true,
        "actual_retained_buffer_capacity_bytes":BUFFER_BYTES,"output_bytes":DEPTH_BYTES,"output_sha256":"b".repeat(64),
        "adapter":{"backend":"Vulkan","name":"authored GPU","device_type":"DiscreteGpu"},
        "verified_resources":282,"execution":a.native});
    let report: Native = serde_json::from_value(value.clone()).unwrap();
    a.validate_native_result(&value, &report).unwrap();
    for field in ["verified_resources", "execution"] {
        let mut bad = value.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(a.validate_native_result(&bad, &report).is_err());
    }
    let mut bad = value;
    bad["execution"]["runtime_sha256"] = json!(vec![8; 32]);
    assert!(a.validate_native_result(&bad, &report).is_err());
}

#[test]
fn canceled_resolution_forgets_an_old_attestation_before_creating_scratch() {
    let root = tempfile::tempdir().unwrap();
    let scratch = root.path().join("absent-scratch");
    let mut backend = Wgsl::new(Config {
        python: root.path().join("no-python"),
        worker: root.path().join("no-worker"),
        program: root.path().join("no-program"),
        scratch: scratch.clone(),
        hardware: Hardware::Vulkan,
    })
    .unwrap();
    backend.attestation = Some(attestation());
    let canceled = Cancellation::default();
    canceled.cancel();
    assert!(matches!(
        backend.execution_identity(&input(), &canceled),
        Err(Error::Canceled)
    ));
    assert!(backend.attestation.is_none());
    assert!(!scratch.exists());
}

#[test]
fn actual_worker_file_change_is_refused_and_cancellation_precedes_file_io() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owned-worker");
    fs::write(&path, b"owned executable fixture").unwrap();
    let cancellation = Cancellation::default();
    let before = worker_digest(&path, &cancellation).unwrap();
    worker_unchanged(&path, &input(), &cancellation, before).unwrap();
    fs::write(&path, b"changed executable fixture").unwrap();
    assert!(worker_unchanged(&path, &input(), &cancellation, before).is_err());
    cancellation.cancel();
    assert!(matches!(
        worker_digest(&root.path().join("missing"), &cancellation),
        Err(Error::Canceled)
    ));
}
