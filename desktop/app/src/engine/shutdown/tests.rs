use super::*;
use motion_host::platform::{SurfaceTarget, VideoDecoder, VideoQuery};
use std::{fs, path::Path, sync::mpsc};

struct BlockingPlatform {
    base: Arc<dyn Platform>,
    entered: mpsc::Sender<()>,
    gate: Mutex<mpsc::Receiver<()>>,
}
impl Platform for BlockingPlatform {
    fn name(&self) -> &'static str {
        self.base.name()
    }
    fn probe_video(
        &self,
        path: &Path,
        track: Option<u32>,
        audio: Option<u32>,
        check: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        self.base.probe_video(path, track, audio, check)
    }
    fn open_decoder(
        &self,
        path: &Path,
        asset: motion_core::VideoAsset,
        pts: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        self.base.open_decoder(path, asset, pts)
    }
    fn decode_audio(
        &self,
        path: &Path,
        pcm: &Path,
        track: Option<u32>,
        limit: u64,
        check: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        self.entered.send(()).unwrap();
        self.gate
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        self.base.decode_audio(path, pcm, track, limit, check)
    }
    fn media_capabilities(&self, query: Option<&VideoQuery>) -> motion_media::Result<Value> {
        self.base.media_capabilities(query)
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
        _budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        self.base.attach_surface(window, width, height, _budget)
    }
}

struct CapturePanic;
impl Drop for CapturePanic {
    fn drop(&mut self) {
        panic!("original video callback capture destructor panic");
    }
}

#[test]
fn real_shutdown_publishes_before_join_and_preserves_default_error_channels() {
    const CHILD: &str = "MOTION_MEDIA_SHUTDOWN_CHILD";
    const TEST: &str = "engine::shutdown::tests::real_shutdown_publishes_before_join_and_preserves_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let platform = Arc::new(BlockingPlatform {
            base: motion_desktop_media::platform(),
            entered,
            gate: Mutex::new(gate),
        });
        let project = root.join("project");
        let engine = Engine::start(project.clone(), platform, 8 << 30).unwrap();
        engine.save().unwrap();
        let saved = fs::read(project.join("project.json")).unwrap();
        let input = root.join("input.wav");
        fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../crates/motion-media/tests/fixtures/tone-stereo-48000.wav"
            ),
            &input,
        )
        .unwrap();
        engine.media(serde_json::json!({"op":"probe_media","request_id":"shutdown-owned-audio","path":input,"kind":"audio"}).to_string()).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        engine.request_shutdown().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = None;
        while Instant::now() < deadline {
            if let Some((operation, elapsed)) = engine.in_flight() {
                if operation.contains("shutdown_cleanup") {
                    observed = Some((operation, elapsed));
                    break;
                }
            }
            std::thread::yield_now();
        }
        let join_marker =
            "media join start request=shutdown-owned-audio operation=probe_audio phase=shutdown";
        while Instant::now() < deadline
            && !fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .contains(join_marker)
        {
            std::thread::yield_now();
        }
        let before_release = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let was_running = !engine.worker.as_ref().unwrap().is_finished();
        let (progress, progressed) = mpsc::channel();
        let other_root = root.join("other-project");
        let other = std::thread::spawn(move || {
            let other =
                Engine::start(other_root, motion_desktop_media::platform(), 8 << 30).unwrap();
            other.save().unwrap();
            drop(other);
            progress.send(()).unwrap();
        });
        let independent = progressed.recv_timeout(Duration::from_secs(5));
        // Release before every assertion so a failed observation does not keep
        // native resources parked or deadlock Engine::Drop in this test.
        release.send(()).unwrap();
        other.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !engine.worker.as_ref().unwrap().is_finished() {
            assert!(
                Instant::now() < deadline,
                "shutdown did not join the media worker"
            );
            std::thread::yield_now();
        }
        assert!(observed.is_some() && was_running);
        assert!(before_release.contains(join_marker));
        assert!(before_release.contains(" shutdown_cleanup session="));
        independent.unwrap();
        assert!(engine.in_flight().is_none());
        assert_eq!(fs::read(project.join("project.json")).unwrap(), saved);
        assert!(fs::read_dir(&project).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".media-import-")));
        let reopened = Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
        reopened.state().unwrap();
        drop(reopened);
        drop(engine);

        // The public video worker catches the primary probe error. Its final
        // callback capture then panics outside that boundary; join must report
        // this secondary failure without replacing the task's original cause.
        let jobs = motion_media::VideoJobs::new(root.clone()).unwrap();
        let bomb = CapturePanic;
        jobs.start(
            move || Ok((Box::new(fs::File::open(input).unwrap()), None)),
            motion_media::VideoImportOptions {
                request_id: "shutdown-uncaught-video".into(),
                at_frame: 0,
                name: "video".into(),
                track: None,
                audio_track: None,
                with_audio: false,
            },
            true,
            Arc::new(move |_, _, _, _| {
                let _retain = &bomb;
                Err("original rejected video probe".into())
            }),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let failed = loop {
            let status = jobs.status("shutdown-uncaught-video").unwrap();
            if status.state == "failed" {
                break status;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(
            failed.error.as_deref(),
            Some("original rejected video probe")
        );
        fs::write(root.join("primary-cause.txt"), failed.error.unwrap()).unwrap();
        jobs.release("shutdown-uncaught-video").unwrap();
        drop(jobs);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
    let file = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    for (phase, cause) in [
        ("failed", "original rejected video probe"),
        (
            "shutdown",
            "original video callback capture destructor panic",
        ),
    ] {
        let marker = format!(
            "media error request=shutdown-uncaught-video operation=probe_video phase={phase} "
        );
        let disk: Vec<_> = file.lines().filter(|line| line.contains(&marker)).collect();
        let console: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(&marker))
            .collect();
        assert_eq!(disk.len(), 1, "{file}");
        assert_eq!(console.len(), 1, "{stderr}");
        assert!(disk[0].contains(cause) && console[0].contains(cause));
        assert_eq!(
            disk[0].split_once(&marker).unwrap().1,
            console[0].split_once(&marker).unwrap().1
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join("primary-cause.txt")).unwrap(),
        "original rejected video probe"
    );
    assert!(!file
        .lines()
        .any(|line| line.contains("ERROR") && line.contains("shutdown-owned-audio")));
    assert!(!stderr.contains("media join start") && !stderr.contains("start #"));
    assert!(
        !stdout.contains("media join")
            && !stdout.contains("media error")
            && !stdout.contains("original video callback")
    );
}
