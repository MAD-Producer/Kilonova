use super::*;
use motion_desktop_media::movie::{
    prerender::{
        disk::Config,
        worker::{Outcome, Worker},
        PrerenderPlan,
    },
    ExportControl,
};

#[test]
fn actual_disk_open_failure_escalates_once_at_default_file_and_stderr_without_stdout() {
    const NAME: &str = "engine::pending_video_tests::prerender_disk_logging::actual_disk_open_failure_escalates_once_at_default_file_and_stderr_without_stdout";
    const CHILD: &str = "MOTION_PRERENDER_DISK_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        std::fs::create_dir(&project_root).unwrap();
        let sentinel = project_root.join("authored.keep");
        std::fs::write(&sentinel, b"authored project bytes").unwrap();
        let session = motion_host::Session::new(
            Project::new(32, 24, 30, 8).unwrap(),
            project_root,
            motion_desktop_media::platform(),
        )
        .unwrap();
        let before = serde_json::to_vec(session.engine.project()).unwrap();
        let blocked = root.join("not-directory");
        std::fs::write(&blocked, b"preserve original file").unwrap();
        let config = Config::new(blocked.clone(), 1 << 20).unwrap();
        let mut worker = Worker::start(1 << 20).unwrap();
        for request in 1..=65 {
            let plan =
                PrerenderPlan::capture(&session, 1, 2, request, 1 << 20, &ExportControl::default())
                    .unwrap()
                    .with_disk_cache(Some(config.clone()));
            let ticket = worker.prepare(plan).unwrap().unwrap();
            let began = Instant::now();
            let completion = loop {
                if let Some(completion) = worker.poll().unwrap() {
                    break completion;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(10),
                    "disk open acknowledgment deadline"
                );
                std::thread::yield_now();
            };
            assert_eq!(completion.id, ticket);
            assert!(
                matches!(completion.outcome, Outcome::Prepared(_)),
                "an optional disk fault must preserve native preparation"
            );
            assert_eq!(completion.disk.io_failures, request);
            assert!(!completion.disk.ready);
            assert!(completion
                .disk
                .last_failure
                .as_ref()
                .unwrap()
                .contains("create prerender cache"));
        }
        assert_eq!(
            serde_json::to_vec(session.engine.project()).unwrap(),
            before
        );
        assert_eq!(std::fs::read(sentinel).unwrap(), b"authored project bytes");
        assert_eq!(std::fs::read(blocked).unwrap(), b"preserve original file");
        worker.shutdown();
        let began = Instant::now();
        while !worker.stopped() {
            assert!(began.elapsed() < Duration::from_secs(10));
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
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed; 0 ignored"));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let faults: Vec<_> = file
        .lines()
        .filter(|line| line.contains("prerender disk ticket="))
        .collect();
    assert_eq!(
        faults.len(),
        2,
        "only first warning and one sustained error may be emitted"
    );
    assert!(faults[0].contains("consecutive=1 ") && faults[0].contains("cache="));
    assert!(faults[1].contains("consecutive=60 ") && faults[1].contains("create prerender cache"));
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("prerender disk ticket="))
            .count(),
        1
    );
    assert!(stderr.contains("consecutive=60 "));
    assert!(file.contains("start #1 prerender_worker operation=prepare"));
    assert!(!stderr.contains("prerender_worker") && !stdout.contains("prerender_worker"));
    assert!(!stdout.contains("prerender disk ticket="));
    if let Some(destination) = std::env::var_os("MOTION_PRERENDER_DISK_LOG_ARTIFACTS") {
        let destination = PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("default-file.log"), file).unwrap();
        std::fs::write(destination.join("default-stderr.log"), &output.stderr).unwrap();
        std::fs::write(destination.join("default-stdout.log"), &output.stdout).unwrap();
    }
}
