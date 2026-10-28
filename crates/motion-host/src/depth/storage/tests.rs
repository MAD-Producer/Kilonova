use super::*;
use motion_model::depth::{DepthMeasure, DepthTime, NearDirection};
use std::cell::Cell;

fn fixture(pts: i64, masked: bool) -> (Input, Estimate) {
    let key = Key {
        source: DepthSource {
            asset: 9,
            revision: 3,
            pts: DepthTime {
                ticks: pts,
                timescale: 1_000_000,
            },
        },
        geometry: DepthGeometry {
            source_size: [257, 65],
            crop: [0, 0, 257, 65],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "spill-test".into(),
            revision: "v1".into(),
            sha256: [4; 32],
            processing: "rgb-test".into(),
        },
    };
    let pixels = 257 * 65;
    let mut input = Input::new(key.clone(), [257, 65], vec![17; pixels * 3].into()).unwrap();
    input.video_end_us = Some(pts as u64 + 10);
    let mut values: Vec<_> = (0..pixels).map(|i| i as f32 / 100.).collect();
    let validity = masked.then(|| {
        values[19] = f32::from_bits(0x7fc0_0042);
        let mut mask = vec![1; pixels];
        mask[19] = 0;
        mask
    });
    let estimate = Estimate {
        resource: DepthResource::new(
            DepthDescriptor {
                size: [257, 65],
                source: key.source,
                geometry: key.geometry,
                model: key.model,
                measure: DepthMeasure::Relative {
                    near: NearDirection::Increasing,
                },
                scale: 1.,
                offset: 0.,
                valid_range: None,
            },
            values,
            validity,
            masked.then(|| vec![0.75; pixels]),
        )
        .unwrap(),
        provider: "controlled_cpu".into(),
        costs: Costs::default(),
    };
    (input, estimate)
}
fn video_key(input: &Input, object: u64) -> VideoKey {
    VideoKey {
        object,
        asset: input.key.source.asset,
        revision: input.key.source.revision,
        target_us: input.key.source.pts.ticks as u64,
        geometry: input.key.geometry.clone(),
        model: input.key.model.clone(),
    }
}
fn append(store: &Store, root: &Path, object: u64, input: &Input, estimate: &Estimate) {
    let cancel = Cancellation::default();
    store
        .append(
            root,
            7,
            object,
            input,
            input.fingerprint(&cancel).unwrap(),
            estimate,
            &cancel,
        )
        .unwrap();
}

#[test]
fn chunked_planes_preserve_bits_masks_and_confidence_after_store_drops() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (input, estimate) = fixture(10, true);
    append(&store, root.path(), 2, &input, &estimate);
    let mut key = video_key(&input, 2);
    key.target_us = 19;
    let sample = store.lookup(&key).unwrap().unwrap();
    assert_eq!(sample.request, 7);
    assert_eq!(store.stats().unwrap().bytes, (257 * 65 * 9) as u64);
    key.target_us = 20;
    assert!(store.lookup(&key).unwrap().is_none());
    key.target_us = 10;
    key.object = 3;
    assert!(store.lookup(&key).unwrap().is_none());
    drop(store);
    let cancel = Cancellation::default();
    let restored = sample
        .restore(&input, input.fingerprint(&cancel).unwrap(), &cancel)
        .unwrap();
    assert_eq!(
        restored
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        estimate
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(restored.resource.validity(), estimate.resource.validity());
    assert_eq!(
        restored.resource.confidence(),
        estimate.resource.confidence()
    );
    assert_eq!(restored.provider, estimate.provider);
}

#[test]
fn independent_file_cursors_allow_concurrent_plane_reads() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (a, ea) = fixture(10, true);
    let (b, eb) = fixture(20, false);
    append(&store, root.path(), 2, &a, &ea);
    append(&store, root.path(), 2, &b, &eb);
    let a = store.lookup(&video_key(&a, 2)).unwrap().unwrap();
    let b = store.lookup(&video_key(&b, 2)).unwrap().unwrap();
    let readers: Vec<_> = (0..4)
        .map(|i| {
            let sample = if i % 2 == 0 { a.clone() } else { b.clone() };
            std::thread::spawn(move || {
                for _ in 0..8 {
                    let restored = sample.plane.load(&Cancellation::default()).unwrap();
                    assert_eq!(restored.resource.values()[100], 1.);
                    assert_eq!(restored.resource.validity().is_some(), i % 2 == 0);
                }
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }
}

#[test]
fn capacities_are_preflighted_and_keep_previous_sample_intact() {
    let root = tempfile::tempdir().unwrap();
    let bytes = (257 * 65 * 4) as u64;
    for store in [
        Store::with_limits(bytes, 8),
        Store::with_limits(8 * bytes, 1),
    ] {
        let (a, ea) = fixture(10, false);
        let (b, eb) = fixture(20, false);
        append(&store, root.path(), 2, &a, &ea);
        let cancel = Cancellation::default();
        let error = store
            .append(
                root.path(),
                8,
                2,
                &b,
                b.fingerprint(&cancel).unwrap(),
                &eb,
                &cancel,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            Error::StorageCapacity {
                request: 8,
                object: 2,
                ..
            }
        ));
        let stats = store.stats().unwrap();
        assert_eq!(
            (stats.samples, stats.bytes, stats.rejections),
            (1, bytes, 1)
        );
        assert!(store.lookup(&video_key(&b, 2)).unwrap().is_none());
        store
            .lookup(&video_key(&a, 2))
            .unwrap()
            .unwrap()
            .plane
            .load(&cancel)
            .unwrap();
    }
}

#[test]
fn changed_rgb_end_model_and_revision_never_reuse_old_depth() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (mut input, estimate) = fixture(10, false);
    append(&store, root.path(), 2, &input, &estimate);
    let sample = store.lookup(&video_key(&input, 2)).unwrap().unwrap();
    let cancel = Cancellation::default();
    let fingerprint = input.fingerprint(&cancel).unwrap();
    input.rgb = vec![18; input.rgb.len()].into();
    assert!(sample
        .restore(&input, input.fingerprint(&cancel).unwrap(), &cancel)
        .is_err());
    input.video_end_us = Some(21);
    assert!(sample.restore(&input, fingerprint, &cancel).is_err());
    let mut key = video_key(&input, 2);
    key.model.sha256 = [5; 32];
    assert!(store.lookup(&key).unwrap().is_none());
    key.model = input.key.model.clone();
    key.revision += 1;
    assert!(store.lookup(&key).unwrap().is_none());
}

