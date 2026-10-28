use super::*;
use glam::{Mat4, Vec3};
use motion_core::ExpressionEvaluator;
use motion_render::ModelSourceCatalog;

#[path = "../support/imported_camera.rs"]
mod fixture;

fn sampled(project: &Project, catalog: &ModelSourceCatalog, frame: f64) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene.set_model_source_catalog(catalog.clone());
    scene
        .sample(project, frame, None, &ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
fn imported_camera_save_reopen_animation_and_missing_resolver_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let authored = fixture::project(root.path(), false);
    motion_core::storage::save(root.path(), &authored).unwrap();
    let project = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(project, authored);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    for (frame, x) in [(0., 0.), (15., 5.), (30., 10.), (60., 20.), (0., 0.)] {
        let scene = sampled(&project, &catalog, frame);
        let eye = Vec3::new(x, 0., 60.);
        assert_eq!(scene.camera.eye, eye);
        let expected =
            Mat4::perspective_rh(1., 1.5, 2., 200.) * Mat4::look_at_rh(eye, eye - Vec3::Z, Vec3::Y);
        assert!(scene.camera.view_projection.abs_diff_eq(expected, 1e-6));
        assert_eq!(scene.camera_viewport, [0., 0., 96., 64.]);
    }
    let mut scene = Scene::new(&project).unwrap();
    let error = scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source=7") && error.contains("node=1") && error.contains("not configured"),
        "{error}"
    );
    assert!(project
        .camera
        .pose(0., 96, 64)
        .unwrap_err()
        .to_string()
        .contains("resolver"));
    for (scene_id, node, clip, cause) in [
        (0, 0, 0, "does not instantiate"),
        (99, 1, 0, "scene"),
        (0, 99, 0, "absent"),
        (0, 1, 99, "animation"),
    ] {
        let mut invalid = project.clone();
        let binding = invalid.camera.imported.as_mut().unwrap();
        binding.scene = scene_id;
        binding.node = node;
        binding.animation.as_mut().unwrap().clip = clip;
        let error = ModelSourceCatalog::prepare(&invalid, root.path(), &catalog)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("phase=selection")
                && error.contains("source=7")
                && error.contains(cause),
            "{error}"
        );
    }
    assert_eq!(
        sampled(&project, &catalog, 0.).camera.eye,
        Vec3::new(0., 0., 60.)
    );
}

#[test]
fn imported_camera_resolves_parent_rig_children_layer_clock_and_observer() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), true);
    let mut rig = Layer::solid(2, "Camera rig", [1.; 2], [55., 28., -2.], [0.; 4]);
    rig.content = Content::Null;
    rig.three_d = true;
    let bind = motion_core::ParentLink {
        object: Some(2),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    };
    project.camera.parent = Some(bind.clone());
    let mut child = Layer::solid(3, "Camera child", [1.; 2], [48., 32., 4.], [1.; 4]);
    child.three_d = true;
    child.parent = Some(motion_core::ParentLink {
        object: Some(0),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    });
    project.layers.extend([rig, child]);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let scene = sampled(&project, &catalog, 30.);
    assert_eq!(scene.camera.eye, Vec3::new(17., 4., 62.));
    assert_eq!(
        scene.world_matrix(3).unwrap().w_axis.truncate(),
        Vec3::new(17., 4., 58.)
    );
    let mut selected = Layer::camera(4, "Imported camera instance", 96, 64);
    if let Content::Camera { camera } = &mut selected.content {
        camera.imported = project.camera.imported.clone();
    }
    selected.parent = Some(bind);
    selected.timeline = Some(motion_core::LayerTimeline {
        in_frame: 20,
        out_frame: 61,
        offset_frame: 20,
    });
    project.layers.push(selected);
    let scene = sampled(&project, &catalog, 30.);
    assert!((scene.camera.eye.x - (7. + 10. / 3.)).abs() < 1e-6);
    // The base camera's child keeps the base camera's clock, not the selected view's clock.
    assert_eq!(
        scene.world_matrix(3).unwrap().w_axis.truncate(),
        Vec3::new(17., 4., 58.)
    );
    let observer = motion_core::Observer::new(96, 64);
    let mut observed = scene.clone();
    observed
        .sample(&project, 30., Some(&observer), &ExpressionEvaluator)
        .unwrap();
    assert_eq!(
        observed.camera.view_projection,
        observer.pose(96, 64).unwrap().view_projection
    );
    assert_eq!(observed.camera_viewport, [0., 0., 96., 64.]);
    assert_eq!(observed.world_matrix(3), scene.world_matrix(3));
    project
        .layers
        .iter_mut()
        .find(|l| l.id == 2)
        .unwrap()
        .parent = Some(motion_core::ParentLink {
        object: Some(0),
        bind: Mat4::IDENTITY.to_cols_array_2d(),
    });
    assert!(project
        .validate()
        .unwrap_err()
        .to_string()
        .contains("cycle"));
}

