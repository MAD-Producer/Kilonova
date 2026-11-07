use super::*;

fn candidate(reference: &DepthResource) -> DepthResource {
    let mut descriptor = reference.descriptor().clone();
    descriptor.model.processing = "aligned-candidate".into();
    DepthResource::new(
        descriptor,
        reference.values().iter().map(|v| (*v - 3.) / 2.).collect(),
        None,
        None,
    )
    .unwrap()
}
fn mapped(
    saved: &SavedDepthCalibration,
    reference: &DepthResource,
    candidate: &DepthResource,
) -> RelativeDepthCalibration {
    let original = saved.rebind(reference, input()).unwrap();
    RelativeDepthCalibration::new(
        candidate.descriptor().clone(),
        original.anchors().map(|mut a| {
            a.value = (a.value - 3.) / 2.;
            a
        }),
        original.curve(),
        original.extrapolation(),
    )
    .unwrap()
}

#[test]
fn aligned_geometry_rechecks_original_saved_calibration_source_and_metric_observations() {
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let reference = field(1);
    let candidate = candidate(&reference);
    let saved = p.layers[0].depth_calibration.as_ref().unwrap();
    let binding = mapped(saved, &reference, &candidate);
    let current = reference.descriptor().source.clone();
    let mut scene = sampled(&p);
    assert!(
        scene
            .attach_calibrated_depth(1, &current, &candidate, input())
            .is_err(),
        "raw rebind must still reject a different model"
    );
    let mut stale = current.clone();
    stale.revision += 1;
    assert!(scene
        .attach_aligned_calibrated_depth(1, &stale, &reference, &candidate, input(), &binding, None)
        .is_err());
    let mut changed = input();
    changed.rgb_sha256 = [23; 32];
    assert!(scene
        .attach_aligned_calibrated_depth(
            1, &current, &reference, &candidate, changed, &binding, None
        )
        .is_err());
    let mut anchors = *binding.anchors();
    anchors[0].camera_z_meters = 14.;
    let wrong = RelativeDepthCalibration::new(
        candidate.descriptor().clone(),
        anchors,
        binding.curve(),
        binding.extrapolation(),
    )
    .unwrap();
    assert!(scene
        .attach_aligned_calibrated_depth(1, &current, &reference, &candidate, input(), &wrong, None)
        .unwrap_err()
        .to_string()
        .contains("metric observations"));
    scene
        .attach_aligned_calibrated_depth(
            1,
            &current,
            &reference,
            &candidate,
            input(),
            &binding,
            None,
        )
        .unwrap();
    assert_eq!(binding.descriptor().model, candidate.descriptor().model);
    assert_eq!(p.layers[0].depth_calibration.as_ref().unwrap(), saved);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn aligned_candidate_preserves_actual_rendered_geometry_and_owned_image_after_source_replacement() {
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut baseline = sampled(&p);
    attach(&mut baseline);
    let expected = renderer.capture(&baseline, &target).unwrap().0;
    let reference = field(1);
    let candidate = candidate(&reference);
    let binding = mapped(
        p.layers[0].depth_calibration.as_ref().unwrap(),
        &reference,
        &candidate,
    );
    let owned =
        motion_render::DepthImage::new([2, 1], vec![255, 0, 0, 255, 255, 0, 0, 255]).unwrap();
    image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 255, 0, 255]))
        .save(root.path().join("assets/red.png"))
        .unwrap();
    renderer.clear_assets();
    let mut aligned = sampled(&p);
    aligned
        .attach_aligned_calibrated_depth(
            1,
            &reference.descriptor().source,
            &reference,
            &candidate,
            input(),
            &binding,
            Some(owned),
        )
        .unwrap();
    renderer
        .configure_assets_with_scene(&p, root.path(), &aligned)
        .unwrap();
    assert!(renderer
        .prepare_scene_assets(
            &aligned,
            motion_render::image_resources::Resolution::Full,
            false
        )
        .unwrap());
    assert_eq!(renderer.capture(&aligned, &target).unwrap().0, expected, "actual depth geometry, occlusion and paired color must survive a cross-model affine change");
    retain_fixture("aligned-candidate-preserves-geometry.png", &expected);
}
