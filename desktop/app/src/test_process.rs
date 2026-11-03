//! Preserve subprocess identity even when native failure bypasses Rust panics.
use std::{
    io,
    path::Path,
    process::{Command, Output, Stdio},
    time::Instant,
};

pub(crate) fn capture(
    command: &mut Command,
    evidence: &Path,
) -> io::Result<(Output, serde_json::Value)> {
    let began = Instant::now();
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis());
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id();
    let output = child.wait_with_output().map_err(|cause| {
        io::Error::new(
            cause.kind(),
            format!(
                "test child pid={pid} phase=wait elapsed_ms={} cause={cause}",
                began.elapsed().as_millis()
            ),
        )
    })?;
    let mut receipt = serde_json::json!({"schema_version":1,"pid":pid,"started_unix_ms":started,
        "elapsed_ms":began.elapsed().as_millis(),"status":output.status.to_string(),
        "success":output.status.success(),"exit_code":output.status.code(),
        "exit_code_hex":output.status.code().map(|code|format!("0x{:08x}",code as u32))});
    // A diagnostic write must never replace the original child outcome.
    let saved = serde_json::to_vec_pretty(&receipt)
        .map_err(io::Error::other)
        .and_then(|bytes| std::fs::write(evidence, bytes));
    if let Err(cause) = saved {
        receipt["receipt_write_error"] = cause.to_string().into();
    }
    Ok((output, receipt))
}

#[test]
fn child_identity_exit_codes_streams_and_secondary_write_failure_are_preserved() {
    const CHILD: &str = "MOTION_TEST_PROCESS_EXIT";
    const NAME: &str =
        "test_process::child_identity_exit_codes_streams_and_secondary_write_failure_are_preserved";
    if let Some(code) = std::env::var_os(CHILD) {
        use std::io::Write;
        std::io::stdout().write_all(b"controlled stdout\n").unwrap();
        std::io::stderr().write_all(b"controlled stderr\n").unwrap();
        std::process::exit(code.to_str().unwrap().parse().unwrap());
    }
    let root = tempfile::tempdir().unwrap();
    let mut codes = vec![0, 23];
    if cfg!(windows) {
        codes.push(0xc0000005_u32 as i32);
    }
    for code in codes {
        let path = root.path().join(format!("exit-{code}.json"));
        let (output, receipt) = capture(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env(CHILD, code.to_string()),
            &path,
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(code));
        assert_eq!(receipt["exit_code"], code);
        assert_eq!(receipt["exit_code_hex"], format!("0x{:08x}", code as u32));
        assert_eq!(receipt["success"], code == 0);
        assert!(receipt["pid"].as_u64().unwrap() > 0);
        assert!(String::from_utf8_lossy(&output.stdout).contains("controlled stdout"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("controlled stderr"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),
            receipt
        );
    }
    let blocked = root.path().join("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    let (output, receipt) = capture(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "23"),
        &blocked,
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(receipt["exit_code"], 23);
    assert!(receipt["receipt_write_error"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
}