#[test]
fn imported_camera_catalog_follows_nested_composition_fps_and_exposure_receiver_clocks() {
    use motion_core::{
        composition::{Composition, CompositionClip},
        render_sampling::Shutter,
    };
    use motion_render::exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes};
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), false);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    project.layers[0].sampling.motion_blur = Some(Shutter {
        frames: 2.,
        samples: 4,
    });
    let document = ExposureDocument::new(std::sync::Arc::new(project.clone())).unwrap();
    let mut exposure = ExposureScenes::new(
        document,
        &project.composition_id,
        30.,
        false,
        None,
        ExpressionEvaluator,
    )
    .unwrap();
    exposure.set_model_source_catalog(catalog.clone()).unwrap();
    assert_eq!(exposure.center_scene().unwrap().camera.eye.x, 10.);
    let mut times = Vec::new();
    loop {
        match exposure.current().unwrap() {
            ExposureSceneStep::Receiver(receiver) => {
                let scene = receiver.scene.unwrap();
                assert!((f64::from(scene.camera.eye.x) - receiver.frame / 3.).abs() < 1e-6);
                assert!(scene.model_source_catalog().shares_sources_with(&catalog));
                times.push(receiver.frame);
                exposure.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => exposure.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert_eq!(times, [29.25, 29.75, 30.25, 30.75]);
    assert!(exposure
        .set_model_source_catalog(Default::default())
        .is_err());
    exposure.set_model_source_catalog(catalog.clone()).unwrap();
    let child = Composition {
        id: "comp-camera".into(),
        name: "Imported camera child".into(),
        width: 96,
        height: 64,
        fps: 15,
        frames: 31,
        work_area: motion_core::WorkArea::full(31),
        guides: vec![],
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: vec![],
    };
    project.compositions.push(child);
    project.camera = motion_core::Camera::new(96, 64);
    project.layers.clear();
    for (id, offset) in [(10, 0), (11, 15)] {
        let mut reference = Layer::solid(id, "Nested camera", [96., 64.], [48., 32., 0.], [1.; 4]);
        reference.content = Content::Composition {
            clip: CompositionClip::new("comp-camera".into()),
        };
        reference.timeline = Some(motion_core::LayerTimeline {
            in_frame: 0,
            out_frame: 61,
            offset_frame: offset,
        });
        project.layers.push(reference);
    }
    project.validate().unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let nested = sampled(&project, &catalog, 30.);
    assert_eq!(nested.nested.len(), 2);
    for (id, x, frame) in [(10, 10., 15.), (11, 5., 7.5)] {
        let child = &nested
            .nested
            .iter()
            .find(|child| child.layer == id)
            .unwrap()
            .scene;
        assert_eq!(child.frame, frame);
        assert_eq!(child.camera.eye.x, x);
        assert!(child.model_source_catalog().shares_sources_with(&catalog));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_camera_shutter_matches_independent_native_alpha_average() {
    use motion_render::{
        exposure_scene::{ExposureDocument, ExposureScenes},
        ExposureFrame, ExposurePoll,
    };
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), false);
    // Keep the reference mean in float, so per-receiver UNORM8 quantization
    // cannot turn an exact red/alpha relation into a different color oracle.
    project.working_precision = motion_effects::image_contract::ChannelPrecision::Float32;
    project.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter {
        frames: 2.,
        samples: 4,
    });
    let animation = project
        .camera
        .imported
        .as_mut()
        .unwrap()
        .animation
        .as_mut()
        .unwrap();
    animation.offset_seconds = -29.;
    animation.speed = 30.;
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
        project.working_precision,
        motion_render::AllocationBudget::default(),
    ))
    .unwrap();
    renderer
        .configure_assets_with_model_catalog(&project, root.path(), &catalog)
        .unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let document = ExposureDocument::new(std::sync::Arc::new(project.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &project.composition_id,
        30.,
        false,
        None,
        ExpressionEvaluator,
    )
    .unwrap();
    let mut task = ExposureFrame::new(&mut renderer, scenes, 96, 64).unwrap();
    let mut ready = false;
    for _ in 0..16 {
        if matches!(
            task.poll(
                &mut renderer,
                std::time::Duration::from_secs(1),
                |renderer, scene| {
                    renderer.retain_video_instances(scene);
                    Ok(true)
                }
            )
            .unwrap(),
            ExposurePoll::Ready(_)
        ) {
            ready = true;
            break;
        }
    }
    assert!(ready);
    let target = renderer
        .sdr_capture_target(96, 64, motion_render::OutputTransform::SDR_CLIP)
        .unwrap();
    let actual = renderer
        .capture_exposure_sdr(task.target(), &target)
        .unwrap()
        .pixels;
    let samples: Vec<_> = [29.25, 29.75, 30.25, 30.75]
        .into_iter()
        .map(|frame| {
            let scene = sampled(&project, &catalog, frame);
            renderer.retain_video_instances(&scene);
            renderer.capture_sdr(&scene, &target).unwrap().0.pixels
        })
        .collect();
    for (index, pixel) in actual.chunks_exact(4).enumerate() {
        let alpha = (samples
            .iter()
            .map(|pixels| u32::from(pixels[index * 4 + 3]))
            .sum::<u32>() as f64
            / 4.)
            .round() as u8;
        assert!(
            pixel[3].abs_diff(alpha) <= 1,
            "pixel={index} alpha={} expected={alpha}",
            pixel[3]
        );
        if alpha > 0 {
            assert_eq!(&pixel[..3], &[255, 0, 0]);
        }
    }
    assert!(
        actual
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0 && pixel[3] < 255),
        "moving camera must produce temporal edge coverage"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_camera_drives_native_pixels_and_rewinds_without_source_io() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture::project(root.path(), false);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    std::fs::remove_file(root.path().join(&project.model_assets[0].path)).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .configure_assets_with_model_catalog(&project, root.path(), &catalog)
        .unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let mut pixels = Vec::new();
    for frame in [0., 30., 0.] {
        let scene = sampled(&project, &catalog, frame);
        renderer.retain_video_instances(&scene);
        pixels.push(renderer.capture(&scene, &target).unwrap().0);
    }
    assert!(
        pixels[0]
            .chunks_exact(4)
            .filter(|p| p[0] == 255 && p[3] == 255)
            .count()
            > 100
    );
    assert!(
        center(&pixels[1]) < center(&pixels[0]) - 8.,
        "camera translation must move the image left"
    );
    assert_eq!(
        pixels[0], pixels[2],
        "rewind must reproduce exact native pixels"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn imported_camera_viewport_clips_planes_and_meshes_but_preserves_2d_layers() {
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(root.path(), true);
    let catalog = ModelSourceCatalog::prepare(&project, root.path(), &Default::default()).unwrap();
    let mut plane = Layer::solid(
        2,
        "Camera plane",
        [1000.; 2],
        [48., 32., 5.],
        [0., 1., 0., 1.],
    );
    plane.three_d = true;
    project.layers.push(plane);
    project.layers.insert(
        0,
        Layer::solid(
            3,
            "2D background",
            [96., 64.],
            [48., 32., 0.],
            [0., 0., 1., 1.],
        ),
    );
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer
        .configure_assets_with_model_catalog(&project, root.path(), &catalog)
        .unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    for mesh in [true, false] {
        project
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap()
            .visible = mesh;
        let scene = sampled(&project, &catalog, 0.);
        assert_eq!(scene.camera_viewport, [16., 0., 64., 64.]);
        renderer.retain_video_instances(&scene);
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        let pixel = |x: usize, y: usize| &pixels[(y * 96 + x) * 4..(y * 96 + x + 1) * 4];
        for x in [0, 8, 15, 80, 88, 95] {
            assert_eq!(pixel(x, 32), [0, 0, 255, 255], "mesh={mesh} x={x}");
        }
        assert_eq!(pixel(18, 32), [0, 255, 0, 255]);
        if mesh {
            assert_eq!(pixel(48, 32), [255, 0, 0, 255]);
        } else {
            assert_eq!(pixel(48, 32), [0, 255, 0, 255]);
        }
        let outside = scene.hit_candidates([8., 32.]).unwrap();
        assert!(outside.iter().all(|hit| hit.id == 3), "{outside:?}");
        assert!(
            !outside.is_empty(),
            "2D background remains hittable in the camera border"
        );
    }
}
