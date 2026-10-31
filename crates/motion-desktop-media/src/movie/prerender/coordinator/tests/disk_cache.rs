use super::*;

fn stop_worker(owner: &mut Coordinator) {
    let worker = owner.worker.as_mut().unwrap();
    worker.shutdown();
    let began = Instant::now();
    while !worker.stopped() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    worker.shutdown();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn reopened_disk_hit_installs_current_native_pixels_without_foreground_disk_access() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("project")).unwrap();
    let session = session(&root.path().join("project"));
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let directory = root.path().join("cache");
    let config = disk::Config::new(directory.clone(), 1 << 20).unwrap();
    let mut cold = Coordinator::new(policy(), 3072, 6144).unwrap();
    cold.set_disk_cache(Some(config.clone())).unwrap();
    prepare(&mut cold, &session, &renderer);
    cold.advance_playback(demand()).unwrap();
    assert!(cold.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut cold, &renderer));
    let expected = pixels(
        &renderer,
        cold.get(&renderer, 1.).unwrap().unwrap().source(),
    );
    assert_eq!(cold.disk_info().writes, 1);
    assert_eq!(cold.disk_info().hits, 0);
    stop_worker(&mut cold);
    drop(cold);

    let mut warm = Coordinator::new(policy(), 3072, 6144).unwrap();
    warm.set_disk_cache(Some(config)).unwrap();
    let foreground = std::thread::current().id();
    warm.worker = Some(
        Worker::start_with_phase_hook(3072, move |_, phase| {
            assert_ne!(
                phase, "initialize",
                "warm disk hit must not open another native renderer"
            );
            assert_ne!(phase, "draw", "warm disk hit must not render again");
            if phase.starts_with("disk_") {
                assert_ne!(
                    std::thread::current().id(),
                    foreground,
                    "disk IO ran on the foreground owner"
                );
            }
        })
        .unwrap(),
    );
    let mut plan =
        PrerenderPlan::capture(&session, 20, 21, 82, 3072, &ExportControl::default()).unwrap();
    plan.adapter = Some(renderer.adapter.clone());
    assert!(warm.prepare(plan, ExportControl::default()).unwrap());
    drain(&mut warm, &renderer);
    warm.advance_playback(demand()).unwrap();
    let token = YieldControl::default();
    assert!(warm.schedule_with_yield(&renderer, token.clone()).unwrap());
    let began = Instant::now();
    while !token.completed() {
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::yield_now();
    }
    // Remove the disk namespace from its configured path AFTER the background
    // read. The actual foreground install/copy must use the admitted completion.
    std::fs::rename(
        directory.join("native-v1"),
        directory.join("retained-native-v1"),
    )
    .unwrap();
    assert!(drain(&mut warm, &renderer));
    let cached = warm.get(&renderer, 1.).unwrap().unwrap();
    assert_eq!(cached.key().context().document_generation, 20);
    assert_eq!(cached.key().context().device_generation, 21);
    assert_eq!(pixels(&renderer, cached.source()), expected);
    assert_eq!(warm.disk_info().hits, 1);
    assert_eq!(warm.disk_info().writes, 0);
    assert_eq!(warm.disk_info().io_failures, 0);
    assert!(
        !directory.join("native-v1").exists(),
        "foreground recreated a cache path"
    );
    stop_worker(&mut warm);
}
