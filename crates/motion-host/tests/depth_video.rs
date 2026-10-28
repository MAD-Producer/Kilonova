#![cfg(feature = "ai-depth")]
#[path = "depth_video/timeline.rs"]
mod timeline;
use motion_core::{Content, Layer, Project, VideoAsset, VideoClip};
use motion_host::{
    depth::{Backend, Cancellation, Costs, Estimate, Input, Key},
    video_frame::{DecodedFrame, VideoPixels},
    Platform, Session, VideoDecoder,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, NearDirection,
};
use std::{
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex, Once,
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

struct Logs(Mutex<Vec<String>>);
static LOG: Logs = Logs(Mutex::new(Vec::new()));
impl log::Log for Logs {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let mut lines = self.0.lock().unwrap();
            if lines.len() == 256 {
                lines.remove(0);
            }
            lines.push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}
fn init_log() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&LOG).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Rgba,
    BlockOpen,
    BlockFrame,
    BlockBackend,
    Yuv,
    WrongPts,
    WrongEnd,
    WrongSize,
    WrongColor,
    WrongRotation,
    UnknownEncoding,
    GrayBt709,
    GraySrgb,
}
#[derive(Debug)]
enum Event {
    Open(ThreadId),
    Frame(u64, ThreadId),
    Drop(ThreadId),
}
struct VideoPlatform {
    mode: Mode,
    events: mpsc::Sender<Event>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
    opens: AtomicUsize,
}
impl Platform for VideoPlatform {
    fn name(&self) -> &'static str {
        "controlled-depth-video"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("must use registered video")
    }
    fn open_decoder(
        &self,
        path: &Path,
        asset: VideoAsset,
        pts: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        assert!(path.is_absolute());
        assert_eq!(std::fs::metadata(path).unwrap().len(), asset.bytes);
        assert_eq!(pts, vec![10_000, 40_000, 110_000]);
        let first = self.opens.fetch_add(1, Ordering::SeqCst) == 0;
        self.events
            .send(Event::Open(std::thread::current().id()))
            .unwrap();
        let release = if first {
            self.release.lock().unwrap().take()
        } else {
            None
        };
        let release = if self.mode == Mode::BlockOpen && first {
            release
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            None
        } else {
            release
        };
        Ok(Box::new(Decoder {
            mode: self.mode,
            asset,
            pts,
            events: self.events.clone(),
            release: if self.mode == Mode::BlockFrame {
                release
            } else {
                None
            },
            owner: std::thread::current().id(),
            local: Rc::new(()),
        }))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("no audio work")
    }
    fn media_capabilities(
        &self,
        _: Option<&motion_host::VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        panic!("no GPU work")
    }
}
// Rc deliberately makes this decoder !Send. It must be born, used and dropped
// on the inference worker, never stored inside Session or sent as job payload.
struct Decoder {
    mode: Mode,
    asset: VideoAsset,
    pts: Vec<u64>,
    events: mpsc::Sender<Event>,
    release: Option<mpsc::Receiver<()>>,
    owner: ThreadId,
    local: Rc<()>,
}
impl VideoDecoder for Decoder {
    fn rgb_encoding(&self) -> motion_media::Result<motion_host::video_frame::RgbEncoding> {
        if self.mode == Mode::UnknownEncoding {
            return Err("controlled unknown transfer metadata".into());
        }
        Ok(if self.mode == Mode::GrayBt709 {
            motion_host::video_frame::RgbEncoding::Bt709
        } else {
            motion_host::video_frame::RgbEncoding::Srgb
        })
    }
    fn frame(
        &mut self,
        target: u64,
        check: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<DecodedFrame> {
        assert_eq!(self.owner, std::thread::current().id());
        self.events.send(Event::Frame(target, self.owner)).unwrap();
        if let Some(release) = self.release.take() {
            loop {
                check()?;
                match release.recv_timeout(Duration::from_millis(1)) {
                    Ok(()) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(error) => return Err(error.to_string().into()),
                }
            }
        }
        check()?;
        let index = self.pts.partition_point(|p| *p <= target) - 1;
        let pts = self.pts[index];
        let end = self
            .pts
            .get(index + 1)
            .copied()
            .unwrap_or(self.asset.video_end_us);
        let pixels = if matches!(
            self.mode,
            Mode::Yuv | Mode::WrongColor | Mode::WrongRotation
        ) {
            VideoPixels::Yuv(motion_render::Yuv420Frame {
                width: 3,
                height: 2,
                rotation: if self.mode == Mode::WrongRotation {
                    270
                } else {
                    90
                },
                standard: if self.mode == Mode::WrongColor { 2 } else { 1 },
                range: 2,
                phase: [0, 0],
                y: vec![16, 235, 16, 235, 16, 235],
                uv: vec![128; 4],
                chroma_layout: motion_render::ChromaLayout::Uv,
            })
        } else if matches!(self.mode, Mode::GrayBt709 | Mode::GraySrgb) {
            VideoPixels::Rgba(vec![128, 128, 128, 255].repeat(6))
        } else {
            VideoPixels::Rgba(
                (0..6)
                    .flat_map(|i| [((pts / 1000) + i) as u8, 2, 3, 255])
                    .collect(),
            )
        };
        Ok(DecodedFrame {
            pixels,
            pts: pts + u64::from(self.mode == Mode::WrongPts),
            end: end - u64::from(self.mode == Mode::WrongEnd),
            width: self.asset.display_width + u32::from(self.mode == Mode::WrongSize),
            height: self.asset.display_height,
            decode_us: 1,
            codec_us: 1,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "controlled-sdr",
            decoder_name: "owned-test".into(),
        })
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        assert_eq!(self.owner, std::thread::current().id());
        assert_eq!(Rc::strong_count(&self.local), 1);
        let _ = self.events.send(Event::Drop(self.owner));
    }
}
struct Seen {
    key: Key,
    size: [u32; 2],
    rgb: Vec<u8>,
}
struct Echo {
    seen: mpsc::Sender<Seen>,
    release: Option<mpsc::Receiver<()>>,
}
impl Backend for Echo {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        cancel.check()?;
        self.seen
            .send(Seen {
                key: input.key().clone(),
                size: input.size(),
                rgb: input.rgb().to_vec(),
            })
            .unwrap();
        // Deliberately return late success after cancellation. Jobs must reject
        // it even when the next request resolves to the same source PTS.
        if let Some(release) = self.release.take() {
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: input.size(),
                    source: input.key().source.clone(),
                    geometry: input.key().geometry.clone(),
                    model: input.key().model.clone(),
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                input
                    .rgb()
                    .chunks_exact(3)
                    .map(|rgb| f32::from(rgb[0]))
                    .collect(),
                None,
                None,
            )?,
            provider: "controlled_cpu".into(),
            costs: Costs::default(),
        })
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    session: Session,
    platform: Arc<VideoPlatform>,
    events: mpsc::Receiver<Event>,
    release: mpsc::Sender<()>,
    seen: mpsc::Receiver<Seen>,
}
fn fixture(mode: Mode) -> Fixture {
    init_log();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/source.mp4"), b"owned").unwrap();
    let rotated = matches!(mode, Mode::Yuv | Mode::WrongColor | Mode::WrongRotation);
    let asset = VideoAsset {
        id: 74,
        path: "assets/source.mp4".into(),
        bytes: 5,
        mime: "video/avc".into(),
        track: 0,
        width: 3,
        height: 2,
        rotation: if rotated { 90 } else { 0 },
        display_width: if rotated { 2 } else { 3 },
        display_height: if rotated { 3 } else { 2 },
        video_start_us: 10_000,
        video_end_us: 150_000,
        duration_us: 200_000,
        frame_count: 3,
        variable_frame_rate: true,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    };
    let path = motion_media::video_cache_path(dir.path(), &asset).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    motion_media::save_index(&path, &asset, &[10_000, 40_000, 110_000]).unwrap();
    let mut project = Project::new(32, 32, 30, 12).unwrap();
    let mut layer = Layer::solid(1, "Owned video", [3., 2.], [0.; 3], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip {
            source_offset_us: 10_000,
            ..VideoClip::new(74)
        },
    };
    layer
        .set_interval(
            12,
            motion_core::ClipRange {
                in_frame: 0.,
                out_frame: 4.,
            },
        )
        .unwrap();
    project.layers.push(layer);
    project.video_assets.push(asset);
    let (events, received) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let (decoder_release, backend_release) = if mode == Mode::BlockBackend {
        (None, Some(blocked))
    } else {
        (Some(blocked), None)
    };
    let platform = Arc::new(VideoPlatform {
        mode,
        events,
        release: Mutex::new(decoder_release),
        opens: AtomicUsize::new(0),
    });
    let mut session = Session::new(project, dir.path().into(), platform.clone()).unwrap();
    let (seen, receiver) = mpsc::channel();
    session
        .configure_depth(Echo {
            seen,
            release: backend_release,
        })
        .unwrap();
    Fixture {
        dir,
        session,
        platform,
        events: received,
        release,
        seen: receiver,
    }
}
fn geometry(session: &Session) -> DepthGeometry {
    let asset = &session.engine.project().video_assets[0];
    DepthGeometry {
        source_size: [asset.display_width, asset.display_height],
        crop: [0, 0, asset.display_width, asset.display_height],
        quarter_turns: 0,
        mirror_x: false,
    }
}
fn model() -> DepthModel {
    DepthModel {
        id: "controlled-video-depth".into(),
        revision: "r1".into(),
        sha256: [7; 32],
        processing: "owned-rgb-test-v1".into(),
    }
}
fn request(session: &mut Session) -> Option<u64> {
    let geometry = geometry(session);
    session.request_video_depth(1, geometry, model()).unwrap()
}
fn done(session: &mut Session, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let counts = session.depth_status().unwrap().unwrap().counts;
        if counts.completed + counts.canceled + counts.failed >= count {
            return;
        }
        assert!(Instant::now() < until, "video depth completion timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn author_indexed_sample(session: &mut Session, low: f64) -> motion_core::Command {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    session.video_depth_calibration_command(1, [
        DepthAnchor { value: low, camera_z_meters: 8. },
        DepthAnchor { value: low + 5., camera_z_meters: 4. },
    ], CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject, 20.).unwrap()
}

