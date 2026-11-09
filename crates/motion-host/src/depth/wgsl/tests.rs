use super::*;
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicit installed Python diagnostic fixture"]
fn actual_owned_python_enables_fault_observation_and_retains_stack_and_exit_cause() {
    let fixture_path = std::env::var_os("MOTION_DEPTH_PYTHON_DIAGNOSTIC_FIXTURE")
        .expect("explicit installed Python fixture required");
    let fixture: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_path).unwrap()).unwrap();
    let python = Path::new(fixture["python"].as_str().unwrap());
    let root = Path::new(fixture["output"].as_str().unwrap());
    assert!(python.is_absolute() && root.is_absolute() && !root.exists());
    fs::create_dir(root).unwrap();
    fs::write(
        root.join("depth_tensor.py"),
        r#"import faulthandler, sys
assert faulthandler.is_enabled(), 'owned Python fault observer was not enabled'
sys.stderr.write('owned-stack-observation-control\n')
sys.stderr.flush()
faulthandler.dump_traceback(file=sys.stderr, all_threads=True)
raise RuntimeError('original-owned-python-exit-sentinel')
"#,
    )
    .unwrap();
    let mut outcomes = Vec::new();
    for (label, mode) in [
        ("plain", TransformInvocation::Plain),
        ("resolve", TransformInvocation::Resolve),
        ("attest", TransformInvocation::Attest),
    ] {
        let mut command = python_transform(python, root, mode);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let cause = super::super::onnx::capture(&mut command, &Cancellation::default())
            .unwrap_err()
            .to_string();
        assert!(cause.contains("owned-stack-observation-control"));
        assert!(cause.contains("depth_tensor.py"));
        assert!(cause.contains("original-owned-python-exit-sentinel"));
        assert!(
            cause.contains("response= stderr="),
            "stdout must remain empty"
        );
        outcomes.push(serde_json::json!({"mode":label,"original_cause":cause}));
        fs::write(root.join("receipt.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Actual owned Python observer enablement and explicit stack/nonzero exit; not an access-violation reproduction or fix",
            "outcomes":outcomes})).unwrap()).unwrap();
    }
}
fn input() -> Input {
    let mut hash = [0u8; 32];
    for (index, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&MODEL_SHA256[index * 2..index * 2 + 2], 16).unwrap();
    }
    Input::new(
        super::super::Key {
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
                sha256: hash,
                processing: super::super::onnx::PROCESSING.into(),
            },
        },
        [2, 3],
        Arc::from([17u8; 18]),
    )
    .unwrap()
}
fn native() -> serde_json::Value {
    serde_json::json!({
        "status":"actual_complete_gpu_output_written_independent_comparison_pending","phase":"completed",
        "model_sha256":MODEL_SHA256,"program_sha256":PROGRAM_SHA256,"request_sha256":"a".repeat(64),
        "encoded_operations":429,"final_readbacks":1,"per_node_cpu_injection":false,"pool_retained_through_completion":true,
        "actual_retained_buffer_capacity_bytes":227968156,"output_bytes":1073296,"output_sha256":"b".repeat(64),
        "adapter":{"backend":"Vulkan","name":"authored GPU","device_type":"DiscreteGpu"}
    })
}
#[test]
fn native_report_rejects_cpu_fallback_wrong_source_and_incomplete_ownership() {
    let valid = native();
    let digest = "a".repeat(64);
    serde_json::from_value::<Native>(valid.clone())
        .unwrap()
        .validate(Hardware::Vulkan, &digest)
        .unwrap();
    for (field, value) in [
        ("model_sha256", serde_json::json!("c".repeat(64))),
        ("request_sha256", serde_json::json!("d".repeat(64))),
        ("program_sha256", serde_json::json!("e".repeat(64))),
        ("encoded_operations", serde_json::json!(428)),
        ("final_readbacks", serde_json::json!(2)),
        ("per_node_cpu_injection", serde_json::json!(true)),
        ("pool_retained_through_completion", serde_json::json!(false)),
        (
            "actual_retained_buffer_capacity_bytes",
            serde_json::json!(227968157),
        ),
        ("output_bytes", serde_json::json!(1073292)),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert!(
            serde_json::from_value::<Native>(bad)
                .unwrap()
                .validate(Hardware::Vulkan, &digest)
                .is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("backend", "Dx12"),
        ("device_type", "Cpu"),
        ("name", "bad\nidentity"),
    ] {
        let mut bad = valid.clone();
        bad["adapter"][field] = value.into();
        assert!(serde_json::from_value::<Native>(bad)
            .unwrap()
            .validate(Hardware::Vulkan, &digest)
            .is_err());
    }
    assert!(serde_json::from_value::<Native>(valid)
        .unwrap()
        .validate(Hardware::Metal, &digest)
        .is_err());
}
#[test]
fn transform_protocol_binds_phase_dimensions_and_both_input_identities() {
    let input = input();
    let rgb = "a".repeat(64);
    let depth = "b".repeat(64);
    let valid = serde_json::json!({"schema":1,"stage":"remap","processing":super::super::onnx::PROCESSING,
        "width":2,"height":3,"rgb_sha256":rgb,"depth_sha256":depth,"output_sha256":"c".repeat(64),"output_bytes":24,"elapsed_ns":1});
    let parsed: Transform = serde_json::from_value(valid.clone()).unwrap();
    parsed
        .validate(&input, "remap", &rgb, Some(&depth))
        .unwrap();
    assert!(parsed.validate(&input, "prepare", &rgb, None).is_err());
    assert!(parsed
        .validate(&input, "remap", &"d".repeat(64), Some(&depth))
        .is_err());
    assert!(parsed.validate(&input, "remap", &rgb, None).is_err());
    for (field, value) in [
        ("width", serde_json::json!(3)),
        ("height", serde_json::json!(2)),
        ("output_bytes", serde_json::json!(20)),
        ("schema", serde_json::json!(2)),
        ("output_sha256", serde_json::json!("unbounded")),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert!(serde_json::from_value::<Transform>(bad)
            .unwrap()
            .validate(&input, "remap", &rgb, Some(&depth))
            .is_err());
    }
}
#[test]
fn finite_owned_bytes_refuse_growth_hash_aliases_and_nan_without_substituting_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("depth");
    let good = 3.25f32.to_le_bytes();
    fs::write(&path, good).unwrap();
    assert_eq!(read_owned(&path, 4, &digest(&good)).unwrap(), good);
    assert!(read_owned(&path, 4, &"0".repeat(64)).is_err());
    fs::write(&path, [0u8; 8]).unwrap();
    assert!(read_owned(&path, 4, &digest(&[0u8; 8])).is_err());
    let nan = f32::NAN.to_le_bytes();
    fs::write(&path, nan).unwrap();
    assert!(read_owned(&path, 4, &digest(&nan)).is_err());
}
#[test]
fn canceled_backend_creates_no_scratch_and_starts_no_child() {
    let directory = tempfile::tempdir().unwrap();
    let scratch = directory.path().join("absent");
    let mut backend = Wgsl::new(Config {
        python: directory.path().join("no-python"),
        worker: directory.path().join("no-worker"),
        program: directory.path().join("no-program"),
        scratch: scratch.clone(),
        hardware: Hardware::Vulkan,
    })
    .unwrap();
    let canceled = Cancellation::default();
    canceled.cancel();
    assert!(matches!(
        backend.infer(&input(), &canceled),
        Err(Error::Canceled)
    ));
    assert!(!scratch.exists());
}

