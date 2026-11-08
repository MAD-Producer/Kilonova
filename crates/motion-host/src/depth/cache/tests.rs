use super::*;
use motion_model::depth::{
    DepthGeometry, DepthMeasure, DepthModel, DepthSource, DepthTime, NearDirection,
};
use std::sync::{atomic::AtomicUsize, mpsc};

fn input() -> Input {
    Input::new(
        Key {
            source: DepthSource {
                asset: 7,
                revision: 3,
                pts: DepthTime {
                    ticks: 1,
                    timescale: 24,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "cache-control".into(),
                revision: "r1".into(),
                sha256: [4; 32],
                processing: "rgb-v1".into(),
            },
        },
        [2, 1],
        Arc::from([1, 2, 3, 4, 5, 6]),
    )
    .unwrap()
}
fn execution() -> Execution {
    Execution {
        backend: "controlled".into(),
        provider: "controlled_cpu".into(),
        runtime_sha256: [5; 32],
        plan_sha256: [6; 32],
        precision: "float32".into(),
    }
}
fn config(directory: &Path) -> Config {
    Config {
        directory: directory.join("cache"),
        byte_limit: 1 << 20,
        entry_limit: 8,
    }
}
fn estimate(input: &Input) -> Estimate {
    Estimate {
        resource: DepthResource::new(
            DepthDescriptor {
                size: [2, 1],
                source: input.key.source.clone(),
                geometry: input.key.geometry.clone(),
                model: input.key.model.clone(),
                measure: DepthMeasure::Relative {
                    near: NearDirection::Increasing,
                },
                scale: 0.125,
                offset: 24.179964641423318,
                valid_range: None,
            },
            vec![0.25, -0.],
            Some(vec![1, 0]),
            Some(vec![0.125, 0.875]),
        )
        .unwrap(),
        provider: execution().provider,
        costs: Costs {
            inference: Duration::from_millis(11),
            ..Costs::default()
        },
    }
}

#[test]
fn persistent_raw_roundtrip_preserves_all_planes_nonintegral_metadata_and_new_instance_identity() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let input = input();
    let cancel = Cancellation::default();
    let fingerprint = input.fingerprint(&cancel).unwrap();
    assert!(config
        .read(&input, fingerprint, &execution(), &cancel)
        .unwrap()
        .is_none());
    let original = estimate(&input);
    config
        .write(&input, fingerprint, &execution(), &original, &cancel)
        .unwrap();
    let restored = config
        .read(&input, fingerprint, &execution(), &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.resource.descriptor(),
        original.resource.descriptor()
    );
    assert_eq!(
        restored
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        original
            .resource
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(restored.resource.validity(), original.resource.validity());
    assert_eq!(
        restored.resource.confidence(),
        original.resource.confidence()
    );
    assert_ne!(
        restored.resource.cache_identity(),
        original.resource.cache_identity()
    );
    assert_eq!(restored.costs.inference, Duration::ZERO);
    let mut equivalent = input.clone();
    equivalent.key.source.pts.ticks = 2;
    equivalent.key.source.pts.timescale = 48;
    let restored = config
        .read(&equivalent, fingerprint, &execution(), &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(restored.resource.descriptor().source, equivalent.key.source);
}

#[test]
fn persistent_keys_bind_actual_rgb_geometry_source_weight_processing_runtime_plan_and_precision() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let original = input();
    let cancel = Cancellation::default();
    let fingerprint = original.fingerprint(&cancel).unwrap();
    config
        .write(
            &original,
            fingerprint,
            &execution(),
            &estimate(&original),
            &cancel,
        )
        .unwrap();
    for field in 0..7 {
        let mut changed = original.clone();
        match field {
            0 => changed.rgb = Arc::from([6, 5, 4, 3, 2, 1]),
            1 => changed.key.source.revision += 1,
            2 => changed.key.source.asset += 1,
            3 => changed.key.source.pts.ticks += 1,
            4 => changed.key.geometry.mirror_x = true,
            5 => changed.key.model.sha256[0] += 1,
            _ => changed.key.model.processing.push('2'),
        }
        assert!(
            config
                .read(
                    &changed,
                    changed.fingerprint(&cancel).unwrap(),
                    &execution(),
                    &cancel
                )
                .unwrap()
                .is_none(),
            "{field}"
        );
    }
    for field in 0..5 {
        let mut changed = execution();
        match field {
            0 => changed.provider.push('2'),
            1 => changed.runtime_sha256[0] += 1,
            2 => changed.plan_sha256[0] += 1,
            3 => changed.precision = "float16".into(),
            _ => changed.backend.push('2'),
        }
        assert!(
            config
                .read(&original, fingerprint, &changed, &cancel)
                .unwrap()
                .is_none(),
            "{field}"
        );
    }
}

#[test]
fn persistent_cache_rejects_finite_corruption_truncation_trailing_data_and_oversized_header() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let input = input();
    let cancel = Cancellation::default();
    let fingerprint = input.fingerprint(&cancel).unwrap();
    config
        .write(
            &input,
            fingerprint,
            &execution(),
            &estimate(&input),
            &cancel,
        )
        .unwrap();
    let path = config.path(&input, fingerprint, &execution()).unwrap();
    let original = fs::read(&path).unwrap();
    for case in 0..5 {
        let mut bytes = original.clone();
        match case {
            0 => {
                let n = bytes.len();
                bytes[n - 33] ^= 1;
            }
            1 => {
                bytes.pop();
            }
            2 => bytes.push(0),
            3 => bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes()),
            _ => bytes[7] = b'2',
        }
        fs::write(&path, bytes).unwrap();
        assert!(
            config
                .read(&input, fingerprint, &execution(), &cancel)
                .is_err(),
            "case={case}"
        );
    }
    fs::write(path, original).unwrap();
    assert!(config
        .read(&input, fingerprint, &execution(), &cancel)
        .unwrap()
        .is_some());
}

