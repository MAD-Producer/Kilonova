use super::*;
use crate::video_frame::VideoPixels;
use motion_core::{render_sampling::FrameBlend, Layer, VideoClip};
use motion_render::Renderer;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

const TIMES: [u64; 3] = [100_000, 170_000, 400_000];
struct Control {
    fail_right: AtomicBool,
    calls: Mutex<Vec<u64>>,
    entered: Option<mpsc::Sender<()>>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
}
struct Backend(Arc<Control>);
struct Decoder(Arc<Control>);
impl crate::Platform for Backend {
    fn name(&self) -> &'static str {
        "authored-vfr"
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
    fn media_capabilities(&self, _: Option<&crate::VideoQuery>) -> motion_media::Result<Value> {
        Err("unused".into())
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _budget: motion_render::AllocationBudget,
    ) -> crate::Result<crate::SurfaceTarget> {
        Err("unused".into())
    }
}
impl VideoDecoder for Decoder {
    fn frame(&mut self, time: u64, check: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<DecodedFrame> {
        check()?;
        self.0.calls.lock().unwrap().push(time);
        if time == TIMES[1] {
            if let Some(release) = self.0.release.lock().unwrap().take() {
                self.0
                    .entered
                    .as_ref()
                    .unwrap()
                    .send(())
                    .map_err(|e| e.to_string())?;
                release
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|e| e.to_string())?;
            }
            check()?;
            if self.0.fail_right.load(Ordering::Acquire) {
                return Err("controlled required-right decode failure".into());
            }
        }
        let index = TIMES
            .iter()
            .position(|pts| *pts == time)
            .ok_or("unknown fixture PTS")?;
        let rgba = [[0, 0, 0, 255], [255; 4], [255, 0, 0, 255]][index];
        Ok(DecodedFrame {
            pixels: VideoPixels::Rgba(rgba.repeat(256)),
            pts: time,
            end: TIMES.get(index + 1).copied().unwrap_or(500_000),
            width: 16,
            height: 16,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "authored-rgba",
            decoder_name: "controlled VFR".into(),
        })
    }
}
fn control() -> Arc<Control> {
    Arc::new(Control {
        fail_right: AtomicBool::new(false),
        calls: Mutex::new(vec![]),
        entered: None,
        release: Mutex::new(None),
    })
}
fn fixture(root: &Path) -> Project {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/vfr.mp4"), b"1234").unwrap();
    let asset: VideoAsset = serde_json::from_value(json!({
        "id":1,"path":"assets/vfr.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":16,"height":16,"rotation":0,"display_width":16,"display_height":16,
        "video_start_us":100000,"video_end_us":500000,"duration_us":500000,
        "frame_count":3,"variable_frame_rate":true,"nominal_frame_rate":24.,
        "color_standard":1,"color_range":2,"audio_asset":null
    }))
    .unwrap();
    let path = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    motion_media::save_index(&path, &asset, &TIMES).unwrap();
    let mut p = Project::new(16, 16, 100, 60).unwrap();
    p.version = motion_core::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    p.video_assets.push(asset);
    let mut layer = Layer::solid(7, "Authored VFR", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 50,
        offset_frame: 0,
    });
    layer.sampling.frame_blend = FrameBlend::Linear;
    p.layers.push(layer);
    p.validate().unwrap();
    p
}
fn scene(p: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(p).unwrap();
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn exact(
    reader: &mut VideoFrames,
    p: &Project,
    root: &Path,
    scene: &Scene,
) -> Vec<(u64, PreparedVideoFrame)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(frames) = reader.prepare_scene_exact(p, root, scene).unwrap() {
            return frames;
        }
        assert!(Instant::now() < deadline, "{}", reader.metrics());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn exact_vfr_pair_matches_gpu_preview_and_frozen_bytes_without_nominal_fps_or_weight_redecoding() {
    let dir = tempfile::tempdir().unwrap();
    let p = fixture(dir.path());
    let control = control();
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let scene = scene(&p, 13.5); // 135ms: half way between 100ms and 170ms.
    let frames = exact(&mut reader, &p, dir.path(), &scene);
    let sample = &frames[0].1;
    assert_eq!(sample.left.pts, 100_000);
    assert_eq!(sample.next.as_ref().unwrap().0.pts, 170_000);
    assert_eq!(sample.next.as_ref().unwrap().1, 0.5);
    assert_eq!(*control.calls.lock().unwrap(), [100_000, 170_000]);
    let frozen = sample.decoded().unwrap();
    let frozen_pixels = frozen.rgba().unwrap();
    assert_eq!(&frozen_pixels[..4], &[188, 188, 188, 255]);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    sample.upload(&mut renderer, 7, 1).unwrap();
    let target = renderer.capture_target(16, 16).unwrap();
    let gpu = renderer.capture(&scene, &target).unwrap().0;
    assert!(gpu
        .iter()
        .zip(frozen_pixels.iter())
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    let uploads = renderer.video_uploads;
    let next_scene = self::scene(&p, 11.75);
    let next = exact(&mut reader, &p, dir.path(), &next_scene);
    next[0].1.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(renderer.video_uploads, uploads);
    assert_eq!(control.calls.lock().unwrap().len(), 2);
    let pixels = renderer.capture(&next_scene, &target).unwrap().0;
    assert_eq!(&pixels[..4], &[137, 137, 137, 255]);
    // Full and proxy paths must not alias the same source/PTS at different sizes.
    let mut cache = crate::preview_video::PreviewVideoCache::default();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(cache
        .upload_prepared(&mut renderer, 7, 1, sample, Some(8), false, &mut encoder)
        .unwrap()
        .is_none());
    assert_eq!(renderer.video_blend_source_size(7, 0), Some((8, 8)));
    let proxy = renderer.capture(&scene, &target).unwrap().0;
    assert!(gpu.iter().zip(&proxy).all(|(a, b)| a.abs_diff(*b) <= 1));
    sample.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(renderer.video_blend_source_size(7, 0), Some((16, 16)));
    assert_eq!(gpu, renderer.capture(&scene, &target).unwrap().0);
    renderer.check_health().unwrap();
    let uploads = renderer.video_uploads;
    let forward_scene = self::scene(&p, 28.5);
    let forward = exact(&mut reader, &p, dir.path(), &forward_scene);
    forward[0].1.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(
        renderer.video_uploads,
        uploads + 1,
        "reuse former right as left"
    );
    assert_eq!(
        &renderer.capture(&forward_scene, &target).unwrap().0[..4],
        &[255, 188, 188, 255]
    );
    let backward = exact(&mut reader, &p, dir.path(), &scene);
    backward[0].1.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(
        renderer.video_uploads,
        uploads + 2,
        "reuse former left as right"
    );
    assert_eq!(gpu, renderer.capture(&scene, &target).unwrap().0);
}

#[test]
fn boundaries_offset_trim_and_old_hold_keep_their_source_clock() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = fixture(dir.path());
    let mut reader = VideoFrames::new(Arc::new(Backend(control())));
    assert!(exact(&mut reader, &p, dir.path(), &scene(&p, 9.9)).is_empty());
    for (time, pts) in [(10., 100_000), (17., 170_000), (49.9, 400_000)] {
        let frames = exact(&mut reader, &p, dir.path(), &scene(&p, time));
        assert_eq!(frames[0].1.left.pts, pts);
        assert!(frames[0].1.next.is_none());
    }
    assert!(exact(&mut reader, &p, dir.path(), &scene(&p, 50.)).is_empty());
    p.layers[0].timeline = Some(motion_core::LayerTimeline {
        in_frame: 20,
        out_frame: 40,
        offset_frame: 10,
    });
    if let Content::Video { video } = &mut p.layers[0].content {
        video.source_offset_us = 35_000;
    }
    p.validate().unwrap();
    let frames = exact(&mut reader, &p, dir.path(), &scene(&p, 20.));
    assert_eq!(frames[0].1.next.as_ref().unwrap().1, 0.5); // 100ms local + 35ms offset.
    let frozen: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    let mut frozen_reader = VideoFrames::new(Arc::new(Backend(control())));
    let frozen_frames = exact(
        &mut frozen_reader,
        &frozen,
        dir.path(),
        &scene(&frozen, 20.),
    );
    assert_eq!(
        frames[0].1.decoded().unwrap().rgba().unwrap(),
        frozen_frames[0].1.decoded().unwrap().rgba().unwrap()
    );
    assert!(exact(&mut reader, &p, dir.path(), &scene(&p, 40.)).is_empty());
    p.layers[0].sampling.frame_blend = FrameBlend::Hold;
    let frames = exact(&mut reader, &p, dir.path(), &scene(&p, 20.));
    assert!(frames[0].1.next.is_none());
    assert_eq!(
        &frames[0].1.decoded().unwrap().rgba().unwrap()[..4],
        &[0, 0, 0, 255]
    );
}

#[test]
fn blocked_right_is_published_and_required_errors_do_not_become_prefetch_failures() {
    let dir = tempfile::tempdir().unwrap();
    let p = fixture(dir.path());
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let control = Arc::new(Control {
        fail_right: AtomicBool::new(true),
        calls: Mutex::new(vec![]),
        entered: Some(entered),
        release: Mutex::new(Some(released)),
    });
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let scene = scene(&p, 13.5);
    assert!(reader
        .prepare_scene_exact(&p, dir.path(), &scene)
        .unwrap()
        .is_none());
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    {
        let state = reader.streams[&7].shared.0.lock().unwrap();
        assert_eq!(status(&state)["state"], "pending");
        assert_eq!(status(&state)["next_pts_us"], 170_000);
        let flight = state.in_flight.unwrap();
        assert_eq!(flight.time, 170_000);
        assert!(!flight.speculative);
        assert_eq!(flight.phase, "decode");
    }
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Err(error) = reader.prepare_scene_exact(&p, dir.path(), &scene) {
            assert_eq!(error.to_string(), "controlled required-right decode failure");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(reader.metrics()["prefetchFailures"], 0);
    // Switching to Hold is a new requirement and must stop requiring the failed right.
    let mut hold = scene.clone();
    hold.layers[0].sampling.frame_blend = FrameBlend::Hold;
    let ready = reader
        .prepare_scene_exact(&p, dir.path(), &hold)
        .unwrap()
        .unwrap();
    assert!(ready[0].1.next.is_none());
}

#[test]
fn yuv_required_sources_mix_on_gpu_and_survive_proxy_and_hold_handoff() {
    let dir = tempfile::tempdir().unwrap();
    let p = fixture(dir.path());
    let scene = scene(&p, 13.5);
    let frame = |pts, y| {
        Arc::new(DecodedFrame {
            pixels: VideoPixels::Yuv(motion_render::Yuv420Frame {
                width: 16,
                height: 16,
                rotation: 0,
                standard: 1,
                range: 2,
                phase: [0, 0],
                chroma_layout: motion_render::ChromaLayout::Uv,
                y: vec![y; 256],
                uv: vec![128; 128],
            }),
            pts,
            end: pts + 70_000,
            width: 16,
            height: 16,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "authored-yuv",
            decoder_name: "controlled".into(),
        })
    };
    let sample = PreparedVideoFrame {
        left: frame(100_000, 16),
        next: Some((frame(170_000, 235), 0.5)),
    };
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(16, 16).unwrap();
    sample.upload(&mut renderer, 7, 1).unwrap();
    let gpu = renderer.capture(&scene, &target).unwrap().0;
    let frozen = sample.decoded().unwrap();
    assert!(gpu
        .iter()
        .zip(frozen.rgba().unwrap().iter())
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    assert_eq!(renderer.video_gpu_conversions, 2);
    let count = renderer.video_blend_submissions;
    sample.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(renderer.video_gpu_conversions, 2);
    assert_eq!(renderer.video_blend_submissions, count);
    let mut cache = crate::preview_video::PreviewVideoCache::default();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(cache
        .upload_prepared(&mut renderer, 7, 1, &sample, Some(8), false, &mut encoder)
        .unwrap()
        .is_none());
    assert_eq!(renderer.video_encoded_conversions, 0);
    assert_eq!(renderer.video_gpu_conversions, 4);
    assert_eq!(renderer.video_blend_source_size(7, 1), Some((8, 8)));
    sample.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(gpu, renderer.capture(&scene, &target).unwrap().0);
    let hold = PreparedVideoFrame {
        left: sample.left.clone(),
        next: None,
    };
    hold.upload(&mut renderer, 7, 1).unwrap();
    assert_eq!(
        &renderer.capture(&scene, &target).unwrap().0[..4],
        &[0, 0, 0, 255]
    );
    renderer.check_health().unwrap();
}

#[test]
fn superseded_blocked_neighbor_does_not_overwrite_the_new_required_target() {
    let dir = tempfile::tempdir().unwrap();
    let p = fixture(dir.path());
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let control = Arc::new(Control {
        fail_right: AtomicBool::new(false),
        calls: Mutex::new(vec![]),
        entered: Some(entered),
        release: Mutex::new(Some(released)),
    });
    let mut reader = VideoFrames::new(Arc::new(Backend(control)));
    assert!(reader
        .prepare_scene_exact(&p, dir.path(), &scene(&p, 13.5))
        .unwrap()
        .is_none());
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(reader
        .prepare_scene_exact(&p, dir.path(), &scene(&p, 45.))
        .unwrap()
        .is_none());
    release.send(()).unwrap();
    let frames = exact(&mut reader, &p, dir.path(), &scene(&p, 45.));
    assert_eq!(frames[0].1.left.pts, 400_000);
    assert!(frames[0].1.next.is_none());
    assert_eq!(&frames[0].1.left.rgba().unwrap()[..4], &[255, 0, 0, 255]);
    assert_eq!(reader.metrics()["cancelledFrames"], 1);
}

#[test]
fn frozen_composition_protocol_keeps_sampling_snapshot_and_rejects_stale_or_closed_reads() {
    use crate::ops::media;
    let dir = tempfile::tempdir().unwrap();
    let p = fixture(dir.path());
    let id = crate::session::open(dir.path().into(), p, Arc::new(Backend(control()))).unwrap();
    let handle = media::freeze_video(id).unwrap()["handle"].as_i64().unwrap();
    crate::session::with_session(id, |s| {
        s.apply_command_batch(vec![motion_core::Command::SetLayerSampling {
            object: 7,
            sampling: Default::default(),
        }])
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if media::request_frozen_composition_frame(handle, 13.5, 1).unwrap()["state"] == "ready" {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let mixed = media::read_frozen_composition_video_into(handle, 7, 1).unwrap();
    assert_eq!(&mixed.rgba().unwrap()[..4], &[188, 188, 188, 255]);
    // The explicit source-frame API remains a decoder read, independently of
    // the stored composition sampling. It must not fabricate a blended source PTS.
    let source = media::request_frozen_video_frame(handle, 7, 13.5, 1).unwrap();
    assert_eq!(source["state"], "ready");
    let raw = media::read_frozen_video_frame_into(handle, 7, 1).unwrap();
    assert_eq!(&raw.rgba().unwrap()[..4], &[0, 0, 0, 255]);
    assert_eq!(
        &media::read_frozen_composition_video_into(handle, 7, 1)
            .unwrap()
            .rgba()
            .unwrap()[..4],
        &[188, 188, 188, 255]
    );
    media::request_frozen_composition_frame(handle, 45., 2).unwrap();
    assert!(media::read_frozen_composition_video_into(handle, 7, 1)
        .err()
        .unwrap()
        .to_string()
        .contains("superseded"));
    assert!(media::request_frozen_composition_frame(handle, 13.5, 1).is_err());
    assert_eq!(
        media::release_frozen_video(handle).unwrap()["released"],
        true
    );
    assert!(media::read_frozen_composition_video_into(handle, 7, 2)
        .err()
        .unwrap()
        .to_string()
        .contains("closed"));
    assert_eq!(&mixed.rgba().unwrap()[..4], &[188, 188, 188, 255]);
    crate::session::close(id);
}

fn exposure_task(renderer: &mut Renderer, p: &Project, preview: bool) -> motion_render::ExposureFrame<motion_core::ExpressionEvaluator> {
    use motion_render::exposure_scene::{ExposureDocument, ExposureScenes};
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(document,&p.composition_id,13.5,preview,None,motion_core::ExpressionEvaluator).unwrap();
    motion_render::ExposureFrame::new(renderer,scenes,16,16).unwrap()
}
fn poll_exposure(task:&mut motion_render::ExposureFrame<motion_core::ExpressionEvaluator>,
    renderer:&mut Renderer, reader:&mut VideoFrames, proxies:&mut crate::preview_video::PreviewVideoCache,
    p:&Project, root:&Path, policy:crate::render_sampling::ExposureMediaPolicy) -> std::result::Result<motion_render::ExposurePoll,motion_render::RenderError> {
    let mut media = crate::render_sampling::ExposureMedia {project:p,root,videos:reader,proxies,policy};
    task.poll(renderer,Duration::from_millis(8),|renderer,scene|media.prepare(renderer,scene))
}

#[test]
fn exposure_media_pending_reuses_required_generation_and_exact_vfr_pair_across_all_phases_and_proxy() {
    use crate::render_sampling::ExposureMediaPolicy;
    use motion_render::ExposurePoll;
    let dir = tempfile::tempdir().unwrap();
    let mut p = fixture(dir.path());
    p.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:1.,samples:4});
    let (entered,waiting) = mpsc::channel();
    let (release,released) = mpsc::channel();
    let control = Arc::new(Control {fail_right:AtomicBool::new(false),calls:Mutex::new(vec![]),entered:Some(entered),release:Mutex::new(Some(released))});
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let mut proxies = crate::preview_video::PreviewVideoCache::default();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut task = exposure_task(&mut renderer,&p,false);
    assert!(matches!(poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap(),ExposurePoll::Pending(_)));
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = reader.streams[&7].shared.0.lock().unwrap().desired.unwrap().sequence;
    for _ in 0..60 {
        assert!(matches!(poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap(),ExposurePoll::Pending(_)));
        let state = reader.streams[&7].shared.0.lock().unwrap();
        assert_eq!(state.desired.unwrap().sequence,generation);
        assert_eq!(state.in_flight.unwrap().time,170_000);
        assert!(!state.in_flight.unwrap().speculative);
        assert_eq!(task.target().phase_index(),0);
    }
    release.send(()).unwrap();
    let deadline = Instant::now()+Duration::from_secs(5);
    loop {
        if matches!(poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap(),ExposurePoll::Ready(_)) { break; }
        assert!(Instant::now()<deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let pixels = renderer.capture_exposure(task.target()).unwrap();
    assert!(pixels.chunks_exact(4).all(|pixel|pixel[0].abs_diff(188)<=1 && pixel[1].abs_diff(188)<=1 && pixel[2].abs_diff(188)<=1 && pixel[3]==255));
    assert_eq!(&*control.calls.lock().unwrap(),&[100_000,170_000]);
    drop(task);
    assert_eq!(renderer.exposure_texture_bytes(),0);
    let mut preview = exposure_task(&mut renderer,&p,true);
    let policy = ExposureMediaPolicy {image_resolution:motion_render::image_resources::Resolution::Preview(8),asynchronous_images:true,video_edge:Some(8)};
    loop {
        if matches!(poll_exposure(&mut preview,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),policy).unwrap(),ExposurePoll::Ready(_)) {break;}
        assert!(Instant::now()<deadline);
    }
    assert_eq!(renderer.capture_exposure(preview.target()).unwrap(),pixels);
    assert_eq!(&*control.calls.lock().unwrap(),&[100_000,170_000]);
}

#[test]
fn exposure_required_decoder_error_is_terminal_and_new_explicit_reader_can_recover() {
    use crate::render_sampling::ExposureMediaPolicy;
    use motion_render::{ExposurePoll,ExposureStage};
    let dir = tempfile::tempdir().unwrap();
    let mut p = fixture(dir.path());
    p.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter {frames:1.,samples:4});
    let control = control();
    control.fail_right.store(true,Ordering::Release);
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let mut proxies = crate::preview_video::PreviewVideoCache::default();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut task = exposure_task(&mut renderer,&p,false);
    let deadline = Instant::now()+Duration::from_secs(5);
    let error = loop {
        if let Err(error) = poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL) {break error.to_string();}
        assert!(Instant::now()<deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(error.contains("controlled required-right decode failure") && error.contains("media_prepare") && error.contains("object=Some(7)"),"{error}");
    assert_eq!(task.progress().stage,ExposureStage::Failed);
    assert_eq!(task.target().phase_index(),0);
    assert_eq!(reader.metrics()["prefetchFailures"],0);
    control.fail_right.store(false,Ordering::Release);
    for _ in 0..60 {
        assert!(poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap_err().to_string().contains("controlled required-right decode failure"));
    }
    assert_eq!(&*control.calls.lock().unwrap(),&[100_000,170_000]);
    assert!(renderer.capture_exposure(task.target()).is_err());
    drop(task);
    assert_eq!(renderer.exposure_texture_bytes(),0);
    reader.clear();
    proxies.clear();
    let mut fresh = exposure_task(&mut renderer,&p,false);
    loop {
        if matches!(poll_exposure(&mut fresh,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap(),ExposurePoll::Ready(_)) {break;}
        assert!(Instant::now()<deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let pixels = renderer.capture_exposure(fresh.target()).unwrap();
    assert!(pixels.chunks_exact(4).all(|p|p[0].abs_diff(188)<=1 && p[3]==255));
}

#[test]
fn nested_exposure_reuses_stable_video_instances_across_parent_visits_and_new_frames() {
    use crate::render_sampling::ExposureMediaPolicy;
    use motion_render::ExposurePoll;
    let dir=tempfile::tempdir().unwrap();
    let mut p=fixture(dir.path());
    let mut video=p.layers.remove(0);
    video.sampling.motion_blur=Some(motion_core::render_sampling::Shutter {frames:1.,samples:4});
    p.compositions.push(motion_core::Composition {id:"comp-exposure-video".into(),name:"video clocks".into(),
        width:16,height:16,fps:p.fps,frames:p.frames,background:[0.;4],camera:motion_core::Camera::new(16,16),
        layers:vec![video],expressions:vec![]});
    for id in 1..=3 {
        let mut reference=Layer::solid(id,"video reference",[16.;2],[8.,8.,0.],[1.;4]);
        reference.content=Content::Composition {clip:motion_core::CompositionClip::new("comp-exposure-video".into())};
        p.layers.push(reference);
    }
    p.validate().unwrap();
    let mut scene=Scene::new(&p).unwrap();
    scene.sample(&p,13.5,None,&motion_core::ExpressionEvaluator).unwrap();
    let expected:std::collections::HashSet<_>=scene.video_layers().into_iter().map(|layer|layer.id).collect();
    assert_eq!(expected.len(),3);
    let control=control();
    let mut reader=VideoFrames::new(Arc::new(Backend(control.clone())));
    let mut proxies=crate::preview_video::PreviewVideoCache::default();
    let mut renderer=pollster::block_on(Renderer::headless()).unwrap();
    let deadline=Instant::now()+Duration::from_secs(15);
    for _ in 0..3 {
        let mut task=exposure_task(&mut renderer,&p,false);
        loop {
            if matches!(poll_exposure(&mut task,&mut renderer,&mut reader,&mut proxies,&p,dir.path(),ExposureMediaPolicy::FULL).unwrap(),ExposurePoll::Ready(_)) {break;}
            assert!(Instant::now()<deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(reader.streams.keys().copied().collect::<std::collections::HashSet<_>>(),expected);
        let pixels=renderer.capture_exposure(task.target()).unwrap();
        assert!(pixels.chunks_exact(4).all(|pixel|pixel[0].abs_diff(188)<=1 && pixel[3]==255));
        assert_eq!(&*control.calls.lock().unwrap(),&[100_000,170_000,100_000,170_000,100_000,170_000]);
        assert_eq!(reader.metrics()["retiringStreams"],0);
    }
    reader.clear();
    assert!(reader.sampling_recent.is_empty());
}

#[test]
fn sampling_idle_cache_evicts_oldest_without_exceeding_four_live_or_retiring_workers() {
    let dir=tempfile::tempdir().unwrap();
    let p=fixture(dir.path());
    let mut source=Scene::new(&p).unwrap();
    source.sample(&p,13.5,None,&motion_core::ExpressionEvaluator).unwrap();
    let control=control();
    let mut reader=VideoFrames::new(Arc::new(Backend(control.clone())));
    let deadline=Instant::now()+Duration::from_secs(10);
    for id in [7,8,9,10,7,11,12] {
        let mut scene=source.clone();scene.layers[0].id=id;
        loop {
            let result=reader.prepare_scene_sampling_exact(&p,dir.path(),&scene).unwrap();
            assert!(reader.streams.len()+reader.retiring.len()<=MAX_DECODER_WORKERS);
            assert!(reader.sampling_recent.len()<=MAX_DECODER_WORKERS);
            if result.is_some() {break;}
            assert!(Instant::now()<deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        if id==11 {assert_eq!(reader.streams.keys().copied().collect::<std::collections::HashSet<_>>(),[7,9,10,11].into_iter().collect());}
    }
    assert_eq!(reader.sampling_recent,vec![10,7,11,12]);
    assert_eq!(control.calls.lock().unwrap().len(),12);
}

#[test]
fn exposure_cancellation_retires_only_pending_reads_and_preserves_ready_and_failed_streams() {
    let dir=tempfile::tempdir().unwrap();let p=fixture(dir.path());let scope=scene(&p,13.5);
    let (entered,waiting)=mpsc::channel();let (release,released)=mpsc::channel();
    let control=Arc::new(Control {fail_right:AtomicBool::new(false),calls:Mutex::new(vec![]),entered:Some(entered),release:Mutex::new(Some(released))});
    let mut reader=VideoFrames::new(Arc::new(Backend(control.clone())));
    assert!(reader.prepare_scene_sampling_exact(&p,dir.path(),&scope).unwrap().is_none());
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    reader.cancel_pending_sampling().unwrap();
    assert!(reader.streams.is_empty());assert_eq!(reader.retiring.len(),1);
    assert!(reader.sampling_recent.is_empty());
    release.send(()).unwrap();
    let deadline=Instant::now()+Duration::from_secs(5);
    while !reader.retiring.is_empty() {reader.reap_retired().unwrap();assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));}
    loop {if reader.prepare_scene_sampling_exact(&p,dir.path(),&scope).unwrap().is_some(){break;}assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));}
    let generation=reader.streams[&7].shared.0.lock().unwrap().desired.unwrap().sequence;
    let calls=control.calls.lock().unwrap().len();
    reader.cancel_pending_sampling().unwrap();
    assert_eq!(reader.streams[&7].shared.0.lock().unwrap().desired.unwrap().sequence,generation);
    assert!(reader.prepare_scene_sampling_exact(&p,dir.path(),&scope).unwrap().is_some());
    assert_eq!(control.calls.lock().unwrap().len(),calls);
    reader.clear();
    control.fail_right.store(true,Ordering::Release);
    let error=loop {match reader.prepare_scene_sampling_exact(&p,dir.path(),&scope) {Err(e)=>break e,Ok(_)=>{assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));}}};
    assert!(error.to_string().contains("controlled required-right decode failure"));
    let calls=control.calls.lock().unwrap().len();
    control.fail_right.store(false,Ordering::Release);
    for _ in 0..60 {
        reader.cancel_pending_sampling().unwrap();
        assert!(reader.prepare_scene_sampling_exact(&p,dir.path(),&scope).err().unwrap().to_string().contains("controlled required-right decode failure"));
    }
    assert_eq!(control.calls.lock().unwrap().len(),calls);
}
