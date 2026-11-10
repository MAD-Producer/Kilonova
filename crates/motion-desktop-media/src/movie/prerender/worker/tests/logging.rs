use super::*;
#[test]
fn worker_start_flushes_before_entering_the_controlled_handler() {
    use std::io::{BufWriter, Write};
    const NAME: &str = "movie::prerender::worker::tests::logging::worker_start_flushes_before_entering_the_controlled_handler";
    const CHILD: &str = "MOTION_PRERENDER_BUFFERED_LOG_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        struct Buffered(Mutex<BufWriter<std::fs::File>>);
        impl log::Log for Buffered {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() <= log::Level::Info
            }
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    writeln!(
                        self.0.lock().unwrap(),
                        "{} {}",
                        record.level(),
                        record.args()
                    )
                    .unwrap();
                }
            }
            fn flush(&self) {
                self.0.lock().unwrap().flush().unwrap();
            }
        }
        let path = PathBuf::from(path);
        let logger = Box::leak(Box::new(Buffered(Mutex::new(BufWriter::new(
            std::fs::File::create(&path).unwrap(),
        )))));
        log::set_logger(logger).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let root = tempfile::tempdir().unwrap();
        let session = session(root.path());
        let (entered, waiting) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let released = Mutex::new(released);
        let mut worker = Worker::start_with_phase_hook(3072, move |_, phase| {
            if phase == "start" {
                entered.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
            }
        })
        .unwrap();
        let id = worker.prepare(plan(&session)).unwrap().unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let flight = worker.observation().snapshot().unwrap().in_flight.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path.with_extension("before-handler.log"), &text).unwrap();
        // Release before assertions so a broken flush guard cannot strand a worker.
        release.send(()).unwrap();
        let completed = poll(&mut worker);
        stop(&mut worker);
        assert_eq!(flight.id, id);
        assert_eq!(flight.operation, "prepare");
        assert_eq!(flight.phase, "start");
        assert!(
            text.contains(&format!(
                "INFO start #{id} prerender_worker operation=prepare"
            )),
            "{text}"
        );
        assert!(
            !text.contains("prerender_prepare"),
            "handler ran before the controlled block"
        );
        assert!(matches!(completed.outcome, Outcome::Prepared(_)));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, root.path().join("buffered.log"))
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("INFO start #"));
    if let Some(directory) = std::env::var_os("MOTION_PRERENDER_BUFFERED_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(
            root.path().join("buffered.before-handler.log"),
            directory.join("before-handler.log"),
        )
        .unwrap();
        std::fs::write(directory.join("stdout.log"), output.stdout).unwrap();
        std::fs::write(directory.join("stderr.log"), output.stderr).unwrap();
    }
}
