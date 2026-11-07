use super::*;
use crate::depth::{
    alignment::{Alignment, Limits},
    transition::{Choices, Phase, Policy},
};
use crate::depth_snapshot::alignment::{Frozen, Spec};
use motion_model::depth::*;

fn fixture() -> DepthSnapshot {
    let mut snapshot = super::fixture();
    let original = &snapshot.entries[0];
    let mut descriptor = original.estimate.resource.descriptor().clone();
    descriptor.size = [3, 3];
    descriptor.geometry.source_size = [3, 3];
    descriptor.geometry.crop = [0, 0, 3, 3];
    let reference = Arc::new(Estimate {
        resource: DepthResource::new(
            descriptor.clone(),
            (1..=9).map(|i| i as f32 * 10.).collect(),
            None,
            None,
        )
        .unwrap(),
        provider: "owned_interactive".into(),
        costs: Default::default(),
    });
    let interactive = Key {
        source: descriptor.source.clone(),
        geometry: descriptor.geometry.clone(),
        model: descriptor.model.clone(),
    };
    descriptor.model.processing = "owned-settled".into();
    let key = Key {
        source: descriptor.source.clone(),
        geometry: descriptor.geometry.clone(),
        model: descriptor.model.clone(),
    };
    let candidate = Arc::new(Estimate {
        resource: DepthResource::new(
            descriptor,
            (1..=9).map(|i| (i as f32 * 10. - 3.) / 2.).collect(),
            None,
            None,
        )
        .unwrap(),
        provider: "owned_settled".into(),
        costs: Default::default(),
    });
    let limits = Limits {
        minimum_fit_pairs: 3,
        minimum_validation_pairs: 3,
        minimum_pair_fraction: 1.,
        minimum_confidence: None,
        minimum_reference_stddev: 1e-8,
        scale_bounds: [0.1, 10.],
        maximum_normalized_rms: 1e-8,
        maximum_normalized_error: 1e-8,
    };
    let proof = Alignment::fit(&reference.resource, &candidate.resource, limits, || false).unwrap();
    let spec = Spec {
        alignment_version: crate::depth::alignment::VERSION,
        transition_version: crate::depth::transition::VERSION,
        generation: 2,
        phase: Phase::Settled,
        choices: Choices {
            interactive,
            settled: key.clone(),
        },
        policy: Policy {
            idle_delay: std::time::Duration::from_millis(100),
            minimum_interactive_residence: std::time::Duration::from_millis(300),
        },
        limits,
        coefficients: proof.coefficients(),
    };
    let frozen = Frozen::from_live(reference.clone(), &candidate, &proof, spec, &key).unwrap();
    for entry in &mut snapshot.entries {
        entry.asset.width = 3;
        entry.asset.height = 3;
        entry.key = key.clone();
        entry.input.rgb_size = [3, 3];
        entry.calibration = SavedDepthCalibration::new(
            &reference.resource,
            entry.input,
            [
                DepthAnchor {
                    value: 10.,
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: 90.,
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
        entry.estimate = candidate.clone();
        entry.alignment = Some(frozen.clone());
    }
    snapshot
}

fn encoded(snapshot: &DepthSnapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes, &|| Ok(())).unwrap();
    bytes
}
fn mutate(bytes: &[u8], change: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut metadata = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
    change(&mut metadata);
    let metadata = serde_json::to_vec(&metadata).unwrap();
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    out.extend_from_slice(&metadata);
    out.extend_from_slice(&bytes[12 + size..]);
    out
}

#[test]
fn aligned_wire_requalifies_actual_instances_and_preserves_original_calibration_and_sharing() {
    let original = fixture();
    let bytes = encoded(&original);
    let restored = DepthSnapshot::read_from(&mut &bytes[..], &|| Ok(())).unwrap();
    assert_eq!(restored.layer_count(), 2);
    assert!(Arc::ptr_eq(
        &restored.entries[0].estimate,
        &restored.entries[1].estimate
    ));
    let entry = &restored.entries[0];
    let frozen = entry.alignment.as_ref().unwrap();
    assert!(Arc::ptr_eq(
        frozen.reference(),
        restored.entries[1].alignment.as_ref().unwrap().reference()
    ));
    assert_ne!(
        entry.estimate.resource.cache_identity(),
        original.entries[0].estimate.resource.cache_identity()
    );
    assert_ne!(
        frozen.reference().resource.cache_identity(),
        original.entries[0]
            .alignment
            .as_ref()
            .unwrap()
            .reference()
            .resource
            .cache_identity()
    );
    assert_eq!(entry.calibration, original.entries[0].calibration);
    assert!(entry
        .calibration
        .rebind(&entry.estimate.resource, entry.input)
        .is_err());
    let binding = frozen
        .calibration(&entry.estimate, &entry.key, &entry.calibration, entry.input)
        .unwrap();
    assert_eq!(binding.anchors().map(|a| a.value), [3.5, 43.5]);
    let bound = binding.bind(&entry.estimate.resource).unwrap();
    for i in 0..9 {
        assert!((bound.camera_z_meters(i).unwrap().unwrap() - (8. - i as f64 / 2.)).abs() < 1e-10);
    }
    assert_eq!(encoded(&restored), bytes);
}

#[test]
fn aligned_wire_refuses_policy_metadata_and_plane_corruption_before_admission() {
    let bytes = encoded(&fixture());
    for (field, value) in [
        ("alignment_version", serde_json::json!(99)),
        ("transition_version", serde_json::json!(99)),
        ("generation", serde_json::json!(0)),
        ("coefficients", serde_json::json!([3., 5.])),
        ("phase", serde_json::json!("interactive")),
    ] {
        let bad = mutate(&bytes, |m| {
            m["entries"][0]["alignment"]["spec"][field] = value
        });
        assert!(
            DepthSnapshot::read_from(&mut &bad[..], &|| Ok(())).is_err(),
            "field={field}"
        );
    }
    let bad = mutate(&bytes, |m| {
        m["entries"][0]["input"]["rgb_sha256"][0] = serde_json::json!(42)
    });
    assert!(DepthSnapshot::read_from(&mut &bad[..], &|| Ok(()))
        .unwrap_err()
        .to_string()
        .contains("metadata digest"));
    let bad = mutate(&bytes, |m| {
        m["resources"][1]["provider"] = serde_json::json!("different_provider")
    });
    assert!(DepthSnapshot::read_from(&mut &bad[..], &|| Ok(()))
        .unwrap_err()
        .to_string()
        .contains("content digest"));
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    for offset in [0, 9 * 4] {
        let mut bad = bytes.clone();
        bad[12 + size + offset..][..4].copy_from_slice(&11_f32.to_le_bytes());
        assert!(DepthSnapshot::read_from(&mut &bad[..], &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("content digest"));
    }
}

#[test]
fn aligned_wire_counts_reference_and_candidate_memory_before_reading_planes() {
    let bytes = encoded(&fixture());
    let bad = mutate(&bytes, |m| {
        for resource in m["resources"].as_array_mut().unwrap() {
            resource["descriptor"]["size"] = serde_json::json!([4096, 4096]);
            resource["validity"] = serde_json::json!(true);
            resource["confidence"] = serde_json::json!(true);
        }
    });
    let size = u32::from_le_bytes(bad[8..12].try_into().unwrap()) as usize;
    // No scalar payload exists here: 2 x 144 MiB must be refused on metadata.
    let cause = DepthSnapshot::read_from(&mut &bad[..12 + size], &|| Ok(())).unwrap_err();
    assert!(
        cause.to_string().contains("retained planes exceed 192 MiB"),
        "{cause}"
    );
}

#[test]
fn aligned_restore_preserves_cancellation_inside_fit_and_checks_reference_identity() {
    let snapshot = fixture();
    let entry = &snapshot.entries[0];
    let frozen = entry.alignment.as_ref().unwrap();
    let calls = std::cell::Cell::new(0);
    let error = Frozen::restore(
        frozen.reference().clone(),
        &entry.estimate,
        frozen.spec().clone(),
        &entry.key,
        entry.request,
        entry.object,
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err("original fit cancellation".into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "original fit cancellation");
    assert_eq!(calls.get(), 2);
    let mut descriptor = frozen.reference().resource.descriptor().clone();
    descriptor.model.processing = "foreign-reference".into();
    let reference = Arc::new(Estimate {
        resource: DepthResource::new(
            descriptor,
            frozen.reference().resource.values().to_vec(),
            None,
            None,
        )
        .unwrap(),
        provider: "owned_interactive".into(),
        costs: Default::default(),
    });
    let error = Frozen::restore(
        reference,
        &entry.estimate,
        frozen.spec().clone(),
        &entry.key,
        entry.request,
        entry.object,
        &|| Ok(()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("reference differs"), "{error}");
}
