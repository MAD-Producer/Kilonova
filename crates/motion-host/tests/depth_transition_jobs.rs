#![cfg(feature = "ai-depth")]

use motion_host::depth::{
    transition::{Activity, Choices, Controller, Error, Policy, Ticket},
    Backend, Cancellation, Costs, Estimate, Input, Jobs, Key,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

fn choices(frame: i64) -> Choices {
    let key = Key {
        source: DepthSource {
            asset: 75,
            revision: 1,
            pts: DepthTime {
                ticks: frame,
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
            id: "controlled-transition".into(),
            revision: "r1".into(),
            sha256: [17; 32],
            processing: "explicit-control".into(),
        },
    };
    let mut settled = key.clone();
    settled.model.sha256 = [23; 32];
    Choices {
        interactive: key,
        settled,
    }
}
fn controller() -> Controller {
    Controller::new(Policy {
        idle_delay: Duration::from_millis(100),
        minimum_interactive_residence: Duration::from_millis(300),
    })
    .unwrap()
}
fn ticket(c: &mut Controller, frame: i64) -> Ticket {
    c.observe(
        choices(frame),
        Activity::Interactive,
        Duration::from_millis(frame as u64),
    )
    .unwrap()
    .unwrap()
}
fn input(key: &Key) -> Input {
    Input::new(key.clone(), [2, 1], Arc::from([1u8, 2, 3, 4, 5, 6])).unwrap()
}
struct Controlled {
    entered: mpsc::Sender<Key>,
    release: mpsc::Receiver<()>,
}
impl Backend for Controlled {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        self.entered.send(input.key().clone()).unwrap();
        self.release.recv_timeout(Duration::from_secs(5)).unwrap();
        let key = input.key();
        if key.source.asset == 999 {
            return Err(motion_host::depth::Error::Backend(
                "original controlled failure".into(),
            ));
        }
        // Deliberately complete after cancellation: the real worker and ticket
        // admission, rather than a cooperative mock success, must reject it.
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: [2, 1],
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
                vec![0.25, 2.],
                None,
                None,
            )?,
            provider: "actual-controlled-provider".into(),
            costs: Costs {
                load: Duration::from_millis(7),
                ..Costs::default()
            },
        })
    }
}
// Declared after Jobs in each test, so assertion unwinding releases a blocked
// backend before Jobs joins. The fixture timeout remains a failed control.
struct Release(mpsc::Sender<()>);
impl Release {
    fn send(&self) {
        self.0.send(()).unwrap();
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
fn worker() -> (Jobs, mpsc::Receiver<Key>, Release) {
    let (entered, rx) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    (
        Jobs::start(Controlled {
            entered,
            release: blocked,
        })
        .unwrap(),
        rx,
        Release(release),
    )
}
fn completed(jobs: &Jobs, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let c = jobs.counts().unwrap();
        if c.completed + c.canceled + c.failed >= count {
            return;
        }
        assert!(
            Instant::now() < until,
            "controlled transition did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn request_rejects_foreign_stale_and_wrong_input_before_touching_the_worker() {
    let mut c = controller();
    let old = ticket(&mut c, 0);
    let current = ticket(&mut c, 1);
    let foreign = ticket(&mut controller(), 1);
    let (mut jobs, entered, release) = worker();
    assert!(c
        .request(&old, &mut jobs, input(old.key()))
        .unwrap()
        .is_none());
    assert!(c
        .request(&foreign, &mut jobs, input(foreign.key()))
        .unwrap()
        .is_none());
    assert!(matches!(
        c.request(&current, &mut jobs, input(old.key())),
        Err(Error::Input)
    ));
    assert_eq!(jobs.counts().unwrap().submitted, 0);
    assert!(c
        .request(&current, &mut jobs, input(current.key()))
        .unwrap()
        .is_some());
    assert!(entered
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .same_sample(current.key()));
    assert!(jobs.in_flight().unwrap().is_some());
    release.send();
    completed(&jobs, 1);
    let ready = c
        .take_ready(&current, &mut jobs, current.key())
        .unwrap()
        .unwrap();
    assert_eq!(ready.estimate().provider, "actual-controlled-provider");
    assert_eq!(ready.estimate().costs.load, Duration::from_millis(7));
    assert_eq!(
        ready.estimate().resource.descriptor().model,
        current.key().model
    );
}

#[test]
fn late_canceled_completion_and_stale_poll_cannot_replace_or_consume_the_new_frame() {
    let mut c = controller();
    let old = ticket(&mut c, 0);
    let (mut jobs, entered, release) = worker();
    c.request(&old, &mut jobs, input(old.key()))
        .unwrap()
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let current = ticket(&mut c, 1);
    c.request(&current, &mut jobs, input(current.key()))
        .unwrap()
        .unwrap();
    assert_eq!(jobs.queue_depth().unwrap(), 1);
    release.send();
    assert!(entered
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .same_sample(current.key()));
    release.send();
    completed(&jobs, 2);
    let counts = jobs.counts().unwrap();
    assert_eq!(
        (counts.completed, counts.canceled, counts.failed),
        (1, 1, 0)
    );
    assert!(c.take_ready(&old, &mut jobs, old.key()).unwrap().is_none());
    assert!(c
        .take_ready(&current, &mut jobs, old.key())
        .unwrap()
        .is_none());
    let ready = c
        .take_ready(&current, &mut jobs, current.key())
        .unwrap()
        .unwrap();
    assert!(ready
        .estimate()
        .resource
        .descriptor()
        .source
        .same_sample(&current.key().source));
    assert!(c
        .take_ready(&current, &mut jobs, current.key())
        .unwrap()
        .is_none());
}

#[test]
fn invalidation_without_replacement_revokes_ticket_and_cancels_actual_in_flight_work() {
    let mut c = controller();
    let current = ticket(&mut c, 0);
    let (mut jobs, entered, release) = worker();
    c.request(&current, &mut jobs, input(current.key()))
        .unwrap()
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    c.invalidate_jobs(&mut jobs, Duration::from_millis(1))
        .unwrap();
    assert!(c.current().is_none());
    release.send();
    completed(&jobs, 1);
    assert_eq!(jobs.counts().unwrap().canceled, 1);
    assert!(c
        .take_ready(&current, &mut jobs, current.key())
        .unwrap()
        .is_none());
    assert!(c
        .request(&current, &mut jobs, input(current.key()))
        .unwrap()
        .is_none());
    assert_eq!(jobs.counts().unwrap().submitted, 1);
}

#[test]
fn actual_worker_failure_preserves_original_cause_and_is_not_retried() {
    let mut c = controller();
    let mut choice = choices(0);
    choice.interactive.source.asset = 999;
    choice.settled.source.asset = 999;
    let current = c
        .observe(choice, Activity::Interactive, Duration::ZERO)
        .unwrap()
        .unwrap();
    let (mut jobs, entered, release) = worker();
    c.request(&current, &mut jobs, input(current.key()))
        .unwrap()
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send();
    completed(&jobs, 1);
    match c.take_ready(&current, &mut jobs, current.key()) {
        Err(Error::Job(motion_host::depth::Error::Backend(cause))) => {
            assert_eq!(cause, "original controlled failure")
        }
        other => panic!("original worker error was not retained: {other:?}"),
    }
    assert!(c
        .take_ready(&current, &mut jobs, current.key())
        .unwrap()
        .is_none());
    assert_eq!(jobs.counts().unwrap().submitted, 1);
    assert_eq!(jobs.counts().unwrap().failed, 1);
}
