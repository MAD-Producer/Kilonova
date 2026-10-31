use super::*;
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn concurrent_cache_writer_skips_without_evicting_or_exceeding_payload_quota() {
    let image = image(ChannelPrecision::Float32);
    let (_temp, first) = fixture(image.pixels.len() as u64 + HEADER_BYTES, 1 << 16);
    let second = Cache::open(first.config().clone(), 1 << 16, &ExportControl::default()).unwrap();
    let first_key = key(&image, context(), 1., 5);
    let second_key = key(&image, context(), 2., 5);
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    let (returned, result) = mpsc::sync_channel(1);
    let other_image = CapturedImage {
        width: image.width,
        height: image.height,
        contract: image.contract,
        storage: image.storage,
        pixels: image.pixels.clone(),
    };
    let owner = thread::spawn(move || {
        let mut blocked = false;
        let result = first.store_with(
            &first_key,
            &other_image,
            &ExportControl::default(),
            |_, _| {
                if !blocked {
                    blocked = true;
                    entered.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                }
                Ok(())
            },
        );
        (first, first_key, result)
    });
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    let contender = thread::spawn(move || {
        let value = second.store(&second_key, &image, &ExportControl::default());
        returned.send(value).unwrap();
        second
    });
    let skipped = result.recv_timeout(Duration::from_secs(2));
    // Release before assertions so even a broken blocking lock retires cleanly.
    release.send(()).unwrap();
    let (first, first_key, stored) = owner.join().unwrap();
    let second = contender.join().unwrap();
    assert!(!skipped.expect("cache writer blocked rendering").unwrap());
    assert!(stored.unwrap());
    assert!(first
        .load(&first_key, &ExportControl::default())
        .unwrap()
        .is_some());
    let bytes: u64 = fs::read_dir(&second.directory)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum();
    assert_eq!(bytes, first_key.bytes + HEADER_BYTES);
    assert!(bytes <= second.config.quota_bytes);
}
