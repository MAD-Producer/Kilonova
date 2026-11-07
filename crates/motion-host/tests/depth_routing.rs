#![cfg(feature = "ai-depth")]

use motion_host::depth::{
    routing::Routed,
    transition::{Activity, Choices, Controller, Policy},
    Backend, Cancellation, Costs, Error, Estimate, Input, Jobs, Key,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

fn key() -> Key {
    Key {
        source: DepthSource {
            asset: 75,
            revision: 9,
            pts: DepthTime {
                ticks: 17,
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
            id: "routing-control".into(),
            revision: "r1".into(),
            sha256: [31; 32],
            processing: "explicit-profile-a".into(),
        },
    }
}
fn choices() -> Choices {
    let first = key();
    let mut second = first.clone();
    // Same named model, distinct exact profile: no resolution/quality heuristic.
    second.model.processing = "explicit-profile-b".into();
    Choices {
        interactive: first,
        settled: second,
    }
}
fn input(key: &Key) -> Input {
    Input::new(key.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap()
}
#[derive(Debug)]
struct Entered {
    route: u8,
    key: Key,
    thread: std::thread::ThreadId,
}
struct Controlled {
    route: u8,
    entered: mpsc::Sender<Entered>,
    release: Option<mpsc::Receiver<()>>,
    fail: bool,
    wrong_identity: bool,
}
impl Backend for Controlled {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        self.entered
            .send(Entered {
                route: self.route,
                key: input.key().clone(),
                thread: std::thread::current().id(),
            })
            .unwrap();
        if let Some(release) = &self.release {
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        if self.fail {
            return Err(Error::Backend("original selected-route cause".into()));
        }
        let mut model = input.key().model.clone();
        if self.wrong_identity {
            model.sha256 = [99; 32];
        }
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: [2, 1],
                    source: input.key().source.clone(),
                    geometry: input.key().geometry.clone(),
                    model,
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                vec![self.route as f32, 2.],
                None,
                None,
            )?,
            provider: format!("actual-controlled-route-{}", self.route),
            costs: Costs {
                inference: Duration::from_millis(self.route as u64),
                ..Costs::default()
            },
        })
    }
}
// A distinct concrete backend type exercises heterogeneous routing, as used by
// an explicitly resolved ONNX/WGSL pair, without asserting hardware capability.
struct OtherBackend(Controlled);
impl Backend for OtherBackend {
    fn infer(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.0.infer(input, cancellation)
    }
}
fn backend(route: u8, entered: &mpsc::Sender<Entered>) -> Controlled {
    Controlled {
        route,
        entered: entered.clone(),
        release: None,
        fail: false,
        wrong_identity: false,
    }
}
fn completed(jobs: &Jobs, expected: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let c = jobs.counts().unwrap();
        if c.completed + c.canceled + c.failed >= expected {
            return;
        }
        assert!(
            Instant::now() < until,
            "controlled routed worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn ambiguous_or_invalid_bindings_are_rejected_before_backend_entry() {
    let (entered, rx) = mpsc::channel();
    let first = key().model;
    assert!(matches!(
        Routed::new(
            first.clone(),
            backend(1, &entered),
            first.clone(),
            backend(2, &entered)
        ),
        Err(Error::Invalid(_))
    ));
    for field in 0..3 {
        let mut invalid = choices().settled.model;
        match field {
            0 => invalid.id.clear(),
            1 => invalid.revision.push('\n'),
            _ => invalid.processing = "x".repeat(257),
        }
        assert!(matches!(
            Routed::new(
                first.clone(),
                backend(1, &entered),
                invalid,
                backend(2, &entered)
            ),
            Err(Error::Resource(_))
        ));
    }
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn unbound_id_revision_hash_or_profile_never_enters_another_route() {
    let (entered, rx) = mpsc::channel();
    let choice = choices();
    let mut routed = Routed::new(
        choice.interactive.model,
        backend(1, &entered),
        choice.settled.model,
        backend(2, &entered),
    )
    .unwrap();
    for field in 0..4 {
        let mut unknown = key();
        match field {
            0 => unknown.model.id.push_str("-foreign"),
            1 => unknown.model.revision.push_str("-foreign"),
            2 => unknown.model.sha256 = [48; 32],
            _ => unknown.model.processing.push_str("-foreign"),
        }
        assert!(matches!(
            routed.infer(&input(&unknown), &Cancellation::default()),
            Err(Error::Invalid(_))
        ));
    }
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn actual_transition_jobs_route_both_profiles_on_one_worker_and_keep_results() {
    let (entered, rx) = mpsc::channel();
    let choice = choices();
    let routed = Routed::new(
        choice.interactive.model.clone(),
        backend(1, &entered),
        choice.settled.model.clone(),
        OtherBackend(backend(2, &entered)),
    )
    .unwrap();
    let mut jobs = Jobs::start(routed).unwrap();
    let mut controller = Controller::new(Policy {
        idle_delay: Duration::from_millis(100),
        minimum_interactive_residence: Duration::from_millis(300),
    })
    .unwrap();
    let mut worker = None;
    let mut previous_request = 0;
    let mut expected_completions = 0;
    // Hysteresis transitions use the real Controller and the same actual Jobs.
    for (activity, clock, route) in [
        (Activity::Interactive, 0, 1),
        (Activity::Idle, 300, 0),
        (Activity::Idle, 400, 2),
        (Activity::Interactive, 401, 1),
    ]
    .into_iter()
    {
        let ticket = controller
            .observe(choice.clone(), activity, Duration::from_millis(clock))
            .unwrap();
        if route == 0 {
            assert!(ticket.is_none());
            continue;
        }
        let ticket = ticket.unwrap();
        let prepared = input(ticket.key());
        let request = controller
            .request(&ticket, &mut jobs, prepared)
            .unwrap()
            .unwrap();
        assert!(request > previous_request);
        previous_request = request;
        let actual = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(actual.route, route);
        assert!(actual.key.same_sample(ticket.key()));
        assert_ne!(actual.thread, std::thread::current().id());
        if let Some(worker) = worker {
            assert_eq!(actual.thread, worker);
        }
        worker = Some(actual.thread);
        expected_completions += 1;
        completed(&jobs, expected_completions);
        let result = controller
            .take_ready(&ticket, &mut jobs, ticket.key())
            .unwrap()
            .unwrap();
        assert_eq!(result.input().rgb_size, [2, 1]);
        // Independently fixed SHA-256 for the versioned 2x1 RGB fixture.
        let fingerprint: String = result
            .input()
            .rgb_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            fingerprint,
            "b750c3dee235af29380294c7c909247089a1d06c770d37a6416c566dfd264d61"
        );
        assert_eq!(
            result.estimate().provider,
            format!("actual-controlled-route-{route}")
        );
        assert_eq!(
            result.estimate().costs.inference,
            Duration::from_millis(route as u64)
        );
        assert_eq!(
            result.estimate().resource.descriptor().model,
            ticket.key().model
        );
        assert_eq!(result.estimate().resource.values(), &[route as f32, 2.]);
    }
    assert_eq!(jobs.counts().unwrap().completed, 3);
    assert_eq!(jobs.counts().unwrap().failed, 0);
    assert!(jobs.in_flight().unwrap().is_none());
    assert_eq!(jobs.queue_depth().unwrap(), 0);
}

#[test]
fn selected_failure_is_preserved_without_trying_the_other_binding() {
    let (entered, rx) = mpsc::channel();
    let choice = choices();
    let mut first = backend(1, &entered);
    first.fail = true;
    let mut jobs = Jobs::start(
        Routed::new(
            choice.interactive.model.clone(),
            first,
            choice.settled.model.clone(),
            backend(2, &entered),
        )
        .unwrap(),
    )
    .unwrap();
    jobs.request(input(&choice.interactive)).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().route, 1);
    completed(&jobs, 1);
    match jobs.take_ready(&choice.interactive) {
        Err(Error::Backend(cause)) => assert_eq!(cause, "original selected-route cause"),
        other => panic!("selected route error was replaced: {other:?}"),
    }
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(jobs.counts().unwrap().failed, 1);
    // A later explicit request may select the other configuration; no retry.
    jobs.request(input(&choice.settled)).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().route, 2);
    completed(&jobs, 2);
    assert_eq!(
        jobs.take_ready(&choice.settled).unwrap().unwrap().provider,
        "actual-controlled-route-2"
    );
    assert_eq!(jobs.counts().unwrap().submitted, 2);
}

