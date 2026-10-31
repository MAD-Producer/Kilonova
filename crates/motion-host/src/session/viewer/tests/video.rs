use super::*;
use crate::{
    video_frame::{DecodedFrame, RgbEncoding, VideoPixels},
    VideoDecoder, VideoQuery,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

struct Control {
    entered: mpsc::Sender<()>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
    cancelled: AtomicUsize,
}
struct Backend(Arc<Control>);
struct Decoder(Arc<Control>);
impl Platform for Backend {
    fn name(&self) -> &'static str {
        "controlled-view-shutter"
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
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
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
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> Result<SurfaceTarget> {
        Err("unused".into())
    }
}
impl VideoDecoder for Decoder {
    fn rgb_encoding(&self) -> motion_media::Result<RgbEncoding> {
        Ok(RgbEncoding::Srgb)
    }
    fn frame(
        &mut self,
        time: u64,
        check: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<DecodedFrame> {
        check()?;
        if time == 325_000 {
            let release = self.0.release.lock().unwrap().take();
            if let Some(release) = release {
                self.0.entered.send(()).map_err(|e| e.to_string())?;
                let until = Instant::now() + Duration::from_secs(30);
                loop {
                    match release.recv_timeout(Duration::from_millis(1)) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err("controlled decoder release lost".into())
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Err(error) = check() {
                        self.0.cancelled.fetch_add(1, Ordering::Relaxed);
                        return Err(error);
                    }
                    if Instant::now() >= until {
                        return Err("controlled decoder release deadline exceeded".into());
                    }
                }
            }
        }
        check()?;
        let index = (time * 120).div_ceil(1_000_000);
        Ok(DecodedFrame {
            pixels: VideoPixels::Rgba([index as u8 * 2, 32, 128, 255].repeat(32 * 32)),
            pts: time,
            end: ((index + 1) * 1_000_000 / 120).min(1_000_000),
            width: 32,
            height: 32,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "authored-rgba",
            decoder_name: "controlled shutter".into(),
        })
    }
}
fn project(root: &Path) -> Project {
    let asset:motion_core::VideoAsset=serde_json::from_value(json!({
        "id":1,"path":"assets/clock.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":32,"height":32,"rotation":0,"display_width":32,"display_height":32,
        "video_start_us":0,"video_end_us":1000000,"duration_us":1000000,"frame_count":120,
        "variable_frame_rate":false,"nominal_frame_rate":120.,"color_standard":1,"color_range":2,"audio_asset":null
    })).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join(&asset.path), b"1234").unwrap();
    let path = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    motion_media::save_index(
        &path,
        &asset,
        &(0..120).map(|i| i * 1_000_000 / 120).collect::<Vec<_>>(),
    )
    .unwrap();
    let mut project = Project::new(32, 32, 30, 30).unwrap();
    project.video_assets.push(asset);
    project
        .layers
        .push(Layer::camera(1, "Video camera A", 32, 32));
    project
        .layers
        .push(Layer::camera(2, "Video camera B", 32, 32));
    let mut layer = Layer::solid(7, "Video shutter", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: motion_core::VideoClip::new(1),
    };
    layer.sampling.motion_blur = Some(Shutter {
        frames: 2.,
        samples: 4,
    });
    layer.sampling.frame_blend = motion_core::render_sampling::FrameBlend::Linear;
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires native Windows surface and GPU"
)]
fn native_video_wait_keeps_whole_group_and_never_supersedes_another_cells_shutter() {
    if !child("session::viewer::tests::video::native_video_wait_keeps_whole_group_and_never_supersedes_another_cells_shutter",
        "MOTION_VIEW_VIDEO_CHILD") {return;}
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let control = Arc::new(Control {
        entered: entered_tx,
        release: Mutex::new(Some(release_rx)),
        cancelled: AtomicUsize::new(0),
    });
    let mut session = Session::new(
        project.clone(),
        root.path().into(),
        Arc::new(Backend(control.clone())),
    )
    .unwrap();
    attach(&mut session, &events);
    let observer = Observer::new(32, 32);
    let cells = views(&observer);
    ready(&mut session, 12, 6., &cells);
    let before: Vec<_> = (0..4)
        .map(|i| pixels(session.graphics.as_ref().unwrap(), i))
        .collect();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(!session
            .render_views_with_overlay(12, 10., &cells, |_, _, _| {})
            .unwrap());
        if entered_rx.try_recv().is_ok() {
            break;
        }
        assert!(
            Instant::now() < until,
            "controlled video decode never entered"
        );
        std::thread::yield_now();
    }
    for _ in 0..8 {
        assert!(!session
            .render_views_with_overlay(12, 10., &cells, |_, _, _| {})
            .unwrap());
        assert_eq!(
            control.cancelled.load(Ordering::Relaxed),
            0,
            "sibling shutter superseded the held video read"
        );
    }
    let request = session
        .graphics
        .as_ref()
        .unwrap()
        .views
        .request
        .as_ref()
        .unwrap();
    assert!(request.tasks[1..]
        .iter()
        .all(|task| task.progress().work_units == 0));
    for (i, bytes) in before.iter().enumerate() {
        assert_eq!(&pixels(session.graphics.as_ref().unwrap(), i), bytes);
    }
    release_tx.send(()).unwrap();
    ready(&mut session, 12, 10., &cells);
    assert_eq!(control.cancelled.load(Ordering::Relaxed), 0);
    assert_eq!(session.video_frames.metrics()["cancelledFrames"], 0);
    let after: Vec<_> = (0..4)
        .map(|i| pixels(session.graphics.as_ref().unwrap(), i))
        .collect();
    assert_ne!(before, after);
    assert!(after.iter().all(|image| image == &after[0]));
    assert_eq!(session.engine.project(), &project);
}
