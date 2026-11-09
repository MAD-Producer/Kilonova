use super::*;

fn physical(reference: &DepthResource, candidate: bool) -> DepthResource {
    let mut d = reference.descriptor().clone();
    if candidate {
        d.model.processing = "physical-candidate".into();
    }
    d.measure = DepthMeasure::Metric {
        meters_per_unit: if candidate { 0.5 } else { 0.25 },
        axis: DepthAxis::CameraZ,
    };
    DepthResource::new(
        d,
        if candidate {
            vec![16., 24.]
        } else {
            vec![32., 48.]
        },
        None,
        None,
    )
    .unwrap()
}
#[test]
fn physical_alignment_rechecks_saved_reference_source_units_grid_and_image_before_attachment() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    let relative = p.clone();
    let reference = physical(&field(1), false);
    let candidate = physical(&reference, true);
    let saved = p.layers[0].depth_calibration.as_ref().unwrap();
    let placement = SavedDepthCalibration::physical(&reference, input(), saved.pixels_per_metre())
        .unwrap()
        .with_reference(saved.reference().unwrap().clone())
        .unwrap();
    p.layers[0].depth_calibration = Some(placement);
    let current = &reference.descriptor().source;
    let mut scene = sampled(&p);
    assert!(scene
        .attach_calibrated_depth(1, current, &candidate, input())
        .is_err());
    let mut stale = current.clone();
    stale.revision += 1;
    assert!(scene
        .attach_aligned_physical_depth(1, &stale, &reference, &candidate, input(), None)
        .is_err());
    let mut changed = input();
    changed.rgb_sha256[0] ^= 1;
    assert!(scene
        .attach_aligned_physical_depth(1, current, &reference, &candidate, changed, None)
        .is_err());
    for mutation in 0..5 {
        let mut d = candidate.descriptor().clone();
        match mutation {
            0 => {
                d.measure = DepthMeasure::InverseMetric {
                    inverse_meters_per_unit: 1.,
                    axis: DepthAxis::CameraZ,
                }
            }
            1 => {
                d.measure = DepthMeasure::Metric {
                    meters_per_unit: 1.,
                    axis: DepthAxis::RayDistance,
                }
            }
            2 => {
                d.measure = DepthMeasure::Relative {
                    near: NearDirection::Increasing,
                }
            }
            3 => d.size = [1, 2],
            _ => d.source.asset += 1,
        }
        let wrong = DepthResource::new(d, vec![16., 24.], None, None).unwrap();
        assert!(
            scene
                .attach_aligned_physical_depth(1, current, &reference, &wrong, input(), None)
                .is_err(),
            "mutation={mutation}"
        );
    }
    let wrong_image = motion_render::DepthImage::new([1, 1], vec![255; 4]).unwrap();
    assert!(scene
        .attach_aligned_physical_depth(
            1,
            current,
            &reference,
            &candidate,
            input(),
            Some(wrong_image)
        )
        .is_err());
    assert!(sampled(&relative)
        .attach_aligned_physical_depth(1, current, &reference, &candidate, input(), None)
        .is_err());
    scene
        .attach_aligned_physical_depth(1, current, &reference, &candidate, input(), None)
        .unwrap();
    assert_eq!(
        candidate.descriptor().model.processing,
        "physical-candidate"
    );
    assert_eq!(candidate.camera_z_meters(0, None).unwrap(), Some(8.));
    assert_eq!(candidate.camera_z_meters(1, None).unwrap(), Some(12.));
}
