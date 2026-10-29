//! Actual calibrated image geometry in the ordinary composition renderer.
use glam::{DVec3, DVec4, Mat4};
use motion_core::{Asset, Content, Layer, Project};
use motion_model::depth::*;
use motion_render::{Renderer, Scene};

#[path = "depth_surface/video.rs"]
mod video;

#[path = "depth_surface/composition_boundaries.rs"]
mod composition_boundaries;

#[path = "depth_surface/composition_blend.rs"]
mod composition_blend;

#[path = "depth_surface/composition_effects.rs"]
mod composition_effects;

#[path = "depth_surface/calibrated_masks.rs"]
mod calibrated_masks;

#[path = "depth_surface/masked_video.rs"]
mod masked_video;

#[path = "depth_surface/calibrated_mattes.rs"]
mod calibrated_mattes;

#[path = "depth_surface/calibrated_matte_sources.rs"]
mod calibrated_matte_sources;

#[path = "depth_surface/ordinary_calibrated_mattes.rs"]
mod ordinary_calibrated_mattes;

#[path = "depth_surface/calibrated_materials.rs"]
mod calibrated_materials;

// Explicit local acceptance output; CI and ordinary tests perform no extra IO.
fn retain_fixture(name: &str, pixels: &[u8]) {
    if let Some(root) = std::env::var_os("MOTION_DEPTH_ACCEPTANCE_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        image::save_buffer(root.join(name), pixels, 96, 64, image::ColorType::Rgba8).unwrap();
    }
}

