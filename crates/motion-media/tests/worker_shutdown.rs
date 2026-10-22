use motion_media::{AudioJobs, ImportOptions, Limits, PackageJobs, VideoImportOptions, VideoJobs};
use motion_model::PackageExportSource;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(5);

struct BlockedReader {
    file: Option<File>,
    entered: Sender<()>,
    release: Receiver<()>,
    closed: Arc<AtomicBool>,
}
impl Read for BlockedReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        self.file.as_mut().unwrap().read(bytes)
    }
}
impl Drop for BlockedReader {
    fn drop(&mut self) {
        drop(self.file.take());
        self.closed.store(true, Ordering::Release);
    }
}

fn source(root: &Path) -> (BlockedReader, Receiver<()>, Sender<()>, Arc<AtomicBool>) {
    let path = root.join("source.bin");
    fs::write(&path, b"owned source held during a real read").unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let closed = Arc::new(AtomicBool::new(false));
    (
        BlockedReader {
            file: Some(File::open(path).unwrap()),
            entered,
            release: gate,
            closed: closed.clone(),
        },
        waiting,
        release,
        closed,
    )
}

// Always release and join before asserting: the negative control must not leave
// its actual blocked worker behind or deadlock during a destructor assertion.
fn drop_while_blocked<T: Send + 'static>(jobs: T, release: Sender<()>) -> bool {
    let (entered, waiting) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let dropper = std::thread::spawn(move || {
        entered.send(()).unwrap();
        drop(jobs);
        done.send(()).unwrap();
    });
    waiting.recv_timeout(WAIT).unwrap();
    let premature = match finished.recv_timeout(Duration::from_millis(200)) {
        Ok(()) => true,
        Err(mpsc::RecvTimeoutError::Timeout) => false,
        Err(error) => panic!("dropper disconnected: {error}"),
    };
    release.send(()).unwrap();
    if !premature {
        finished.recv_timeout(WAIT).unwrap();
    }
    dropper.join().unwrap();
    premature
}

fn no_staging(root: &Path) -> bool {
    fs::read_dir(root).unwrap().all(|entry| {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        !name.starts_with(".media-import-")
            && !name.starts_with(".video-import-")
            && !name.starts_with(".ms-package-")
    })
}

fn audio(released: bool) {
    let root = tempfile::tempdir().unwrap();
    let (reader, entered, release, closed) = source(root.path());
    let jobs = AudioJobs::new(root.path().into(), Limits::default()).unwrap();
    jobs.start_with_source(
        move || Ok((Box::new(reader), None)),
        ImportOptions {
            request_id: "shutdown-audio".into(),
            at_frame: 0,
            track: None,
            name: "audio".into(),
        },
        true,
    )
    .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    assert!(!closed.load(Ordering::Acquire));
    if released {
        assert_eq!(jobs.cancel("shutdown-audio").unwrap().state, "cancelled");
        jobs.release("shutdown-audio").unwrap();
        assert!(!jobs.contains("shutdown-audio"));
    }
    let premature = drop_while_blocked(jobs, release);
    // The original detached implementation can still be unwinding here.
    let began = std::time::Instant::now();
    while !closed.load(Ordering::Acquire) && began.elapsed() < WAIT {
        std::thread::yield_now();
    }
    assert!(
        !premature,
        "AudioJobs dropped while its source read was blocked"
    );
    assert!(closed.load(Ordering::Acquire));
    assert!(no_staging(root.path()));
}

#[test]
fn audio_drop_joins_the_blocked_source_and_removes_staging() {
    audio(false);
}

#[test]
fn released_audio_cancellation_keeps_its_worker_owned_until_shutdown() {
    audio(true);
}

