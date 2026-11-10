use super::*;

fn measure(inverse: bool, unit: f64, axis: DepthAxis) -> DepthMeasure {
    if inverse {
        DepthMeasure::InverseMetric {
            inverse_meters_per_unit: unit,
            axis,
        }
    } else {
        DepthMeasure::Metric {
            meters_per_unit: unit,
            axis,
        }
    }
}

#[test]
fn physical_comparison_converts_declared_units_without_fitting_bias() {
    for inverse in [false, true] {
        let raw = resource((1..=20).map(|i| i as f32).collect(), None, None);
        let reference = changed(&raw, |d| {
            d.measure = measure(inverse, 1., DepthAxis::CameraZ)
        });
        let candidate = changed(&raw, |d| {
            d.measure = measure(inverse, 0.5, DepthAxis::CameraZ);
            d.scale = 2.;
            d.model.processing = "another-physical-model".into();
        });
        let original = candidate.descriptor().clone();
        let proof = Alignment::verify_metric(&reference, &candidate, limits(), || false).unwrap();
        assert_eq!(proof.coefficients(), [0.5, 0.]);
        let mapped = proof.bind(&reference, &candidate).unwrap();
        assert_eq!(mapped.sample_measure(), reference.descriptor().measure);
        assert_ne!(
            mapped.sample_measure(),
            mapped.resource().descriptor().measure
        );
        for i in 0..20 {
            assert_eq!(mapped.sample(i).unwrap(), reference.sample(i));
            assert_eq!(
                reference.camera_z_meters(i, None).unwrap(),
                candidate.camera_z_meters(i, None).unwrap()
            );
        }
        assert_eq!(candidate.descriptor(), &original);
        // Both a multiplicative metric bias and an additive one must fail.
        // The relative fitter could conceal them, so it must remain unavailable.
        for (scale, offset) in [(2.2, 0.), (2., 1.)] {
            let biased = changed(&candidate, |d| {
                d.scale = scale;
                d.offset = offset;
            });
            assert!(matches!(
                Alignment::verify_metric(&reference, &biased, limits(), || false),
                Err(Rejection::Residual { .. })
            ));
            assert!(matches!(
                Alignment::fit(&reference, &biased, limits(), || false),
                Err(Rejection::Incompatible(_))
            ));
        }
    }
}

#[test]
fn physical_comparison_rejects_axis_form_relative_and_unqualified_unit_changes() {
    let raw = resource((1..=20).map(|i| i as f32).collect(), None, None);
    let reference = changed(&raw, |d| d.measure = measure(false, 1., DepthAxis::CameraZ));
    for m in [
        measure(false, 1., DepthAxis::RayDistance),
        measure(true, 1., DepthAxis::CameraZ),
        DepthMeasure::Relative {
            near: NearDirection::Decreasing,
        },
    ] {
        let candidate = changed(&raw, |d| d.measure = m);
        assert!(matches!(
            Alignment::verify_metric(&reference, &candidate, limits(), || false),
            Err(Rejection::Incompatible(_))
        ));
    }
    assert!(matches!(
        Alignment::verify_metric(&raw, &raw, limits(), || false),
        Err(Rejection::Incompatible(_))
    ));
    let candidate = changed(&reference, |d| {
        d.measure = measure(false, 0.5, DepthAxis::CameraZ);
        d.scale = 2.;
    });
    let mut bounds = limits();
    bounds.scale_bounds = [0.75, 1.5];
    assert!(matches!(
        Alignment::verify_metric(&reference, &candidate, bounds, || false),
        Err(Rejection::Scale { actual: 0.5, .. })
    ));
}

#[test]
fn physical_comparison_preserves_coverage_holdouts_cancellation_and_instance_identity() {
    let raw = resource((1..=20).map(|i| i as f32).collect(), None, None);
    let reference = changed(&raw, |d| d.measure = measure(false, 1., DepthAxis::CameraZ));
    let candidate = changed(&reference, |_| {});
    let proof = Alignment::verify_metric(&reference, &candidate, limits(), || false).unwrap();
    assert!(matches!(
        proof.bind(&reference, &changed(&candidate, |_| {})),
        Err(Rejection::Stale)
    ));
    assert!(matches!(
        Alignment::verify_metric(&reference, &candidate, limits(), || true),
        Err(Rejection::Canceled)
    ));
    let mut confidence = limits();
    confidence.minimum_confidence = Some(0.5);
    assert!(matches!(
        Alignment::verify_metric(&reference, &candidate, confidence, || false),
        Err(Rejection::Incompatible(_))
    ));
    let mut damaged = candidate.values().to_vec();
    damaged[19] += 3.;
    let damaged = DepthResource::new(candidate.descriptor().clone(), damaged, None, None).unwrap();
    assert!(matches!(
        Alignment::verify_metric(&reference, &damaged, limits(), || false),
        Err(Rejection::Residual {
            population: "held-out",
            ..
        })
    ));
    let mut mask = vec![1; 20];
    mask[..3].fill(0);
    let sparse = DepthResource::new(
        candidate.descriptor().clone(),
        candidate.values().to_vec(),
        Some(mask),
        None,
    )
    .unwrap();
    assert!(matches!(
        Alignment::verify_metric(&reference, &sparse, limits(), || false),
        Err(Rejection::Coverage { .. })
    ));
}