#[test]
fn indexed_authoring_checks_actual_decoder_interval_rgb_and_object_before_commit() {
    use motion_model::depth::VideoDepthSample;
    let mut f = fixture(Mode::Rgba);
    let mut project = f.session.engine.snapshot();
    let mut other = project.layers[0].clone();
    other.id = 2;
    project.layers.push(other);
    f.session.replace_project(motion_core::Engine::new(project).unwrap(), f.dir.path().into()).unwrap();
    request(&mut f.session).unwrap();
    done(&mut f.session, 1);
    f.session.poll_depth().unwrap().unwrap();
    let command = author_indexed_sample(&mut f.session, 10.);
    let motion_core::Command::VideoDepthCalibration { sample, .. } = &command else { panic!("wrong command"); };
    assert_eq!((sample.start_us(), sample.end_us()), (10_000, 40_000));
    let before = f.session.engine.snapshot();
    let history = f.session.engine.history_diagnostics();
    let wrong_object = f.session.apply_command_batch(vec![motion_core::Command::VideoDepthCalibration {
        object: 2, sample: sample.clone(),
    }]).unwrap_err();
    assert!(wrong_object.to_string().contains("current original source"), "{wrong_object}");
    assert_eq!(f.session.engine.snapshot(), before);
    assert_eq!(f.session.engine.history_diagnostics(), history);
    let wrong_end = VideoDepthSample::new(sample.calibration().clone(), 39_999).unwrap();
    let mut wrong_rgb = serde_json::to_value(sample).unwrap();
    wrong_rgb["calibration"]["input"]["rgb_sha256"][0] = 99.into();
    let wrong_rgb = serde_json::from_value(wrong_rgb).unwrap();
    for sample in [wrong_end, wrong_rgb] {
        let error = f.session.apply_command_batch(vec![motion_core::Command::VideoDepthCalibration { object: 1, sample }]).unwrap_err();
        assert!(error.to_string().contains("actual"), "{error}");
        assert_eq!(f.session.engine.snapshot(), before);
        assert_eq!(f.session.engine.history_diagnostics(), history);
    }
    f.session.apply_command_batch(vec![command.clone()]).unwrap();
    assert_eq!(f.session.engine.project().layers[0].video_depth_calibration.as_ref().unwrap().samples().len(), 1);
    f.session.frame = 1.;
    request(&mut f.session).unwrap();
    done(&mut f.session, 2);
    f.session.poll_depth().unwrap().unwrap();
    let before = f.session.engine.snapshot();
    assert!(f.session.apply_command_batch(vec![command]).unwrap_err().to_string().contains("actual"));
    assert_eq!(f.session.engine.snapshot(), before);
    // Multiple surviving entries in one batch cannot be laundered through the
    // last command: each one must match the installed current decoder result.
    let valid = author_indexed_sample(&mut f.session, 40.);
    let old = before.layers[0].video_depth_calibration.as_ref().unwrap().samples()[0].clone();
    assert!(f.session.apply_command_batch(vec![
        motion_core::Command::VideoDepthCalibration { object: 1, sample: old }, valid,
    ]).is_err());
    assert_eq!(f.session.engine.snapshot(), before);
}