fn field(revision: u64) -> DepthResource {
    DepthResource::new(
        DepthDescriptor {
            size: [2, 1],
            source: DepthSource {
                asset: 7,
                revision,
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry: DepthGeometry {
                source_size: [2, 1],
                crop: [0, 0, 2, 1],
                quarter_turns: 0,
                mirror_x: false,
            },
            model: DepthModel {
                id: "owned-surface".into(),
                revision: "1".into(),
                sha256: [7; 32],
                processing: "owned".into(),
            },
            measure: DepthMeasure::Relative {
                near: NearDirection::Increasing,
            },
            scale: 1.,
            offset: 0.,
            valid_range: None,
        },
        vec![1., 0.],
        None,
        None,
    )
    .unwrap()
}
fn input() -> PreparedDepthInput {
    PreparedDepthInput {
        rgb_size: [2, 1],
        rgb_sha256: [17; 32],
    }
}
fn sampled(project: &Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn project(root: &std::path::Path) -> Project {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    image::save_buffer(
        root.join("assets/red.png"),
        &[255, 0, 0, 255, 255, 0, 0, 255],
        2,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut p = Project::new(96, 64, 30, 60).unwrap();
    p.background = [0.; 4];
    p.camera.created = true;
    p.camera.position.set_at(0, [48., 32., -100.]).unwrap();
    p.camera.target.set_at(0, [48., 32., 0.]).unwrap();
    p.camera.fov.set_at(0, 60.).unwrap();
    p.assets.push(Asset {
        id: 7,
        path: "assets/red.png".into(),
        width: 2,
        height: 1,
    });
    let mut image = Layer::solid(
        1,
        "Owned calibrated source",
        [64., 40.],
        [48., 32., 0.],
        [1.; 4],
    );
    image.content = Content::Image { asset: 7 };
    image.three_d = true;
    let mut blue = Layer::solid(
        2,
        "Actual spatial plane",
        [64., 40.],
        [48., 32., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    p.layers = vec![image, blue];
    let reference = sampled(&p);
    let image = reference.layers.iter().find(|l| l.id == 1).unwrap();
    p.layers[0].depth_calibration = Some(
        SavedDepthCalibration::new(
            &field(1),
            input(),
            [
                DepthAnchor {
                    value: 0.,
                    camera_z_meters: 12.,
                },
                DepthAnchor {
                    value: 1.,
                    camera_z_meters: 8.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            10.,
        )
        .unwrap()
        .with_reference(DepthReference {
            camera: reference.native_camera(),
            world: image.model,
            size: image.size,
        })
        .unwrap(),
    );
    p.validate().unwrap();
    p
}
fn attach(scene: &mut Scene) {
    let resource = field(1);
    scene
        .attach_calibrated_depth(1, &resource.descriptor().source, &resource, input())
        .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owned_depth_image_recovers_eviction_and_quality_changes_without_source_io_or_repeat_uploads() {
    use motion_render::{image_resources::Resolution, DepthImage};
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let expected = renderer.capture(&scene, &target).unwrap().0;
    renderer.clear_assets();
    let pixels = vec![255, 0, 0, 255, 255, 0, 0, 255];
    let image = DepthImage::new([2, 1], pixels).unwrap();
    let resource = field(1);
    scene.attach_calibrated_depth_image(1, &resource.descriptor().source, &resource, input(), image).unwrap();
    std::fs::remove_file(root.path().join("assets/red.png")).unwrap();
    renderer.configure_assets_with_scene(&p, root.path(), &scene).unwrap();
    for resolution in [Resolution::Full, Resolution::Preview(1), Resolution::Full] {
        assert!(renderer.prepare_scene_assets(&scene, resolution, false).unwrap());
        let bytes = renderer.image_upload_bytes;
        assert!(renderer.prepare_scene_assets(&scene, resolution, false).unwrap());
        assert_eq!(renderer.image_upload_bytes, bytes, "healthy preparation must not reupload immutable pixels");
        assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
    }
    retain_fixture("owned-image-after-eviction.png", &expected);
    // A source without an owned admission still needs an actual valid file.
    assert!(renderer.configure_assets(&p, root.path()).is_err());
}
fn prepare(renderer: &mut Renderer, p: &Project, root: &std::path::Path) {
    renderer.configure_assets(p, root).unwrap();
    assert!(renderer
        .prepare_scene_assets(
            &sampled(p),
            motion_render::image_resources::Resolution::Full,
            false
        )
        .unwrap());
}

// Independent analytic surface: reference camera Z=100, source local 64x40,
// left distance80/right120. Reference rays scale XY by distance/100. The centre
// strip connects (-12.8,20) to (19.2,-20): world Z=4-1.25X.
fn surface_hit(origin: DVec3, direction: DVec3, world: glam::DMat4) -> Option<f64> {
    let inverse = world.inverse();
    let o = inverse.transform_point3(origin);
    let d = inverse.transform_vector3(direction);
    let mut hit: Option<f64> = None;
    for (low, high, slope, intercept) in [
        (-25.6, -12.8, 0., 20.),
        (-12.8, 19.2, -1.25, 4.),
        (19.2, 38.4, 0., -20.),
    ] {
        let denominator = d.z - slope * d.x;
        if denominator.abs() < 1e-12 {
            continue;
        }
        let t = (intercept + slope * o.x - o.z) / denominator;
        let q = o + d * t;
        if t > 0. && q.x > low + 1. && q.x < high - 1. && q.y.abs() < 20. * (100. - q.z) / 100. - 1.
        {
            hit = Some(hit.map_or(t, |old| old.min(t)));
        }
    }
    hit
}
fn check_oracle(scene: &Scene, pixels: &[u8]) {
    let image = scene.layers.iter().find(|l| l.id == 1).unwrap();
    let blue = scene.layers.iter().find(|l| l.id == 2).unwrap();
    let inverse = scene.camera.view_projection.as_dmat4().inverse();
    let blue_inverse = blue.model.as_dmat4().inverse();
    let mut checked = [0usize; 2];
    for y in 0..64usize {
        for x in 0..96usize {
            let ndc = [(x as f64 + 0.5) / 48. - 1., 1. - (y as f64 + 0.5) / 32.];
            let a = inverse * DVec4::new(ndc[0], ndc[1], 0., 1.);
            let b = inverse * DVec4::new(ndc[0], ndc[1], 0.5, 1.);
            let origin = a.truncate() / a.w;
            let direction = (b.truncate() / b.w - origin).normalize();
            let authored_surface = image.model.as_dmat4()
                * glam::DMat4::from_scale(DVec3::new(
                    f64::from(image.size[0]) / 64.,
                    f64::from(image.size[1]) / 40.,
                    1.,
                ));
            let Some(surface) = surface_hit(origin, direction, authored_surface) else {
                continue;
            };
            let o = blue_inverse.transform_point3(origin);
            let d = blue_inverse.transform_vector3(direction);
            if d.z.abs() < 1e-12 {
                continue;
            }
            let plane = -o.z / d.z;
            let q = o + d * plane;
            if plane <= 0. || q.x.abs() > 30. || q.y.abs() > 18. || (surface - plane).abs() < 2. {
                continue;
            }
            let nearer = usize::from(surface < plane);
            let expected = if nearer == 1 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                &pixels[(y * 96 + x) * 4..][..4],
                &expected,
                "ray x={x} y={y} surface={surface} plane={plane}"
            );
            checked[nearer] += 1;
        }
    }
    assert!(
        checked.iter().all(|n| *n >= 30),
        "both physical surfaces require interior coverage: {checked:?}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_geometry_matches_independent_rays_after_camera_and_parent_edits_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    motion_core::storage::save(root.path(), &authored).unwrap();
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), authored);
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        eprintln!("calibrated depth native precision={precision:?}");
        let mut p = authored.clone();
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &p, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        for moved in [false, true] {
            eprintln!("calibrated depth native precision={precision:?} moved={moved}");
            if moved {
                p.camera.position.set_at(0, [60., 29., -100.]).unwrap();
                let mut parent = Layer::solid(9, "Depth parent", [1.; 2], [48., 32., 0.], [0.; 4]);
                parent.content = Content::Null;
                parent.three_d = true;
                parent.transform.rotation.set_at(0, [0., 9., 4.]).unwrap();
                p.layers[0].parent = Some(motion_core::ParentLink {
                    object: Some(9),
                    bind: Mat4::IDENTITY.to_cols_array_2d(),
                });
                p.layers.push(parent);
            }
            let mut scene = sampled(&p);
            attach(&mut scene);
            let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
            retain_fixture(&format!("camera-{precision:?}-moved-{moved}.png"), &pixels);
            check_oracle(&scene, &pixels);
            p.layers.reverse();
            let mut reversed = sampled(&p);
            attach(&mut reversed);
            let reversed_pixels = renderer.capture_sdr(&reversed, &target).unwrap().0.pixels;
            let difference = reversed_pixels
                .iter()
                .zip(&pixels)
                .position(|(a, b)| a != b);
            assert!(
                difference.is_none(),
                "opaque depth cannot depend on authoring order: first byte {difference:?}"
            );
            p.layers.reverse();
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_geometry_resizes_xy_without_rescaling_metric_depth_in_four_precisions() {
    use motion_effects::image_contract::ChannelPrecision::*;
    let root = tempfile::tempdir().unwrap();
    let authored = project(root.path());
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare(&mut renderer, &authored, root.path());
        let target = renderer
            .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
            .unwrap();
        for size in [[96., 28.], [40., 56.]] {
            let mut p = authored.clone();
            p.layers[0].size = size;
            p.camera.position.set_at(0, [52., 31., -100.]).unwrap();
            let mut scene = sampled(&p);
            attach(&mut scene);
            let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
            retain_fixture(
                &format!("resize-{precision:?}-{}x{}.png", size[0], size[1]),
                &pixels,
            );
            check_oracle(&scene, &pixels);
            p.layers.reverse();
            let mut reversed = sampled(&p);
            attach(&mut reversed);
            assert_eq!(
                renderer.capture_sdr(&reversed, &target).unwrap().0.pixels,
                pixels,
                "resized opaque surfaces cannot depend on authoring order"
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn missing_or_changed_current_depth_cannot_render_flat_and_admission_recovers() {
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let mut scene = sampled(&p);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let error = renderer.capture(&scene, &target).unwrap_err().to_string();
    assert!(
        error.contains("object=1") && error.contains("pending"),
        "{error}"
    );
    let resource = field(1);
    let mut wrong = resource.descriptor().source.clone();
    wrong.revision += 1;
    assert!(scene
        .attach_calibrated_depth(1, &wrong, &resource, input())
        .is_err());
    let mut changed = input();
    changed.rgb_sha256[0] ^= 1;
    assert!(scene
        .attach_calibrated_depth(1, &resource.descriptor().source, &resource, changed)
        .is_err());
    attach(&mut scene);
    let good = renderer.capture(&scene, &target).unwrap().0;
    check_oracle(&scene, &good);
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(renderer
        .capture(&scene, &target)
        .unwrap_err()
        .to_string()
        .contains("pending"));
    assert_eq!(
        renderer.read_target(&target).unwrap(),
        good,
        "refusal cannot overwrite the previous front"
    );
    attach(&mut scene);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, good);
    let mut unsupported = p.clone();
    // A supported ordinary spatial matte source must retain its own geometry
    // and the calibrated receiver's fragment depth, rather than flattening it.
    unsupported.layers[0].track_matte = Some(motion_model::compositing::TrackMatte {
        source: 2,
        mode: motion_model::compositing::MatteMode::Alpha,
        hide_source: false,
    });
    let mut scene = sampled(&unsupported);
    attach(&mut scene);
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    check_oracle(&scene, &pixels);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn reference_camera_preserves_the_original_photo_without_depth_dependent_texture_warp() {
    let root = tempfile::tempdir().unwrap();
    let mut p = project(root.path());
    p.layers.retain(|l| l.id == 1);
    p.layers[0].size = [96., 40.];
    let mut reference = p.layers[0]
        .depth_calibration
        .as_ref()
        .unwrap()
        .reference()
        .unwrap()
        .clone();
    reference.size = [96., 40.];
    p.layers[0].depth_calibration = Some(
        p.layers[0]
            .depth_calibration
            .take()
            .unwrap()
            .with_reference(reference)
            .unwrap(),
    );
    image::save_buffer(
        root.path().join("assets/red.png"),
        &[255, 0, 0, 255, 0, 0, 255, 255],
        2,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut scene = sampled(&p);
    attach(&mut scene);
    let actual = renderer.capture(&scene, &target).unwrap().0;
    p.layers[0].depth_calibration = None;
    let expected = renderer.capture(&sampled(&p), &target).unwrap().0;
    retain_fixture("reference-photo-flat.png", &expected);
    retain_fixture("reference-photo-depth.png", &actual);
    let mut colors = std::collections::BTreeSet::new();
    for y in 26..38 {
        for x in 35..61 {
            let index = (y * 96 + x) * 4;
            colors.insert(expected[index]);
            for c in 0..4 {
                assert!(
                    actual[index + c].abs_diff(expected[index + c]) <= 1,
                    "reference photo changed at x={x} y={y} actual={:?} expected={:?}",
                    &actual[index..index + 4],
                    &expected[index..index + 4]
                );
            }
        }
    }
    assert!(colors.len()>20,"the oracle must exercise a nonuniform source, not a solid successful return: count={} red={colors:?}", colors.len());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_geometry_is_immutable_for_reverse_submission_and_budget_refusal_preserves_front() {
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let mut a_scene = sampled(&p);
    attach(&mut a_scene);
    let mut b_scene = sampled(&p);
    let changed =
        DepthResource::new(field(2).descriptor().clone(), vec![0., 1.], None, None).unwrap();
    b_scene
        .attach_calibrated_depth(1, &changed.descriptor().source, &changed, input())
        .unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    let expected_a = renderer.capture(&a_scene, &a).unwrap().0;
    let expected_b = renderer.capture(&b_scene, &b).unwrap().0;
    assert!(
        expected_a != expected_b,
        "the pending frames must contain different physical geometry"
    );
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&a_scene, &a.view, 96, 64, &mut first, None)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert!(renderer.read_target(&a).unwrap() == expected_a);
    assert!(renderer.read_target(&b).unwrap() == expected_b);
    let budget = renderer.allocation_budget().clone();
    let info = budget.info().unwrap();
    budget.set_limit(info.admitted_bytes).unwrap();
    let error = renderer.capture(&a_scene, &b).unwrap_err();
    assert!(
        error.resource_rejection().is_some(),
        "the failure must retain the allocation budget cause: {error}"
    );
    budget.set_limit(info.budget_bytes).unwrap();
    assert!(
        renderer.read_target(&b).unwrap() == expected_b,
        "failed admission cannot overwrite the previous front"
    );
    assert!(
        renderer.capture(&a_scene, &b).unwrap().0 == expected_a,
        "admission must recover after restoring capacity"
    );
    let used = budget.info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&b_scene, &b.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    assert!(renderer.read_target(&b).unwrap() == expected_a);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn calibrated_geometry_replacement_with_identical_metadata_uses_the_current_resource() {
    let root = tempfile::tempdir().unwrap();
    let p = project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &p, root.path());
    let target = renderer.capture_target(96, 64).unwrap();
    let mut retained = sampled(&p);
    attach(&mut retained);
    let old_scene = retained.clone();
    let old = renderer.capture(&retained, &target).unwrap().0;
    // Same source revision, RGB fingerprint, dimensions and interpretation;
    // a newly admitted inference result may still contain different samples.
    let replacement =
        DepthResource::new(field(1).descriptor().clone(), vec![0., 1.], None, None).unwrap();
    let mut fresh = sampled(&p);
    fresh
        .attach_calibrated_depth(1, &replacement.descriptor().source, &replacement, input())
        .unwrap();
    let expected = renderer.capture(&fresh, &target).unwrap().0;
    assert_ne!(
        expected, old,
        "the independent replacement must change physical occlusion"
    );
    retained
        .attach_calibrated_depth(1, &replacement.descriptor().source, &replacement, input())
        .unwrap();
    assert_eq!(
        renderer.capture(&retained, &target).unwrap().0,
        expected,
        "matching source metadata cannot reuse geometry from an older inference result"
    );
    assert_eq!(
        renderer.capture(&old_scene, &target).unwrap().0,
        old,
        "previously sampled scenes must keep their immutable geometry"
    );
}
