#![cfg(feature = "ai-depth")]
use motion_host::depth::{
    alignment::{Limits, Rejection},
    transition::{
        admission::{Admission, Error},
        Activity, Choices, Controller, Policy, Ticket,
    },
    Backend, Cancellation, Costs, Estimate, Input, Jobs, Key, PreparedEstimate,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn choices() -> Choices {
    let first = Key {
        source: DepthSource {
            asset: 75,
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
            id: "admission-control".into(),
            revision: "r1".into(),
            sha256: [1; 32],
            processing: "reference".into(),
        },
    };
    let mut second = first.clone();
    second.model.processing = "candidate".into();
    Choices {
        interactive: first,
        settled: second,
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
struct Analytic;
impl Backend for Analytic {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        let key = input.key();
        let values = (0..20)
            .map(|i| {
                let x = i as f32 * 0.25;
                if key.model.processing == "reference" {
                    2. * x + 3.
                } else {
                    x
                }
            })
            .collect();
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
                values,
                None,
                None,
            )?,
            provider: "actual-analytic-worker".into(),
            costs: Costs {
                inference: Duration::from_millis(7),
                ..Costs::default()
            },
        })
    }
}
fn ready(c: &mut Controller, jobs: &mut Jobs, ticket: &Ticket, pixel: u8) -> PreparedEstimate {
    c.request(
        ticket,
        jobs,
        Input::new(ticket.key().clone(), [20, 1], Arc::from(vec![pixel; 60])).unwrap(),
    )
    .unwrap()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = c.take_ready(ticket, jobs, ticket.key()).unwrap() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "actual admission fixture worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn pair(pixel: u8) -> (Controller, Ticket, PreparedEstimate, PreparedEstimate) {
    let mut c = controller();
    let mut jobs = Jobs::start(Analytic).unwrap();
    let first = c
        .observe(choices(), Activity::Idle, Duration::ZERO)
        .unwrap()
        .unwrap();
    let reference = ready(&mut c, &mut jobs, &first, 1);
    let second = c
        .observe(choices(), Activity::Idle, Duration::from_millis(300))
        .unwrap()
        .unwrap();
    let candidate = ready(&mut c, &mut jobs, &second, pixel);
    (c, second, reference, candidate)
}
#[test]
fn actual_prepared_results_admit_aligned_samples_with_original_provenance() {
    let (c, ticket, reference, candidate) = pair(1);
    let raw = candidate.estimate().resource.clone();
    let fingerprint = candidate.input();
    let admitted = Admission::prepare(
        &c,
        &ticket,
        ticket.key(),
        &reference,
        candidate,
        limits(),
        || false,
    )
    .unwrap()
    .unwrap();
    let view = admitted.view(&c, ticket.key()).unwrap().unwrap();
    for i in 0..20 {
        assert_eq!(
            view.sample(i).unwrap(),
            reference.estimate().resource.sample(i)
        );
    }
    assert_eq!(view.sample(20).unwrap(), None);
    assert_eq!(view.descriptor(), raw.descriptor());
    assert_eq!(view.provider(), "actual-analytic-worker");
    assert_eq!(view.costs().inference, Duration::from_millis(7));
    assert_eq!(view.input(), fingerprint);
    assert_eq!(
        raw.values(),
        &(0..20).map(|i| i as f32 * 0.25).collect::<Vec<_>>()
    );
    assert_eq!(admitted.statistics().validation_pairs, 10);
}
#[test]
fn matching_metadata_cannot_hide_different_worker_owned_pixels() {
    let (c, ticket, reference, candidate) = pair(2);
    assert_ne!(reference.input(), candidate.input());
    let mut entered = false;
    let result = Admission::prepare(
        &c,
        &ticket,
        ticket.key(),
        &reference,
        candidate,
        limits(),
        || {
            entered = true;
            false
        },
    );
    assert!(matches!(result, Err(Error::Pixels)));
    assert!(!entered);
}
#[test]
fn retained_alignment_is_rejected_after_invalidation_or_by_foreign_controller() {
    let (mut c, ticket, reference, candidate) = pair(1);
    let admitted = Admission::prepare(
        &c,
        &ticket,
        ticket.key(),
        &reference,
        candidate,
        limits(),
        || false,
    )
    .unwrap()
    .unwrap();
    let (foreign, foreign_ticket, _, _) = pair(1);
    assert!(admitted
        .view(&foreign, foreign_ticket.key())
        .unwrap()
        .is_none());
    let mut changed = ticket.key().clone();
    changed.source.pts.ticks = 1;
    assert!(admitted.view(&c, &changed).unwrap().is_none());
    c.invalidate(Duration::from_millis(301)).unwrap();
    c.observe(choices(), Activity::Idle, Duration::from_millis(301))
        .unwrap();
    c.observe(choices(), Activity::Idle, Duration::from_millis(601))
        .unwrap();
    assert!(admitted.view(&c, ticket.key()).unwrap().is_none());
}
#[test]
fn stale_candidate_is_a_normal_miss_before_alignment_work() {
    let (mut c, ticket, reference, candidate) = pair(1);
    c.invalidate(Duration::from_millis(301)).unwrap();
    let mut entered = false;
    assert!(Admission::prepare(
        &c,
        &ticket,
        ticket.key(),
        &reference,
        candidate,
        limits(),
        || {
            entered = true;
            false
        }
    )
    .unwrap()
    .is_none());
    assert!(!entered);
}
#[test]
fn original_cancellation_and_unqualified_scale_refusals_are_preserved() {
    let (c, ticket, reference, candidate) = pair(1);
    assert!(matches!(
        Admission::prepare(
            &c,
            &ticket,
            ticket.key(),
            &reference,
            candidate,
            limits(),
            || true
        ),
        Err(Error::Alignment(Rejection::Canceled))
    ));
    let (c, ticket, reference, candidate) = pair(1);
    let mut bound = limits();
    bound.scale_bounds = [0.5, 1.5];
    assert!(matches!(
        Admission::prepare(
            &c,
            &ticket,
            ticket.key(),
            &reference,
            candidate,
            bound,
            || false
        ),
        Err(Error::Alignment(Rejection::Scale { actual: 2., .. }))
    ));
}
