use super::*;
mod foreground_logging;
mod source_freshness;

fn policy() -> schedule::Policy {
    schedule::Policy {
        enabled: true,
        window: 2,
        ..Default::default()
    }
}
fn demand() -> Demand {
    Demand {
        frame: 0.,
        direction: schedule::Direction::Forward,
        playing: true,
        editing: false,
        foreground_pending: false,
        pressure: false,
        now: Duration::ZERO,
    }
}
fn session(root: &Path) -> Session {
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Coordinator pixels",
        [32., 24.],
        [16., 12., 0.],
        [1., 0., 0., 1.],
    ));
    Session::new(project, root.into(), crate::platform()).unwrap()
}
fn prepare(owner: &mut Coordinator, session: &Session, renderer: &Renderer) {
    let mut plan =
        PrerenderPlan::capture(session, 1, 2, 82, 3072, &ExportControl::default()).unwrap();
    plan.adapter = Some(renderer.adapter.clone());
    assert!(owner.prepare(plan, ExportControl::default()).unwrap());
    drain(owner, renderer);
    assert!(owner.profile.is_some());
}
fn drain(owner: &mut Coordinator, renderer: &Renderer) -> bool {
    let began = Instant::now();
    let mut installed = false;
    while owner.active.is_some() {
        installed |= owner.poll(renderer).unwrap();
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "coordinator failed to acknowledge"
        );
        std::thread::yield_now();
    }
    installed
}
fn pixels(renderer: &Renderer, source: &motion_render::RenderTarget) -> Vec<u8> {
    renderer.read_native_render_target(source).unwrap().pixels
}

