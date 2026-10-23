use super::*;

fn layer(id: u64, source: u64) -> Layer {
    let mut layer = Layer::solid(id, "Native retirement", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(source),
    };
    layer
}

fn fixture(root: &Path) -> (Project, VideoProbe) {
    std::fs::create_dir(root.join("assets")).unwrap();
    let source = root.join("assets/source.mp4");
    std::fs::copy(super::super::fixture("silent-24fps.mp4"), &source).unwrap();
    let mut probe = DesktopPlatform::new()
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &probe.asset).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let mut project = Project::new(32, 32, 24, 72).unwrap();
    project.video_assets.push(probe.asset.clone());
    project.layers.push(layer(7, 1));
    project.validate().unwrap();
    (project, probe)
}

fn gate(open: bool) -> (Arc<Gate>, std::sync::mpsc::Receiver<()>) {
    let (entered, waiting) = channel();
    (
        Arc::new(Gate {
            open: Mutex::new(open),
            changed: Condvar::new(),
            entered,
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            closed: AtomicUsize::new(0),
        }),
        waiting,
    )
}

fn ready(
    frames: &mut VideoFrames,
    project: &Project,
    root: &Path,
    object: u64,
    sequence: u64,
    probe: &VideoProbe,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = frames.request(project, root, object, 0., sequence).unwrap();
        assert_ne!(status["state"], "failed", "{status}");
        if status["state"] == "ready" {
            break;
        }
        assert!(Instant::now() < deadline, "{status}");
        std::thread::sleep(Duration::from_millis(1));
    }
    let frame = frames.frame(project, object, sequence).unwrap();
    assert_eq!(frame.pts, probe.timestamps[0]);
    assert_eq!(frame.rgba().unwrap().as_ref(), probe.first_rgba.as_slice());
}

fn remove_and_readd(replacement: u64) {
    let root = tempfile::tempdir().unwrap();
    let (mut project, probe) = fixture(root.path());
    let (gate, waiting) = gate(false);
    let _release = ReleaseOnDrop(gate.clone());
    let mut frames = VideoFrames::new(Arc::new(NativePlatform(gate.clone())));
    frames.request(&project, root.path(), 7, 0., 1).unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 1);

    // Exercise scene removal, rather than treating clear() as layer deletion.
    project.layers.clear();
    let mut empty = motion_render::Scene::new(&project).unwrap();
    empty
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(frames
        .prepare_scene_exact(&project, root.path(), &empty)
        .unwrap()
        .unwrap()
        .is_empty());
    assert_eq!(frames.metrics()["retiringStreams"], 1);
    project.layers.push(layer(replacement, 1));
    project.validate().unwrap();

    let status = frames
        .request(&project, root.path(), replacement, 0., 2)
        .unwrap();
    if status["reason"] != "decoder_retirement" {
        // In the old implementation, wait until the extra real FFmpeg decoder
        // has opened and decoded before testing its actual resource count.
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert_eq!(
        gate.live.load(Ordering::Acquire),
        1,
        "same source reopened before native closure: {status}"
    );
    assert_eq!(gate.peak.load(Ordering::Acquire), 1);
    assert_eq!(status["state"], "pending");
    assert_eq!(status["reason"], "decoder_retirement");
    assert_eq!(status["blocked_object"], 7);
    assert_eq!(frames.metrics()["streams"], 0);
    for _ in 0..120 {
        let status = frames
            .request(&project, root.path(), replacement, 0., 2)
            .unwrap();
        assert_eq!(status["reason"], "decoder_retirement");
        assert_eq!(gate.live.load(Ordering::Acquire), 1);
    }
    gate.release();
    ready(&mut frames, &project, root.path(), replacement, 2, &probe);
    assert_eq!(gate.peak.load(Ordering::Acquire), 1);
    assert_eq!(gate.closed.load(Ordering::Acquire), 1);
    frames.finish().unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 0);
    assert_eq!(gate.closed.load(Ordering::Acquire), 2);
    std::fs::rename(
        root.path().join("assets/source.mp4"),
        root.path().join("assets/closed.mp4"),
    )
    .unwrap();
}

#[test]
fn removed_and_readded_layer_waits_for_native_close_below_the_four_worker_limit() {
    remove_and_readd(7);
}

#[test]
fn replacement_object_cannot_double_the_retiring_sources_native_decoders() {
    remove_and_readd(8);
}

#[test]
fn two_active_instances_of_one_source_still_decode_independently() {
    let root = tempfile::tempdir().unwrap();
    let (mut project, probe) = fixture(root.path());
    project.layers.push(layer(8, 1));
    project.validate().unwrap();
    let (gate, _waiting) = gate(true);
    let _release = ReleaseOnDrop(gate.clone());
    let mut frames = VideoFrames::new(Arc::new(NativePlatform(gate.clone())));
    ready(&mut frames, &project, root.path(), 7, 1, &probe);
    ready(&mut frames, &project, root.path(), 8, 1, &probe);
    assert_eq!(gate.live.load(Ordering::Acquire), 2);
    assert_eq!(gate.peak.load(Ordering::Acquire), 2);
    frames.finish().unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 0);
}

#[test]
fn a_different_source_can_open_while_the_removed_source_is_still_retiring() {
    let root = tempfile::tempdir().unwrap();
    let (mut project, probe) = fixture(root.path());
    let mut other = probe.asset.clone();
    other.id = 2;
    other.path = "assets/other.mp4".into();
    std::fs::copy(
        root.path().join("assets/source.mp4"),
        root.path().join(&other.path),
    )
    .unwrap();
    let index = motion_media::video_cache_path(root.path(), &other).unwrap();
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &other, &probe.timestamps).unwrap();
    project.video_assets.push(other);
    let (gate, waiting) = gate(false);
    let _release = ReleaseOnDrop(gate.clone());
    let mut frames = VideoFrames::new(Arc::new(NativePlatform(gate.clone())));
    frames.request(&project, root.path(), 7, 0., 1).unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    frames.clear();
    project.layers = vec![layer(8, 2)];
    project.validate().unwrap();
    let status = frames.request(&project, root.path(), 8, 0., 2).unwrap();
    assert_ne!(status["reason"], "decoder_retirement");
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 2);
    gate.release();
    ready(&mut frames, &project, root.path(), 8, 2, &probe);
    frames.finish().unwrap();
    assert_eq!(gate.live.load(Ordering::Acquire), 0);
}
