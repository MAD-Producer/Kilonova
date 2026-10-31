use super::*;

fn disk_session(root: &Path) -> Session {
    std::fs::create_dir(root).unwrap();
    super::session(root)
}

#[test]
fn disk_io_publishes_in_flight_before_entry_and_acknowledges_cancellation() {
    for blocked_phase in ["disk_open", "disk_read"] {
        let temp = tempfile::tempdir().unwrap();
        let session = disk_session(&temp.path().join("project"));
        let config = disk::Config::new(temp.path().join("cache"), 1 << 20).unwrap();
        let (entered, observe) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::sync_channel(1);
        let wait = Mutex::new(wait);
        let mut worker = Worker::start_with_phase_hook(32 * 24 * 4, move |_, phase| {
            if phase == blocked_phase {
                entered.send(()).unwrap();
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap();
            }
        })
        .unwrap();
        worker
            .prepare(cached_plan(&session, &config, 10))
            .unwrap()
            .unwrap();
        if blocked_phase == "disk_read" {
            assert!(matches!(poll(&mut worker).outcome, Outcome::Prepared(_)));
            worker.render(1.).unwrap().unwrap();
        }
        observe.recv_timeout(Duration::from_secs(5)).unwrap();
        let observation = worker.observation();
        let snapshot = thread::spawn(move || observation.snapshot().unwrap())
            .join()
            .unwrap();
        let flight = snapshot
            .in_flight
            .expect("disk IO was entered without published state");
        worker.cancel();
        release.send(()).unwrap();
        let completion = poll(&mut worker);
        assert_eq!(flight.phase, blocked_phase);
        assert_eq!(
            flight.operation,
            if blocked_phase == "disk_read" {
                "render"
            } else {
                "prepare"
            }
        );
        assert!(matches!(completion.outcome, Outcome::Cancelled));
        assert_eq!(completion.disk.io_failures, 0);
        assert!(worker.snapshot().unwrap().in_flight.is_none());
        assert_eq!(worker.snapshot().unwrap().retained_cpu_bytes, 0);
        stop(&mut worker);
    }
}

fn cached_plan(session: &Session, config: &disk::Config, document: u64) -> PrerenderPlan {
    let mut plan = PrerenderPlan::capture(
        session,
        document,
        document + 1,
        82,
        frame_bytes(session),
        &ExportControl::default(),
    )
    .unwrap();
    plan.disk = Some(config.clone());
    plan
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn disk_warm_worker_rebinds_generations_without_opening_a_gpu_owner() {
    let temp = tempfile::tempdir().unwrap();
    let session = disk_session(&temp.path().join("project"));
    verify_warm_worker(&session, &temp);
}

fn frame_bytes(session: &Session) -> u64 {
    motion_render::image_formats::color_storage(session.engine.project().working_precision)
        .capacity(32, 24)
        .unwrap()
}

#[test]
#[ignore = "explicit four-precision disk acceptance requires optional native GPU features"]
fn disk_warm_worker_preserves_all_four_actual_native_precision_layouts() {
    use motion_effects::image_contract::ChannelPrecision;
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::new(32, 24, 30, 8).unwrap();
        project.working_precision = precision;
        project.layers.push(motion_core::Layer::solid(
            1,
            "Native precision disk probe",
            [32., 24.],
            [16., 12., 0.],
            [1., 0., 0., 1.],
        ));
        std::fs::create_dir(temp.path().join("project")).unwrap();
        let session =
            Session::new(project, temp.path().join("project"), crate::platform()).unwrap();
        verify_warm_worker(&session, &temp);
    }
}

