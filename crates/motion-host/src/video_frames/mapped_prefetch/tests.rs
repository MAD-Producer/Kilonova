use super::*;
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Composition, CompositionClip, Easing, Layer, VideoClip, WorkArea,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

const PTS: [u64; 7] = [0, 70_000, 170_000, 300_000, 450_000, 620_000, 800_000];
fn fixture(root: &Path) -> Project {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/vfr.mp4"), b"1234").unwrap();
    let asset: VideoAsset = serde_json::from_value(json!({
        "id":1,"path":"assets/vfr.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":1000000,"duration_us":1000000,
        "frame_count":7,"variable_frame_rate":true,"nominal_frame_rate":24.,
        "color_standard":1,"color_range":2,"audio_asset":null
    }))
    .unwrap();
    let cache = motion_media::video_cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    motion_media::save_index(&cache, &asset, &PTS).unwrap();
    let mut p = Project::new(16, 16, 10, 10).unwrap();
    let mut layer = Layer::solid(7, "Mapped VFR", [16.; 2], [8., 8., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    p.layers.push(layer);
    p.video_assets.push(asset);
    p
}
fn reverse(pivot: f64) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., pivot).unwrap(),
        remap: None,
    }
}
fn remap(points: &[(f64, f64)]) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::default(),
        remap: Some(
            TimeRemap::new(
                points
                    .iter()
                    .map(|&(frame, source_frame)| TimeRemapKey {
                        frame,
                        source_frame,
                        easing: Easing::default(),
                    })
                    .collect(),
            )
            .unwrap(),
        ),
    }
}
fn scene(p: &Project, frame: f64) -> Scene {
    p.validate().unwrap();
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
fn prediction(p: &Project, frame: f64) -> Times {
    let s = scene(p, frame);
    let layers = s.video_decode_layers();
    assert_eq!(layers.len(), 1);
    plans(p, &s, &layers).unwrap()[&layers[0].id]
}

#[test]
fn predicts_reverse_turn_freeze_stretch_and_exclusive_edges_without_pts_rounding() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(reverse(10.));
    assert_eq!(
        prediction(&p, 1.),
        [Some(799_999), Some(699_999), Some(599_999), Some(499_999)]
    );
    assert_eq!(prediction(&p, 8.), [Some(99_999), None, None, None]);
    p.layers[0].time_mapping = Some(remap(&[(0., 0.), (3., 9.), (6., 0.), (10., 9.)]));
    assert_eq!(
        prediction(&p, 1.5),
        [Some(750_000), Some(749_999), Some(449_999), Some(149_999)]
    );
    p.layers[0].time_mapping = Some(remap(&[(0., 5.)]));
    assert_eq!(prediction(&p, 1.), [Some(500_000); LOOKAHEAD]);
    p.layers[0].time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(1, 2).unwrap(),
            TimeDirection::Forward,
            0.,
            0.,
        )
        .unwrap(),
        remap: None,
    });
    assert_eq!(
        prediction(&p, 1.),
        [Some(400_000), Some(600_000), Some(800_000), None]
    );
    p.layers[0].time_mapping = None;
    let s = scene(&p, 1.);
    assert!(
        plans(&p, &s, &s.video_decode_layers()).unwrap().is_empty(),
        "ordinary clips retain legacy policy"
    );
}

#[test]
fn nested_double_reverse_retains_source_start_fps_and_instance_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(reverse(20.));
    p.compositions.push(Composition {
        id: "comp-prefetch".into(),
        name: "20 fps child".into(),
        width: 16,
        height: 16,
        fps: 20,
        frames: 20,
        work_area: WorkArea::full(20),
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: std::mem::take(&mut p.layers),
        expressions: vec![],
    });
    let mut reference = Layer::solid(17, "Reference", [16.; 2], [8., 8., 0.], [1.; 4]);
    let mut clip = CompositionClip::new("comp-prefetch".into());
    clip.source_start_frame = 2;
    reference.content = Content::Composition { clip };
    reference.time_mapping = Some(reverse(10.));
    p.layers.push(reference.clone());
    reference.id = 18;
    if let Content::Composition { clip } = &mut reference.content {
        clip.source_start_frame = 0;
    }
    p.layers.push(reference);
    let s = scene(&p, 1.);
    let layers = s.video_decode_layers();
    assert_eq!(layers.len(), 2);
    assert_ne!(layers[0].id, layers[1].id);
    let result = plans(&p, &s, &layers).unwrap();
    for (nested, expected) in s.nested.iter().zip([
        [Some(100_000), Some(200_000), Some(300_000), Some(400_000)],
        [Some(200_000), Some(300_000), Some(400_000), Some(500_000)],
    ]) {
        let id = nested.scene.video_decode_layers()[0].id;
        assert_eq!(result[&id], expected);
    }
}

