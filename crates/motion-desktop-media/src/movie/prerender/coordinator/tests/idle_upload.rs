use super::*;

fn flush(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

fn completion_boundary(playing: bool) {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut policy = policy();
    policy.idle_delay = Duration::ZERO;
    let mut owner = Coordinator::new(policy, 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    // The frozen producer shares the session allocation budget but creates its
    // own persistent renderer on the first draw. Initialize that real owner
    // without installing any foreground cache entry, before taking the upload
    // baseline; its live pipeline resources are not an unsubmitted transfer.
    let worker = owner.worker.as_mut().unwrap();
    assert!(worker.render(0.).unwrap().is_some());
    let began = Instant::now();
    loop {
        if let Some(done) = worker.poll().unwrap() {
            let worker::Outcome::Frame(frame) = done.outcome else {
                panic!("native upload fixture failed to initialize its producer");
            };
            assert_eq!(frame.image().pixels, [255, 0, 0, 255].repeat(32 * 24));
            drop(frame);
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    flush(&renderer);
    renderer.set_submission_tracking(true);
    let submitted = || {
        renderer
            .submission_memory_info(&Default::default())
            .submitted
    };
    let before_submissions = submitted();
    let before_bytes = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut demand = demand();
    demand.playing = playing;
    demand.now = Duration::from_secs(2);
    if playing {
        owner.advance_playback(demand).unwrap();
    } else {
        owner.update(demand).unwrap();
    }
    assert!(
        owner
            .schedule_with_yield(&renderer, YieldControl::default())
            .unwrap()
    );
    assert!(drain(&mut owner, &renderer));
    assert_eq!(owner.cache_info().unwrap().resident_frames, 1);
    assert_eq!(
        submitted(),
        before_submissions + u64::from(!playing),
        "idle warmup must submit its own upload; playback must retain foreground fusion"
    );

    // Editing invalidates the cached pixels before any foreground submission.
    // A stale optional upload must not keep an idle destination charged until
    // that next foreground frame merely because it was called 'warm'.
    owner.invalidate().unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before_bytes + if playing { 3072 } else { 0 },
        "idle invalidation still leaves an unsubmitted optional allocation"
    );
    flush(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before_bytes
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn idle_warmup_submits_and_retires_before_the_next_foreground_frame() {
    completion_boundary(false);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn playback_warmup_still_fuses_with_its_next_foreground_submission() {
    completion_boundary(true);
}
