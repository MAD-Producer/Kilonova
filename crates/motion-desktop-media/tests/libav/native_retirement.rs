use motion_core::{AudioAsset, Content, Layer, Project, VideoAsset, VideoClip};
use motion_desktop_media::DesktopPlatform;
use motion_host::{
    platform::{Platform, SurfaceTarget, VideoDecoder, VideoQuery},
    video_frame::DecodedFrame,
    video_frames::VideoFrames,
};
use motion_media::{Result, VideoProbe};
use serde_json::Value;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{channel, Sender},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
    entered: Sender<()>,
    live: AtomicUsize,
    peak: AtomicUsize,
    closed: AtomicUsize,
}

impl Gate {
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

// Release blocked workers on a failed assertion too; their ordinary five-second
// deadline remains a test failure, never a hidden successful fallback.
struct ReleaseOnDrop(Arc<Gate>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct NativePlatform(Arc<Gate>);
struct NativeDecoder {
    native: Option<Box<dyn VideoDecoder>>,
    gate: Arc<Gate>,
}
impl Drop for NativeDecoder {
    fn drop(&mut self) {
        // Count closure only after the real FFmpeg decoder/file context drops.
        drop(self.native.take());
        self.gate.closed.fetch_add(1, Ordering::AcqRel);
        self.gate.live.fetch_sub(1, Ordering::AcqRel);
    }
}
impl VideoDecoder for NativeDecoder {
    fn frame(&mut self, time: u64, check: &dyn Fn() -> Result<()>) -> Result<DecodedFrame> {
        let frame = self.native.as_mut().unwrap().frame(time, check)?;
        // The actual file is open and its first frame has decoded. Hold those
        // native resources while the caller clears and attempts replacement.
        self.gate.entered.send(()).map_err(|e| e.to_string())?;
        let open = self
            .gate
            .open
            .lock()
            .map_err(|_| "native test gate poisoned")?;
        let (open, deadline) = self
            .gate
            .changed
            .wait_timeout_while(open, Duration::from_secs(5), |open| !*open)
            .map_err(|_| "native test gate poisoned")?;
        if deadline.timed_out() && !*open {
            return Err("native test gate timed out".into());
        }
        drop(open);
        check()?;
        Ok(frame)
    }
    fn seeks(&self) -> u64 {
        self.native.as_ref().unwrap().seeks()
    }
}
impl Platform for NativePlatform {
    fn name(&self) -> &'static str {
        "counted-libav"
    }
    fn probe_video(
        &self,
        path: &Path,
        video: Option<u32>,
        audio: Option<u32>,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<VideoProbe> {
        DesktopPlatform::new().probe_video(path, video, audio, check)
    }
    fn open_decoder(
        &self,
        path: &Path,
        asset: VideoAsset,
        pts: Vec<u64>,
    ) -> Result<Box<dyn VideoDecoder>> {
        let native = DesktopPlatform::new().open_decoder(path, asset, pts)?;
        let live = self.0.live.fetch_add(1, Ordering::AcqRel) + 1;
        self.0.peak.fetch_max(live, Ordering::AcqRel);
        Ok(Box::new(NativeDecoder {
            native: Some(native),
            gate: self.0.clone(),
        }))
    }
    fn decode_audio(
        &self,
        path: &Path,
        pcm: &Path,
        track: Option<u32>,
        max: u64,
        check: &mut dyn FnMut(Option<f64>) -> Result<()>,
    ) -> Result<AudioAsset> {
        DesktopPlatform::new().decode_audio(path, pcm, track, max, check)
    }
    fn media_capabilities(&self, query: Option<&VideoQuery>) -> Result<Value> {
        DesktopPlatform::new().media_capabilities(query)
    }
    fn attach_surface(
        &self,
        window: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        width: u32,
        height: u32,
    ) -> motion_host::Result<SurfaceTarget> {
        DesktopPlatform::new().attach_surface(window, width, height)
    }
}

#[test]
fn real_libav_decoders_retain_retiring_slots_then_close_and_reopen_without_losing_pixels() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let source = root.path().join("assets/source.mp4");
    std::fs::copy(super::fixture("silent-24fps.mp4"), &source).unwrap();
    let mut probe = DesktopPlatform::new()
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root.path(), &probe.asset).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let mut project = Project::new(32, 32, 24, 72).unwrap();
    project.video_assets.push(probe.asset.clone());
    for id in 1..=5 {
        let mut layer = Layer::solid(id, "Video", [32.; 2], [16., 16., 0.], [1.; 4]);
        layer.content = Content::Video {
            video: VideoClip::new(1),
        };
        project.layers.push(layer);
    }
    project.validate().unwrap();
    let (entered, waiting) = channel();
    let gate = Arc::new(Gate {
        open: Mutex::new(false),
        changed: Condvar::new(),
        entered,
        live: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        closed: AtomicUsize::new(0),
    });
    let mut frames = VideoFrames::new(Arc::new(NativePlatform(gate.clone())));
    let _release = ReleaseOnDrop(gate.clone());
    for id in 1..=4 {
        frames.request(&project, root.path(), id, 0., id).unwrap();
    }
    for _ in 0..4 {
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert_eq!(gate.live.load(Ordering::Acquire), 4);
    assert_eq!(gate.closed.load(Ordering::Acquire), 0);
    frames.clear();
    assert_eq!(frames.metrics()["streams"], 0);
    assert_eq!(frames.metrics()["retiringStreams"], 4);
    for sequence in 10..70 {
        let result = frames
            .request(&project, root.path(), 5, 0., sequence)
            .unwrap();
        assert_eq!(result["state"], "pending");
        assert_eq!(result["reason"], "decoder_retirement");
        assert_eq!(gate.live.load(Ordering::Acquire), 4);
        assert_eq!(gate.closed.load(Ordering::Acquire), 0);
    }
    gate.release();
    frames.finish().unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 0);
    assert_eq!(gate.closed.load(Ordering::Acquire), 4);
    assert_eq!(frames.metrics()["retiringStreams"], 0);

    for sequence in 70..78 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = frames
                .request(&project, root.path(), 5, 0., sequence)
                .unwrap();
            assert_ne!(result["state"], "failed", "{result}");
            if result["state"] == "ready" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native frame never became ready: {result}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = frames.frame(&project, 5, sequence).unwrap();
        assert_eq!(frame.pts, probe.timestamps[0]);
        assert_eq!(frame.rgba().unwrap().as_ref(), probe.first_rgba.as_slice());
        frames.finish().unwrap();
        assert_eq!(gate.live.load(Ordering::Acquire), 0);
        assert_eq!(frames.metrics()["streams"], 0);
        assert_eq!(frames.metrics()["retiringStreams"], 0);
        assert_eq!(frames.metrics()["cacheBytes"], 0);
    }
    assert_eq!(gate.closed.load(Ordering::Acquire), 12);
    assert_eq!(gate.peak.load(Ordering::Acquire), 4);
    // Windows also rejects this if a native input still holds an incompatible
    // file handle. This is an extra OS-level check, not the portable counter.
    std::fs::rename(&source, root.path().join("assets/closed.mp4")).unwrap();
}
