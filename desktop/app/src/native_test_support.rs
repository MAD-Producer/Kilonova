//! Hidden-window tests must service the owner thread while a GPU worker runs.
use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::pump_events::EventLoopExtPumpEvents,
    window::WindowId,
};

struct HiddenWindows;
impl ApplicationHandler for HiddenWindows {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

/// The hidden window owner must keep dispatching messages during screenshot
/// rendering too. Preserve Engine::capture_ui's existing sixty-second wait.
pub(crate) fn capture_ui(
    event_loop: &mut EventLoop<()>,
    engine: &crate::engine::Engine,
    ui: crate::engine::UiFrame,
    path: std::path::PathBuf,
) -> Result<serde_json::Value, String> {
    let window = ui.window;
    let evidence = path.with_extension("failure.json");
    let reply = engine.request(|reply| crate::engine::Command::CaptureUi { ui, path, reply })?;
    receive(event_loop, &reply, Duration::from_secs(60))
        .map_err(|error| report_capture_wait_failure(engine, &evidence, window, error))?
}

pub(crate) fn report_capture_wait_failure(
    engine: &crate::engine::Engine,
    evidence: &std::path::Path,
    window: u64,
    error: RecvTimeoutError,
) -> String {
    let mut snapshot = engine.native_wait_snapshot();
    snapshot["schema_version"] = serde_json::json!(1);
    snapshot["phase"] = serde_json::json!("capture_ui_reply");
    snapshot["window"] = serde_json::json!(window);
    snapshot["cause"] = serde_json::json!(error.to_string());
    let saved = serde_json::to_vec_pretty(&snapshot)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(evidence, bytes));
    format!("capture_ui native reply wait failed: {error}; context={snapshot}; save={saved:?}")
}

/// Preserve the caller's deadline while dispatching native window messages.
/// DXGI can synchronously message the window owner from the render thread.
/// These hidden fixtures have no interactive event handling; production uses
/// its normal ApplicationHandler instead of this test-only driver.
pub(crate) fn receive<T>(
    event_loop: &mut EventLoop<()>,
    reply: &Receiver<T>,
    timeout: Duration,
) -> Result<T, RecvTimeoutError> {
    let deadline = Instant::now() + timeout;
    loop {
        match reply.try_recv() {
            Ok(value) => return Ok(value),
            Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
            Err(TryRecvError::Empty) => {}
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RecvTimeoutError::Timeout);
        }
        event_loop.pump_app_events(
            Some(remaining.min(Duration::from_millis(1))),
            &mut HiddenWindows,
        );
    }
}

#[test]
fn capture_wait_evidence_preserves_timeout_write_failure_and_worker_progress() {
    let root = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let (entered, entry) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let reply = engine
        .request(|reply| crate::engine::Command::TestWait {
            entered,
            release: gate,
            reply,
        })
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let timeout = reply.recv_timeout(Duration::from_millis(20)).unwrap_err();
    assert_eq!(timeout, RecvTimeoutError::Timeout);
    let evidence = root.path().join("capture.failure.json");
    let message = report_capture_wait_failure(&engine, &evidence, 7, timeout);
    assert!(message.contains(&timeout.to_string()));
    assert!(message.contains("save=Ok(())"));
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(evidence).unwrap()).unwrap();
    assert_eq!(snapshot["cause"], timeout.to_string());
    assert_eq!(snapshot["phase"], "capture_ui_reply");
    assert_eq!(snapshot["window"], 7);
    assert_eq!(snapshot["in_flight"]["operation"], "test_wait");
    assert_eq!(snapshot["native"]["progress"]["value"]["phase"], "dispatch");
    assert_eq!(
        snapshot["native"]["progress"]["value"]["request"],
        snapshot["in_flight"]["request"]
    );
    let blocked = root.path().join("not-a-directory");
    std::fs::write(&blocked, b"held file").unwrap();
    let message =
        report_capture_wait_failure(&engine, &blocked.join("capture.failure.json"), 7, timeout);
    assert!(message.contains(&timeout.to_string()));
    assert!(message.contains("save=Err("));
    release.send(()).unwrap();
    reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(!engine.failed());
    assert!(engine.state().is_ok());
}

#[test]
fn native_reply_wait_services_cross_thread_messages_and_preserves_deadlines() {
    const CHILD: &str = "MOTION_NATIVE_REPLY_WAIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "native_test_support::native_reply_wait_services_cross_thread_messages_and_preserves_deadlines"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::mpsc::channel;
    use winit::platform::windows::EventLoopBuilderExtWindows;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(
            window: *mut std::ffi::c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
            flags: u32,
            timeout_ms: u32,
            result: *mut usize,
        ) -> isize;
    }
    let mut event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = event_loop
        .create_window(winit::window::Window::default_attributes().with_visible(false))
        .unwrap();
    let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
        panic!("expected native Win32 test window");
    };
    let hwnd = handle.hwnd.get();
    let (entered, started) = channel();
    let (send, reply) = channel();
    let worker = std::thread::spawn(move || {
        entered.send(()).unwrap();
        let mut result = 0;
        // WM_NULL is harmless, but still needs the owner thread's message pump.
        // The native timeout bounds this negative-control worker if a test fails.
        let serviced =
            unsafe { SendMessageTimeoutW(hwnd as *mut _, 0, 0, 0, 0, 5_000, &mut result) };
        send.send(serviced != 0).unwrap();
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        reply.recv_timeout(Duration::from_millis(20)),
        Err(RecvTimeoutError::Timeout)
    );
    assert!(receive(&mut event_loop, &reply, Duration::from_secs(1)).unwrap());
    worker.join().unwrap();

    let (send, reply) = channel::<()>();
    let began = Instant::now();
    assert_eq!(
        receive(&mut event_loop, &reply, Duration::from_millis(30)),
        Err(RecvTimeoutError::Timeout)
    );
    assert!(began.elapsed() >= Duration::from_millis(30));
    drop(send);
    assert_eq!(
        receive(&mut event_loop, &reply, Duration::from_secs(1)),
        Err(RecvTimeoutError::Disconnected)
    );
    let (send, reply) = channel();
    send.send(17).unwrap();
    assert_eq!(receive(&mut event_loop, &reply, Duration::ZERO), Ok(17));
}