#[test]
fn indexed_track_reuses_only_matching_vfr_intervals_and_revisions_release_all_cached_pixels() {
    let mut f = fixture(Mode::Rgba);
    for (index, frame, low, pts, end) in [
        (1, 0., 10., 10_000, 40_000), (2, 1., 40., 40_000, 110_000), (3, 3., 110., 110_000, 150_000),
    ] {
        f.session.frame = frame;
        request(&mut f.session).unwrap();
        done(&mut f.session, index);
        f.session.poll_depth().unwrap().unwrap();
        let command = author_indexed_sample(&mut f.session, low);
        let motion_core::Command::VideoDepthCalibration { sample, .. } = &command else { panic!("wrong command"); };
        assert_eq!((sample.start_us(), sample.end_us()), (pts, end));
        f.session.apply_command_batch(vec![command]).unwrap();
    }
    assert_eq!(f.session.engine.project().layers[0].video_depth_calibration.as_ref().unwrap().samples().len(), 3);
    // After the last authoring revision, prewarm independently verified frames.
    for (index, frame) in [(4, 0.), (5, 1.), (6, 3.)] {
        f.session.frame = frame;
        request(&mut f.session).unwrap();
        done(&mut f.session, index);
        f.session.poll_depth().unwrap().unwrap();
    }
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.retained_samples, status.retained_image_bytes), (3, 72));
    let opens = f.platform.opens.load(Ordering::SeqCst);
    for frame in [0., 0.5, 1., 2., 2.9, 3., 0.] {
        f.session.frame = frame;
        assert!(f.session.bound_depth_calibration(1).unwrap().is_some(), "frame={frame}");
        assert_eq!(f.session.depth_status().unwrap().unwrap().retained_samples, 3);
    }
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), opens);
    // Same-path document replacement has a distinct generation, even when its
    // revision number happens to match. Saved metadata survives, live RGB does not.
    let replacement = motion_core::Engine::new(f.session.engine.snapshot()).unwrap();
    f.session.replace_project(replacement, f.dir.path().into()).unwrap();
    assert!(f.session.bound_depth_calibration(1).unwrap().is_none());
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.retained_samples, status.retained_image_bytes), (0, 0));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_video_depth_uses_its_owned_color_and_refuses_other_source_times() {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    use motion_effects::image_contract::ChannelPrecision::{Unorm8, Unorm16, Float16, Float32};
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let f = fixture(Mode::Rgba);
        let mut project = f.session.engine.project().clone();
        project.working_precision = precision;
        project.layers[0].three_d = true;
        project.layers[0].size = [80.; 2];
        project.layers[0].transform.position.set_at(0, [16., 16., 0.]).unwrap();
        project.camera.created = true;
        project.camera.position.set_at(0, [16., 16., -120.]).unwrap();
        project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
        project.camera.fov.set_at(0, 60.).unwrap();
        let mut plane = Layer::solid(2, "Independent occlusion plane", [80.; 2], [16., 16., 0.], [0., 0., 1., 1.]);
        plane.three_d = true;
        project.layers.push(plane);
        let id = motion_host::session::open(f.dir.path().into(), project, f.platform.clone()).unwrap();
        struct Close(i64);
        impl Drop for Close { fn drop(&mut self) { motion_host::session::close(self.0); } }
        let _close = Close(id);
        let (sender, seen) = mpsc::channel();
        motion_host::session::with_session(id, |s| {
            s.configure_depth(Echo { seen: sender, release: None })?;
            request(s).unwrap();
            Ok(())
        }).unwrap();
        let decoded = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(decoded.key.source.pts.ticks, 10_000);
        motion_host::session::with_session(id, |s| {
            done(s, 1);
            s.poll_depth()?;
            let command = s.depth_calibration_command(1, [
                DepthAnchor { value: 10., camera_z_meters: 8. },
                DepthAnchor { value: 15., camera_z_meters: 4. },
            ], CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject, 20.)?;
            s.apply_command_batch(vec![command])?;
            request(s).unwrap();
            Ok(())
        }).unwrap();
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        motion_host::session::with_session(id, |s| { done(s, 2); s.poll_depth()?; Ok(()) }).unwrap();
        let opens = f.platform.opens.load(Ordering::SeqCst);
        let capture = motion_host::ops::export::capture(id).unwrap();
        let pixels = image::open(capture["path"].as_str().unwrap()).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (32, 32));
        assert_eq!(pixels.get_pixel(16, 10).0, [0, 0, 255, 255], "far video geometry must be behind the independent plane");
        let near = pixels.get_pixel(16, 22).0;
        assert!(near[0] >= 10 && near[1] <= 3 && near[2] <= 4 && near[3] == 255, "near video must use its admitted colors: {near:?}");
        assert_eq!(f.platform.opens.load(Ordering::SeqCst), opens, "rendering must not decode a second color frame");
        // A new headless renderer must recover from owned pixels after source
        // removal, without codec/file IO or a stale GPU texture dependency.
        std::fs::remove_file(f.dir.path().join("assets/source.mp4")).unwrap();
        let again = motion_host::ops::export::capture(id).unwrap();
        assert_eq!(image::open(again["path"].as_str().unwrap()).unwrap().to_rgba8(), pixels);
        assert_eq!(f.platform.opens.load(Ordering::SeqCst), opens);
        if let Some(dir) = std::env::var_os("MOTION_VIDEO_DEPTH_ARTIFACTS") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            pixels.save(dir.join(format!("native-{precision:?}.png"))).unwrap();
        }
        motion_host::session::with_session(id, |s| {
            assert_eq!(s.depth_status()?.unwrap().retained_image_bytes, 24);
            s.frame = 1.; // 43,333us selects the distinct 40,000us source PTS.
            Ok(())
        }).unwrap();
        assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
        assert!(!f.dir.path().join("exports/frame-000001.png").exists());
        motion_host::session::with_session(id, |s| {
            assert_eq!(s.depth_status()?.unwrap().retained_image_bytes, 0);
            s.frame = 0.;
            Ok(())
        }).unwrap();
        assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
        assert_eq!(image::open(f.dir.path().join("exports/frame-000000.png")).unwrap().to_rgba8(), pixels);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_video_owned_colors_use_the_same_bt709_conversion_as_inference() {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    let f = fixture(Mode::GrayBt709);
    let mut project = f.session.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.layers[0].transform.position.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.created = true;
    project.camera.position.set_at(0, [16., 16., -120.]).unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    let id = motion_host::session::open(f.dir.path().into(), project, f.platform.clone()).unwrap();
    struct Close(i64);
    impl Drop for Close { fn drop(&mut self) { motion_host::session::close(self.0); } }
    let _close = Close(id);
    let (sender, seen) = mpsc::channel();
    motion_host::session::with_session(id, |s| {
        s.configure_depth(Echo { seen: sender, release: None })?;
        request(s).unwrap();
        Ok(())
    }).unwrap();
    let decoded = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(decoded.rgb[0], 128, "BT.709 and sRGB encodings must not be treated as identical");
    motion_host::session::with_session(id, |s| {
        done(s, 1);
        s.poll_depth()?;
        let command = s.depth_calibration_command(1, [
            DepthAnchor { value: 0., camera_z_meters: 8. },
            DepthAnchor { value: 255., camera_z_meters: 4. },
        ], CalibrationCurve::CameraZ, CalibrationExtrapolation::Reject, 20.)?;
        s.apply_command_batch(vec![command])?;
        request(s).unwrap();
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id, |s| { done(s, 2); s.poll_depth()?; Ok(()) }).unwrap();
    let capture = motion_host::ops::export::capture(id).unwrap();
    let pixels = image::open(capture["path"].as_str().unwrap()).unwrap().to_rgba8();
    assert_eq!(pixels.get_pixel(16, 16).0, [decoded.rgb[0], decoded.rgb[1], decoded.rgb[2], 255]);
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 2, "capture must not decode again");
}

