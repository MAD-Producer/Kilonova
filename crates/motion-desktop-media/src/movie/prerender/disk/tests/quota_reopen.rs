use super::*;

#[test]
fn reopening_a_smaller_quota_trims_owned_payload_before_any_warm_lookup() {
    let image = image(ChannelPrecision::Float32);
    let entry = HEADER_BYTES + image.pixels.len() as u64;
    let (temp, cache) = fixture(entry * 4, 1 << 16);
    let control = ExportControl::default();
    let project = temp.path().join("project.msproj");
    fs::write(&project, b"preserve authored project").unwrap();
    for frame in 0..4 {
        let key = key(&image, context(), f64::from(frame), 5);
        assert!(cache.store(&key, &image, &control).unwrap());
    }
    let smaller = Config::new(cache.config.directory.clone(), entry).unwrap();
    let reopened = Cache::open(smaller, 1 << 16, &control).unwrap();
    let bytes: u64 = fs::read_dir(&reopened.directory)
        .unwrap()
        .map(|file| file.unwrap().metadata().unwrap().len())
        .sum();
    assert!(
        bytes <= entry,
        "reopening must enforce the new quota before a hit: {bytes} > {entry}"
    );
    assert_eq!(fs::read(project).unwrap(), b"preserve authored project");
}

#[test]
fn a_new_write_reserves_an_entry_slot_at_the_bounded_scan_limit() {
    let (_temp, cache) = fixture(1 << 20, 1 << 16);
    for number in 0..MAX_ENTRIES {
        fs::write(cache.directory.join(format!("{number:064x}.mspc")), []).unwrap();
    }
    let image = image(ChannelPrecision::Float32);
    let key = key(&image, context(), 1., 5);
    let control = ExportControl::default();
    assert!(cache.store(&key, &image, &control).unwrap());
    let entries = fs::read_dir(&cache.directory).unwrap().count();
    assert!(
        entries <= MAX_ENTRIES,
        "one write exceeded the next scan's bound: {entries}"
    );
    assert!(cache.load(&key, &control).unwrap().is_some());
    // The next write must remain usable, not fail permanently on our own entry.
    let next = self::key(&image, context(), 2., 5);
    assert!(cache.store(&next, &image, &control).unwrap());
    assert!(fs::read_dir(&cache.directory).unwrap().count() <= MAX_ENTRIES);
}

#[test]
fn a_busy_open_is_a_normal_hold_and_recovers_after_the_actual_writer_releases() {
    let (_temp, cache) = fixture(1 << 20, 1 << 16);
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.config.directory.join("native-v1.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let mut owner = Owner::default();
    let control = ExportControl::default();
    owner
        .configure(Some(cache.config().clone()), 1 << 16, 82, &control)
        .unwrap();
    assert!(owner.stats.enabled);
    assert!(
        !owner.stats.ready,
        "a contending open must not skip initial quota admission"
    );
    assert_eq!(
        owner.stats.io_failures, 0,
        "writer contention is not a fault"
    );
    drop(lock);
    owner
        .configure(Some(cache.config().clone()), 1 << 16, 83, &control)
        .unwrap();
    assert!(owner.stats.ready);
    assert_eq!(owner.stats.io_failures, 0);
}
