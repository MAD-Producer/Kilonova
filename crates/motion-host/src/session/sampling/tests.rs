use super::*;
use motion_core::{render_sampling::Shutter, Ease, Layer};

fn project() -> Project {
    let mut p = Project::new(32, 32, 30, 60).unwrap();
    p.version = motion_core::CURRENT_PROJECT_VERSION;
    p.background = [0.; 4];
    for (id, color, y) in [(1, [1., 0., 0., 1.], 8.), (2, [0., 1., 0., 1.], 24.)] {
        let mut layer = Layer::solid(id, "Preview exposure", [4.; 2], [16., y, 0.], color);
        for (frame, x) in [(0, 8.), (2, 24.)] {
            layer
                .transform
                .position
                .upsert(frame, [x, y, 0.], Ease::Linear)
                .unwrap();
        }
        if id == 1 {
            layer.sampling.motion_blur = Some(Shutter {
                frames: 2.,
                samples: 4,
            });
        }
        p.layers.push(layer);
    }
    p.validate().unwrap();
    p
}
fn key() -> RequestKey {
    RequestKey {
        frame: 1.,
        size: [32, 32],
        preview: false,
        quality: motion_render::preview_memory::PreviewMemoryPolicy::default().quality(true),
        view: 0,
        surface: 1,
        surface_size: [64, 64],
        viewport: [0., 0., 64., 64.],
        image: None,
        observer: None,
    }
}
fn poll(
    state: &mut SamplingPreview,
    r: &mut Renderer,
    scratch: &mut PreviewFrameTargets,
    videos: &mut VideoFrames,
    proxies: &mut crate::preview_video::PreviewVideoCache,
    root: &std::path::Path,
    key: RequestKey,
    slice: Duration,
) -> std::result::Result<ExposurePoll, RenderError> {
    state.poll_into(r, scratch, videos, proxies, root, key, None, slice)
}
fn texture_id(target: &motion_render::RenderTarget) -> String {
    let mut ledger = TextureResourceLedger::default();
    target.collect_texture_resources(
        &mut ledger,
        "test",
        motion_render::TextureState::PreviewFront,
    );
    let id = ledger.resources().next().unwrap().id.clone();
    id
}
#[test]
fn preview_candidate_preserves_front_until_complete_and_matches_receiver_oracle() {
    let root = tempfile::tempdir().unwrap();
    let p = project();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    r.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&r, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    assert!(state.synchronize(&p, 7, &mut videos).unwrap());
    let snapshot = state.document.as_ref().unwrap().source().clone();
    let front_id = texture_id(&scratch);
    let mut pending = 0;
    for _ in 0..1000 {
        assert!(state.synchronize(&p, 7, &mut videos).unwrap());
        assert!(Arc::ptr_eq(
            &snapshot,
            state.document.as_ref().unwrap().source()
        ));
        match poll(
            &mut state,
            &mut r,
            &mut scratch,
            &mut videos,
            &mut proxies,
            root.path(),
            key(),
            Duration::from_nanos(1),
        )
        .unwrap()
        {
            ExposurePoll::Pending(_) => {
                pending += 1;
                assert!(!scratch.valid());
                assert!(scratch.prepared().is_none());
            }
            ExposurePoll::Ready(_) => break,
        }
    }
    assert!(pending > 1);
    assert_eq!(texture_id(&scratch), front_id);
    assert!(!scratch.valid());
    let actual = r
        .capture_exposure(
            state
                .request
                .as_ref()
                .unwrap()
                .task
                .as_ref()
                .unwrap()
                .target(),
        )
        .unwrap();
    let mut center = Scene::new(&p);
    center
        .sample(&p, 1., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut oracle = r.exposure_target(32, 32, &[0.25; 4]).unwrap();
    for time in [0.25, 0.75, 1.25, 1.75] {
        let mut sample = Scene::new(&p);
        sample
            .sample(&p, time, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut phase = center.clone();
        phase.layers[0] = sample.layers[0].clone();
        r.accumulate_exposure(&mut oracle, Some(&phase), false)
            .unwrap();
    }
    let expected = r.capture_exposure(&oracle).unwrap();
    assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1));
    let candidate = texture_id(scratch.prepared().unwrap());
    scratch.commit_submitted();
    assert!(scratch.valid());
    assert_eq!(texture_id(&scratch), candidate);
    let units = state.status()["workUnits"].clone();
    assert!(matches!(
        poll(
            &mut state,
            &mut r,
            &mut scratch,
            &mut videos,
            &mut proxies,
            root.path(),
            key(),
            Duration::from_secs(1)
        )
        .unwrap(),
        ExposurePoll::Ready(_)
    ));
    assert_eq!(state.status()["workUnits"], units);
    drop(oracle);
    state.cancel(&mut videos).unwrap();
    assert_eq!(r.exposure_texture_bytes(), 0);
}

