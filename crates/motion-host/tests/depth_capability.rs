#![cfg(feature = "ai-depth")]

use motion_host::depth::{
    cache::{Config, Execution},
    Backend, Cancellation, Costs, Error, Estimate, Input, Jobs, Key, PreparedEstimate, Result,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

fn key() -> Key {
    Key {
        source: DepthSource {
            asset: 75,
            revision: 2,
            pts: DepthTime {
                ticks: 3,
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
            id: "capability-control".into(),
            revision: "r1".into(),
            sha256: [42; 32],
            processing: "explicit-profile".into(),
        },
    }
}
fn input() -> Input {
    Input::new(key(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap()
}
fn execution() -> Execution {
    Execution {
        backend: "controlled".into(),
        provider: "actual-provider".into(),
        runtime_sha256: [24; 32],
        plan_sha256: [25; 32],
        precision: "float32".into(),
    }
}
#[derive(Clone, Copy)]
enum Fault {
    None,
    Unsupported,
    InvalidIdentity,
    Provider,
    Resource,
    Backend,
    Panic,
}
struct Controlled {
    resolves: Arc<AtomicUsize>,
    infers: Arc<AtomicUsize>,
    fault: Fault,
    execution: Execution,
    retained_gpu_buffers: Option<u64>,
    block: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl Backend for Controlled {
    fn retained_gpu_buffer_capacity(&self, _: &Input) -> Option<u64> {
        self.retained_gpu_buffers
    }
    fn execution_identity(&mut self, _: &Input, _: &Cancellation) -> Result<Option<Execution>> {
        self.resolves.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, release)) = self.block.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        // Deliberately ignore cancellation here: Jobs must still reject it.
        match self.fault {
            Fault::Unsupported => Ok(None),
            Fault::InvalidIdentity => Ok(Some(Execution {
                runtime_sha256: [0; 32],
                ..execution()
            })),
            _ => Ok(Some(self.execution.clone())),
        }
    }
    fn infer(&mut self, input: &Input, _: &Cancellation) -> Result<Estimate> {
        self.infers.fetch_add(1, Ordering::SeqCst);
        match self.fault {
            Fault::Backend => return Err(Error::Backend("original probe cause".into())),
            Fault::Panic => panic!("original probe panic"),
            _ => (),
        }
        let k = input.key();
        let mut model = k.model.clone();
        if matches!(self.fault, Fault::Resource) {
            model.sha256 = [99; 32];
        }
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: [2, 1],
                    source: k.source.clone(),
                    geometry: k.geometry.clone(),
                    model,
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                vec![0.25, 2.],
                None,
                None,
            )?,
            provider: if matches!(self.fault, Fault::Provider) {
                "foreign-provider".into()
            } else {
                self.execution.provider.clone()
            },
            // A backend's claimed total cannot become a worker measurement.
            costs: Costs {
                total: Duration::MAX,
                ..Costs::default()
            },
        })
    }
}
fn backend(fault: Fault) -> Controlled {
    Controlled {
        resolves: Arc::new(AtomicUsize::new(0)),
        infers: Arc::new(AtomicUsize::new(0)),
        fault,
        execution: execution(),
        retained_gpu_buffers: None,
        block: None,
    }
}
fn wait_done(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let c = jobs.counts().unwrap();
        if c.completed + c.canceled + c.failed >= count {
            return;
        }
        assert!(
            Instant::now() < until,
            "controlled probe worker did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn take(jobs: &mut Jobs, count: u64) -> PreparedEstimate {
    wait_done(jobs, count);
    jobs.take_prepared_ready(&key()).unwrap().unwrap()
}
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn measured_worker_publishes_resolution_before_entry_and_owns_full_timing() {
    let mut controlled = backend(Fault::None);
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    controlled.block = Some((entered, blocked));
    let mut jobs = Jobs::start_measured(controlled).unwrap();
    let guard = Release(release);
    let request = jobs.request(input()).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.request, request);
    assert_eq!(flight.phase, "capability_resolve");
    assert_eq!(flight.source, Some(key().source));
    assert!(jobs.take_prepared_ready(&key()).unwrap().is_none());
    let held = Instant::now();
    std::thread::sleep(Duration::from_millis(10));
    let lower_bound = held.elapsed();
    drop(guard);
    let ready = take(&mut jobs, 1);
    let measured = ready.measurement().unwrap();
    assert_eq!(measured.request(), request);
    assert_eq!(measured.execution(), &execution());
    assert!(measured.resolve() >= lower_bound);
    assert!(measured.total() >= measured.resolve());
    assert_eq!(measured.total(), ready.estimate().costs.total);
    assert_ne!(measured.total(), Duration::MAX);
    assert_eq!(ready.input().rgb_size, [2, 1]);
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
}