fn verify_warm_worker(session: &Session, temp: &tempfile::TempDir) {
    let config = disk::Config::new(temp.path().join("cache"), 1 << 20).unwrap();
    let mut cold = Worker::start(frame_bytes(session)).unwrap();
    cold.prepare(cached_plan(&session, &config, 10))
        .unwrap()
        .unwrap();
    assert!(matches!(poll(&mut cold).outcome, Outcome::Prepared(_)));
    cold.render(1.).unwrap().unwrap();
    let completion = poll(&mut cold);
    let expected = match completion.outcome {
        Outcome::Frame(frame) => frame.frame.image().pixels.clone(),
        _ => panic!("cold render did not produce a native frame"),
    };
    assert_eq!(completion.disk.hits, 0);
    assert_eq!(completion.disk.misses, 1);
    assert_eq!(completion.disk.writes, 1);
    stop(&mut cold);
    let mut warm = Worker::start_with_phase_hook(frame_bytes(session), |_, phase| {
        assert_ne!(
            phase, "initialize",
            "disk hit must not allocate a background GPU renderer"
        );
        assert_ne!(phase, "draw", "disk hit must not render a second time");
    })
    .unwrap();
    warm.prepare(cached_plan(&session, &config, 20))
        .unwrap()
        .unwrap();
    let prepared = poll(&mut warm);
    let profile = match prepared.outcome {
        Outcome::Prepared(profile) => profile,
        _ => panic!("warm preparation failed"),
    };
    warm.render(1.).unwrap().unwrap();
    let completion = poll(&mut warm);
    let frame = match completion.outcome {
        Outcome::Frame(frame) => frame,
        _ => panic!("warm disk load failed"),
    };
    assert_eq!(frame.frame.key().context(), profile.context);
    assert_eq!(profile.context.document_generation, 20);
    assert_eq!(profile.context.device_generation, 21);
    assert_eq!(frame.frame.image().pixels, expected);
    assert_eq!(completion.disk.reads, 1);
    assert_eq!(completion.disk.hits, 1);
    assert_eq!(completion.disk.misses, 0);
    assert_eq!(completion.disk.io_failures, 0);
    assert_eq!(warm.snapshot().unwrap().disk.hits, 1);
    drop(frame);
    assert_eq!(warm.snapshot().unwrap().retained_cpu_bytes, 0);
    stop(&mut warm);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn corrupt_disk_entry_falls_back_to_real_render_and_repairs_only_cache_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let session = disk_session(&root);
    let before = serde_json::to_vec(session.engine.project()).unwrap();
    let config = disk::Config::new(temp.path().join("cache"), 1 << 20).unwrap();
    let mut worker = Worker::start(32 * 24 * 4).unwrap();
    worker
        .prepare(cached_plan(&session, &config, 1))
        .unwrap()
        .unwrap();
    poll(&mut worker);
    worker.render(1.).unwrap().unwrap();
    let completion = poll(&mut worker);
    let expected = match completion.outcome {
        Outcome::Frame(frame) => frame.frame.image().pixels.clone(),
        _ => panic!("cold render failed"),
    };
    let cache = temp.path().join("cache/native-v1");
    let file = std::fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(&file, b"corrupt regenerable cache").unwrap();
    worker.render(1.).unwrap().unwrap();
    let completion = poll(&mut worker);
    let frame = match completion.outcome {
        Outcome::Frame(frame) => frame,
        _ => panic!("cache corruption must not fail normal rendering"),
    };
    assert_eq!(frame.frame.image().pixels, expected);
    assert_eq!(completion.disk.io_failures, 1);
    assert_eq!(completion.disk.writes, 2);
    assert!(completion
        .disk
        .last_failure
        .unwrap()
        .contains("invalid type or length"));
    drop(frame);
    worker.render(1.).unwrap().unwrap();
    let completion = poll(&mut worker);
    assert!(matches!(completion.outcome, Outcome::Frame(_)));
    assert_eq!(completion.disk.hits, 1);
    assert_eq!(
        serde_json::to_vec(session.engine.project()).unwrap(),
        before
    );
    stop(&mut worker);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unavailable_disk_directory_preserves_the_original_cause_and_still_renders() {
    let temp = tempfile::tempdir().unwrap();
    let session = disk_session(&temp.path().join("project"));
    let blocked = temp.path().join("not-a-directory");
    std::fs::write(&blocked, b"keep original file").unwrap();
    let config = disk::Config::new(blocked.clone(), 1 << 20).unwrap();
    let mut worker = Worker::start(32 * 24 * 4).unwrap();
    worker
        .prepare(cached_plan(&session, &config, 1))
        .unwrap()
        .unwrap();
    let prepared = poll(&mut worker);
    assert!(matches!(prepared.outcome, Outcome::Prepared(_)));
    assert!(prepared.disk.enabled);
    assert!(!prepared.disk.ready);
    assert_eq!(prepared.disk.io_failures, 1);
    assert!(prepared
        .disk
        .last_failure
        .unwrap()
        .starts_with("create prerender cache:"));
    worker.render(1.).unwrap().unwrap();
    let completion = poll(&mut worker);
    assert!(matches!(completion.outcome, Outcome::Frame(_)));
    assert_eq!(std::fs::read(blocked).unwrap(), b"keep original file");
    stop(&mut worker);
}

mod full_volume;
