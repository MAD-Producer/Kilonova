use motion_core::{Content, Layer, Project, VideoAsset, VideoClip};
use motion_host::{
    video_frame::{DecodedFrame, VideoPixels},
    video_frames::VideoFrames,
    Platform, VideoDecoder,
};
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
    ready: Condvar,
    entered: Sender<()>,
    live: AtomicUsize,
    peak: AtomicUsize,
}
struct Stub(Arc<Gate>);
struct Decoder(Arc<Gate>);
impl Drop for Decoder {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::AcqRel);
    }
}
impl VideoDecoder for Decoder {
    fn frame(&mut self, _: u64, check: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<DecodedFrame> {
        self.0.entered.send(()).map_err(|e| e.to_string())?;
        let gate = self.0.open.lock().map_err(|_| "test gate poisoned")?;
        // Deliberately simulate a native call that cannot check cancellation.
        let (gate, timeout) = self
            .0
            .ready
            .wait_timeout_while(gate, Duration::from_secs(5), |open| !*open)
            .map_err(|_| "test gate poisoned")?;
        if timeout.timed_out() && !*gate {
            return Err("test decoder gate timed out".into());
        }
        drop(gate);
        check()?;
        Ok(DecodedFrame {
            pixels: VideoPixels::Rgba(vec![90; 32]),
            pts: 0,
            end: 100_000,
            width: 2,
            height: 4,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "fixture",
            decoder_name: "gated fixture".into(),
        })
    }
}
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "retirement-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(&self, _: &Path, _: VideoAsset, _: Vec<u64>) -> motion_media::Result<Box<dyn VideoDecoder>> {
        let live = self.0.live.fetch_add(1, Ordering::AcqRel) + 1;
        self.0.peak.fetch_max(live, Ordering::AcqRel);
        Ok(Box::new(Decoder(self.0.clone())))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&motion_host::VideoQuery>) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        Err("unused".into())
    }
}

fn project(root: &Path) -> Project {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/source.mp4"), b"123").unwrap();
    let asset = VideoAsset {
        id: 1,
        path: "assets/source.mp4".into(),
        bytes: 3,
        mime: "video/avc".into(),
        track: 0,
        width: 2,
        height: 4,
        rotation: 0,
        display_width: 2,
        display_height: 4,
        video_start_us: 0,
        video_end_us: 100_000,
        duration_us: 100_000,
        frame_count: 1,
        variable_frame_rate: false,
        nominal_frame_rate: 10.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    };
    let cache = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    motion_media::save_index(&cache, &asset, &[0]).unwrap();
    let mut p = Project::new(32, 32, 30, 3).unwrap();
    p.video_assets.push(asset);
    for id in 1..=8 {
        let mut layer = Layer::solid(id, "Video", [2., 4.], [16., 16., 0.], [1.; 4]);
        layer.content = Content::Video {
            video: VideoClip::new(1),
        };
        p.layers.push(layer);
    }
    p.validate().unwrap();
    p
}

#[test]
fn retiring_native_calls_keep_their_slots_without_blocking_or_spawning_more_decoders() {
    let dir = tempfile::tempdir().unwrap();
    let project = project(dir.path());
    let (entered, waiting) = channel();
    let gate = Arc::new(Gate {
        open: Mutex::new(false),
        ready: Condvar::new(),
        entered,
        live: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    });
    let mut reader = VideoFrames::new(Arc::new(Stub(gate.clone())));
    for id in 1..=4 {
        reader.request(&project, dir.path(), id, 0., id).unwrap();
    }
    for _ in 0..4 {
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    // Source replacement uses generation() too; it must retain the old slot.
    let mut changed = project.clone();
    let mut replacement = changed.video_assets[0].clone();
    replacement.id = 2;
    replacement.path = "assets/replacement.mp4".into();
    std::fs::write(dir.path().join(&replacement.path), b"123").unwrap();
    let index = motion_media::video_cache_path(dir.path(), &replacement).unwrap();
    motion_media::save_index(&index, &replacement, &[0]).unwrap();
    changed.video_assets.push(replacement);
    let Content::Video { video } = &mut changed.layers[0].content else {
        unreachable!()
    };
    video.asset = 2;
    changed.validate().unwrap();
    let replacement = reader.request(&changed, dir.path(), 1, 0., 5).unwrap();
    assert_eq!(replacement["reason"], "decoder_retirement");
    assert_eq!(reader.metrics()["streams"], 3);
    assert_eq!(reader.metrics()["retiringStreams"], 1);
    let began = Instant::now();
    reader.clear();
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(reader.metrics()["streams"], 0);
    assert_eq!(reader.metrics()["cacheBytes"], 0);
    assert_eq!(reader.metrics()["retiringStreams"], 4);
    for sequence in 10..110 {
        let result = reader
            .request(&project, dir.path(), 5, 0., sequence)
            .unwrap();
        assert_eq!(result["state"], "pending");
        assert_eq!(result["reason"], "decoder_retirement");
    }
    assert_eq!(gate.live.load(Ordering::Acquire), 4);
    *gate.open.lock().unwrap() = true;
    gate.ready.notify_all();
    for sequence in 110..170 {
        reader.clear();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline);
            let result = reader
                .request(&project, dir.path(), 5, 0., sequence)
                .unwrap();
            if result["state"] == "ready" {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = reader.frame(&project, 5, sequence).unwrap();
        assert_eq!(frame.rgba().unwrap().as_ref(), &[90; 32]);
        assert!(
            reader.metrics()["cacheBytes"].as_u64().unwrap()
                <= reader.metrics()["cacheBudgetBytes"].as_u64().unwrap()
        );
    }
    reader.clear();
    let deadline = Instant::now() + Duration::from_secs(5);
    while gate.live.load(Ordering::Acquire) != 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(gate.peak.load(Ordering::Acquire) <= 4);
    assert_eq!(reader.metrics()["cacheBytes"], 0);
}
