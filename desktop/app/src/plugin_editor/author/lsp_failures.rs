use super::*;
#[test]
#[ignore = "explicit process/failure-channel acceptance; rustc required"]
fn language_failures_publish_before_blocking_and_use_default_error_channels() {
    const CHILD: &str = "MOTION_PLUGIN_LANGUAGE_FAILURE_CHILD";
    const TEST:&str="plugin_editor::author::lsp::failures::language_failures_publish_before_blocking_and_use_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logs = crate::logging::init(root.join("logs")).unwrap();
        let source = root.join("server.rs");
        std::fs::write(&source, include_str!("lsp_fixture.rs.txt")).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2021", "--crate-name", "lsp_fixture"])
            .arg(&source)
            .arg("-o")
            .arg(root.join(format!("fixture{}", std::env::consts::EXE_SUFFIX)))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let logs = || std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let wait = |c: &Client, predicate: &dyn Fn(&Snapshot) -> bool| {
            let start = Instant::now();
            loop {
                let s = c.snapshot().unwrap();
                if predicate(&s) {
                    return s;
                }
                assert!(start.elapsed() < Duration::from_secs(10), "{:?}", s.status);
                thread::sleep(Duration::from_millis(2));
            }
        };
        let request = || Request {
            revision: 11,
            text: "fn hello() {}".into(),
            prefix: 0,
            position: [0, 3],
            kind: Kind::Completion,
        };
        let missing = Client::start(root.join("missing-server.exe")).unwrap();
        let missing = wait(&missing, &|s| matches!(s.status, Status::Failed(_)));
        assert!(matches!(&missing.status,Status::Failed(e) if e.contains("spawn language server")));
        assert!(missing.flight.is_none());
        for mode in ["block", "malformed", "reject"] {
            let path = root.join(format!("{mode}{}", std::env::consts::EXE_SUFFIX));
            match std::fs::remove_file(path.with_extension("blocked")) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => panic!("remove stale fixture sentinel: {e}"),
            }
            std::fs::copy(
                root.join(format!("fixture{}", std::env::consts::EXE_SUFFIX)),
                &path,
            )
            .unwrap();
            let client = Client::start(path.clone()).unwrap();
            wait(&client, &|s| s.status == Status::Ready);
            assert!(client.submit(request()).unwrap());
            if mode == "block" {
                let start = Instant::now();
                while !path.with_extension("blocked").exists() {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(2));
                }
                let state = client.snapshot().unwrap();
                assert_eq!(state.flight.unwrap().0, "textDocument/completion");
                assert!(state.reply.is_none());
                assert!(!client.submit(request()).unwrap());
                // No release is sent: observing this start proves it precedes a
                // handler that never returns. The server is killed only on close.
                assert!(logs()
                    .contains("start request=1 revision=11 operation=textDocument/completion"));
            } else if mode == "malformed" {
                let state = wait(&client, &|s| matches!(s.status, Status::Failed(_)));
                assert!(matches!(state.status,Status::Failed(e) if e.contains("content length")));
                assert!(state.flight.is_none());
            } else {
                let state = wait(&client, &|s| s.reply.is_some());
                assert_eq!(
                    state.reply.unwrap().result["languageError"],
                    "controlled request rejection"
                );
                assert!(state.flight.is_none());
            }
            let state = client.state.clone();
            drop(client);
            let start = Instant::now();
            loop {
                let status = state.lock().unwrap().status.clone();
                if matches!(status, Status::Stopped | Status::Failed(_)) {
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "server not reaped after close"
                );
                thread::sleep(Duration::from_millis(2));
            }
        }
        crate::logging::flush();
        let file = logs();
        for marker in [
            "spawn language server",
            "content length",
            "controlled request rejection",
            "revision=11",
            "operation=textDocument/completion",
        ] {
            assert!(file.contains(marker), "{marker}");
        }
        #[cfg(windows)]
        if std::env::var_os("MOTION_PLUGIN_LANGUAGE_ABRUPT").is_some() {
            let path = root.join("block.exe");
            std::fs::remove_file(path.with_extension("blocked")).unwrap();
            let client = Client::start(path.clone()).unwrap();
            wait(&client, &|s| s.status == Status::Ready);
            assert!(client.submit(request()).unwrap());
            let start = Instant::now();
            while !path.with_extension("blocked").exists() {
                assert!(start.elapsed() < Duration::from_secs(5));
                thread::sleep(Duration::from_millis(2));
            }
            // Deliberately bypass every Rust destructor, including Client::Drop.
            std::mem::forget(client);
            std::process::exit(0);
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_LANGUAGE_FAILURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--ignored", "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_LOG", "info")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for cause in [
        "spawn language server",
        "content length",
        "controlled request rejection",
    ] {
        assert!(stderr.contains(cause), "{cause}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("plugin.language") && !stdout.contains("motion_studio::"));
    #[cfg(windows)]
    verify_abrupt_owner(TEST, CHILD, &root);
}

#[cfg(windows)]
fn verify_abrupt_owner(test: &str, child_env: &str, root: &std::path::Path) {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn TerminateProcess(handle: *mut c_void, exit: u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let mut owner = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env(child_env, root)
        .env("MOTION_PLUGIN_LANGUAGE_ABRUPT", "1")
        .env("MOTION_LOG", "info")
        .stdout(std::fs::File::create(root.join("abrupt-stdout.txt")).unwrap())
        .stderr(std::fs::File::create(root.join("abrupt-stderr.txt")).unwrap())
        .spawn()
        .unwrap();
    let began = Instant::now();
    let status = loop {
        if let Some(status) = owner.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(20) {
            owner.kill().unwrap();
            owner.wait().unwrap();
            panic!("abrupt-owner fixture did not finish");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    let pid: u32 = std::fs::read_to_string(root.join("block.blocked"))
        .unwrap()
        .parse()
        .unwrap();
    let handle = unsafe { OpenProcess(0x00100001, 0, pid) }; // synchronize + terminate
    if handle.is_null() {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(87),
            "unexpected process query failure"
        );
        return; // exited PID no longer exists
    }
    let result = unsafe { WaitForSingleObject(handle, 5000) };
    if result != 0 {
        unsafe {
            TerminateProcess(handle, 1);
        }
    }
    assert_ne!(unsafe { CloseHandle(handle) }, 0);
    assert_eq!(result, 0, "language server survived abrupt owner exit");
}