#[test]
fn video_prepared_fingerprint_is_computed_after_bt709_to_srgb_conversion() {
    for (mode, digest) in [
        (
            Mode::GrayBt709,
            "d83feade52615bb7344abd4ddb5fa5ae6b5d3b007421f6161fef468a3748464f",
        ),
        (
            Mode::GraySrgb,
            "c0321cc20fccec0a81e5dff75fed8350de8e89712104513aadc6403db9109329",
        ),
    ] {
        let mut f = fixture(mode);
        request(&mut f.session).unwrap();
        f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
        done(&mut f.session, 1);
        f.session.poll_depth().unwrap().unwrap();
        let identity = f.session.depth_input_fingerprint().unwrap().unwrap();
        assert_eq!(identity.rgb_size, [3, 2]);
        let actual = identity
            .rgb_sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual, digest);
        f.session.cancel_depth().unwrap();
        assert!(f.session.depth_input_fingerprint().unwrap().is_none());
    }
}

#[test]
fn variable_rate_sample_uses_actual_decoder_pts_and_explicit_crop_orientation() {
    let mut f = fixture(Mode::Rgba);
    f.session.frame = 1.;
    f.session.sample().unwrap();
    let before = f.session.engine.snapshot();
    let mut geometry = geometry(&f.session);
    geometry.crop = [1, 0, 2, 2];
    geometry.quarter_turns = 1;
    geometry.mirror_x = true;
    f.session
        .request_video_depth(1, geometry, model())
        .unwrap()
        .unwrap();
    let seen = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(seen.key.source.pts.ticks, 40_000);
    assert_eq!(seen.key.source.pts.timescale, 1_000_000);
    assert_eq!(seen.key.source.asset, 74);
    assert_eq!(seen.key.source.revision, f.session.engine.revision());
    assert_eq!(seen.size, [2, 2]);
    assert_eq!(seen.rgb, vec![41, 2, 3, 44, 2, 3, 42, 2, 3, 45, 2, 3]);
    done(&mut f.session, 1);
    let result = f.session.poll_depth().unwrap().unwrap();
    assert_eq!(result.resource.values(), &[41., 44., 42., 45.]);
    assert!(result.costs.source_prepare > Duration::ZERO);
    assert_eq!(result.costs.source_rgb_bytes, 12);
    assert!(result.costs.total >= result.costs.source_prepare);
    assert_eq!(f.session.engine.snapshot(), before);
    let events = f.events.try_iter().collect::<Vec<_>>();
    assert!(
        matches!(events.as_slice(),[Event::Open(a),Event::Frame(43_333,b),Event::Drop(c)] if a==b && b==c && *a!=f.session.owner)
    );
}