#[test]
fn invalid_scene_plan_retains_the_original_model_cause_and_source_context() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    let s = scene(&p, 1.);
    p.layers.clear();
    let error = plans(&p, &s, &s.video_decode_layers()).unwrap_err();
    match error.primary() {
        MediaError::Model(cause) => {
            assert!(matches!(cause.as_ref(), motion_core::Error::Missing(7)))
        }
        other => panic!("lost model cause: {other:?}"),
    }
    let text = error.to_string();
    assert!(
        text.contains("object=7") && text.contains("source=1") && text.contains("phase=resolve"),
        "{text}"
    );
}

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
#[derive(Default)]
struct Control {
    calls: Mutex<Vec<u64>>,
    fail: AtomicU64,
    gate: Mutex<Option<(u64, Arc<Gate>)>>,
}
struct Backend(Arc<Control>);
struct Decoder(Arc<Control>);
impl crate::Platform for Backend {
    fn name(&self) -> &'static str {
        "mapped-prefetch-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<motion_media::VideoProbe> {
        Err("unused".into())
    }
    fn open_decoder(&self, _: &Path, _: VideoAsset, _: Vec<u64>) -> Result<Box<dyn VideoDecoder>> {
        Ok(Box::new(Decoder(self.0.clone())))
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> Result<()>,
    ) -> Result<motion_core::AudioAsset> {
        Err("unused".into())
    }
    fn media_capabilities(&self, _: Option<&crate::VideoQuery>) -> Result<Value> {
        Err("unused".into())
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> crate::Result<crate::SurfaceTarget> {
        Err("unused".into())
    }
}
impl VideoDecoder for Decoder {
    fn frame(&mut self, time: u64, check: &dyn Fn() -> Result<()>) -> Result<DecodedFrame> {
        check()?;
        self.0.calls.lock().unwrap().push(time);
        let gate = self.0.gate.lock().unwrap().take();
        if let Some((target, gate)) = gate {
            assert_eq!(target, time);
            gate.entered.send(()).unwrap();
            gate.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        check()?;
        if time != 0 && self.0.fail.load(Ordering::Acquire) == time {
            return Err("controlled mapped speculative failure".into());
        }
        let index = PTS.iter().position(|p| *p == time).unwrap();
        Ok(DecodedFrame {
            pixels: crate::video_frame::VideoPixels::Rgba(vec![index as u8; 16]),
            pts: time,
            end: PTS.get(index + 1).copied().unwrap_or(1_000_000),
            width: 2,
            height: 2,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "authored",
            decoder_name: "controlled".into(),
        })
    }
}
fn wait(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "mapped worker did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ready(reader: &mut VideoFrames, p: &Project, root: &Path, s: &Scene) {
    wait(|| reader.prepare_scene(p, root, s, s.frame).unwrap().is_some());
}

#[test]
fn real_worker_prefetches_reverse_pts_and_reuses_them_on_the_next_output_frame() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(reverse(10.));
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let s = scene(&p, 1.);
    let id = s.video_decode_layers()[0].id;
    ready(&mut reader, &p, root.path(), &s);
    wait(|| {
        let state = reader.streams[&id].shared.0.lock().unwrap();
        state.cache.find(450_000).is_some() && state.in_flight.is_none()
    });
    assert_eq!(*control.calls.lock().unwrap(), [800_000, 620_000, 450_000]);
    let hits = reader.streams[&id].shared.0.lock().unwrap().hits;
    let next = scene(&p, 2.);
    let frames = reader
        .prepare_scene(&p, root.path(), &next, 2.)
        .unwrap()
        .expect("prefetched reverse frame must already be ready");
    assert_eq!(frames[0].1.left.pts, 620_000);
    assert_eq!(reader.streams[&id].shared.0.lock().unwrap().hits, hits + 1);
    wait(|| {
        let state = reader.streams[&id].shared.0.lock().unwrap();
        job(&state, &PTS).is_none() && state.in_flight.is_none()
    });
    assert_eq!(
        *control.calls.lock().unwrap(),
        [800_000, 620_000, 450_000, 300_000]
    );
    let later = scene(&p, 4.);
    let frames = reader
        .prepare_scene(&p, root.path(), &later, 4.)
        .unwrap()
        .expect("reverse lookahead must survive the intervening request");
    assert_eq!(frames[0].1.left.pts, 450_000);
    assert_eq!(
        control
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|time| **time == 450_000)
            .count(),
        1
    );
    {
        let state = reader.streams[&id].shared.0.lock().unwrap();
        assert!(state.cache.bytes() <= state.cache.budget());
        assert!(state.cache.len() <= LOOKAHEAD + 1);
    }
    reader.finish().unwrap();
}

