use super::*;
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    thread,
};

fn image_session(root: &Path) -> Session {
    fs::create_dir_all(root.join("assets")).unwrap();
    write_image(&root.join("assets/source.png"), [255, 0, 0, 255]);
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.assets.push(motion_core::Asset {
        id: 17,
        path: "assets/source.png".into(),
        width: 1,
        height: 1,
    });
    let mut layer =
        motion_core::Layer::solid(1, "Source freshness", [32., 24.], [16., 12., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 17 };
    project.layers.push(layer);
    Session::new(project, root.into(), crate::platform()).unwrap()
}
fn write_image(path: &Path, color: [u8; 4]) {
    image::save_buffer_with_format(
        path,
        &color,
        1,
        1,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
}
fn renderer(session: &Session) -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap()
}
fn wait_invalid(observation: &source_watch::Observation) {
    let began = Instant::now();
    while observation.valid() {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "native filesystem event not delivered"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn assert_color(renderer: &Renderer, frame: &motion_render::CachedCompositionFrame, green: bool) {
    let bytes = pixels(renderer, frame.source());
    let center = &bytes[(12 * 32 + 16) * 4..][..4];
    assert_eq!(
        center,
        if green {
            &[0, 255, 0, 255]
        } else {
            &[255, 0, 0, 255]
        }
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_equal_size_equal_mtime_replacement_closes_warm_lookup_and_rebuilds_green_pixels() {
    let root = tempfile::tempdir().unwrap();
    let session = image_session(root.path());
    let renderer = renderer(&session);
    let source = root.path().join("assets/source.png");
    let replacement = tempfile::NamedTempFile::new_in(source.parent().unwrap()).unwrap();
    write_image(replacement.path(), [0, 255, 0, 255]);
    let before = fs::metadata(&source).unwrap();
    assert_eq!(
        replacement.as_file().metadata().unwrap().len(),
        before.len()
    );
    replacement
        .as_file()
        .set_times(fs::FileTimes::new().set_modified(before.modified().unwrap()))
        .unwrap();
    let revision = session.engine.revision();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.advance_playback(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let retained_red = owner.get(&renderer, 1.).unwrap().unwrap();
    let old_key = retained_red.key();
    assert_color(&renderer, &retained_red, false);
    let watch = owner.source.clone().unwrap();
    source_watch::replace_test_source(replacement, &source);
    assert_eq!(
        fs::metadata(&source).unwrap().modified().unwrap(),
        before.modified().unwrap()
    );
    wait_invalid(&watch);
    assert!(owner.profile().is_none());
    assert!(
        owner.get(&renderer, 1.).unwrap().is_none(),
        "same-path replacement must close warm lookup without a document edit"
    );
    assert_eq!(owner.cache_info().unwrap().resident_bytes, 0);
    assert_eq!(session.engine.revision(), revision);
    prepare(&mut owner, &session, &renderer);
    owner.advance_playback(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let green = owner.get(&renderer, 1.).unwrap().unwrap();
    assert_ne!(green.key().context().content, old_key.context().content);
    assert_color(&renderer, &green, true);
    assert_color(&renderer, &retained_red, false);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_source_event_cancels_a_blocked_render_before_owner_poll_and_rejects_its_pixels() {
    let root = tempfile::tempdir().unwrap();
    let session = image_session(root.path());
    let renderer = renderer(&session);
    let block = Arc::new(AtomicBool::new(false));
    let worker_block = block.clone();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    owner.worker = Some(
        Worker::start_with_phase_hook(3072, move |_, phase| {
            if phase == "draw" && worker_block.swap(false, Ordering::AcqRel) {
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
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let retained_red = owner.get(&renderer, 1.).unwrap().unwrap();
    block.store(true, Ordering::Release);
    let control = ExportControl::default();
    assert!(owner.schedule(&renderer, control.clone()).unwrap());
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    let watch = owner.source.clone().unwrap();
    write_image(&root.path().join("assets/source.png"), [0, 255, 0, 255]);
    wait_invalid(&watch);
    let began = Instant::now();
    while control.check().is_ok() {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "callback did not cancel the real blocked job"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        owner.busy(),
        "cancellation does not release unacknowledged native work"
    );
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .in_flight
            .unwrap()
            .phase,
        "draw"
    );
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    release.send(()).unwrap();
    assert!(!drain(&mut owner, &renderer));
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
    assert!(owner.get(&renderer, 2.).unwrap().is_none());
    assert_color(&renderer, &retained_red, false);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn watcher_fault_drains_real_ack_and_preserves_simultaneous_worker_failure() {
    for panic_after_release in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let session = image_session(root.path());
        let renderer = renderer(&session);
        let (entered, waiting) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let blocked = Mutex::new(blocked);
        let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
        owner.worker = Some(
            Worker::start_with_phase_hook(3072, move |_, phase| {
                if phase == "draw" {
                    entered.send(()).unwrap();
                    blocked
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(30))
                        .unwrap();
                    assert!(!panic_after_release, "controlled primary worker failure");
                }
            })
            .unwrap(),
        );
        prepare(&mut owner, &session, &renderer);
        owner.advance_playback(demand()).unwrap();
        let control = ExportControl::default();
        assert!(owner.schedule(&renderer, control.clone()).unwrap());
        waiting.recv_timeout(Duration::from_secs(30)).unwrap();
        let watch = owner.source.clone().unwrap();
        // Inject only the native watcher's error delivery. Rendering, its
        // blocking phase, cancellation and acknowledgment are real production paths.
        source_watch::fail_test_observation(&watch, "controlled native watcher failure");
        assert!(control.check().is_err());
        let first = owner.poll(&renderer).unwrap_err();
        assert!(
            first.contains("controlled native watcher failure"),
            "{first}"
        );
        assert!(owner.busy(), "no acknowledgment has arrived yet");
        assert!(
            owner
                .observation()
                .unwrap()
                .snapshot()
                .unwrap()
                .retained_cpu_bytes
                > 0
        );
        release.send(()).unwrap();
        let began = Instant::now();
        let mut acknowledged_cause = None;
        while owner.busy() {
            if let Err(cause) = owner.poll(&renderer) {
                if !owner.busy() {
                    acknowledged_cause = Some(cause);
                }
            }
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "watcher fault must not prevent draining the real worker acknowledgment"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let cause = acknowledged_cause
            .expect("watcher failure must remain observable after acknowledgment");
        assert!(
            cause.contains(if panic_after_release {
                "panicked"
            } else {
                "controlled native watcher failure"
            }),
            "{cause}"
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
        assert_eq!(owner.status().pending, 0);
        assert!(owner.profile().is_none());
        assert!(
            owner.get(&renderer, 1.).is_err(),
            "fault stays closed until explicit recovery"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn a_change_followed_by_watcher_failure_stays_faulted_after_real_ack() {
    let root = tempfile::tempdir().unwrap();
    let session = image_session(root.path());
    let renderer = renderer(&session);
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    owner.worker = Some(
        Worker::start_with_phase_hook(3072, move |_, phase| {
            if phase == "draw" {
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
    let control = ExportControl::default();
    assert!(owner.schedule(&renderer, control.clone()).unwrap());
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    let watch = owner.source.clone().unwrap();
    source_watch::change_test_observation(&watch, "controlled source replacement");
    source_watch::fail_test_observation(&watch, "controlled subsequent watcher failure");
    // Release before asserting so a negative control never strands the real
    // worker in its fixture. Acknowledgment is still drained by production poll.
    release.send(()).unwrap();
    assert!(control.check().is_err());
    let first = owner.poll(&renderer);
    assert!(
        first
            .as_ref()
            .is_err_and(|cause| cause.contains("controlled subsequent watcher failure")),
        "a normal change must not mask the later watcher fault: {first:?}"
    );
    let began = Instant::now();
    while owner.busy() {
        let _ = owner.poll(&renderer);
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "real cancellation acknowledgment must drain"
        );
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(owner.status().pending, 0);
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
    let error = owner
        .get(&renderer, 1.)
        .err()
        .expect("the fault must remain closed");
    assert!(
        error.contains("controlled subsequent watcher failure"),
        "{error}"
    );
    assert_eq!(
        watch.snapshot().unwrap().cause.as_deref(),
        Some("controlled source replacement")
    );
}
