//! Observe frame work without changing its result or entering the Session.
use super::*;
use motion_render::{AllocationFrame, AllocationFrameOutcome, FrameOperation};

pub(super) fn begin(
    budget: &motion_render::AllocationBudget,
    operation: FrameOperation,
) -> Option<AllocationFrame> {
    static OMITTED: motion_render::observability::FailureCounter =
        motion_render::observability::FailureCounter::new();
    static FAULT: AtomicBool = AtomicBool::new(false);
    let request = current_command().map_or(0, |command| command.0);
    match budget.begin_frame(request, operation) {
        Ok(frame) => {
            FAULT.store(false, Ordering::Relaxed);
            if frame.is_some() {
                OMITTED.success();
            } else {
                OMITTED.failure(
                    "desktop.frame_allocation.observation",
                    format_args!(
                        "request={request} operation={operation:?} active_limit={}",
                        motion_render::MAX_ACTIVE_ALLOCATION_FRAMES
                    ),
                );
            }
            frame
        }
        Err(error) => {
            if !FAULT.swap(true, Ordering::Relaxed) {
                // Before with_session: no business or observation lock held.
                log::error!("frame allocation observation failed request={request} operation={operation:?} cause={error}");
            }
            None
        }
    }
}

pub(super) fn finish<T, E>(
    frame: Option<AllocationFrame>,
    result: &Result<T, E>,
    rendered: impl FnOnce(&T) -> bool,
) {
    if let Some(frame) = frame {
        frame.finish(match result {
            Ok(value) if rendered(value) => AllocationFrameOutcome::Rendered,
            Ok(_) => AllocationFrameOutcome::Skipped,
            Err(_) => AllocationFrameOutcome::Failed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_escalates_once_and_resets_with_default_file_and_stderr_channels() {
        const CHILD: &str = "MOTION_FRAME_WINDOW_LOG_CHILD";
        const NAME: &str = "engine::frame_allocation::tests::saturation_escalates_once_and_resets_with_default_file_and_stderr_channels";
        if let Some(root) = std::env::var_os(CHILD) {
            let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
            CURRENT_COMMAND.set(Some(Flight {
                request: 901,
                operation: "present",
                started: Instant::now(),
            }));
            let budget = motion_render::AllocationBudget::new(8).unwrap();
            let mut frames: Vec<_> = (0..motion_render::MAX_ACTIVE_ALLOCATION_FRAMES)
                .map(|id| {
                    budget
                        .begin_frame(id as u64, FrameOperation::Present)
                        .unwrap()
                        .unwrap()
                })
                .collect();
            for _ in 0..120 {
                assert!(begin(&budget, FrameOperation::Present).is_none());
            }
            drop(frames.pop());
            let recovered = begin(&budget, FrameOperation::Present).unwrap();
            for _ in 0..60 {
                assert!(begin(&budget, FrameOperation::Present).is_none());
            }
            assert_eq!(budget.frame_peaks().unwrap().omitted_frames, 180);
            drop((frames, recovered));
            assert!(begin(&budget, FrameOperation::Present).is_some());
            crate::logging::flush();
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let site = "site=desktop.frame_allocation.observation";
        assert!(file.contains("logging initialized level=INFO"));
        assert_eq!(file.matches(site).count(), 2);
        assert_eq!(stderr.matches(site).count(), 2);
        for line in file.lines().filter(|line| line.contains(site)) {
            assert!(
                line.contains("ERROR")
                    && line.contains("count=60 request=901 operation=Present active_limit=64"),
                "{line}"
            );
        }
        assert!(!stdout.contains(site));
        assert!(!stdout.contains("logging initialized"));
        assert!(!file.contains("count=1 "));
    }

    #[test]
    fn failed_present_publishes_its_actual_request_and_preserves_the_original_error() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            root.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let ui = UiFrame {
            paint: motion_ui::PaintList::default(),
            atlas: None,
            logical_size: [64., 64.],
            composition: None,
            composition_image: None,
            window: 0,
            node_preview: None,
        };
        let reply = engine.present_async(ui).unwrap();
        let error = reply
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err();
        assert!(error.contains("desktop surface is not attached"), "{error}");
        let snapshot = engine.allocation_frame_peaks().unwrap();
        assert!(snapshot.active.is_empty());
        assert_eq!(snapshot.recent.len(), 1);
        let frame = &snapshot.recent[0];
        assert!(frame.request > 0);
        assert_eq!(frame.operation, FrameOperation::Present);
        assert_eq!(frame.outcome, Some(AllocationFrameOutcome::Failed));
        assert_eq!(frame.submissions, 0);
    }

    #[test]
    fn readout_does_not_queue_behind_a_blocked_worker() {
        let root = tempfile::tempdir().unwrap();
        let engine = Arc::new(
            Engine::start(
                root.path().join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap(),
        );
        // Controlled metadata window plus a real blocked Engine. The actual
        // Present-path registration is checked separately above and on native GPU.
        let frame = engine
            .allocation_budget
            .begin_frame(42, FrameOperation::Render)
            .unwrap()
            .unwrap();
        let (entered, waiting) = channel();
        let (release, blocked) = channel();
        let reply = engine
            .request(|reply| Command::TestWait {
                entered,
                release: blocked,
                reply,
            })
            .unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let (send, receive) = channel();
        let reader = {
            let engine = engine.clone();
            std::thread::spawn(move || send.send(engine.allocation_frame_peaks()).unwrap())
        };
        let snapshot = receive.recv_timeout(Duration::from_secs(5));
        // Always unblock before assertions, including a failed readout timeout.
        release.send(()).unwrap();
        reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        reader.join().unwrap();
        let snapshot = snapshot.unwrap().unwrap();
        assert_eq!(snapshot.active.len(), 1);
        assert_eq!(snapshot.active[0].request, 42);
        assert!(snapshot.active[0].outcome.is_none());
        frame.finish(AllocationFrameOutcome::Rendered);
        assert!(engine.allocation_frame_peaks().unwrap().active.is_empty());
    }
}