#[test]
fn request_identity_releases_partial_images_and_revision_replaces_immutable_document() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    r.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&r, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    state.synchronize(&p, 1, &mut videos).unwrap();
    let original = state.document.as_ref().unwrap().source().clone();
    let mut keys = vec![key(); 9];
    keys[1].frame = 1.25;
    keys[2].view = 2;
    keys[3].surface = 2;
    keys[4].size = [16, 16];
    keys[5].viewport = [1., 0., 63., 64.];
    keys[6].image = Some([0., 0., 32., 32.]);
    keys[7].preview = true;
    keys[8].quality.video_edge = Some(512);
    let mut ids = Vec::new();
    for request in keys {
        assert!(matches!(
            poll(
                &mut state,
                &mut r,
                &mut scratch,
                &mut videos,
                &mut proxies,
                root.path(),
                request.clone(),
                Duration::from_nanos(1)
            )
            .unwrap(),
            ExposurePoll::Pending(_)
        ));
        let ledger = state.ledger();
        assert_eq!(ledger.total_known_bytes(), r.exposure_texture_bytes());
        let id = state
            .request
            .as_ref()
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .target()
            .texture_resource_ledger()
            .resources()
            .next()
            .unwrap()
            .id
            .clone();
        assert!(!ids.contains(&id));
        ids.push(id);
        assert_eq!(state.request.as_ref().unwrap().key, request);
    }
    p.layers[0].content = motion_core::Content::Solid {
        color: [0., 0., 1., 1.],
    };
    state.synchronize(&p, 2, &mut videos).unwrap();
    assert!(state.request.is_none());
    assert_eq!(r.exposure_texture_bytes(), 0);
    assert!(!Arc::ptr_eq(
        &original,
        state.document.as_ref().unwrap().source()
    ));
    assert_eq!(
        original.layers[0].content,
        motion_core::Content::Solid {
            color: [1., 0., 0., 1.]
        }
    );
    p.layers[0].sampling.motion_blur = None;
    assert!(!state.synchronize(&p, 3, &mut videos).unwrap());
    assert!(state.document.is_none());
}

#[test]
fn allocation_failure_is_terminal_until_explicit_memory_policy_disposition() {
    let root = tempfile::tempdir().unwrap();
    let p = project();
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    r.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&r, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    state.synchronize(&p, 1, &mut videos).unwrap();
    let mut invalid = key();
    invalid.size = [r.device.limits().max_texture_dimension_2d + 1, 1];
    let error = poll(
        &mut state,
        &mut r,
        &mut scratch,
        &mut videos,
        &mut proxies,
        root.path(),
        invalid.clone(),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error.to_string().contains("phase=request"));
    assert_eq!(state.status()["state"], "failed");
    assert_eq!(r.exposure_texture_bytes(), 0);
    let before = state.cpu_us();
    for _ in 0..60 {
        let again = poll(
            &mut state,
            &mut r,
            &mut scratch,
            &mut videos,
            &mut proxies,
            root.path(),
            invalid.clone(),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(again.to_string(), error.to_string());
        assert_eq!(again.resource_rejection(), error.resource_rejection());
    }
    assert_eq!(state.cpu_us(), before);
    state.discard_request();
    for _ in 0..100 {
        if matches!(
            poll(
                &mut state,
                &mut r,
                &mut scratch,
                &mut videos,
                &mut proxies,
                root.path(),
                key(),
                Duration::from_secs(1)
            )
            .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            return;
        }
    }
    panic!("replacement quality did not complete");
}
