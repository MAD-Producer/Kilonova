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
    block: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl Backend for Controlled {
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
            _ => Ok(Some(execution())),
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
                "foreign-provider"
            } else {
                "actual-provider"
            }
            .into(),
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