#[test]
fn ordinary_and_cached_results_cannot_be_misreported_as_capability_samples() {
    let controlled = backend(Fault::None);
    let resolves = controlled.resolves.clone();
    let mut ordinary = Jobs::start(controlled).unwrap();
    ordinary.request(input()).unwrap();
    assert!(take(&mut ordinary, 1).measurement().is_none());
    assert_eq!(resolves.load(Ordering::SeqCst), 0);
    drop(ordinary);

    let root = tempfile::tempdir().unwrap();
    let controlled = backend(Fault::None);
    let infers = controlled.infers.clone();
    let mut cached = Jobs::start_cached(
        controlled,
        Config {
            directory: root.path().to_path_buf(),
            byte_limit: 1 << 20,
            entry_limit: 4,
        },
    )
    .unwrap();
    cached.request(input()).unwrap();
    assert!(take(&mut cached, 1).measurement().is_none());
    cached.request(input()).unwrap();
    assert!(take(&mut cached, 2).measurement().is_none());
    assert_eq!(infers.load(Ordering::SeqCst), 1);
    assert_eq!(cached.counts().unwrap().cache_hits, 1);
    drop(cached);

    let controlled = backend(Fault::None);
    let infers = controlled.infers.clone();
    let resolves = controlled.resolves.clone();
    let mut measured = Jobs::start_measured(controlled).unwrap();
    measured.request(input()).unwrap();
    assert!(take(&mut measured, 1).measurement().is_some());
    measured.request(input()).unwrap();
    assert!(take(&mut measured, 2).measurement().is_some());
    assert_eq!(resolves.load(Ordering::SeqCst), 2);
    assert_eq!(infers.load(Ordering::SeqCst), 2);
    assert_eq!(measured.counts().unwrap().cache_hits, 0);
}

#[test]
fn missing_or_invalid_execution_refuses_inference_without_a_fallback() {
    for fault in [Fault::Unsupported, Fault::InvalidIdentity] {
        let controlled = backend(fault);
        let infers = controlled.infers.clone();
        let mut jobs = Jobs::start_measured(controlled).unwrap();
        jobs.request(input()).unwrap();
        wait_done(&jobs, 1);
        assert!(matches!(
            jobs.take_prepared_ready(&key()),
            Err(Error::Invalid(_))
        ));
        assert_eq!(infers.load(Ordering::SeqCst), 0);
        assert_eq!(jobs.counts().unwrap().failed, 1);
        assert!(jobs.in_flight().unwrap().is_none());
    }
}

#[test]
fn provider_resource_and_original_backend_failures_never_publish_a_measurement() {
    for fault in [Fault::Provider, Fault::Resource, Fault::Backend] {
        let controlled = backend(fault);
        let infers = controlled.infers.clone();
        let mut jobs = Jobs::start_measured(controlled).unwrap();
        jobs.request(input()).unwrap();
        wait_done(&jobs, 1);
        let cause = jobs.take_prepared_ready(&key()).unwrap_err();
        match fault {
            Fault::Backend => {
                assert!(matches!(cause, Error::Backend(ref c) if c == "original probe cause"))
            }
            _ => assert!(matches!(cause, Error::Invalid(_))),
        }
        assert_eq!(infers.load(Ordering::SeqCst), 1);
        assert_eq!(jobs.counts().unwrap().failed, 1);
        assert!(jobs.in_flight().unwrap().is_none());
    }
}