#[test]
fn audio_cache_drop_joins_the_decoder_and_removes_staging() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("assets")).unwrap();
    let source = root.path().join("assets/source.wav");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tone-stereo-48000.wav"
        ),
        &source,
    )
    .unwrap();
    let mut asset = motion_media::decode_audio(
        &source,
        &root.path().join("original.pcm"),
        None,
        1 << 20,
        &mut |_| Ok(()),
    )
    .unwrap();
    asset.id = 1;
    asset.path = "assets/source.wav".into();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = std::sync::Mutex::new(gate);
    let jobs = AudioJobs::with_decoder(
        root.path().into(),
        Limits::default(),
        Arc::new(move |source, pcm, track, limit, check| {
            entered.send(()).unwrap();
            gate.lock().unwrap().recv_timeout(WAIT).unwrap();
            motion_media::decode_audio(source, pcm, track, limit, check)
        }),
    )
    .unwrap();
    jobs.prepare_cache("shutdown-cache", asset).unwrap();
    waiting.recv_timeout(WAIT).unwrap();
    let premature = drop_while_blocked(jobs, release);
    assert!(
        !premature,
        "AudioJobs dropped while its cache decoder was blocked"
    );
    assert!(no_staging(root.path()));
}

fn video(released: bool) {
    let root = tempfile::tempdir().unwrap();
    let (reader, entered, release, closed) = source(root.path());
    let jobs = VideoJobs::new(root.path().into()).unwrap();
    jobs.start(
        move || Ok((Box::new(reader), None)),
        VideoImportOptions {
            request_id: "shutdown-video".into(),
            at_frame: 0,
            name: "video".into(),
            track: None,
            audio_track: None,
            with_audio: false,
        },
        true,
        Arc::new(|_, _, _, _| panic!("cancelled copy must not reach the probe")),
    )
    .unwrap();
    entered.recv_timeout(WAIT).unwrap();
    if released {
        assert_eq!(jobs.cancel("shutdown-video").unwrap().state, "cancelled");
        jobs.release("shutdown-video").unwrap();
        assert!(!jobs.contains("shutdown-video"));
    }
    let premature = drop_while_blocked(jobs, release);
    let began = std::time::Instant::now();
    while !closed.load(Ordering::Acquire) && began.elapsed() < WAIT {
        std::thread::yield_now();
    }
    assert!(
        !premature,
        "VideoJobs dropped while its source read was blocked"
    );
    assert!(closed.load(Ordering::Acquire));
    assert!(no_staging(root.path()));
}

#[test]
fn video_drop_joins_the_blocked_source_and_removes_staging() {
    video(false);
}

#[test]
fn released_video_cancellation_keeps_its_worker_owned_until_shutdown() {
    video(true);
}

struct BlockedSnapshot {
    snapshot: motion_core::storage::PackageSnapshot,
    entered: Sender<()>,
    release: Receiver<()>,
}
impl PackageExportSource for BlockedSnapshot {
    type Prepared = motion_core::storage::PreparedPackage;
    fn total_bytes(&self) -> u64 {
        self.snapshot.total_bytes()
    }
    fn source_count(&self) -> usize {
        self.snapshot.source_count()
    }
    fn prepare(
        self,
        output: &Path,
        progress: impl FnMut(u64, u64) -> motion_model::Result<()>,
    ) -> motion_model::Result<Self::Prepared> {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        self.snapshot.prepare(output, progress)
    }
}

#[test]
fn package_drop_joins_owned_frozen_sources_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let project = motion_model::Project::new(32, 32, 30, 30).unwrap();
    let snapshot = motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let output = root.path().join("output.msproj");
    fs::write(&output, b"original destination").unwrap();
    let jobs = PackageJobs::default();
    jobs.start_export(
        "shutdown-package",
        BlockedSnapshot {
            snapshot,
            entered,
            release: gate,
        },
        output.clone(),
        0,
    )
    .unwrap();
    waiting.recv_timeout(WAIT).unwrap();
    let premature = drop_while_blocked(jobs, release);
    assert!(
        !premature,
        "PackageJobs dropped while frozen source preparation was blocked"
    );
    assert_eq!(fs::read(&output).unwrap(), b"original destination");
    assert!(no_staging(root.path()));
}