#[test]
fn persistent_quota_lock_refusal_and_cancellation_leave_no_published_partial_result() {
    let root = tempfile::tempdir().unwrap();
    let mut config = config(root.path());
    let input = input();
    let cancel = Cancellation::default();
    let fingerprint = input.fingerprint(&cancel).unwrap();
    config.byte_limit = 1;
    assert!(config
        .write(
            &input,
            fingerprint,
            &execution(),
            &estimate(&input),
            &cancel
        )
        .is_err());
    assert_eq!(fs::read_dir(&config.directory).unwrap().count(), 0);
    config.byte_limit = 1 << 20;
    let lock = DirectoryLock::acquire(&config.directory).unwrap();
    assert!(config
        .read(&input, fingerprint, &execution(), &cancel)
        .is_err());
    drop(lock);
    config
        .write(
            &input,
            fingerprint,
            &execution(),
            &estimate(&input),
            &cancel,
        )
        .unwrap();
    let checks = AtomicUsize::new(0);
    let result = config.read_checked(&input, fingerprint, &execution(), &|| {
        if checks.fetch_add(1, Ordering::SeqCst) >= 2 {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(result, Err(Error::Canceled)));
    assert!(config
        .read(&input, fingerprint, &execution(), &cancel)
        .unwrap()
        .is_some());
    config.entry_limit = 1;
    let mut changed = input.clone();
    changed.rgb = Arc::from([6, 5, 4, 3, 2, 1]);
    assert!(config
        .write(
            &changed,
            changed.fingerprint(&cancel).unwrap(),
            &execution(),
            &estimate(&changed),
            &cancel
        )
        .is_err());
    assert_eq!(fs::read_dir(&config.directory).unwrap().count(), 1);
}

struct Controlled {
    calls: Arc<AtomicUsize>,
    attested: bool,
    wrong_provider: bool,
    blocked: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl Backend for Controlled {
    fn execution_identity(&mut self, _: &Input, _: &Cancellation) -> Result<Option<Execution>> {
        if let Some((entered, release)) = self.blocked.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(self.attested.then(execution))
    }
    fn infer(&mut self, input: &Input, _: &Cancellation) -> Result<Estimate> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut result = estimate(input);
        if self.wrong_provider {
            result.provider = "different_actual_provider".into();
        }
        Ok(result)
    }
}
fn backend(calls: &Arc<AtomicUsize>) -> Controlled {
    Controlled {
        calls: calls.clone(),
        attested: true,
        wrong_provider: false,
        blocked: None,
    }
}
fn wait(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let counts = jobs.counts().unwrap();
        if counts.completed + counts.failed + counts.canceled >= count {
            break;
        }
        assert!(Instant::now() < until, "cache worker did not complete");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn cached_worker_reopens_without_inference_and_refuses_corruption_without_a_hidden_retry() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let input = input();
    let mut jobs = Jobs::start_cached(backend(&calls), config.clone()).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 1);
    let original = jobs.take_ready(input.key()).unwrap().unwrap();
    assert_eq!(jobs.counts().unwrap().cache_misses, 1);
    drop(jobs);
    let mut jobs = Jobs::start_cached(backend(&calls), config.clone()).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 1);
    let restored = jobs.take_ready(input.key()).unwrap().unwrap();
    assert_ne!(
        original.resource.cache_identity(),
        restored.resource.cache_identity()
    );
    assert_eq!(restored.resource.values(), original.resource.values());
    assert_eq!(restored.costs.inference, Duration::ZERO);
    assert_eq!(jobs.counts().unwrap().cache_hits, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let path = config
        .path(
            &input,
            input.fingerprint(&Cancellation::default()).unwrap(),
            &execution(),
        )
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let n = bytes.len();
    bytes[n - 33] ^= 1;
    fs::write(path, bytes).unwrap();
    jobs.request(input.clone()).unwrap();
    wait(&jobs, 2);
    assert!(jobs
        .take_ready(input.key())
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(jobs.counts().unwrap().failed, 1);
}

#[test]
fn cache_resolve_publishes_flight_before_blocking_and_never_installs_a_superseded_result() {
    let root = tempfile::tempdir().unwrap();
    let config = config(root.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let input = input();
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut backend = backend(&calls);
    backend.blocked = Some((entered, blocked));
    let mut jobs = Jobs::start_cached(backend, config).unwrap();
    let first = jobs.request(input.clone()).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.request, first);
    assert_eq!(flight.phase, "cache_resolve");
    let second = jobs.request(input.clone()).unwrap();
    assert!(second > first);
    assert_eq!(jobs.queue_depth().unwrap(), 1);
    release.send(()).unwrap();
    wait(&jobs, 2);
    assert!(jobs.take_ready(input.key()).unwrap().is_some());
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn persistent_cache_refuses_unattested_and_changed_actual_provider_without_publishing() {
    for attested in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let config = config(root.path());
        let calls = Arc::new(AtomicUsize::new(0));
        let input = input();
        let mut backend = backend(&calls);
        backend.attested = attested;
        backend.wrong_provider = true;
        let mut jobs = Jobs::start_cached(backend, config.clone()).unwrap();
        jobs.request(input.clone()).unwrap();
        wait(&jobs, 1);
        assert!(jobs.take_ready(input.key()).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(attested));
        assert!(!config.directory.exists());
    }
}