#[test]
fn corruption_and_truncation_are_failures_without_inference_fallback() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (input, estimate) = fixture(10, false);
    append(&store, root.path(), 2, &input, &estimate);
    let sample = store.lookup(&video_key(&input, 2)).unwrap().unwrap();
    write_at(&sample.plane.file, &[0xff], sample.plane.offset).unwrap();
    assert!(sample
        .plane
        .load(&Cancellation::default())
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
    sample.plane.file.set_len(sample.plane.offset + 1).unwrap();
    assert!(matches!(
        sample.plane.load(&Cancellation::default()),
        Err(Error::Io(_))
    ));
}

#[test]
fn cancelled_write_does_not_publish_and_preserves_previous_extent() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (input, estimate) = fixture(10, true);
    append(&store, root.path(), 2, &input, &estimate);
    let (other, other_estimate) = fixture(20, true);
    let cancel = Cancellation::default();
    let fingerprint = other.fingerprint(&cancel).unwrap();
    cancel.cancel();
    assert!(matches!(
        store.append(
            root.path(),
            8,
            2,
            &other,
            fingerprint,
            &other_estimate,
            &cancel
        ),
        Err(Error::Canceled)
    ));
    assert_eq!(store.stats().unwrap().samples, 1);
    assert!(store.lookup(&video_key(&other, 2)).unwrap().is_none());
    let sample = store.lookup(&video_key(&input, 2)).unwrap().unwrap();
    assert!(matches!(sample.plane.load(&cancel), Err(Error::Canceled)));
    sample.plane.load(&Cancellation::default()).unwrap();
}

#[test]
fn cancellation_between_chunks_truncates_only_the_new_tail_and_keeps_index_readable() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new();
    let (input, estimate) = fixture(10, true);
    append(&store, root.path(), 2, &input, &estimate);
    let previous = store.stats().unwrap();
    let (other, other_estimate) = fixture(20, true);
    let fingerprint = other.fingerprint(&Cancellation::default()).unwrap();
    let calls = Cell::new(0);
    let error = store
        .append_checked(
            root.path(),
            8,
            2,
            &other,
            fingerprint,
            &other_estimate,
            &|| {
                calls.set(calls.get() + 1);
                // This metadata read must succeed during IO: the writer must not hold
                // the Session-facing index lock while writing any scalar chunk.
                assert_eq!(store.stats()?.bytes, previous.bytes);
                if calls.get() == 3 {
                    Err(Error::Canceled)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
    assert!(matches!(error, Error::Canceled));
    assert_eq!(calls.get(), 3);
    assert_eq!(
        store
            .writer
            .lock()
            .unwrap()
            .file
            .as_ref()
            .unwrap()
            .metadata()
            .unwrap()
            .len(),
        previous.bytes
    );
    let after = store.stats().unwrap();
    assert_eq!(
        (after.samples, after.bytes, after.rejections),
        (previous.samples, previous.bytes, previous.rejections)
    );
    let sample = store.lookup(&video_key(&input, 2)).unwrap().unwrap();
    sample.plane.load(&Cancellation::default()).unwrap();
    append(&store, root.path(), 2, &other, &other_estimate);
    assert_eq!(store.stats().unwrap().samples, 2);
    let calls = Cell::new(0);
    let error = sample
        .plane
        .load_checked(&|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                Err(Error::Canceled)
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(matches!(error, Error::Canceled));
    sample.plane.load(&Cancellation::default()).unwrap();
}

#[test]
fn write_and_rollback_failures_preserve_the_primary_io_cause() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("read-only-source");
    std::fs::write(&path, b"immutable").unwrap();
    let store = Store::new();
    store.writer.lock().unwrap().file = Some(Arc::new(File::open(&path).unwrap()));
    let (input, estimate) = fixture(10, false);
    let cancel = Cancellation::default();
    let error = store
        .append(
            root.path(),
            8,
            2,
            &input,
            input.fingerprint(&cancel).unwrap(),
            &estimate,
            &cancel,
        )
        .unwrap_err();
    assert!(matches!(error, Error::Cleanup { primary, .. } if matches!(*primary, Error::Io(_))));
    assert_eq!(
        (store.stats().unwrap().samples, store.stats().unwrap().bytes),
        (0, 0)
    );
    assert_eq!(std::fs::read(path).unwrap(), b"immutable");
}
