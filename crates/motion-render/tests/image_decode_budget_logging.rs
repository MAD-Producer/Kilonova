use motion_render::image_resources::{DecodeTask, Error, Resolution, Source};
use std::{
    sync::{Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};

struct Capture {
    records: Mutex<Vec<String>>,
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = format!("{} {}", record.level(), record.args());
        self.records.lock().unwrap().push(text.clone());
        if text.starts_with("INFO start image decode ") {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
    }
    fn flush(&self) {}
}

#[test]
fn info_start_precedes_real_decoder_access_and_does_not_require_debug() {
    // This binary has one test and owns its logger. The existing application
    // tests separately verify real file/Error-stderr/stdout routing.
    static LOGGER: OnceLock<Capture> = OnceLock::new();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let capture = LOGGER.get_or_init(|| Capture {
        records: Mutex::new(vec![]),
        entered,
        release: Mutex::new(released),
    });
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    image::save_buffer(
        root.path().join("assets/image.png"),
        &[31, 129, 221, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let source = Source::new(
        root.path(),
        &motion_core::Asset {
            id: 17,
            path: "assets/image.png".into(),
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    let mut task = DecodeTask::default();
    task.start_with_budget(source.clone(), Resolution::Full, false, 4)
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(task.busy());
    assert!(
        task.poll().is_none(),
        "decoder returned before its start record"
    );
    let records = capture.records.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert!(
        records[0].contains("source_id=17")
            && records[0].contains("resolution=Full")
            && records[0].contains("decoded_budget_bytes=4")
            && records[0].contains("speculative=false")
    );
    // Release into the real file decoder after removing the input. Its native
    // NotFound result proves decoding had not already happened before start.
    std::fs::remove_file(&source.path).unwrap();
    release.send(()).unwrap();
    let began = Instant::now();
    loop {
        if let Some((_, _, _, result)) = task.poll() {
            assert!(
                matches!(result, Err(Error::Io(cause)) if cause.kind() == std::io::ErrorKind::NotFound)
            );
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        capture.records.lock().unwrap().len(),
        1,
        "Debug completion leaked at Info"
    );
}
