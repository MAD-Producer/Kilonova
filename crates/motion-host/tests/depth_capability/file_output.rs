use super::*;

const TEST: &str =
    "bindings::file_output::qualified_identity_error_reaches_file_stderr_and_only_harness_stdout";
const CHILD: &str = "MOTION_TEST_QUALIFIED_FILE_CHILD";
const EVIDENCE: &str = "MOTION_TEST_QUALIFIED_FILE_EVIDENCE";

#[test]
fn qualified_identity_error_reaches_file_stderr_and_only_harness_stdout() {
    if let Some(root) = std::env::var_os(CHILD) {
        // The child runs only this exact test; no in-memory test logger is
        // installed. Reuse the desktop's actual file/stderr sink unchanged.
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let root = std::path::PathBuf::from(root);
        let _guard = crate::file_logging::init(root.join("logs")).unwrap();
        assert_eq!(log::max_level(), log::LevelFilter::Info);
        let admitted = bindings();
        let mut controlled = backend(Fault::None);
        controlled.execution.provider = "changed-provider".into();
        let infers = controlled.infers.clone();
        let (entered, rx) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        controlled.block = Some((entered, blocked));
        let mut jobs = Jobs::start_qualified(controlled, admitted, None).unwrap();
        let guard = Release(release);
        let mut k = key();
        k.source.asset = 75002;
        let request = jobs
            .request(Input::new(k.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
            .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let start =
            format!("start #{request} depth_qualification asset=75002 phase=qualified_resolve");
        let path = root.join("logs/motion-studio.log");
        let before = std::fs::read_to_string(&path).unwrap();
        assert!(
            before.contains(&start),
            "actual file start must precede blocked resolution"
        );
        assert!(!before.contains("qualified execution differs"));
        assert_eq!(
            jobs.in_flight().unwrap().unwrap().phase,
            "qualified_resolve"
        );
        drop(guard);
        wait_done(&jobs, 1);
        let cause = jobs.take_prepared_ready(&k).unwrap_err();
        assert!(matches!(cause, Error::Backend(ref c) if c ==
            "qualified execution differs field=provider required=\"actual-provider\" resolved=\"changed-provider\""));
        assert_eq!(infers.load(Ordering::SeqCst), 0);
        assert_eq!(jobs.counts().unwrap().failed, 1);
        let after = std::fs::read_to_string(path).unwrap();
        assert!(after.contains(" ERROR [motion-depth] motion_host::depth:"));
        assert!(after.contains("operation=depth_infer asset=75002 revision=2"));
        assert!(after.contains("stage=qualified_resolve"));
        assert!(after.contains(&cause.to_string()));
        assert!(jobs.in_flight().unwrap().is_none());
        std::fs::write(root.join("proof.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "default_level":"Info", "request":request, "asset":75002,
            "file_start_before_resolution_return":true, "error_file_before_drop":true,
            "backend_inferences":0, "failed":1, "cause":cause.to_string(),
            "scope":"Controlled admission reports; actual desktop sink; no real model/device qualification"
        })).unwrap()).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, root.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(output.status.success(), "child failed: {stderr}\n{stdout}");
    assert!(stderr.contains("ERROR motion_host::depth:"));
    assert!(stderr.contains("operation=depth_infer asset=75002 revision=2"));
    assert!(stderr.contains("stage=qualified_resolve"));
    assert!(stderr.contains("qualified execution differs field=provider"));
    assert!(stderr.contains("required=\"actual-provider\" resolved=\"changed-provider\""));
    for line in stdout.lines() {
        assert!(
            line.is_empty()
                || line == "running 1 test"
                || line == format!("test {TEST} ... ok")
                || line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "),
            "stdout may contain only the Rust test harness: {line}"
        );
    }
    let proof: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join("proof.json")).unwrap()).unwrap();
    assert_eq!(proof["backend_inferences"], 0);
    if let Some(evidence) = std::env::var_os(EVIDENCE) {
        let evidence = std::path::PathBuf::from(evidence);
        std::fs::create_dir(&evidence).unwrap();
        for (name, raw) in [
            (
                "motion-studio.log",
                std::fs::read(root.path().join("logs/motion-studio.log")).unwrap(),
            ),
            (
                "proof.json",
                std::fs::read(root.path().join("proof.json")).unwrap(),
            ),
            ("child.stdout.log", output.stdout),
            ("child.stderr.log", output.stderr),
        ] {
            std::fs::write(evidence.join(name), raw).unwrap();
        }
    }
}
