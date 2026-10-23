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
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
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
    let mut center = Scene::new(&p).unwrap();
    center
        .sample(&p, 1., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut oracle = r.exposure_target(32, 32, &[0.25; 4]).unwrap();
    for time in [0.25, 0.75, 1.25, 1.75] {
        let mut sample = Scene::new(&p).unwrap();
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
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
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
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
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

#[test]
fn observation_request_and_poll_exhaustion_never_wrap_or_refuse_the_request() {
    let sequence = AtomicU64::new(u64::MAX - 1);
    assert_eq!(next_request_id(&sequence), Some(u64::MAX - 1));
    assert_eq!(next_request_id(&sequence), None);
    assert_eq!(next_request_id(&sequence), None);
    let mut request = Request::new(key());
    let id = request.id;
    assert!(id.is_some());
    assert_eq!(request.begin_poll(), ExposureCommandContext { request: id, poll: Some(1) });
    request.poll = Some(u64::MAX - 1);
    assert_eq!(request.begin_poll().poll, Some(u64::MAX));
    for _ in 0..3 {
        assert_eq!(request.begin_poll(), ExposureCommandContext { request: id, poll: None });
    }
    assert!(request.failure.is_none());
    assert!(request.task.is_none());
    assert_eq!(request.cpu_us, 0);
    assert_eq!(request.key, key());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_exposure_request_survives_pending_polls_and_late_commands_keep_the_original_identity() {
    use motion_render::{gpu_commands::CommandStage, AllocationFrameOutcome, FrameOperation};
    let root = tempfile::tempdir().unwrap();
    let p = project();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut r = pollster::block_on(Renderer::new_profiled(
        &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, true,
    )).unwrap();
    r.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&r, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    state.synchronize(&p, 1, &mut videos).unwrap();
    let Some(observer) = r.start_command_profiling(32).unwrap() else {
        eprintln!("GPU_EXPOSURE_IDENTITY available=false; no native identity acceptance");
        return;
    };
    let mut calls = 0;
    let mut poll_contexts = std::collections::BTreeMap::new();
    let mut drive = |state: &mut SamplingPreview, r: &mut Renderer, key: RequestKey| {
        calls += 1;
        let request = 900 + calls;
        let window = r.allocation_budget().begin_frame_at(request, FrameOperation::Present, key.frame)
            .unwrap().unwrap();
        let result = poll(state, r, &mut scratch, &mut videos, &mut proxies, root.path(), key,
            Duration::from_nanos(1)).unwrap();
        let exposure = state.command_context().unwrap();
        assert!(!state.status()["identityExhausted"].as_bool().unwrap());
        poll_contexts.insert((exposure.request.unwrap(), exposure.poll.unwrap()), request);
        assert!(observer.snapshot().commands.active_exposure_scopes.is_empty());
        window.finish(if matches!(result, ExposurePoll::Ready(_)) {
            AllocationFrameOutcome::Rendered
        } else {
            AllocationFrameOutcome::Skipped
        });
        result
    };
    let mut original = None;
    let mut old_commands = Vec::new();
    for _ in 0..100 {
        assert!(matches!(drive(&mut state, &mut r, key()), ExposurePoll::Pending(_)));
        let identity = state.command_context().unwrap();
        let request = *original.get_or_insert(identity.request.unwrap());
        assert_eq!(identity.request, Some(request));
        old_commands = observer.snapshot().commands.active;
        if !old_commands.is_empty() { break; }
    }
    assert!(!old_commands.is_empty(), "first request never submitted actual GPU work");
    assert!(old_commands.iter().all(|record| record.exposure.unwrap().request == original));
    let mut replacement = key();
    replacement.frame = 1.25;
    let mut ready = false;
    let mut replacement_id = None;
    for _ in 0..1000 {
        let result = drive(&mut state, &mut r, replacement.clone());
        let identity = state.command_context().unwrap();
        let id = *replacement_id.get_or_insert(identity.request.unwrap());
        assert_ne!(Some(id), original);
        assert_eq!(identity.request, Some(id));
        // Callbacks for the abandoned request can now run while its replacement
        // is current. Original words, IDs and frame contexts must survive.
        r.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
        if matches!(result, ExposurePoll::Ready(_)) { ready = true; break; }
    }
    assert!(ready, "replacement exposure did not complete");
    drop(drive);
    let snapshot = observer.snapshot();
    assert_eq!(snapshot.commands.counts.omitted_slots, 0);
    assert_eq!(snapshot.commands.counts.evicted_recent, 0);
    assert!(!snapshot.commands.observation_fault);
    assert!(snapshot.commands.active.is_empty());
    for old in old_commands {
        let actual = snapshot.commands.recent.iter().find(|record| record.id == old.id).unwrap();
        assert_eq!(actual.exposure, old.exposure);
        assert_eq!(actual.timestamp_words.is_some_and(|w| w[1] > w[0]), true);
    }
    let records: Vec<_> = snapshot.commands.recent.iter().filter(|record| record.exposure.is_some()).collect();
    assert!(records.len() > 2);
    assert!(records.iter().any(|record| record.stage == CommandStage::ExposurePhase));
    assert!(records.iter().any(|record| record.stage == CommandStage::ExposureResolve));
    for record in &records {
        let exposure = record.exposure.unwrap();
        let expected = poll_contexts[&(exposure.request.unwrap(), exposure.poll.unwrap())];
        assert_eq!(record.contexts.len(), 1);
        assert_eq!(record.contexts[0].request, Some(expected));
        assert_eq!(record.contexts[0].exposure_request, exposure.request);
        assert_eq!(record.contexts[0].exposure_poll, exposure.poll);
    }
    let pixels = r.capture_exposure(state.request.as_ref().unwrap().task.as_ref().unwrap().target()).unwrap();
    assert_eq!(pixels.len(), 32 * 32 * 4);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    // Readback after the poll must not inherit its exposure identity.
    assert!(observer.snapshot().commands.recent.iter()
        .filter(|record| record.stage == CommandStage::ReadbackCopy).all(|record| record.exposure.is_none()));
    eprintln!("GPU_EXPOSURE_IDENTITY {}", json!({
        "available":true,"requests":[original,replacement_id],"actualPolls":calls,
        "actualCommandRecords":records.len(),"actualPixelsChecked":32*32,
        "scope":"actual host exposure polls, native timestamp callbacks and supersession; no video decoding or surface presentation acceptance"
    }));
}
