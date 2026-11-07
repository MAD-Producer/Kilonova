#![cfg(feature = "ai-depth")]

use motion_host::depth::{
    transition::{Activity, Choices, Controller, Error, Phase, Policy},
    Key,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, DepthSource,
    DepthTime, NearDirection,
};
use std::time::Duration;

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}
fn controller() -> Controller {
    Controller::new(Policy {
        idle_delay: ms(100),
        minimum_interactive_residence: ms(300),
    })
    .unwrap()
}
fn choices(ticks: i64) -> Choices {
    let key = Key {
        source: DepthSource {
            asset: 75,
            revision: 1,
            pts: DepthTime {
                ticks,
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
            id: "explicit-synthetic-choice".into(),
            revision: "r1".into(),
            sha256: [17; 32],
            processing: "caller-qualified-interactive".into(),
        },
    };
    let mut settled = key.clone();
    settled.model.sha256 = [23; 32];
    settled.model.processing = "caller-qualified-settled".into();
    Choices {
        interactive: key,
        settled,
    }
}
fn resource(key: &Key) -> DepthResource {
    DepthResource::new(
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
        (0..20).map(|n| n as f32).collect(),
        None,
        None,
    )
    .unwrap()
}

#[test]
fn requires_both_idle_delay_and_interactive_residence_at_exact_boundaries() {
    let mut policy = controller();
    let input = choices(0);
    let initial = policy
        .observe(input.clone(), Activity::Interactive, ms(0))
        .unwrap()
        .unwrap();
    assert_eq!(initial.phase(), Phase::Interactive);
    assert!(policy
        .observe(input.clone(), Activity::Idle, ms(10))
        .unwrap()
        .is_none());
    // Idle delay has passed but residence has not.
    assert!(policy
        .observe(input.clone(), Activity::Idle, ms(110))
        .unwrap()
        .is_none());
    assert!(policy
        .observe(input.clone(), Activity::Idle, ms(299))
        .unwrap()
        .is_none());
    let settled = policy
        .observe(input.clone(), Activity::Idle, ms(300))
        .unwrap()
        .unwrap();
    assert_eq!(settled.phase(), Phase::Settled);
    assert!(settled.key().same_sample(&input.settled));
    assert!(settled.generation() > initial.generation());
    assert!(policy
        .observe(input, Activity::Idle, ms(1000))
        .unwrap()
        .is_none());
}

#[test]
fn interaction_downgrades_immediately_and_chatter_restarts_continuous_idle() {
    let mut policy = controller();
    let input = choices(0);
    policy
        .observe(input.clone(), Activity::Idle, ms(0))
        .unwrap();
    policy
        .observe(input.clone(), Activity::Idle, ms(300))
        .unwrap()
        .unwrap();
    let down = policy
        .observe(input.clone(), Activity::Interactive, ms(301))
        .unwrap()
        .unwrap();
    assert_eq!(down.phase(), Phase::Interactive);
    policy
        .observe(input.clone(), Activity::Idle, ms(400))
        .unwrap();
    policy
        .observe(input.clone(), Activity::Interactive, ms(499))
        .unwrap();
    policy
        .observe(input.clone(), Activity::Idle, ms(550))
        .unwrap();
    // Residence passed; continuous idle still has not.
    assert!(policy
        .observe(input.clone(), Activity::Idle, ms(649))
        .unwrap()
        .is_none());
    let up = policy
        .observe(input, Activity::Idle, ms(650))
        .unwrap()
        .unwrap();
    assert_eq!(up.phase(), Phase::Settled);
}

#[test]
fn advancing_frames_invalidates_old_work_without_resetting_continuous_idle() {
    let mut policy = controller();
    let first = policy
        .observe(choices(0), Activity::Idle, ms(0))
        .unwrap()
        .unwrap();
    let frame = policy
        .observe(choices(1), Activity::Idle, ms(200))
        .unwrap()
        .unwrap();
    assert!(frame.generation() > first.generation());
    let settled = policy
        .observe(choices(2), Activity::Idle, ms(300))
        .unwrap()
        .unwrap();
    assert_eq!(settled.phase(), Phase::Settled);
    let live = choices(2).settled;
    assert!(policy.is_current(&settled, &live, &resource(&live)));
    assert!(!policy.is_current(&first, &choices(0).interactive, &resource(first.key())));
}

#[test]
fn repeated_keys_do_not_admit_aba_foreign_or_explicitly_canceled_work() {
    let mut policy = controller();
    let input = choices(0);
    let first = policy
        .observe(input.clone(), Activity::Interactive, ms(0))
        .unwrap()
        .unwrap();
    policy
        .observe(choices(1), Activity::Interactive, ms(1))
        .unwrap();
    let again = policy
        .observe(input.clone(), Activity::Interactive, ms(2))
        .unwrap()
        .unwrap();
    let value = resource(&input.interactive);
    assert!(!policy.is_current(&first, &input.interactive, &value));
    assert!(policy.is_current(&again, &input.interactive, &value));
    let mut other = controller();
    let foreign = other
        .observe(input.clone(), Activity::Interactive, ms(0))
        .unwrap()
        .unwrap();
    assert!(!policy.is_current(&foreign, &input.interactive, &value));
    policy.invalidate(ms(3)).unwrap();
    assert!(policy.current().is_none());
    assert!(!policy.is_current(&again, &input.interactive, &value));
    let replacement = policy
        .observe(input.clone(), Activity::Idle, ms(4))
        .unwrap()
        .unwrap();
    assert_eq!(replacement.phase(), Phase::Interactive);
    assert!(replacement.generation() > again.generation());
    assert!(!policy.is_current(&again, &input.interactive, &value));
}

#[test]
fn installation_gate_requires_independent_live_key_and_exact_result_identity() {
    let mut policy = controller();
    let input = choices(3);
    let ticket = policy
        .observe(input.clone(), Activity::Interactive, ms(0))
        .unwrap()
        .unwrap();
    let correct = resource(&input.interactive);
    let mut equivalent = input.interactive.clone();
    equivalent.source.pts = DepthTime {
        ticks: 1,
        timescale: 8,
    };
    assert!(policy.is_current(&ticket, &equivalent, &correct));
    assert!(!policy.is_current(&ticket, &choices(4).interactive, &correct));
    assert!(!policy.is_current(&ticket, &input.interactive, &resource(&input.settled)));
    for field in 0..5 {
        let mut wrong = input.interactive.clone();
        match field {
            0 => wrong.source.revision += 1,
            1 => wrong.source.asset += 1,
            2 => wrong.model.sha256 = [99; 32],
            3 => wrong.model.processing = "different-profile".into(),
            4 => wrong.geometry.mirror_x = true,
            _ => unreachable!(),
        }
        assert!(!policy.is_current(&ticket, &input.interactive, &resource(&wrong)));
    }
}

#[test]
fn configuration_change_invalidates_even_when_selected_interactive_key_is_identical() {
    let mut policy = controller();
    let input = choices(0);
    let old = policy
        .observe(input.clone(), Activity::Idle, ms(0))
        .unwrap()
        .unwrap();
    let mut changed = input.clone();
    changed.settled.model.processing = "new-qualified-profile".into();
    let new = policy
        .observe(changed.clone(), Activity::Idle, ms(299))
        .unwrap()
        .unwrap();
    assert!(new.key().same_sample(old.key()));
    assert!(new.generation() > old.generation());
    assert!(!policy.is_current(&old, old.key(), &resource(old.key())));
    // New configuration restarts both timers.
    assert!(policy
        .observe(changed.clone(), Activity::Idle, ms(598))
        .unwrap()
        .is_none());
    assert_eq!(
        policy
            .observe(changed, Activity::Idle, ms(599))
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Settled
    );
}

#[test]
fn invalid_input_or_clock_does_not_mutate_valid_admission_state() {
    let mut policy = controller();
    let input = choices(0);
    let ticket = policy
        .observe(input.clone(), Activity::Interactive, ms(100))
        .unwrap()
        .unwrap();
    let mut different = input.clone();
    different.settled.source.pts.ticks = 1;
    assert!(matches!(
        policy.observe(different, Activity::Idle, ms(200)),
        Err(Error::DifferentSample)
    ));
    let mut invalid = input.clone();
    invalid.interactive.source.pts.timescale = 0;
    assert!(matches!(
        policy.observe(invalid, Activity::Idle, ms(200)),
        Err(Error::Choice(_))
    ));
    assert!(matches!(
        policy.observe(input.clone(), Activity::Idle, ms(99)),
        Err(Error::Clock)
    ));
    assert!(matches!(policy.invalidate(ms(99)), Err(Error::Clock)));
    assert!(policy.is_current(&ticket, &input.interactive, &resource(&input.interactive)));
    assert!(policy
        .observe(input, Activity::Interactive, ms(100))
        .unwrap()
        .is_none());
}

#[test]
fn never_relabels_processing_profiles_or_infers_quality_from_backend_names() {
    for idle in [Duration::ZERO, ms(1)] {
        assert!(Controller::new(Policy {
            idle_delay: idle,
            minimum_interactive_residence: Duration::ZERO
        })
        .is_err());
    }
    assert!(Controller::new(Policy {
        idle_delay: Duration::ZERO,
        minimum_interactive_residence: ms(1)
    })
    .is_err());
    let mut policy = controller();
    let mut input = choices(0);
    // Explicit caller choices may be the same model. Phase still changes and
    // invalidates the old request; no invented better model/provider is used.
    input.settled = input.interactive.clone();
    let first = policy
        .observe(input.clone(), Activity::Idle, ms(0))
        .unwrap()
        .unwrap();
    let next = policy
        .observe(input.clone(), Activity::Idle, ms(300))
        .unwrap()
        .unwrap();
    assert!(next.key().same_sample(first.key()));
    assert_eq!(next.phase(), Phase::Settled);
    assert!(!policy.is_current(&first, &input.settled, &resource(&input.settled)));
}

#[test]
fn successful_alignment_cannot_override_a_generation_changed_while_it_ran() {
    use motion_host::depth::alignment::{Alignment, Limits};
    let mut policy = controller();
    let input = choices(0);
    policy
        .observe(input.clone(), Activity::Idle, ms(0))
        .unwrap();
    let ticket = policy
        .observe(input.clone(), Activity::Idle, ms(300))
        .unwrap()
        .unwrap();
    let reference = resource(&input.interactive);
    let candidate = resource(&input.settled);
    assert!(policy.is_current(&ticket, &input.settled, &candidate));
    let proof = Alignment::fit(
        &reference,
        &candidate,
        Limits {
            minimum_fit_pairs: 3,
            minimum_validation_pairs: 3,
            minimum_pair_fraction: 1.,
            minimum_confidence: None,
            minimum_reference_stddev: 1e-8,
            scale_bounds: [0.5, 2.],
            maximum_normalized_rms: 1e-8,
            maximum_normalized_error: 1e-8,
        },
        || false,
    )
    .unwrap();
    // The same pair still has a valid proof, but the live composition moved on.
    // The independent post-work identity check must reject installation.
    let live = choices(1);
    policy
        .observe(live.clone(), Activity::Interactive, ms(301))
        .unwrap();
    assert!(proof.bind(&reference, &candidate).is_ok());
    assert!(!policy.is_current(&ticket, &live.settled, &candidate));
    // Passing the old public key does not bypass the private generation either.
    assert!(!policy.is_current(&ticket, &input.settled, &candidate));
}