#[test]
fn retained_capacity_is_key_bound_and_cleared_before_cancelled_work() {
    let directory = tempfile::tempdir().unwrap();
    let scratch = directory.path().join("absent");
    let mut backend = Wgsl::new(Config {
        python: directory.path().join("no-python"),
        worker: directory.path().join("no-worker"),
        program: directory.path().join("no-program"),
        scratch: scratch.clone(), hardware: Hardware::Vulkan,
    }).unwrap();
    let sample = input();
    assert_eq!(backend.retained_gpu_buffer_capacity(&sample), None);
    backend.retained_gpu_buffers = Some((sample.key().clone(), BUFFER_BYTES));
    assert_eq!(backend.retained_gpu_buffer_capacity(&sample), Some(BUFFER_BYTES));
    let mut foreign_key = sample.key().clone();
    foreign_key.source.revision += 1;
    let foreign = Input::new(foreign_key, sample.size(), Arc::from(sample.rgb())).unwrap();
    assert_eq!(backend.retained_gpu_buffer_capacity(&foreign), None);
    let canceled = Cancellation::default();
    canceled.cancel();
    assert!(matches!(backend.infer(&sample, &canceled), Err(Error::Canceled)));
    assert_eq!(backend.retained_gpu_buffer_capacity(&sample), None);
    backend.retained_gpu_buffers = Some((sample.key().clone(), BUFFER_BYTES));
    assert!(matches!(backend.execution_identity(&sample, &canceled), Err(Error::Canceled)));
    assert_eq!(backend.retained_gpu_buffer_capacity(&sample), None);
    assert!(!scratch.exists());
}
#[test]
fn missing_real_child_retains_its_original_phase_and_cause() {
    let directory = tempfile::tempdir().unwrap();
    let mut backend = Wgsl::new(Config {
        python: directory.path().join("no-python"),
        worker: directory.path().join("no-worker"),
        program: directory.path().join("no-program"),
        scratch: directory.path().join("scratch"),
        hardware: Hardware::Vulkan,
    })
    .unwrap();
    let cause = backend
        .infer(&input(), &Cancellation::default())
        .unwrap_err()
        .to_string();
    assert!(
        cause.contains("phase=prepare") && cause.contains("os error"),
        "{cause}"
    );
    assert_eq!(fs::read_dir(&backend.config.scratch).unwrap().count(), 0);
}