#[test]
fn decoder_open_is_observable_before_blocking_and_seek_discards_old_request() {
    let mut f = fixture(Mode::BlockOpen);
    f.session.frame = 1.;
    f.session.sample().unwrap();
    let first = request(&mut f.session).unwrap();
    assert!(
        matches!(f.events.recv_timeout(Duration::from_secs(5)).unwrap(),Event::Open(owner) if owner!=f.session.owner)
    );
    let flight = f
        .session
        .depth_status()
        .unwrap()
        .unwrap()
        .in_flight
        .unwrap();
    assert_eq!(flight.request, first);
    assert_eq!(flight.asset, 74);
    assert!(flight.source.is_none());
    assert_eq!(flight.target_time_us, Some(43_333));
    assert_eq!(flight.phase, "source_prepare");
    assert!(LOG
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|line| line.contains("start #1 depth_infer asset=74")
            && line.contains("target_us=43333 phase=video_decode")));
    f.session.frame = 3.;
    f.session.sample().unwrap();
    assert!(f.session.poll_depth().unwrap().is_none());
    let second = request(&mut f.session).unwrap();
    assert!(second > first);
    assert_eq!(f.session.depth_status().unwrap().unwrap().queue_depth, 1);
    f.release.send(()).unwrap();
    let seen = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(seen.key.source.pts.ticks, 110_000);
    done(&mut f.session, 2);
    assert_eq!(
        f.session.depth_status().unwrap().unwrap().counts.canceled,
        1
    );
    assert_eq!(
        f.session
            .poll_depth()
            .unwrap()
            .unwrap()
            .resource
            .descriptor()
            .source
            .pts
            .ticks,
        110_000
    );
    assert!(matches!(f.seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn packet_cancellation_releases_non_send_decoder_without_entering_inference() {
    let mut f = fixture(Mode::BlockFrame);
    request(&mut f.session).unwrap();
    assert!(matches!(
        f.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Open(_)
    ));
    assert!(matches!(
        f.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Frame(10_000, _)
    ));
    f.session.cancel_depth().unwrap();
    done(&mut f.session, 1);
    assert_eq!(
        f.session.depth_status().unwrap().unwrap().counts.canceled,
        1
    );
    assert!(f.session.poll_depth().unwrap().is_none());
    assert!(
        matches!(f.events.recv_timeout(Duration::from_secs(5)).unwrap(),Event::Drop(owner) if owner!=f.session.owner)
    );
    assert!(matches!(f.seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(f.session.depth_status().unwrap().unwrap().counts.failed, 0);
}

#[test]
fn rotated_sdr_yuv_is_converted_before_inference_using_display_pixel_coordinates() {
    let mut f = fixture(Mode::Yuv);
    request(&mut f.session).unwrap();
    let seen = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(seen.size, [2, 3]);
    assert_eq!(
        seen.rgb,
        vec![255, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0]
    );
    done(&mut f.session, 1);
    assert!(f.session.poll_depth().unwrap().is_some());
}

#[test]
fn mismatched_frame_pts_end_dimensions_and_yuv_color_fail_before_backend() {
    for mode in [
        Mode::WrongPts,
        Mode::WrongEnd,
        Mode::WrongSize,
        Mode::WrongColor,
        Mode::WrongRotation,
    ] {
        let mut f = fixture(mode);
        request(&mut f.session).unwrap();
        done(&mut f.session, 1);
        let error = f.session.poll_depth().unwrap_err();
        assert!(
            matches!(
                error,
                motion_host::Error::Depth(motion_host::depth::Error::Invalid(_))
            ),
            "{mode:?}: {error}"
        );
        assert_eq!(f.session.depth_status().unwrap().unwrap().counts.failed, 1);
        assert!(matches!(f.seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(f.session.poll_depth().unwrap().is_none());
    }
}

#[test]
fn missing_or_wrong_index_retains_native_cause_and_new_request_can_recover() {
    let mut f = fixture(Mode::Rgba);
    let asset = f.session.engine.project().video_assets[0].clone();
    let path = motion_media::video_cache_path(f.dir.path(), &asset).unwrap();
    std::fs::remove_file(&path).unwrap();
    request(&mut f.session).unwrap();
    done(&mut f.session, 1);
    let error = f.session.poll_depth().unwrap_err();
    assert!(matches!(
        error,
        motion_host::Error::Depth(motion_host::depth::Error::Media(_))
    ));
    assert!(error.to_string().contains("video cache missing"));
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 0);
    motion_media::save_index(&path, &asset, &[10_000, 40_000, 110_000]).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[8..16].copy_from_slice(&999u64.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    request(&mut f.session).unwrap();
    done(&mut f.session, 2);
    let error = f.session.poll_depth().unwrap_err();
    assert!(matches!(
        error,
        motion_host::Error::Depth(motion_host::depth::Error::Media(_))
    ));
    assert!(error.to_string().contains("video cache identity mismatch"));
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 0);
    motion_media::save_index(&path, &asset, &[10_000, 40_000, 110_000]).unwrap();
    request(&mut f.session).unwrap();
    done(&mut f.session, 3);
    assert!(f.session.poll_depth().unwrap().is_some());
    assert_eq!(f.session.depth_status().unwrap().unwrap().counts.failed, 2);
}

#[test]
fn payload_budget_is_checked_before_decoder_open_and_outside_time_is_a_normal_miss() {
    let mut f = fixture(Mode::Rgba);
    let mut policy = f.session.effects.memory_policy();
    policy.decoded_image_budget_bytes = 129; // 6*12 + 6*3 + (16+3*8) = 130.
    f.session.configure_memory_policy(policy).unwrap();
    request(&mut f.session).unwrap();
    done(&mut f.session, 1);
    assert!(f
        .session
        .poll_depth()
        .unwrap_err()
        .to_string()
        .contains("payload budget"));
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 0);
    f.session.frame = 5.;
    f.session.sample().unwrap();
    assert!(request(&mut f.session).is_none());
    let counts = f.session.depth_status().unwrap().unwrap().counts;
    assert_eq!(counts.submitted, 1);
    assert_eq!(counts.failed, 1);
    assert!(f.session.poll_depth().unwrap().is_none());
}

#[test]
fn backend_flight_has_actual_pts_and_same_sample_seek_cannot_install_late_success() {
    let mut f = fixture(Mode::BlockBackend);
    let first = request(&mut f.session).unwrap();
    let old = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let flight = f
        .session
        .depth_status()
        .unwrap()
        .unwrap()
        .in_flight
        .unwrap();
    assert_eq!(flight.request, first);
    assert_eq!(flight.phase, "backend");
    assert_eq!(flight.target_time_us, Some(10_000));
    assert_eq!(flight.source, Some(old.key.source.clone()));
    assert_eq!(old.key.source.pts.ticks, 10_000);
    // A different timeline coordinate resolves to the same VFR source sample.
    // poll must independently cancel the old binding without requiring sample().
    f.session.frame = 0.5;
    assert!(f.session.poll_depth().unwrap().is_none());
    let second = request(&mut f.session).unwrap();
    assert!(second > first);
    assert_eq!(f.session.depth_status().unwrap().unwrap().queue_depth, 1);
    f.release.send(()).unwrap();
    let new = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(old.key.same_sample(&new.key));
    done(&mut f.session, 2);
    assert!(f.session.poll_depth().unwrap().is_some());
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!(status.request, Some(second));
    assert_eq!(status.counts.completed, 1);
    assert_eq!(status.counts.canceled, 1);
    assert!(status.in_flight.is_none());
}

#[test]
fn installed_video_result_checks_live_frame_and_registered_relink_without_sample() {
    let mut f = fixture(Mode::Rgba);
    request(&mut f.session).unwrap();
    done(&mut f.session, 1);
    assert!(f.session.poll_depth().unwrap().is_some());
    f.session.frame = 0.5;
    assert!(f.session.poll_depth().unwrap().is_none());
    request(&mut f.session).unwrap();
    done(&mut f.session, 2);
    let old = f.session.poll_depth().unwrap().unwrap();
    let mut asset = f.session.engine.project().video_assets[0].clone();
    asset.path = "assets/relinked.mp4".into();
    std::fs::write(f.dir.path().join(&asset.path), b"owned").unwrap();
    let index = motion_media::video_cache_path(f.dir.path(), &asset).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &asset, &[10_000, 40_000, 110_000]).unwrap();
    f.session
        .engine
        .apply(motion_core::Command::MediaAsset {
            action: motion_core::media_assets::AssetAction::ReplaceVideo {
                video: asset,
                audio: None,
            },
        })
        .unwrap();
    assert!(f.session.poll_depth().unwrap().is_none());
    request(&mut f.session).unwrap();
    done(&mut f.session, 3);
    let new = f.session.poll_depth().unwrap().unwrap();
    assert!(!old
        .resource
        .descriptor()
        .source
        .same_sample(&new.resource.descriptor().source));
    assert_eq!(
        f.session.depth_status().unwrap().unwrap().counts.completed,
        3
    );
}

#[test]
fn replacing_same_path_video_document_rejects_old_backend_success() {
    let mut f = fixture(Mode::BlockBackend);
    request(&mut f.session).unwrap();
    f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let replacement = motion_core::Engine::new(f.session.engine.snapshot()).unwrap();
    assert_eq!(replacement.revision(), f.session.engine.revision());
    f.session
        .replace_project(replacement, f.dir.path().into())
        .unwrap();
    f.release.send(()).unwrap();
    done(&mut f.session, 1);
    assert!(f.session.poll_depth().unwrap().is_none());
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.canceled, 1);
    assert!(!status.installed && status.in_flight.is_none() && status.queue_depth == 0);
}

#[test]
fn video_bt709_transfer_is_converted_to_model_srgb_and_declared_srgb_is_preserved() {
    for (mode, expected) in [(Mode::GrayBt709, 140u8), (Mode::GraySrgb, 128u8)] {
        let mut f = fixture(mode);
        request(&mut f.session).unwrap();
        let seen = f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(seen.rgb, vec![expected; 18]);
        done(&mut f.session, 1);
        assert_eq!(
            f.session.poll_depth().unwrap().unwrap().resource.values(),
            &[f32::from(expected); 6]
        );
    }
}

#[test]
fn video_with_unknown_transfer_is_rejected_before_inference_and_keeps_cause() {
    let mut f = fixture(Mode::UnknownEncoding);
    request(&mut f.session).unwrap();
    done(&mut f.session, 1);
    let error = f.session.poll_depth().unwrap_err();
    assert!(matches!(
        error,
        motion_host::Error::Depth(motion_host::depth::Error::Media(_))
    ));
    assert!(error
        .to_string()
        .contains("controlled unknown transfer metadata"));
    assert!(matches!(f.seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(f.session.depth_status().unwrap().unwrap().counts.failed, 1);
    assert!(f.session.poll_depth().unwrap().is_none());
}