#[test]
fn cancellation_after_resolution_prevents_inference_and_measurement_publication() {
    let mut controlled = backend(Fault::None);
    let infers = controlled.infers.clone();
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    controlled.block = Some((entered, blocked));
    let mut jobs = Jobs::start_measured(controlled).unwrap();
    let guard = Release(release);
    jobs.request(input()).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    jobs.cancel().unwrap();
    drop(guard);
    wait_done(&jobs, 1);
    assert_eq!(infers.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert!(jobs.take_prepared_ready(&key()).unwrap().is_none());
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn backend_panic_retains_its_cause_and_stops_the_probe_without_restart() {
    let mut jobs = Jobs::start_measured(backend(Fault::Panic)).unwrap();
    jobs.request(input()).unwrap();
    wait_done(&jobs, 1);
    assert!(
        matches!(jobs.take_prepared_ready(&key()), Err(Error::Panic(ref c)) if c == "original probe panic")
    );
    assert!(matches!(jobs.request(input()), Err(Error::Stopped)));
    assert!(jobs.in_flight().unwrap().is_none());
}

#[test]
fn observation_identity_distinguishes_clones_requests_and_worker_instances() {
    let mut jobs = Jobs::start_measured(backend(Fault::None)).unwrap();
    jobs.request(input()).unwrap();
    let first = take(&mut jobs, 1).measurement().unwrap().clone();
    assert!(first.same_observation(&first.clone()));
    jobs.request(input()).unwrap();
    let second = take(&mut jobs, 2).measurement().unwrap().clone();
    assert!(!first.same_observation(&second));
    drop(jobs);
    let mut other = Jobs::start_measured(backend(Fault::None)).unwrap();
    other.request(input()).unwrap();
    let third = take(&mut other, 1).measurement().unwrap().clone();
    assert_eq!(first.request(), third.request());
    assert!(!first.same_observation(&third));
}

#[test]
fn bounded_profile_keeps_every_independent_observation_and_exact_order_statistics() {
    use motion_host::depth::capability::Profile;
    let mut profile = Profile::new(2).unwrap();
    assert!(profile.summary().is_err());
    let mut jobs = Jobs::start_measured(backend(Fault::None)).unwrap();
    jobs.request(input()).unwrap();
    let first = take(&mut jobs, 1);
    profile.record(&first).unwrap();
    assert!(profile.record(&first.clone()).is_err());
    assert_eq!(profile.observations().len(), 1);
    assert_eq!(profile.observations()[0].source(), &key().source);
    assert_eq!(profile.observations()[0].input(), first.input());
    assert!(profile.observations()[0]
        .measurement()
        .same_observation(first.measurement().unwrap()));
    jobs.request(input()).unwrap();
    let second = take(&mut jobs, 2);
    profile.record(&second).unwrap();
    let summary = profile.summary().unwrap();
    let mut expected = [
        first.measurement().unwrap().total(),
        second.measurement().unwrap().total(),
    ];
    expected.sort_unstable();
    assert_eq!(summary.samples(), 2);
    assert_eq!(summary.fastest(), expected[0]);
    assert_eq!(summary.median(), expected[1]);
    assert_eq!(summary.slowest(), expected[1]);
    assert_eq!(
        summary.resolve_slowest(),
        first
            .measurement()
            .unwrap()
            .resolve()
            .max(second.measurement().unwrap().resolve())
    );
    assert_eq!(summary.model(), &key().model);
    assert_eq!(summary.geometry(), &key().geometry);
    assert_eq!(summary.execution(), &execution());
    jobs.request(input()).unwrap();
    let third = take(&mut jobs, 3);
    assert!(profile.record(&third).is_err());
    assert_eq!(profile.observations().len(), 2);
    assert_eq!(profile.summary().unwrap().slowest(), summary.slowest());
}

#[test]
fn profile_refuses_raw_results_and_changed_model_geometry_or_execution_atomically() {
    use motion_host::depth::capability::Profile;
    for capacity in [0, 65, usize::MAX] {
        assert!(Profile::new(capacity).is_err());
    }
    let mut profile = Profile::new(8).unwrap();
    let mut ordinary = Jobs::start(backend(Fault::None)).unwrap();
    ordinary.request(input()).unwrap();
    assert!(profile.record(&take(&mut ordinary, 1)).is_err());
    assert!(profile.observations().is_empty());
    drop(ordinary);
    let mut jobs = Jobs::start_measured(backend(Fault::None)).unwrap();
    jobs.request(input()).unwrap();
    profile.record(&take(&mut jobs, 1)).unwrap();
    let original = profile.summary().unwrap();
    drop(jobs);
    for field in 0..7 {
        let mut k = key();
        let mut controlled = backend(Fault::None);
        match field {
            0 => k.model.processing.push_str("-other"),
            1 => k.geometry.mirror_x = true,
            2 => controlled.execution.provider.push_str("-other"),
            3 => controlled.execution.runtime_sha256 = [61; 32],
            4 => controlled.execution.plan_sha256 = [62; 32],
            5 => controlled.execution.precision = "float16".into(),
            _ => controlled.retained_gpu_buffers = Some(1 << 20),
        }
        let candidate = Input::new(k.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap();
        let mut other = Jobs::start_measured(controlled).unwrap();
        other.request(candidate).unwrap();
        wait_done(&other, 1);
        let ready = other.take_prepared_ready(&k).unwrap().unwrap();
        assert!(profile.record(&ready).is_err());
        let retained = profile.summary().unwrap();
        assert_eq!(retained.samples(), 1);
        assert_eq!(retained.model(), original.model());
        assert_eq!(retained.geometry(), original.geometry());
        assert_eq!(retained.execution(), original.execution());
        assert_eq!(retained.slowest(), original.slowest());
    }
    // Another worker's request #1 is independent, and source/PTS changes remain
    // explicit observations rather than being silently replaced or trimmed.
    let mut k = key();
    k.source.asset += 1;
    k.source.pts.ticks += 2;
    let candidate = Input::new(k.clone(), [2, 1], Arc::from([6u8, 5, 4, 3, 2, 1])).unwrap();
    let mut other = Jobs::start_measured(backend(Fault::None)).unwrap();
    other.request(candidate).unwrap();
    wait_done(&other, 1);
    profile
        .record(&other.take_prepared_ready(&k).unwrap().unwrap())
        .unwrap();
    assert_eq!(profile.observations().len(), 2);
    assert_eq!(profile.observations()[1].source(), &k.source);
    assert_ne!(
        profile.observations()[0].input().rgb_sha256,
        profile.observations()[1].input().rgb_sha256
    );
}

#[test]
fn measured_buffer_capacity_preserves_unknown_and_refuses_impossible_metadata() {
    for capacity in [None, Some(1 << 20), Some(512 << 20)] {
        let mut controlled = backend(Fault::None);
        controlled.retained_gpu_buffers = capacity;
        let mut jobs = Jobs::start_measured(controlled).unwrap();
        jobs.request(input()).unwrap();
        let ready = take(&mut jobs, 1);
        assert_eq!(
            ready.measurement().unwrap().retained_gpu_buffer_capacity(),
            capacity
        );
        let mut profile = motion_host::depth::capability::Profile::new(1).unwrap();
        profile.record(&ready).unwrap();
        assert_eq!(
            profile.summary().unwrap().retained_gpu_buffer_capacity(),
            capacity
        );
    }
    for capacity in [0, (512 << 20) + 1, u64::MAX] {
        let mut controlled = backend(Fault::None);
        controlled.retained_gpu_buffers = Some(capacity);
        let mut jobs = Jobs::start_measured(controlled).unwrap();
        jobs.request(input()).unwrap();
        wait_done(&jobs, 1);
        assert!(matches!(
            jobs.take_prepared_ready(&key()),
            Err(Error::Invalid(_))
        ));
        assert_eq!(jobs.counts().unwrap().failed, 1);
        assert!(jobs.in_flight().unwrap().is_none());
    }
}

#[test]
fn routed_probes_keep_only_the_selected_backends_buffer_observation() {
    let mut first = backend(Fault::None);
    first.retained_gpu_buffers = Some(1 << 20);
    let second = backend(Fault::None);
    let mut second_key = key();
    second_key.model.processing.push_str("-second");
    let routed = motion_host::depth::routing::Routed::new(
        key().model,
        first,
        second_key.model.clone(),
        second,
    )
    .unwrap();
    let mut jobs = Jobs::start_measured(routed).unwrap();
    jobs.request(input()).unwrap();
    let first = take(&mut jobs, 1);
    assert_eq!(
        first.measurement().unwrap().retained_gpu_buffer_capacity(),
        Some(1 << 20)
    );
    jobs.request(Input::new(second_key.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
        .unwrap();
    wait_done(&jobs, 2);
    let second = jobs.take_prepared_ready(&second_key).unwrap().unwrap();
    assert_eq!(
        second.measurement().unwrap().retained_gpu_buffer_capacity(),
        None
    );
    assert_eq!(
        second.estimate().resource.descriptor().model,
        second_key.model
    );
}

fn sampled_profile(
    k: Key,
    execution: Execution,
    rgb: [u8; 6],
    count: usize,
) -> motion_host::depth::capability::Profile {
    let mut controlled = backend(Fault::None);
    controlled.execution = execution;
    let mut jobs = Jobs::start_measured(controlled).unwrap();
    let mut profile = motion_host::depth::capability::Profile::new(count).unwrap();
    for n in 1..=count {
        jobs.request(Input::new(k.clone(), [2, 1], Arc::from(rgb)).unwrap())
            .unwrap();
        wait_done(&jobs, n as u64);
        profile
            .record(&jobs.take_prepared_ready(&k).unwrap().unwrap())
            .unwrap();
    }
    profile
}

fn latency_request() -> motion_host::depth::capability::latency::Request {
    motion_host::depth::capability::latency::Request {
        model: key().model,
        geometry: key().geometry,
        backend: None,
        maximum_worker_latency: Duration::MAX,
        minimum_samples: 2,
    }
}

#[test]
fn latency_selection_retains_requested_and_actual_execution_without_backend_fallback() {
    use motion_host::depth::capability::latency::{self, Admission, Disposition};
    let first = sampled_profile(key(), execution(), [1, 2, 3, 4, 5, 6], 2);
    let mut actual = execution();
    actual.backend = "other-backend".into();
    actual.provider = "actual-other-provider".into();
    actual.runtime_sha256 = [81; 32];
    actual.precision = "float16".into();
    let second = sampled_profile(key(), actual.clone(), [1, 2, 3, 4, 5, 6], 2);
    let mut request = latency_request();
    request.backend = Some(actual.backend.clone());
    let decision = latency::select(request, &[&first, &second]).unwrap();
    assert_eq!(decision.disposition(), Disposition::Selected);
    assert_eq!(
        decision.requested().backend.as_deref(),
        Some("other-backend")
    );
    assert_eq!(decision.requested().model, key().model);
    assert_eq!(decision.resolved().unwrap().execution(), &actual);
    assert_eq!(
        decision.candidates()[0].admission(),
        Admission::DifferentBackend
    );
    assert_eq!(decision.candidates()[1].admission(), Admission::Eligible);

    let automatic = latency::select(latency_request(), &[&first, &second]).unwrap();
    let resolved = automatic.resolved().unwrap();
    let a = first.summary().unwrap();
    let b = second.summary().unwrap();
    assert_eq!(resolved.slowest(), a.slowest().min(b.slowest()));
    assert!(resolved.execution() == &execution() || resolved.execution() == &actual);
    assert!(automatic.requested().backend.is_none());

    let mut unavailable = latency_request();
    unavailable.backend = Some("unavailable-backend".into());
    let refused = latency::select(unavailable, &[&first, &second]).unwrap();
    assert_eq!(refused.disposition(), Disposition::PrecomputedRequired);
    assert!(refused.resolved().is_none());
    assert!(refused
        .candidates()
        .iter()
        .all(|c| c.admission() == Admission::DifferentBackend));
    let empty = latency::select(latency_request(), &[]).unwrap();
    assert_eq!(empty.disposition(), Disposition::PrecomputedRequired);
    assert!(empty.resolved().is_none());
}

#[test]
fn latency_selection_uses_all_samples_and_preserves_the_exact_budget_boundary() {
    use motion_host::depth::capability::latency::{self, Admission};
    let profile = sampled_profile(key(), execution(), [1, 2, 3, 4, 5, 6], 3);
    let slowest = profile.summary().unwrap().slowest();
    assert!(!slowest.is_zero());
    let mut request = latency_request();
    request.maximum_worker_latency = slowest;
    let accepted = latency::select(request.clone(), &[&profile]).unwrap();
    assert_eq!(accepted.candidates()[0].admission(), Admission::Eligible);
    assert_eq!(accepted.resolved().unwrap().samples(), 3);
    assert_eq!(accepted.resolved().unwrap().slowest(), slowest);
    request.maximum_worker_latency = slowest - Duration::from_nanos(1);
    let refused = latency::select(request, &[&profile]).unwrap();
    assert_eq!(
        refused.candidates()[0].admission(),
        Admission::OverLatencyBudget
    );
    assert!(refused.resolved().is_none());
    assert_eq!(refused.candidates()[0].summary().samples(), 3);
    assert_eq!(profile.observations().len(), 3);
    let mut request = latency_request();
    request.minimum_samples = 4;
    let insufficient = latency::select(request, &[&profile]).unwrap();
    assert_eq!(
        insufficient.candidates()[0].admission(),
        Admission::InsufficientSamples
    );
    assert!(insufficient.resolved().is_none());
}

#[test]
fn latency_selection_refuses_unpaired_source_rgb_counts_and_duplicate_executions() {
    use motion_host::depth::capability::latency;
    let first = sampled_profile(key(), execution(), [1, 2, 3, 4, 5, 6], 2);
    assert!(latency::select(latency_request(), &[&first, &first]).is_err());
    let duplicate = sampled_profile(key(), execution(), [1, 2, 3, 4, 5, 6], 2);
    assert!(latency::select(latency_request(), &[&first, &duplicate]).is_err());
    let mut alternate = execution();
    alternate.provider.push_str("-other");
    for field in 0..5 {
        let mut k = key();
        let mut rgb = [1, 2, 3, 4, 5, 6];
        let mut count = 2;
        match field {
            0 => k.source.asset += 1,
            1 => k.source.revision += 1,
            2 => k.source.pts.ticks += 1,
            3 => rgb[0] += 1,
            _ => count = 1,
        }
        let other = sampled_profile(k, alternate.clone(), rgb, count);
        assert!(latency::select(latency_request(), &[&first, &other]).is_err());
    }
    // Rationally equivalent PTS denotes the same sample, rather than a false
    // source mismatch introduced by an alternate clock representation.
    let mut equivalent = key();
    equivalent.source.pts.ticks *= 2;
    equivalent.source.pts.timescale *= 2;
    let paired = sampled_profile(equivalent, alternate, [1, 2, 3, 4, 5, 6], 2);
    assert!(latency::select(latency_request(), &[&first, &paired])
        .unwrap()
        .resolved()
        .is_some());
    let mut forward = motion_host::depth::capability::Profile::new(2).unwrap();
    let mut reverse = motion_host::depth::capability::Profile::new(2).unwrap();
    for (index, profile) in [&mut forward, &mut reverse].into_iter().enumerate() {
        let mut controlled = backend(Fault::None);
        controlled.execution.provider = format!("ordered-provider-{index}");
        let mut jobs = Jobs::start_measured(controlled).unwrap();
        for n in 0..2 {
            let mut k = key();
            k.source.pts.ticks += if index == 0 { n } else { 1 - n };
            jobs.request(Input::new(k.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap())
                .unwrap();
            wait_done(&jobs, (n + 1) as u64);
            profile
                .record(&jobs.take_prepared_ready(&k).unwrap().unwrap())
                .unwrap();
        }
    }
    assert!(latency::select(latency_request(), &[&forward, &reverse]).is_err());
}

#[test]
fn latency_selection_bounds_requests_and_keeps_model_geometry_refusal_reasons() {
    use motion_host::depth::capability::{
        latency::{self, Admission},
        Profile,
    };
    let mut other_model = key();
    other_model.model.processing.push_str("-other");
    let model = sampled_profile(other_model, execution(), [1, 2, 3, 4, 5, 6], 2);
    let mut other_geometry = key();
    other_geometry.geometry.mirror_x = true;
    let geometry = sampled_profile(other_geometry, execution(), [1, 2, 3, 4, 5, 6], 2);
    let refused = latency::select(latency_request(), &[&model, &geometry]).unwrap();
    assert_eq!(
        refused.candidates()[0].admission(),
        Admission::DifferentModel
    );
    assert_eq!(
        refused.candidates()[1].admission(),
        Admission::DifferentGeometry
    );
    assert!(refused.resolved().is_none());
    let empty = Profile::new(2).unwrap();
    assert!(latency::select(latency_request(), &[&empty]).is_err());
    assert!(latency::select(latency_request(), &[&model; 9]).is_err());
    for field in 0..7 {
        let mut request = latency_request();
        match field {
            0 => request.maximum_worker_latency = Duration::ZERO,
            1 => request.minimum_samples = 0,
            2 => request.minimum_samples = 65,
            3 => request.backend = Some(String::new()),
            4 => request.backend = Some("a".repeat(129)),
            5 => request.backend = Some("bad\nidentity".into()),
            _ => request.model.sha256 = [0; 32],
        }
        assert!(latency::select(request, &[]).is_err());
    }
}
