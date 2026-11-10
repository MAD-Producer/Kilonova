use super::*;
use motion_model::depth::{
    DepthAxis, DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource,
    DepthSource, DepthTime,
};

fn estimate(candidate: bool, bias: f64) -> Estimate {
    Estimate {
        resource: DepthResource::new(
            DepthDescriptor {
                size: [20, 1],
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
                    id: "physical-restore-control".into(),
                    revision: "r1".into(),
                    sha256: [if candidate { 2 } else { 1 }; 32],
                    processing: "authored-scalar-grid".into(),
                },
                measure: DepthMeasure::Metric {
                    meters_per_unit: if candidate { 0.5 } else { 1. },
                    axis: DepthAxis::CameraZ,
                },
                scale: if candidate { 2. } else { 1. },
                offset: bias,
                valid_range: None,
            },
            (1..=20).map(|i| i as f32).collect(),
            None,
            None,
        )
        .unwrap(),
        provider: "analytic-physical-restore-control".into(),
        costs: Default::default(),
    }
}

fn key(estimate: &Estimate) -> Key {
    let d = estimate.resource.descriptor();
    Key {
        source: d.source.clone(),
        geometry: d.geometry.clone(),
        model: d.model.clone(),
    }
}

#[test]
fn physical_restore_rechecks_declared_units_and_rejects_bias_and_old_policy() {
    let reference = Arc::new(estimate(false, 0.));
    let candidate = estimate(true, 0.);
    let key = key(&candidate);
    let spec = Spec {
        alignment_version: crate::depth::alignment::VERSION,
        transition_version: crate::depth::transition::VERSION,
        generation: 1,
        phase: Phase::Settled,
        choices: Choices {
            interactive: self::key(&reference),
            settled: key.clone(),
        },
        policy: Policy {
            idle_delay: std::time::Duration::from_millis(10),
            minimum_interactive_residence: std::time::Duration::from_millis(10),
        },
        limits: Limits {
            minimum_fit_pairs: 3,
            minimum_validation_pairs: 3,
            minimum_pair_fraction: 1.,
            minimum_confidence: None,
            minimum_reference_stddev: 1e-8,
            scale_bounds: [0.1, 10.],
            maximum_normalized_rms: 1e-8,
            maximum_normalized_error: 1e-8,
        },
        coefficients: [0.5, 0.],
    };
    let restored = Frozen::restore(
        reference.clone(),
        &candidate,
        spec.clone(),
        &key,
        1,
        2,
        &|| Ok(()),
    )
    .unwrap();
    assert_eq!(restored.proof.coefficients(), [0.5, 0.]);
    let mapped = restored
        .proof
        .bind(&reference.resource, &candidate.resource)
        .unwrap();
    assert_eq!(
        mapped.sample_measure(),
        reference.resource.descriptor().measure
    );
    for i in 0..20 {
        assert_eq!(mapped.sample(i).unwrap(), reference.resource.sample(i));
    }
    let biased = estimate(true, 2.);
    let error = Frozen::restore(
        reference.clone(),
        &biased,
        spec.clone(),
        &key,
        1,
        2,
        &|| Ok(()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("residual"), "{error}");
    let mut old = spec;
    old.alignment_version = 1;
    assert!(
        Frozen::restore(reference, &candidate, old, &key, 1, 2, &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("policy version")
    );
}
