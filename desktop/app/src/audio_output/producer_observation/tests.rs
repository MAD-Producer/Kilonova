use crate::audio_output::*;
use std::{fs, sync::mpsc, thread};

fn finish(worker: JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "mixer observation deadline exceeded"
        );
        thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
}

struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn start_precedes_blocking_read_and_all_producer_exits_remain_observable() {
    const CHILD: &str = "MOTION_MIXER_LIFECYCLE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let stopped = Arc::new(Shared::new());
        stopped.stop.store(true, Ordering::Release);
        finish(spawn_producer(stopped.clone(), || panic!("stopped producer read source")).unwrap());
        assert!(!stopped.done.load(Ordering::Acquire));

        let eof = Arc::new(Shared::new());
        finish(spawn_producer(eof.clone(), || Ok(None)).unwrap());
        assert!(eof.done.load(Ordering::Acquire) && eof.ready.load(Ordering::Acquire));
        assert!(!eof.stop.load(Ordering::Acquire));

        let blocked = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let release = Release(release);
        let worker = spawn_producer(blocked.clone(), move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(15)).unwrap();
            Ok(Some([0.25, -0.25]))
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        // No flush on the test thread: start must be durable before next().
        let before = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let prefix = format!(
            "audio mixer start playback={} operation=audio_mix",
            blocked.observation
        );
        assert!(before.contains(&prefix), "{before}");
        assert!(!before.contains(&format!(
            "audio mixer stopped playback={}",
            blocked.observation
        )));
        blocked.stop.store(true, Ordering::Release);
        drop(release);
        finish(worker);
        assert!(blocked.ring.pop().is_none());
        assert!(!blocked.done.load(Ordering::Acquire));

        // Fill the real bounded queue. Thousands of frames still produce only
        // lifecycle records, and a stopped output does not refill it.
        let full = Arc::new(Shared::new());
        let (entered, entry) = mpsc::channel();
        let mut reads = 0;
        let worker = spawn_producer(full.clone(), move || {
            reads += 1;
            if reads == QUEUE_FRAMES + 1 {
                entered.send(()).unwrap();
            }
            Ok(Some([0.5, -0.5]))
        })
        .unwrap();
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        full.stop.store(true, Ordering::Release);
        worker.thread().unpark();
        finish(worker);
        assert!(full.ready.load(Ordering::Acquire));

        let failed = Arc::new(Shared::new());
        finish(
            spawn_producer(failed.clone(), || Err("controlled PCM read failed".into())).unwrap(),
        );
        assert_eq!(
            failed.error.lock().unwrap().as_deref(),
            Some("audio mix: controlled PCM read failed")
        );
        let panicked = Arc::new(Shared::new());
        finish(
            spawn_producer(panicked.clone(), || panic!("controlled mixer source panic")).unwrap(),
        );
        assert_eq!(
            panicked.error.lock().unwrap().as_deref(),
            Some("audio mixer worker panicked: controlled mixer source panic")
        );

        let text = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for (state, reason) in [
            (&stopped, "stop"),
            (&eof, "eof"),
            (&blocked, "stop_after_read"),
            (&full, "stop_after_read"),
            (&failed, "mix_error"),
            (&panicked, "panic"),
        ] {
            let line = text
                .lines()
                .find(|line| {
                    line.contains(&format!(
                        "audio mixer stopped playback={}",
                        state.observation
                    ))
                })
                .unwrap();
            assert!(line.contains(&format!("ms reason={reason}")), "{line}");
        }
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("audio mixer start playback="))
                .count(),
            6
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("audio mixer stopped playback="))
                .count(),
            6
        );
        let failures: Vec<_> = text
            .lines()
            .filter(|line| line.contains("audio mixer failed playback="))
            .collect();
        assert_eq!(failures.len(), 2);
        assert!(
            failures.iter().all(|line| line.contains(" ERROR ")
                && line.contains("operation=audio_mix")
                && line.contains("ms phase=")),
            "{text}"
        );
        assert!(failures
            .iter()
            .any(|line| line
                .contains("phase=mix_error reason=audio mix: controlled PCM read failed")));
        assert!(failures.iter().any(|line| line.contains(
            "phase=panic reason=audio mixer worker panicked: controlled mixer source panic"
        )));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "audio_output::producer_observation::tests::start_precedes_blocking_read_and_all_producer_exits_remain_observable"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("audio mixer failed playback=").count(),
        2,
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("audio mixer "));
}