#[test]
fn disabled_coordinator_owns_no_worker_or_native_cache() {
    let owner = Coordinator::new(Default::default(), 0, 0).unwrap();
    assert!(owner.observation().is_none());
    assert!(owner.cache_info().is_none());
    assert!(owner.active.is_none());
    assert!(!owner.status().enabled);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn yielded_coordinator_keeps_warm_pixels_and_resumes_without_preparing_again() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex,
    };
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let preparations = Arc::new(AtomicUsize::new(0));
    let initialized = Arc::new(AtomicUsize::new(0));
    let freezes = preparations.clone();
    let initializations = initialized.clone();
    let draws = AtomicUsize::new(0);
    owner.worker = Some(
        Worker::start_with_phase_hook(3072, move |_, phase| {
            if phase == "freeze" {
                freezes.fetch_add(1, Ordering::AcqRel);
            }
            if phase == "initialize" {
                initializations.fetch_add(1, Ordering::AcqRel);
            }
            if phase == "draw" && draws.fetch_add(1, Ordering::AcqRel) == 1 {
                entered.send(()).unwrap();
                blocked
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
            }
        })
        .unwrap(),
    );
    prepare(&mut owner, &session, &renderer);
    owner.advance_playback(demand()).unwrap();
    assert!(owner
        .schedule_with_yield(&renderer, YieldControl::default())
        .unwrap());
    assert!(drain(&mut owner, &renderer));
    let warm = owner.get(&renderer, 1.).unwrap().unwrap();
    let token = YieldControl::default();
    assert!(owner.schedule_with_yield(&renderer, token.clone()).unwrap());
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(token.preempt());
    assert_eq!(owner.cache_info().unwrap().pending_bytes, 3072);
    release.send(()).unwrap();
    assert!(!drain(&mut owner, &renderer));
    assert!(owner.profile().is_some());
    assert_eq!(owner.status().counts.completed, 1);
    assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);
    let kept = owner.get(&renderer, 1.).unwrap().unwrap();
    assert!(Arc::ptr_eq(&warm, &kept));
    owner
        .advance_playback(Demand {
            frame: 1.,
            now: Duration::from_millis(34),
            ..demand()
        })
        .unwrap();
    assert!(owner
        .schedule_with_yield(&renderer, YieldControl::default())
        .unwrap());
    assert!(drain(&mut owner, &renderer));
    let resumed = owner.get(&renderer, 2.).unwrap().unwrap();
    assert_eq!(resumed.key().frame(), 2.);
    assert_eq!(
        pixels(&renderer, resumed.source()),
        [255, 0, 0, 255].repeat(32 * 24)
    );
    assert_eq!(owner.status().counts.completed, 2);
    assert_eq!(preparations.load(Ordering::Acquire), 1);
    assert_eq!(initialized.load(Ordering::Acquire), 1);
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn scheduler_foreground_cancellation_reuses_native_work_but_pressure_retires_it() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex,
    };
    for case in [
        "running_foreground",
        "completed_foreground",
        "running_pressure",
    ] {
        let root = tempfile::tempdir().unwrap();
        let session = session(root.path());
        let renderer = pollster::block_on(Renderer::headless_with_working_precision(
            session.engine.project().working_precision,
            session.allocation_budget().clone(),
        ))
        .unwrap();
        let (entered, waiting) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let blocked = Mutex::new(blocked);
        let preparations = Arc::new(AtomicUsize::new(0));
        let initialized = Arc::new(AtomicUsize::new(0));
        let freezes = preparations.clone();
        let initializations = initialized.clone();
        let first_draw = std::sync::atomic::AtomicBool::new(true);
        let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
        owner.worker = Some(
            Worker::start_with_phase_hook(3072, move |_, phase| {
                if phase == "freeze" {
                    freezes.fetch_add(1, Ordering::AcqRel);
                }
                if phase == "initialize" {
                    initializations.fetch_add(1, Ordering::AcqRel);
                }
                if case != "completed_foreground"
                    && phase == "draw"
                    && first_draw.swap(false, Ordering::AcqRel)
                {
                    entered.send(()).unwrap();
                    blocked
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(30))
                        .unwrap();
                }
            })
            .unwrap(),
        );
        prepare(&mut owner, &session, &renderer);
        owner.advance_playback(demand()).unwrap();
        let token = YieldControl::default();
        assert!(owner.schedule_with_yield(&renderer, token.clone()).unwrap());
        if case == "completed_foreground" {
            let began = Instant::now();
            while !token.completed() {
                assert!(began.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
        } else {
            waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        }
        let pressure = case == "running_pressure";
        // Exercise the scheduler/Coordinator cancellation path, rather than
        // calling token.preempt directly and bypassing the production defect.
        owner
            .advance_playback(Demand {
                foreground_pending: !pressure,
                pressure,
                ..demand()
            })
            .unwrap();
        assert_eq!(owner.status().counts.canceled, 1);
        assert_eq!(owner.cache_info().unwrap().pending_bytes, 3072);
        if case != "completed_foreground" {
            release.send(()).unwrap();
        }
        assert!(!drain(&mut owner, &renderer));
        assert_eq!(owner.profile().is_some(), !pressure, "{case}");
        assert_eq!(owner.cache_info().unwrap().installed, 0);
        assert_eq!(owner.status().counts.completed, 0);
        assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);
        if pressure {
            prepare(&mut owner, &session, &renderer);
        }
        owner.advance_playback(demand()).unwrap();
        assert!(owner
            .schedule_with_yield(&renderer, YieldControl::default())
            .unwrap());
        assert!(drain(&mut owner, &renderer));
        let frame = owner.get(&renderer, 1.).unwrap().unwrap();
        assert_eq!(
            pixels(&renderer, frame.source()),
            [255, 0, 0, 255].repeat(32 * 24)
        );
        let expected = if pressure { 2 } else { 1 };
        assert_eq!(preparations.load(Ordering::Acquire), expected, "{case}");
        assert_eq!(initialized.load(Ordering::Acquire), expected, "{case}");
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn playback_admission_keeps_due_and_nearer_frames_without_counting_lookup_hits() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    for direction in [schedule::Direction::Forward, schedule::Direction::Backward] {
        let (origin, sign) = if direction == schedule::Direction::Forward {
            (0., 1.)
        } else {
            (3., -1.)
        };
        let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
        prepare(&mut owner, &session, &renderer);
        owner
            .advance_playback(Demand {
                frame: origin,
                direction,
                ..demand()
            })
            .unwrap();
        for _ in 0..2 {
            assert!(owner
                .schedule_with_yield(&renderer, YieldControl::default())
                .unwrap());
            assert!(drain(&mut owner, &renderer));
        }
        let first = origin + sign;
        let second = origin + sign * 2.;
        owner
            .advance_playback(Demand {
                frame: first,
                direction,
                now: Duration::from_millis(34),
                ..demand()
            })
            .unwrap();
        let before = owner.cache_info().unwrap();
        let starts = owner.status().counts.started;
        assert!(!owner
            .schedule_with_yield(&renderer, YieldControl::default())
            .unwrap());
        assert_eq!(owner.status().counts.started, starts);
        assert_eq!(owner.status().hold, schedule::Hold::Budget);
        assert_eq!(owner.cache_info().unwrap().evicted, before.evicted);
        assert_eq!(owner.cache_info().unwrap().hits, before.hits);
        assert_eq!(owner.cache_info().unwrap().misses, before.misses);
        assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);
        let due = owner.get(&renderer, first).unwrap().unwrap();
        let nearer = owner.get(&renderer, second).unwrap().unwrap();
        assert_eq!(
            pixels(&renderer, due.source()),
            [255, 0, 0, 255].repeat(32 * 24)
        );
        owner
            .advance_playback(Demand {
                frame: second,
                direction,
                now: Duration::from_millis(68),
                ..demand()
            })
            .unwrap();
        assert!(owner
            .schedule_with_yield(&renderer, YieldControl::default())
            .unwrap());
        assert!(drain(&mut owner, &renderer));
        let kept = owner.get(&renderer, second).unwrap().unwrap();
        assert!(Arc::ptr_eq(&nearer, &kept));
        assert_eq!(owner.cache_info().unwrap().resident_frames, 2);
        assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);
        assert_eq!(
            owner
                .observation()
                .unwrap()
                .snapshot()
                .unwrap()
                .retained_cpu_bytes,
            0
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn explicit_disable_retires_the_actual_native_owner_before_reenable_can_replace_it() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let old = owner.observation().unwrap();
    owner.set_policy(Default::default()).unwrap();
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    assert!(owner.retiring);
    let began = Instant::now();
    while !owner.worker.as_ref().unwrap().stopped() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(old.snapshot().unwrap().stopped);
    assert_eq!(old.snapshot().unwrap().retained_cpu_bytes, 0);
    owner.set_policy(policy()).unwrap();
    prepare(&mut owner, &session, &renderer);
    assert!(!owner.retiring);
    assert!(old.snapshot().unwrap().stopped);
    assert!(!owner.observation().unwrap().snapshot().unwrap().stopped);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_coordinator_admits_before_dispatch_installs_exact_pixels_and_counts_hits() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert_eq!(owner.cache_info().unwrap().pending_bytes, 3072);
    assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
    assert!(!owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let frame = owner.get(&renderer, 1.).unwrap().unwrap();
    assert_eq!(frame.key().frame(), 1.);
    assert_eq!(
        pixels(&renderer, frame.source()),
        [255, 0, 0, 255].repeat(32 * 24)
    );
    assert_eq!(owner.status().counts.completed, 1);
    let info = owner.cache_info().unwrap();
    assert_eq!(
        (info.hits, info.misses, info.installed, info.pending_bytes),
        (1, 1, 1, 0)
    );
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalidated_actual_job_keeps_its_slot_until_ack_and_cannot_install_then_recovers() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    owner.invalidate().unwrap();
    assert!(owner.active.is_some());
    assert!(!owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(!drain(&mut owner, &renderer));
    assert_eq!(owner.cache_info().unwrap().installed, 0);
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    assert!(owner.get(&renderer, 1.).unwrap().is_some());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn canceled_render_preserves_completed_pixels_only_after_matching_source_acknowledgment() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let cached = owner.get(&renderer, 1.).unwrap().unwrap();
    let context = cached.key().context();

    // Cancel after actual worker completion, before foreground acknowledgment.
    // This deterministically exercises the publication/control race as well as
    // preserving the first frame while the second destination is still owned.
    let control = ExportControl::default();
    assert!(owner.schedule(&renderer, control.clone()).unwrap());
    let began = Instant::now();
    while !owner
        .observation()
        .unwrap()
        .snapshot()
        .unwrap()
        .completion_waiting
    {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    control.cancel();
    assert!(!drain(&mut owner, &renderer));
    assert!(owner.profile().is_none());
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    assert_eq!(owner.cache_info().unwrap().resident_frames, 1);
    assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);

    prepare(&mut owner, &session, &renderer);
    let reused = owner.get(&renderer, 1.).unwrap().unwrap();
    assert!(Arc::ptr_eq(&cached, &reused));
    assert_eq!(owner.cache_info().unwrap().installed, 1);
    assert_eq!(
        pixels(&renderer, reused.source()),
        [255, 0, 0, 255].repeat(32 * 24)
    );

    // Same generation tokens and dimensions are insufficient. A changed frozen
    // source must produce a different digest and discard the retained cache.
    let changed_root = tempfile::tempdir().unwrap();
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Coordinator pixels",
        [32., 24.],
        [16., 12., 0.],
        [0., 1., 0., 1.],
    ));
    let changed = Session::new(project, changed_root.path().into(), crate::platform()).unwrap();
    prepare(&mut owner, &changed, &renderer);
    assert_ne!(owner.profile().unwrap().context.content, context.content);
    assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn completed_due_playback_frame_installs_before_poll_but_invalidation_still_wins() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    for invalidation in ["yield", "strong", "pressure", "generation"] {
        let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
        prepare(&mut owner, &session, &renderer);
        owner.advance_playback(demand()).unwrap();
        let token = YieldControl::default();
        assert!(owner.schedule_with_yield(&renderer, token.clone()).unwrap());
        let began = Instant::now();
        while !token.completed() {
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        assert!(!owner.poll_due(&renderer, 2.).unwrap());
        assert!(owner.busy());
        assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
        assert!(!token.preempt());
        if invalidation == "strong" {
            token.control().cancel();
        }
        if invalidation == "generation" {
            owner.invalidate().unwrap();
        }
        owner
            .advance_playback(Demand {
                frame: 1.,
                pressure: invalidation == "pressure",
                now: Duration::from_millis(34),
                ..demand()
            })
            .unwrap();
        let mut installed = false;
        while owner.busy() {
            installed |= owner.poll_due(&renderer, 1.).unwrap();
            assert!(began.elapsed() < Duration::from_secs(30));
            std::thread::yield_now();
        }
        assert_eq!(installed, invalidation == "yield", "{invalidation}");
        assert_eq!(owner.cache_info().unwrap().pending_bytes, 0);
        assert_eq!(
            owner
                .observation()
                .unwrap()
                .snapshot()
                .unwrap()
                .retained_cpu_bytes,
            0
        );
        if installed {
            let frame = owner.get(&renderer, 1.).unwrap().unwrap();
            assert_eq!(frame.key().frame(), 1.);
            assert_eq!(
                pixels(&renderer, frame.source()),
                [255, 0, 0, 255].repeat(32 * 24)
            );
            assert_eq!(owner.status().counts.completed, 1);
        } else {
            assert!(owner.get(&renderer, 1.).unwrap().is_none());
            assert_eq!(owner.cache_info().unwrap().installed, 0);
            assert_eq!(owner.status().counts.completed, 0);
        }
    }
}