#[test]
fn native_failure_receipt_is_bounded_before_cleanup_and_never_replaces_a_primary_error() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("receipt.json"),
        serde_json::to_vec(&serde_json::json!({
        "status":"failed_original_cause_retained","phase":"read_hash_finite_upload_resource",
        "failure":format!("missing-weight-sentinel\n{}","x".repeat(10000))}))
        .unwrap(),
    )
    .unwrap();
    let cause = native_failure(directory.path());
    assert!(
        cause.contains("missing-weight-sentinel")
            && cause.contains("phase=read_hash_finite_upload_resource")
    );
    assert!(cause.len() < 2000 && !cause.contains('\n'));
    fs::write(directory.path().join("receipt.json"), b"malformed").unwrap();
    let primary = "original child exit preserved";
    let combined = native_error(Error::Backend(primary.into()), directory.path()).to_string();
    assert!(combined.contains(primary) && combined.contains("native_receipt unavailable"));
}

#[test]
fn native_panic_cause_precedes_long_child_backtrace_before_temporary_cleanup() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("panic.json"),serde_json::to_vec(&serde_json::json!({
        "schema":1,"status":"native_panic_original_cause_retained","cause":"original-panic-sentinel",
        "location":"native-worker-source.rs:37:9"})).unwrap()).unwrap();
    fs::write(
        root.path().join("receipt.json"),
        b"malformed receipt from an interrupted operation",
    )
    .unwrap();
    let error =
        native_error(Error::Backend("backtrace tail".repeat(4096)), root.path()).to_string();
    assert!(error
        .chars()
        .take(512)
        .collect::<String>()
        .contains("original-panic-sentinel"));
    assert!(error
        .chars()
        .take(768)
        .collect::<String>()
        .contains("native-worker-source.rs:37:9"));
    assert!(error.contains("backtrace tail"));
}

#[test]
fn completed_panic_observation_does_not_invent_failures_for_normal_completion() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("receipt.json"),
        b"ordinary completion, not a panic",
    )
    .unwrap();
    assert!(completed_panic(root.path()).is_none());
    fs::write(
        root.path().join("panic.json"),
        serde_json::to_vec(&serde_json::json!({
        "schema":1,"status":"native_panic_original_cause_retained","cause":"caught-native-panic"}))
        .unwrap(),
    )
    .unwrap();
    assert!(completed_panic(root.path())
        .unwrap()
        .contains("caught-native-panic"));
}

#[test]
fn later_fatal_panic_precedes_an_earlier_caught_panic_with_its_own_location() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("panic.json"),
        serde_json::to_vec(&serde_json::json!({
        "schema":1,"status":"native_panic_original_cause_retained","cause":"earlier-caught-panic",
        "location":"earlier.rs:1:2"}))
        .unwrap(),
    )
    .unwrap();
    let child = Error::Backend(format!(
        "{}\nERROR motion_depth_wgsl: panic cause=later-fatal-sentinel at=later.rs:37:9\n",
        "backtrace".repeat(4096)
    ));
    let error = native_error(child, root.path()).to_string();
    let default = error.chars().take(768).collect::<String>();
    assert!(default.contains("later-fatal-sentinel") && default.contains("at=later.rs:37:9"));
    assert!(
        error.find("later-fatal-sentinel").unwrap() < error.find("earlier-caught-panic").unwrap()
    );
}

#[test]
fn a_recorded_model_failure_stays_primary_after_an_internally_caught_panic() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("panic.json"),
        serde_json::to_vec(&serde_json::json!({
        "schema":1,"status":"native_panic_original_cause_retained","cause":"earlier-caught-panic"}))
        .unwrap(),
    )
    .unwrap();
    fs::write(root.path().join("receipt.json"),serde_json::to_vec(&serde_json::json!({
        "status":"failed_original_cause_retained","phase":"resource","failure":"actual-model-failure"})).unwrap()).unwrap();
    let child = Error::Backend(
        "ERROR motion_depth_wgsl: panic cause=earlier-caught-panic at=old.rs:1:2".into(),
    );
    let error = native_error(child, root.path()).to_string();
    assert!(error
        .chars()
        .take(512)
        .collect::<String>()
        .contains("actual-model-failure"));
    assert!(
        error.find("actual-model-failure").unwrap() < error.find("earlier-caught-panic").unwrap()
    );
}

#[test]
fn panic_footer_requires_a_record_boundary_and_keeps_the_last_cause_and_location() {
    assert!(last_panic(
        "ordinary diagnostic containing ERROR motion_depth_wgsl: panic cause=not-a-record"
    )
    .is_none());
    assert_eq!(last_panic("ERROR motion_depth_wgsl: panic cause=first at=first.rs:1:2\nERROR motion_depth_wgsl: panic cause=last at=last.rs:3:4\n").as_deref(),Some("last at=last.rs:3:4"));
    assert_eq!(last_panic("runtime exit=101 response= stderr=ERROR motion_depth_wgsl: panic cause=wrapped at=wrapped.rs:5:6\n").as_deref(),Some("wrapped at=wrapped.rs:5:6"));
}