#[test]
fn cancellation_between_routes_discards_the_blocked_old_result() {
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let choice = choices();
    let mut first = backend(1, &entered);
    first.release = Some(blocked);
    let mut jobs = Jobs::start(
        Routed::new(
            choice.interactive.model.clone(),
            first,
            choice.settled.model.clone(),
            backend(2, &entered),
        )
        .unwrap(),
    )
    .unwrap();
    // Release before Jobs joins if an assertion unwinds.
    let release = Release(release);
    jobs.request(input(&choice.interactive)).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().route, 1);
    let flight = jobs.in_flight().unwrap().unwrap();
    assert_eq!(flight.phase, "backend");
    assert_eq!(flight.source.unwrap(), choice.interactive.source);
    jobs.request(input(&choice.settled)).unwrap();
    assert_eq!(jobs.queue_depth().unwrap(), 1);
    release.0.send(()).unwrap();
    let second = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(second.route, 2);
    completed(&jobs, 2);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert_eq!(jobs.counts().unwrap().completed, 1);
    assert_eq!(
        jobs.take_ready(&choice.settled).unwrap().unwrap().provider,
        "actual-controlled-route-2"
    );
    assert!(jobs.take_ready(&choice.interactive).unwrap().is_none());
}

#[test]
fn routed_result_still_passes_the_existing_worker_identity_gate() {
    let (entered, rx) = mpsc::channel();
    let choice = choices();
    let mut first = backend(1, &entered);
    first.wrong_identity = true;
    let mut jobs = Jobs::start(
        Routed::new(
            choice.interactive.model.clone(),
            first,
            choice.settled.model,
            backend(2, &entered),
        )
        .unwrap(),
    )
    .unwrap();
    jobs.request(input(&choice.interactive)).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().route, 1);
    completed(&jobs, 1);
    assert!(matches!(
        jobs.take_ready(&choice.interactive),
        Err(Error::Invalid(
            "backend result identity differs from request"
        ))
    ));
    assert_eq!(jobs.counts().unwrap().completed, 0);
    assert_eq!(jobs.counts().unwrap().failed, 1);
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
}
