#![cfg(feature = "ai-depth")]

use motion_host::depth::alignment::{Alignment, Limits, Rejection};
#[path = "depth_alignment/physical.rs"]
mod physical;
use motion_model::depth::{
    DepthAxis, DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource,
    DepthSource, DepthTime, NearDirection,
};

fn limits() -> Limits {
    Limits {
        minimum_fit_pairs: 3,
        minimum_validation_pairs: 3,
        minimum_pair_fraction: 0.9,
        minimum_confidence: None,
        minimum_reference_stddev: 1e-8,
        scale_bounds: [0.1, 10.],
        maximum_normalized_rms: 1e-8,
        maximum_normalized_error: 1e-8,
    }
}

fn resource(
    values: Vec<f32>,
    mask: Option<Vec<u8>>,
    confidence: Option<Vec<f32>>,
) -> DepthResource {
    let pixels = values.len() as u32;
    let size = if pixels > 4096 {
        [128, pixels / 128]
    } else {
        [pixels, 1]
    };
    DepthResource::new(
        DepthDescriptor {
            size,
            source: DepthSource {
                asset: 75,
                revision: 3,
                pts: DepthTime {
                    ticks: 3,
                    timescale: 24,
                },
            },
            geometry: DepthGeometry {
                source_size: size,
                crop: [0, 0, size[0], size[1]],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "alignment-control".into(),
                revision: "r1".into(),
                sha256: [17; 32],
                processing: "explicit-scalar-grid-v1".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        values,
        mask,
        confidence,
    )
    .unwrap()
}

fn changed(resource: &DepthResource, edit: impl FnOnce(&mut DepthDescriptor)) -> DepthResource {
    let mut descriptor = resource.descriptor().clone();
    edit(&mut descriptor);
    DepthResource::new(
        descriptor,
        resource.values().to_vec(),
        resource.validity().map(<[u8]>::to_vec),
        resource.confidence().map(<[f32]>::to_vec),
    )
    .unwrap()
}

fn affine_pair(pixels: usize) -> (DepthResource, DepthResource) {
    let candidate: Vec<_> = (1..=pixels).map(|i| i as f32 * 0.25).collect();
    let reference = candidate.iter().map(|x| 2. * x + 3.).collect();
    (
        resource(reference, None, None),
        resource(candidate, None, None),
    )
}

#[test]
fn recovers_known_affine_map_without_rewriting_raw_resources() {
    let (reference, candidate) = affine_pair(20);
    let raw = candidate.values().to_vec();
    let descriptor = candidate.descriptor().clone();
    let proof = Alignment::fit(&reference, &candidate, limits(), || false).unwrap();
    let [scale, offset] = proof.coefficients();
    assert!((scale - 2.).abs() < 1e-12 && (offset - 3.).abs() < 1e-12);
    let aligned = proof.bind(&reference, &candidate).unwrap();
    for index in 0..20 {
        assert!(
            (aligned.sample(index).unwrap().unwrap() - reference.sample(index).unwrap()).abs()
                < 1e-12
        );
    }
    assert_eq!(aligned.sample(20).unwrap(), None);
    assert_eq!(candidate.values(), raw);
    assert_eq!(candidate.descriptor(), &descriptor);
    let stats = proof.statistics();
    assert_eq!(
        (stats.fit_pairs, stats.validation_pairs, stats.skipped_pairs),
        (10, 10, 0)
    );
}

#[test]
fn fits_decoded_values_and_accepts_equivalent_rational_pts() {
    let (_, raw) = affine_pair(20);
    let reference = changed(&raw, |d| {
        d.scale = 2.;
        d.offset = 3.;
    });
    let candidate = changed(&raw, |d| {
        d.scale = 0.5;
        d.offset = 1.;
        d.source.pts = DepthTime {
            ticks: 1,
            timescale: 8,
        };
        d.model.processing = "different-explicit-profile-v1".into();
        d.model.sha256 = [29; 32];
    });
    let proof = Alignment::fit(&reference, &candidate, limits(), || false).unwrap();
    let [scale, offset] = proof.coefficients();
    assert!((scale - 4.).abs() < 1e-12 && (offset + 1.).abs() < 1e-12);
    let bound = proof.bind(&reference, &candidate).unwrap();
    assert!((bound.sample(19).unwrap().unwrap() - reference.sample(19).unwrap()).abs() < 1e-12);
}

#[test]
fn rejects_a_different_source_revision_pts_crop_orientation_or_grid() {
    let (reference, candidate) = affine_pair(20);
    let edits: [fn(&mut DepthDescriptor); 7] = [
        |d| d.source.asset += 1,
        |d| d.source.revision += 1,
        |d| d.source.pts.ticks += 1,
        |d| d.geometry.crop[2] -= 1,
        |d| d.geometry.quarter_turns = 1,
        |d| d.geometry.mirror_x = true,
        |d| d.size = [10, 2],
    ];
    for edit in edits {
        let other = changed(&candidate, edit);
        assert!(matches!(
            Alignment::fit(&reference, &other, limits(), || false),
            Err(Rejection::Incompatible(_))
        ));
    }
}

#[test]
fn never_relabels_metric_or_opposite_near_direction_as_relative_alignment() {
    let (reference, candidate) = affine_pair(20);
    for measure in [
        DepthMeasure::Relative {
            near: NearDirection::Decreasing,
        },
        DepthMeasure::Metric {
            meters_per_unit: 1.,
            axis: DepthAxis::CameraZ,
        },
        DepthMeasure::InverseMetric {
            inverse_meters_per_unit: 1.,
            axis: DepthAxis::CameraZ,
        },
    ] {
        let other = changed(&candidate, |d| d.measure = measure);
        assert!(matches!(
            Alignment::fit(&reference, &other, limits(), || false),
            Err(Rejection::Incompatible(_))
        ));
    }
}

#[test]
fn constant_populations_and_reversed_order_do_not_manufacture_a_map() {
    let (reference, candidate) = affine_pair(20);
    let constant = resource(vec![4.; 20], None, None);
    assert!(matches!(
        Alignment::fit(&reference, &constant, limits(), || false),
        Err(Rejection::Degenerate(_))
    ));
    assert!(matches!(
        Alignment::fit(&constant, &candidate, limits(), || false),
        Err(Rejection::Degenerate(_))
    ));
    let reversed = resource(
        candidate.values().iter().rev().copied().collect(),
        None,
        None,
    );
    assert!(
        matches!(Alignment::fit(&reference, &reversed, limits(), || false), Err(Rejection::Scale { actual, .. }) if actual < 0.)
    );
}

#[test]
fn rejects_unqualified_scale_even_for_a_perfect_affine_fit() {
    let (reference, candidate) = affine_pair(20);
    let mut gate = limits();
    gate.scale_bounds = [0.5, 1.5];
    assert!(
        matches!(Alignment::fit(&reference, &candidate, gate, || false), Err(Rejection::Scale { actual, .. }) if (actual - 2.).abs() < 1e-12)
    );
}

#[test]
fn checks_all_heldout_pixels_beyond_the_bounded_fit_set() {
    let (reference, candidate) = affine_pair(32768);
    let proof = Alignment::fit(&reference, &candidate, limits(), || false).unwrap();
    assert_eq!(proof.statistics().fit_pairs, 4096);
    assert_eq!(proof.statistics().validation_pairs, 28672);
    let mut values = candidate.values().to_vec();
    values[32767] += 50.;
    let corrupted = resource(values, None, None);
    assert!(matches!(
        Alignment::fit(&reference, &corrupted, limits(), || false),
        Err(Rejection::Residual {
            population: "held-out",
            ..
        })
    ));
}

#[test]
fn also_checks_fit_residuals_instead_of_only_the_holdout_score() {
    let (reference, candidate) = affine_pair(20);
    let mut values = candidate.values().to_vec();
    values[0] += 0.25;
    let corrupted = resource(values, None, None);
    let mut gate = limits();
    gate.scale_bounds = [1e-8, 1e8];
    assert!(matches!(
        Alignment::fit(&reference, &corrupted, gate, || false),
        Err(Rejection::Residual {
            population: "fit",
            ..
        })
    ));
}

#[test]
fn confidence_absence_is_not_invented_as_confidence_one() {
    let (reference, candidate) = affine_pair(20);
    let mut gate = limits();
    gate.minimum_confidence = Some(0.8);
    assert!(matches!(
        Alignment::fit(&reference, &candidate, gate, || false),
        Err(Rejection::Incompatible(_))
    ));
    let reference = resource(reference.values().to_vec(), None, Some(vec![1.; 20]));
    let mut confidence = vec![1.; 20];
    confidence[0] = 0.7;
    let candidate = resource(candidate.values().to_vec(), None, Some(confidence));
    let proof = Alignment::fit(&reference, &candidate, gate, || false).unwrap();
    assert_eq!(
        (
            proof.statistics().fit_pairs,
            proof.statistics().validation_pairs,
            proof.statistics().skipped_pairs
        ),
        (9, 10, 1)
    );
}

#[test]
fn preserves_invalid_samples_and_reports_insufficient_support() {
    let (reference, candidate) = affine_pair(20);
    let mut values = candidate.values().to_vec();
    values[0] = f32::NAN;
    let mut mask = vec![1; 20];
    mask[0] = 0;
    let candidate = resource(values, Some(mask), None);
    let proof = Alignment::fit(&reference, &candidate, limits(), || false).unwrap();
    assert_eq!(
        proof
            .bind(&reference, &candidate)
            .unwrap()
            .sample(0)
            .unwrap(),
        None
    );
    let sparse = resource(vec![1.; 20], Some(vec![0; 20]), None);
    assert!(matches!(
        Alignment::fit(&reference, &sparse, limits(), || false),
        Err(Rejection::InsufficientPairs {
            population: "fit",
            actual: 0,
            minimum: 3
        })
    ));
}

#[test]
fn a_few_valid_pairs_cannot_claim_support_for_the_entire_grid() {
    let (reference, candidate) = affine_pair(20);
    let mut mask = vec![0; 20];
    mask[..8].fill(1);
    let sparse = resource(candidate.values().to_vec(), Some(mask), None);
    assert!(matches!(
        Alignment::fit(&reference, &sparse, limits(), || false),
        Err(Rejection::Coverage {
            eligible: 8,
            pixels: 20,
            minimum
        }) if minimum == 0.9
    ));
}

#[test]
fn proof_rejects_replacement_instances_even_with_identical_metadata_and_values() {
    let (reference, candidate) = affine_pair(20);
    let proof = Alignment::fit(&reference, &candidate, limits(), || false).unwrap();
    let replacement = changed(&candidate, |_| {});
    assert!(matches!(
        proof.bind(&reference, &replacement),
        Err(Rejection::Stale)
    ));
    let replacement = changed(&reference, |_| {});
    assert!(matches!(
        proof.bind(&replacement, &candidate),
        Err(Rejection::Stale)
    ));
    assert!(proof.bind(&reference.clone(), &candidate.clone()).is_ok());
}

#[test]
fn cancellation_is_propagated_before_and_during_linear_validation() {
    let (reference, candidate) = affine_pair(32768);
    assert!(matches!(
        Alignment::fit(&reference, &candidate, limits(), || true),
        Err(Rejection::Canceled)
    ));
    let mut polls = 0;
    let result = Alignment::fit(&reference, &candidate, limits(), || {
        polls += 1;
        polls >= 18
    });
    assert!(matches!(result, Err(Rejection::Canceled)));
    assert_eq!(polls, 18);
}

#[test]
fn rejects_invalid_limits_instead_of_clamping_them() {
    let (reference, candidate) = affine_pair(20);
    let mut gates = vec![limits(); 6];
    gates[0].minimum_fit_pairs = 4097;
    gates[1].minimum_validation_pairs = 0;
    gates[2].minimum_confidence = Some(f32::NAN);
    gates[3].scale_bounds = [0., 1.];
    gates[4].maximum_normalized_rms = f64::INFINITY;
    gates[5].minimum_pair_fraction = f64::NAN;
    for gate in gates {
        assert!(matches!(
            Alignment::fit(&reference, &candidate, gate, || false),
            Err(Rejection::InvalidLimits(_))
        ));
    }
}
