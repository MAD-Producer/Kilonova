#![cfg(feature = "ai-depth")]

use motion_host::depth::{
    alignment::{Limits, Rejection},
    transition::{
        admission::{Admission, Error as AdmissionError},
        Activity, Choices, Controller, Error as TransitionError, Policy, Ticket,
    },
    Backend, Cancellation, Costs, Error, Estimate, Input, Jobs, Key, PreparedEstimate,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::{mpsc, Arc, Mutex},
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

fn choices(asset: u64) -> Choices {
    let reference = Key {
        source: DepthSource {
            asset,
            revision: 1,
            pts: DepthTime {
                ticks: 0,
                timescale: 24,
            },
        },
        geometry: DepthGeometry {
            source_size: [20, 1],
            crop: [0, 0, 20, 1],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "worker-alignment-control".into(),
            revision: "r1".into(),
            sha256: [1; 32],
            processing: "reference".into(),
        },
    };
    let mut candidate = reference.clone();
    candidate.model.processing = "candidate".into();
    Choices {
        interactive: reference,
        settled: candidate,
    }
}
fn controller() -> Controller {
    Controller::new(Policy {
        idle_delay: Duration::from_millis(100),
        minimum_interactive_residence: Duration::from_millis(300),
    })
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        minimum_fit_pairs: 3,
        minimum_validation_pairs: 3,
        minimum_pair_fraction: 1.,
        minimum_confidence: None,
        minimum_reference_stddev: 1e-8,
        scale_bounds: [0.1, 10.],
        maximum_normalized_rms: 1e-8,
        maximum_normalized_error: 1e-8,
    }
}
fn input(key: &Key, pixel: u8) -> Input {
    Input::new(key.clone(), [20, 1], Arc::from(vec![pixel; 60])).unwrap()
}
struct Analytic {
    calls: mpsc::Sender<ThreadId>,
    fail_candidate: bool,
}
impl Backend for Analytic {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        self.calls.send(thread::current().id()).unwrap();
        if self.fail_candidate && input.key().model.processing == "candidate" {
            return Err(Error::Backend(
                "original worker alignment inference failure".into(),
            ));
        }
        let key = input.key();
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: [20, 1],
                    source: key.source.clone(),
                    geometry: key.geometry.clone(),
                    model: key.model.clone(),
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                (0..20)
                    .map(|i| {
                        let x = i as f32 * 0.25;
                        if key.model.processing == "reference" {
                            2. * x + 3.
                        } else {
                            x
                        }
                    })
                    .collect(),
                None,
                None,
            )?,
            provider: "actual-worker-alignment-provider".into(),
            costs: Costs {
                inference: Duration::from_millis(7),
                ..Costs::default()
            },
        })
    }
}
fn worker(fail_candidate: bool) -> (Jobs, mpsc::Receiver<ThreadId>) {
    let (calls, received) = mpsc::channel();
    (
        Jobs::start(Analytic {
            calls,
            fail_candidate,
        })
        .unwrap(),
        received,
    )
}
fn completed(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let counts = jobs.counts().unwrap();
        if counts.completed + counts.canceled + counts.failed >= count {
            return;
        }
        assert!(
            Instant::now() < until,
            "worker alignment fixture did not complete"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn reference(c: &mut Controller, jobs: &mut Jobs, asset: u64) -> (Ticket, PreparedEstimate) {
    let first = c
        .observe(choices(asset), Activity::Idle, Duration::ZERO)
        .unwrap()
        .unwrap();
    c.request(&first, jobs, input(first.key(), 1))
        .unwrap()
        .unwrap();
    completed(jobs, 1);
    let reference = c.take_ready(&first, jobs, first.key()).unwrap().unwrap();
    let next = c
        .observe(choices(asset), Activity::Idle, Duration::from_millis(300))
        .unwrap()
        .unwrap();
    (next, reference)
}
fn aligned(c: &Controller, ticket: &Ticket, jobs: &mut Jobs) -> Admission {
    completed(jobs, 2);
    c.take_aligned_ready(ticket, jobs, ticket.key())
        .unwrap()
        .unwrap()
}

#[test]
fn actual_worker_returns_a_proof_with_original_provider_pixels_and_measured_alignment() {
    let mut c = controller();
    let (mut jobs, calls) = worker(false);
    let (ticket, reference) = reference(&mut c, &mut jobs, 571);
    let first_thread = calls.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(first_thread, thread::current().id());
    c.request_aligned(
        &ticket,
        &mut jobs,
        input(ticket.key(), 1),
        &reference,
        limits(),
    )
    .unwrap()
    .unwrap();
    let admitted = aligned(&c, &ticket, &mut jobs);
    assert_eq!(
        calls.recv_timeout(Duration::from_secs(5)).unwrap(),
        first_thread
    );
    let view = admitted.view(&c, ticket.key()).unwrap().unwrap();
    for i in 0..20 {
        assert_eq!(
            view.sample(i).unwrap(),
            reference.estimate().resource.sample(i)
        );
    }
    assert_eq!(view.provider(), "actual-worker-alignment-provider");
    assert_eq!(view.descriptor().model, ticket.key().model);
    assert_eq!(view.input(), reference.input());
    assert_eq!(view.costs().inference, Duration::from_millis(7));
    assert!(!view.costs().alignment.is_zero());
    assert!(view.costs().total >= view.costs().alignment);
    assert_eq!(admitted.statistics().validation_pairs, 10);
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
    assert_eq!(jobs.counts().unwrap().completed, 2);
}

#[test]
fn stale_foreign_and_wrong_input_aligned_requests_do_not_submit_work() {
    let mut c = controller();
    let (mut jobs, _calls) = worker(false);
    let (ticket, reference) = reference(&mut c, &mut jobs, 572);
    let foreign = controller()
        .observe(choices(572), Activity::Idle, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert!(c
        .request_aligned(
            &foreign,
            &mut jobs,
            input(foreign.key(), 1),
            &reference,
            limits()
        )
        .unwrap()
        .is_none());
    let mut wrong = ticket.key().clone();
    wrong.source.revision += 1;
    assert!(matches!(
        c.request_aligned(&ticket, &mut jobs, input(&wrong, 1), &reference, limits()),
        Err(TransitionError::Input)
    ));
    c.invalidate(Duration::from_millis(301)).unwrap();
    assert!(c
        .request_aligned(
            &ticket,
            &mut jobs,
            input(ticket.key(), 1),
            &reference,
            limits()
        )
        .unwrap()
        .is_none());
    assert_eq!(jobs.counts().unwrap().submitted, 1);
}

#[test]
fn real_worker_preserves_pixel_and_original_scale_refusals_without_raw_output() {
    for (asset, pixel, bounds) in [(573, 2, [0.1, 10.]), (574, 1, [0.5, 1.5])] {
        let mut c = controller();
        let (mut jobs, _calls) = worker(false);
        let (ticket, reference) = reference(&mut c, &mut jobs, asset);
        let mut qualified = limits();
        qualified.scale_bounds = bounds;
        c.request_aligned(
            &ticket,
            &mut jobs,
            input(ticket.key(), pixel),
            &reference,
            qualified,
        )
        .unwrap()
        .unwrap();
        completed(&jobs, 2);
        let result = c.take_aligned_ready(&ticket, &mut jobs, ticket.key());
        match (pixel, result) {
            (2, Err(TransitionError::Job(Error::Alignment(AdmissionError::Pixels)))) => {}
            (
                1,
                Err(TransitionError::Job(Error::Alignment(AdmissionError::Alignment(
                    Rejection::Scale {
                        actual,
                        minimum,
                        maximum,
                    },
                )))),
            ) => {
                assert_eq!([actual, minimum, maximum], [2., 0.5, 1.5]);
            }
            _ => panic!("original worker alignment cause was replaced"),
        }
        assert_eq!(jobs.counts().unwrap().failed, 1);
        assert_eq!(jobs.counts().unwrap().completed, 1);
        assert!(jobs.take_ready(ticket.key()).unwrap().is_none());
    }
}

struct Gate {
    entered: mpsc::Sender<ThreadId>,
    release: mpsc::Receiver<()>,
}
struct GateLog(Mutex<Option<Gate>>);
static LOGGER: GateLog = GateLog(Mutex::new(None));
impl log::Log for GateLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn flush(&self) {}
    fn log(&self, record: &log::Record<'_>) {
        if !record.args().to_string().contains("depth_align asset=575 ") {
            return;
        }
        let gate = self.0.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.send(thread::current().id()).unwrap();
            gate.release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
}
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn alignment_phase_is_published_before_blocking_and_cancellation_cannot_publish_old_work() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let mut c = controller();
    let (mut jobs, calls) = worker(false);
    let (ticket, reference) = reference(&mut c, &mut jobs, 575);
    let worker_thread = calls.recv_timeout(Duration::from_secs(5)).unwrap();
    let (entered, received) = mpsc::channel();
    let (send, release) = mpsc::channel();
    // Drop order releases the logger before Jobs joins if any assertion fails.
    let guard = Release(send);
    *LOGGER.0.lock().unwrap() = Some(Gate { entered, release });
    let request = c
        .request_aligned(
            &ticket,
            &mut jobs,
            input(ticket.key(), 1),
            &reference,
            limits(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        worker_thread
    );
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.request, request);
    assert_eq!(flight.phase, "alignment");
    assert_eq!(flight.asset, 575);
    assert_eq!(flight.source.as_ref(), Some(&ticket.key().source));
    assert_eq!(jobs.counts().unwrap().completed, 1);
    c.invalidate_jobs(&mut jobs, Duration::from_millis(301))
        .unwrap();
    let next = c
        .observe(
            choices(575),
            Activity::Interactive,
            Duration::from_millis(301),
        )
        .unwrap()
        .unwrap();
    c.request(&next, &mut jobs, input(next.key(), 1))
        .unwrap()
        .unwrap();
    assert_eq!(jobs.queue_depth().unwrap(), 1);
    drop(guard);
    completed(&jobs, 3);
    assert!(c
        .take_aligned_ready(&ticket, &mut jobs, ticket.key())
        .unwrap()
        .is_none());
    let current = c.take_ready(&next, &mut jobs, next.key()).unwrap().unwrap();
    assert_eq!(
        current.estimate().resource.descriptor().model,
        next.key().model
    );
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert_eq!(jobs.counts().unwrap().failed, 0);
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
}

#[test]
fn raw_and_aligned_completion_apis_cannot_silently_bypass_the_proof() {
    let mut c = controller();
    let (mut jobs, _calls) = worker(false);
    let (ticket, reference) = reference(&mut c, &mut jobs, 576);
    c.request_aligned(
        &ticket,
        &mut jobs,
        input(ticket.key(), 1),
        &reference,
        limits(),
    )
    .unwrap()
    .unwrap();
    completed(&jobs, 2);
    assert!(matches!(
        jobs.take_ready(ticket.key()),
        Err(Error::Invalid(
            "aligned completion requires its admission API"
        ))
    ));
    c.request(&ticket, &mut jobs, input(ticket.key(), 1))
        .unwrap()
        .unwrap();
    completed(&jobs, 3);
    assert!(matches!(
        c.take_aligned_ready(&ticket, &mut jobs, ticket.key()),
        Err(TransitionError::Job(Error::Invalid(
            "raw completion requires its prepared result API"
        )))
    ));
}

#[test]
fn an_unbound_request_or_foreign_worker_cannot_steal_the_completed_alignment() {
    let mut c = controller();
    let (mut jobs, _calls) = worker(false);
    let (ticket, reference) = reference(&mut c, &mut jobs, 577);
    c.request_aligned(
        &ticket,
        &mut jobs,
        input(ticket.key(), 1),
        &reference,
        limits(),
    )
    .unwrap()
    .unwrap();
    completed(&jobs, 2);
    let (mut foreign, _foreign_calls) = worker(false);
    assert!(c
        .take_aligned_ready(&ticket, &mut foreign, ticket.key())
        .unwrap()
        .is_none());
    let mut wrong_live = ticket.key().clone();
    wrong_live.source.pts.ticks += 1;
    assert!(c
        .take_aligned_ready(&ticket, &mut jobs, &wrong_live)
        .unwrap()
        .is_none());
    let admitted = c
        .take_aligned_ready(&ticket, &mut jobs, ticket.key())
        .unwrap()
        .unwrap();
    assert!(admitted.view(&c, ticket.key()).unwrap().is_some());
    jobs.request(input(ticket.key(), 1)).unwrap();
    completed(&jobs, 3);
    assert!(c
        .take_aligned_ready(&ticket, &mut jobs, ticket.key())
        .unwrap()
        .is_none());
    assert!(jobs.take_prepared_ready(ticket.key()).unwrap().is_some());
    c.invalidate(Duration::from_millis(301)).unwrap();
    assert!(admitted.view(&c, ticket.key()).unwrap().is_none());
}

#[test]
fn actual_backend_failure_keeps_the_original_cause_without_alignment_or_retry() {
    let mut c = controller();
    let (mut jobs, calls) = worker(true);
    let (ticket, reference) = reference(&mut c, &mut jobs, 578);
    c.request_aligned(
        &ticket,
        &mut jobs,
        input(ticket.key(), 1),
        &reference,
        limits(),
    )
    .unwrap()
    .unwrap();
    completed(&jobs, 2);
    assert!(
        matches!(c.take_aligned_ready(&ticket, &mut jobs, ticket.key()),
        Err(TransitionError::Job(Error::Backend(cause))) if cause == "original worker alignment inference failure")
    );
    assert_eq!(calls.try_iter().count(), 2);
    assert_eq!(jobs.counts().unwrap().submitted, 2);
    assert_eq!(jobs.counts().unwrap().failed, 1);
}
