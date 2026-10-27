use super::*;
use std::{sync::mpsc, time::Instant};

fn child(directory: &std::path::Path, windows: &str, unix: &str) -> Command {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let script = directory.join("child.ps1");
        fs::write(&script, windows).unwrap();
        let mut command = Command::new(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        command.arg(script).arg(directory);
        command.creation_flags(0x08000000);
        let _ = unix;
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let script = directory.join("child.sh");
        fs::write(&script, unix).unwrap();
        let mut command = Command::new("/bin/sh");
        command.arg(script).arg(directory);
        let _ = windows;
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn actual_child_keeps_protocol_and_diagnostics_in_separate_pipes() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = child(
        directory.path(),
        "[Console]::Out.Write('{\"schema\":1}'); [Console]::Error.Write('runtime diagnostic')",
        "printf '%s' '{\"schema\":1}'; printf '%s' 'runtime diagnostic' >&2",
    );
    let (stdout, stderr) = capture(&mut command, &Cancellation::default()).unwrap();
    assert_eq!(stdout, br#"{"schema":1}"#);
    assert_eq!(stderr, "runtime diagnostic");
}

#[test]
fn actual_failed_child_retains_exit_response_and_bounded_diagnostic_tail() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = child(directory.path(),
        "[Console]::Out.Write('model load failed'); [Console]::Error.Write(('x' * 20000) + 'missing-model-sentinel'); exit 7",
        "printf '%s' 'model load failed'; i=0; while [ $i -lt 20000 ]; do printf x >&2; i=$((i+1)); done; printf '%s' 'missing-model-sentinel' >&2; exit 7");
    let error = capture(&mut command, &Cancellation::default()).unwrap_err();
    assert!(matches!(error, Error::Backend(_)));
    let message = error.to_string();
    assert!(message.contains('7') && message.contains("model load failed"));
    assert!(message.contains("missing-model-sentinel"));
    assert!(
        message.len() < 900,
        "diagnostics escaped the retained bound"
    );
}

#[test]
fn actual_protocol_overflow_terminates_the_child_and_reaps_its_pipes() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = child(directory.path(),
        "[Console]::Out.Write(('x' * 20000)); [Console]::Out.Flush(); while ($true) { Start-Sleep -Milliseconds 100 }",
        "i=0; while [ $i -lt 20000 ]; do printf x; i=$((i+1)); done; while :; do :; done");
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        sender
            .send(capture(&mut command, &Cancellation::default()))
            .unwrap()
    });
    let error = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("overflow did not reap child")
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Invalid("runtime stdout exceeds protocol budget")
    ));
    worker.join().unwrap();
    directory.close().unwrap();
}

#[test]
fn cancellation_after_real_child_entry_reaps_the_process_before_returning() {
    let directory = tempfile::tempdir().unwrap();
    let entered = directory.path().join("entered");
    let mut command = child(directory.path(),
        "param([string]$directory)\n[IO.File]::WriteAllText((Join-Path $directory 'entered'), [string]$PID); while ($true) { Start-Sleep -Milliseconds 100 }",
        "printf '%s' \"$$\" > \"$1/entered\"; while :; do :; done");
    let cancellation = Cancellation::default();
    let child_cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        sender
            .send(capture(&mut command, &child_cancellation))
            .unwrap()
    });
    let began = Instant::now();
    while !entered.is_file() && began.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    cancellation.cancel();
    let result = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("canceled child did not exit");
    worker.join().unwrap();
    assert!(
        entered.is_file(),
        "child never entered its controlled blocking point"
    );
    assert!(matches!(result, Err(Error::Canceled)));
    directory.close().unwrap();
}
