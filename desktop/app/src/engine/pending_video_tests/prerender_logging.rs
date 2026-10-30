use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn prerender_worker_start_is_on_disk_before_blocked_decode_and_error_uses_default_sinks() {
    use motion_desktop_media::movie::{
        prerender::{
            worker::{Outcome, Worker},
            PrerenderPlan,
        },
        ExportControl,
    };
    const NAME: &str = "engine::pending_video_tests::prerender_logging::prerender_worker_start_is_on_disk_before_blocked_decode_and_error_uses_default_sinks";
    const CHILD: &str = "MOTION_PRERENDER_LOG_CHILD";
    const CAUSE: &str = "controlled first video frame failure";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        fixture(&project_root, false);
        let (entered, waiting) = channel();
        let gate = Arc::new(Gate {
            released: Mutex::new(false),
            ready: Condvar::new(),
            entered,
            fail: true,
        });
        let session = motion_host::Session::new(
            motion_core::storage::load(&project_root).unwrap(),
            project_root,
            Arc::new(Backend(gate.clone())),
        )
        .unwrap();
        let input = PrerenderPlan::capture(
            &session,
            session.document_generation,
            session.surface_epoch,
            8201,
            1 << 20,
            &ExportControl::default(),
        )
        .unwrap();
        let mut worker = Worker::start(1 << 20).unwrap();
        let prepare_id = worker.prepare(input).unwrap().unwrap();
        let began = Instant::now();
        let prepared = loop {
            if let Some(result) = worker.poll().unwrap() {
                break result;
            }
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        };
        assert_eq!(prepared.id, prepare_id);
        assert!(matches!(prepared.outcome, Outcome::Prepared(_)));
        let render_id = worker.render(0.).unwrap().unwrap();
        waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        let flight = worker.observation().snapshot().unwrap().in_flight.unwrap();
        assert_eq!(flight.id, render_id);
        assert_eq!(flight.operation, "render");
        assert_eq!(flight.phase, "video");
        // Read before releasing the decoder and without a test-side flush.
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let start = format!("start #{render_id} prerender_worker operation=render");
        assert!(text.contains(&start), "{text}");
        assert!(!text.contains(CAUSE));
        std::fs::write(root.join("blocked-start.log"), text).unwrap();
        *gate.released.lock().unwrap() = true;
        gate.ready.notify_all();
        let began = Instant::now();
        let completed = loop {
            if let Some(result) = worker.poll().unwrap() {
                break result;
            }
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        };
        assert_eq!(completed.id, render_id);
        let Outcome::Failed(cause) = completed.outcome else {
            panic!("failed decoder became a successful frame");
        };
        assert!(cause.contains(CAUSE), "{cause}");
        assert_eq!(
            worker.observation().snapshot().unwrap().retained_cpu_bytes,
            0
        );
        worker.shutdown();
        let began = Instant::now();
        while !worker.stopped() {
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        worker.shutdown();
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
    let text = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(text.contains("prerender_worker operation=render"));
    assert!(text.contains(CAUSE));
    assert!(String::from_utf8_lossy(&output.stderr).contains(CAUSE));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(CAUSE));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("prerender_worker operation=render"));
    if let Some(directory) = std::env::var_os("MOTION_PRERENDER_LOG_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("default-file.log"), text).unwrap();
        std::fs::copy(
            root.path().join("blocked-start.log"),
            directory.join("blocked-start.log"),
        )
        .unwrap();
        std::fs::write(directory.join("default-stderr.log"), output.stderr).unwrap();
        std::fs::write(directory.join("default-stdout.log"), output.stdout).unwrap();
    }
}