#[test]
fn freeze_and_nonmonotonic_plan_are_deduplicated_and_keep_required_blend_pair_bounded() {
    let root = tempfile::tempdir().unwrap();
    let mut p = fixture(root.path());
    p.layers[0].time_mapping = Some(remap(&[(0., 5.)]));
    let control = Arc::new(Control::default());
    let mut reader = VideoFrames::new(Arc::new(Backend(control.clone())));
    let s = scene(&p, 1.);
    let id = s.video_decode_layers()[0].id;
    ready(&mut reader, &p, root.path(), &s);
    {
        let state = reader.streams[&id].shared.0.lock().unwrap();
        assert!(job(&state, &PTS).is_none());
        assert_eq!(state.cache.len(), 1);
    }
    assert_eq!(*control.calls.lock().unwrap(), [450_000]);
    p.layers[0].time_mapping = Some(remap(&[(0., 0.), (3., 9.), (6., 0.), (10., 9.)]));
    p.layers[0].sampling.frame_blend = motion_core::render_sampling::FrameBlend::Linear;
    let s = scene(&p, 1.6);
    ready(&mut reader, &p, root.path(), &s);
    wait(|| {
        let state = reader.streams[&id].shared.0.lock().unwrap();
        job(&state, &PTS).is_none() && state.in_flight.is_none()
    });
    let state = reader.streams[&id].shared.0.lock().unwrap();
    assert_eq!(state.desired.unwrap().next, Some(620_000));
    assert!(state.cache.find(450_000).is_some() && state.cache.find(620_000).is_some());
    assert!(
        state.cache.find(800_000).is_some()
            && state.cache.find(300_000).is_some()
            && state.cache.find(70_000).is_some()
    );
    assert_eq!(state.cache.len(), LOOKAHEAD + 1);
    assert!(state.cache.bytes() <= state.cache.budget());
    drop(state);
    reader.finish().unwrap();
}

#[test]
fn superseded_mapped_speculation_cancels_without_poisoning_the_current_frame_or_retrying() {
    let root = tempfile::tempdir().unwrap();
    let p = fixture(root.path());
    let control = Arc::new(Control::default());
    let stream = Stream::new(
        root.path().into(),
        p.video_assets[0].clone(),
        Arc::new(Backend(control.clone())),
        7,
    )
    .unwrap();
    let empty = [None; LOOKAHEAD];
    stream
        .request_blend_plan(800_000, 1, true, false, Some(&empty))
        .unwrap();
    wait(|| stream.ready(1).is_ok());
    let (entered, entry) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    *control.gate.lock().unwrap() = Some((
        620_000,
        Arc::new(Gate {
            entered,
            release: Mutex::new(resume),
        }),
    ));
    stream
        .request_blend_plan(
            800_000,
            1,
            true,
            false,
            Some(&[Some(620_000), None, None, None]),
        )
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    stream
        .request_blend_plan(800_000, 1, true, false, Some(&empty))
        .unwrap();
    release.send(()).unwrap();
    wait(|| stream.shared.0.lock().unwrap().in_flight.is_none());
    let state = stream.shared.0.lock().unwrap();
    assert!(state.cache.find(620_000).is_none());
    assert!(state.error.is_none() && state.failed_prefetch.is_none());
    assert_eq!(state.cancelled, 1);
    assert_eq!(*control.calls.lock().unwrap(), [800_000, 620_000]);
    drop(state);
    assert_eq!(stream.ready(1).unwrap().pts, 800_000);
}

#[test]
fn mapped_speculative_failure_stays_visible_without_retrying_or_poisoning_the_required_frame() {
    let root = tempfile::tempdir().unwrap();
    let p = fixture(root.path());
    let control = Arc::new(Control::default());
    control.fail.store(620_000, Ordering::Release);
    let stream = Stream::new(
        root.path().into(),
        p.video_assets[0].clone(),
        Arc::new(Backend(control.clone())),
        7,
    )
    .unwrap();
    let future = [Some(620_000), Some(450_000), None, None];
    stream
        .request_blend_plan(800_000, 1, true, false, Some(&future))
        .unwrap();
    wait(|| stream.shared.0.lock().unwrap().failed_prefetch == Some(620_000));
    assert_eq!(stream.ready(1).unwrap().pts, 800_000);
    for _ in 0..120 {
        stream
            .request_blend_plan(800_000, 1, true, false, Some(&future))
            .unwrap();
    }
    {
        let state = stream.shared.0.lock().unwrap();
        assert_eq!(state.prefetch_failures, 1);
        assert_eq!(
            state.last_prefetch_error.as_deref(),
            Some("controlled mapped speculative failure")
        );
        assert!(job(&state, &PTS).is_none());
    }
    assert_eq!(*control.calls.lock().unwrap(), [800_000, 620_000]);
    stream
        .request_blend_plan(620_000, 2, false, false, None)
        .unwrap();
    wait(|| stream.shared.0.lock().unwrap().error.is_some());
    assert_eq!(
        stream.ready(2).err().unwrap().to_string(),
        "controlled mapped speculative failure"
    );
    control.fail.store(0, Ordering::Release);
    stream
        .request_blend_plan(620_000, 3, false, false, None)
        .unwrap();
    wait(|| stream.ready(3).is_ok());
    assert_eq!(stream.ready(3).unwrap().pts, 620_000);
}
