//! Camera ownership, explicit boundaries, and native composition precision.
use super::{contract, routing, *};
use motion_model::*;
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes},
    Scene, ViewCamera,
};
use std::{borrow::Cow, cell::Cell, sync::Arc};

struct Once(Cell<usize>);
impl FrameEvaluator for Once {
    fn evaluate<'a>(&self, p: &'a Project, _: f64) -> motion_model::Result<Cow<'a, Project>> {
        self.0.set(self.0.get() + 1);
        assert_eq!(
            self.0.get(),
            1,
            "forking a sampled frame must not reevaluate"
        );
        Ok(Cow::Borrowed(p))
    }
}

#[test]
fn named_shared_and_observer_views_retain_the_selected_lens_without_reevaluation() {
    let mut p = routing::project(Rotation::None, 0.018);
    p.camera.created = true;
    let mut named = Layer::camera(99, "Independent named lens", 640, 480);
    let Content::Camera { camera } = &mut named.content else {
        unreachable!()
    };
    **camera = p.camera.clone();
    camera.tracking.as_mut().unwrap().rotation = Rotation::Clockwise90;
    p.layers.push(named);
    p.validate().unwrap();
    let evaluator = Once(Cell::new(0));
    let mut source = Scene::new(&p).unwrap();
    source
        .sample_view(&p, 0.5, ViewCamera::Camera(0), &evaluator)
        .unwrap();
    let before = source.project_point([480., 200., 0.]).unwrap();
    let selected = source.fork_view(ViewCamera::Camera(99)).unwrap();
    let mut fresh = Scene::new(&p).unwrap();
    fresh
        .sample_view(
            &p,
            0.5,
            ViewCamera::Camera(99),
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    assert_eq!(selected.native_lens(), fresh.native_lens());
    assert_eq!(
        selected.project_point([480., 200., 0.]).unwrap(),
        fresh.project_point([480., 200., 0.]).unwrap()
    );
    assert_ne!(source.native_lens(), selected.native_lens());
    let observer = Observer::new(640, 480);
    let observed = source.fork_view(ViewCamera::Observer(&observer)).unwrap();
    assert!(observed.native_lens().is_none());
    assert_eq!(
        observed.camera.view_projection,
        observer.pose(640, 480).unwrap().view_projection
    );
    // Previously sampled frames own calibration values independently of edits.
    p.tracking_assets.clear();
    p.camera.tracking = None;
    assert_eq!(source.project_point([480., 200., 0.]).unwrap(), before);
    assert_eq!(evaluator.0.get(), 1);
    let restored = observed.fork_view(ViewCamera::Camera(0)).unwrap();
    assert_eq!(restored.native_lens(), source.native_lens());
}

fn nested(p: &Project) -> Project {
    let mut parent = p.clone();
    parent.compositions.push(Composition {
        id: "comp-independent-lens".into(),
        name: "Independent lens child".into(),
        width: 640,
        height: 480,
        fps: p.fps,
        frames: p.frames,
        work_area: p.work_area,
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: p.layers.clone(),
        expressions: vec![],
    });
    // The flat reference must never be distorted by this different root lens.
    parent.camera.tracking.as_mut().unwrap().rotation = Rotation::HalfTurn;
    let mut reference = Layer::solid(
        50,
        "Flat child reference",
        [640., 480.],
        [320., 240., 0.],
        [1.; 4],
    );
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-independent-lens".into()),
    };
    parent.layers = vec![reference];
    parent.validate().unwrap();
    parent
}

#[test]
fn exposure_receivers_keep_fractional_calibration_and_portable_output_refuses_nested_lenses() {
    let mut p = routing::project(Rotation::Clockwise270, -0.018);
    p.layers[0].sampling.motion_blur = Some(render_sampling::Shutter {
        frames: 1.,
        samples: 4,
    });
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let mut exposure = ExposureScenes::new(
        document,
        &p.composition_id,
        1.,
        false,
        None,
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let mut times = Vec::new();
    loop {
        match exposure.current().unwrap() {
            ExposureSceneStep::Receiver(receiver) => {
                let scene = receiver.scene.unwrap();
                let expected = routing::sample(&p, receiver.frame);
                assert_eq!(scene.native_lens(), expected.native_lens());
                assert_eq!(
                    scene.camera.view_projection,
                    expected.camera.view_projection
                );
                if receiver.object == 10 {
                    times.push(receiver.frame);
                }
                exposure.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => exposure.finish_phase().unwrap(),
            ExposureSceneStep::Complete => break,
        }
    }
    assert_eq!(times, [0.625, 0.875, 1.125, 1.375]);
    let mut builder = motion_render::effect_plan::PlanBuilder::new(
        motion_effects::Registry::new_with_builtins().unwrap(),
    )
    .unwrap();
    let plain = Project::new_with_frame_rate(640, 480, p.fps, p.frames).unwrap();
    let plan = builder
        .build(&routing::sample(&plain, 0.), &[0], 640, 480, true)
        .unwrap();
    for scene in [routing::sample(&p, 0.), routing::sample(&nested(&p), 0.)] {
        let mut bytes = [0xabu8; 32];
        let error = plan.write(&scene, &mut bytes).unwrap_err().to_string();
        assert!(error.contains("phase=lens_pack"), "{error}");
        assert_eq!(bytes, [0xab; 32], "rejection precedes serialization");
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn nested_camera_pixels_are_independent_of_the_parent_lens() {
    let child = routing::project(Rotation::Clockwise90, 0.018);
    let parent = nested(&child);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let direct = renderer
        .capture(&routing::sample(&child, 0.), &target)
        .unwrap()
        .0;
    let sampled = routing::sample(&parent, 0.);
    assert_ne!(sampled.native_lens(), sampled.nested[0].scene.native_lens());
    let actual = renderer.capture(&sampled, &target).unwrap().0;
    assert_eq!(
        direct, actual,
        "flat nested raster is not warped by the parent camera"
    );
    let observer = Observer::new(640, 480);
    let observed = sampled.fork_view(ViewCamera::Observer(&observer)).unwrap();
    assert!(observed.native_lens().is_none());
    assert!(
        observed.has_native_lens(),
        "child owns its lens when root view changes"
    );
    assert_eq!(renderer.capture(&observed, &target).unwrap().0, direct);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn unsupported_spatial_model_blend_is_refused_and_valid_render_recovers() {
    let root = tempfile::tempdir().unwrap();
    let mut p = routing::project(Rotation::None, 0.018);
    routing::install_mesh(&mut p, root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let good = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap()
        .0;
    p.layers[1].blend.mode = compositing::BlendMode::Multiply;
    let error = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("object=11") && error.contains("phase=shared_geometry"),
        "{error}"
    );
    p.layers[1].blend = Default::default();
    assert_eq!(
        renderer
            .capture(&routing::sample(&p, 0.), &target)
            .unwrap()
            .0,
        good
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn composition_lens_preserves_native_alpha_at_all_four_precisions() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut p = routing::project(Rotation::None, 0.018);
        p.layers.truncate(1);
        p.layers[0].size = [10000.; 2];
        p.layers[0].transform.position.value = [320., 240., 0.];
        p.layers[0].content = Content::Solid {
            color: [1., 0., 0., 0.50003],
        };
        p.working_precision = precision;
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            Default::default(),
        ))
        .unwrap();
        let target = renderer
            .image_capture_target(640, 480, contract(precision))
            .unwrap();
        renderer
            .draw(&routing::sample(&p, 0.), target.view(), 640, 480)
            .unwrap();
        let image = renderer.read_image_target(&target).unwrap();
        let bpp = image.storage.bytes_per_pixel() as usize;
        for pixel in image.pixels.chunks_exact(bpp) {
            match precision {
                ChannelPrecision::Unorm8 => assert!(pixel[3].abs_diff(128) <= 1),
                ChannelPrecision::Unorm16 => assert!(
                    u16::from_le_bytes(pixel[6..8].try_into().unwrap())
                        .abs_diff((0.50003f32 * 65535.).round() as u16)
                        <= 2
                ),
                ChannelPrecision::Float16 => {
                    assert_eq!(u16::from_le_bytes(pixel[6..8].try_into().unwrap()), 0x3800)
                }
                ChannelPrecision::Float32 => assert!(
                    (f32::from_le_bytes(pixel[12..16].try_into().unwrap()) - 0.50003).abs()
                        < 0.000001
                ),
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn queued_compositions_keep_distinct_lens_parameters_until_submission() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scenes = [-0.018, 0.018].map(|k| routing::sample(&routing::project(Rotation::None, k), 0.));
    let targets: Vec<_> = (0..2)
        .map(|_| {
            renderer
                .image_capture_target(640, 480, contract(ChannelPrecision::Unorm8))
                .unwrap()
        })
        .collect();
    let mut expected = Vec::new();
    for (scene, target) in scenes.iter().zip(&targets) {
        renderer.draw(scene, target.view(), 640, 480).unwrap();
        expected.push(renderer.read_image_target(target).unwrap().pixels);
    }
    assert!(
        expected[0]
            .iter()
            .zip(&expected[1])
            .filter(|(a, b)| a.abs_diff(**b) > 3)
            .count()
            > 2000
    );
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    for (scene, target) in scenes.iter().zip(&targets) {
        renderer
            .encode(scene, target.view(), 640, 480, &mut encoder, None)
            .unwrap();
    }
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            for target in &targets {
                ledger.merge(target.texture_resource_ledger());
            }
        })
        .unwrap();
    for (i, target) in targets.iter().enumerate() {
        assert_eq!(
            renderer.read_image_target(target).unwrap().pixels,
            expected[i],
            "queued composition {i}"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn flat_identity_effect_matches_the_fast_composition_path() {
    let mut p = routing::project(Rotation::None, 0.018);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let expected = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap()
        .0;
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "brightness_contrast")
        .unwrap();
    let size = p.layers[3].size;
    p.layers[3].effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        size,
    ));
    p.rebuild_plugin_dependencies();
    let actual = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap()
        .0;
    let error = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        error <= 2,
        "identity effect must preserve the spatial lens raster: {error}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn overscan_source_budget_and_aperture_boundaries_are_explicit_and_recoverable() {
    let mut p = routing::project(Rotation::None, 0.018);
    p.layers.truncate(1);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let policy = renderer.memory_policy();
    let bytes = 640 * 480 * 4;
    renderer
        .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
            source_texture_budget_bytes: bytes,
            ..policy
        })
        .unwrap();
    let error = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap_err();
    assert!(error.resource_rejection().is_some(), "{error}");
    assert!(
        error
            .to_string()
            .contains("shared raster exceeds source texture budget"),
        "{error}"
    );
    renderer.set_memory_policy(policy).unwrap();
    let expected = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap()
        .0;
    p.camera.depth_of_field = Some(DepthOfField {
        focus_distance: Track::constant(500.),
        aperture_diameter: Track::constant(10.),
        max_radius_pixels: 8.,
    });
    let error = renderer
        .capture(&routing::sample(&p, 0.), &target)
        .unwrap_err()
        .to_string();
    assert!(error.contains("phase=aperture_lens"), "{error}");
    p.camera.depth_of_field = None;
    assert_eq!(
        renderer
            .capture(&routing::sample(&p, 0.), &target)
            .unwrap()
            .0,
        expected
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn retained_images_and_actual_exposure_tasks_cannot_bypass_nonlinear_output() {
    use motion_render::{ExposureFrame, ExposurePoll};
    let mut p = routing::project(Rotation::None, 0.018);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = routing::sample(&p, 0.);
    assert!(renderer
        .materialize_layer(&scene, 999, 640, 480, false)
        .unwrap()
        .is_none());
    let error = renderer
        .materialize_layer(&scene, 10, 640, 480, false)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("object=10") && error.contains("phase=retained_lens"),
        "{error}"
    );
    let flat = renderer
        .materialize_layer(&scene, 13, 640, 480, false)
        .unwrap()
        .unwrap()
        .0;
    let target = renderer
        .image_capture_target(640, 480, contract(ChannelPrecision::Unorm8))
        .unwrap();
    renderer
        .draw_layer_rasters(&scene, &[&flat], target.view(), 640, 480)
        .unwrap();
    let image = renderer.read_image_target(&target).unwrap();
    assert_eq!(
        &image.pixels[(100 * 640 + 600) * 4..(100 * 640 + 601) * 4],
        &[255, 255, 0, 255]
    );
    let mut plain = p.clone();
    plain.camera.tracking = None;
    let ideal = routing::sample(&plain, 0.);
    let retained = renderer
        .materialize_layer(&ideal, 10, 640, 480, false)
        .unwrap()
        .unwrap()
        .0;
    let error = renderer
        .draw_layer_rasters(&scene, &[&retained], target.view(), 640, 480)
        .unwrap_err()
        .to_string();
    assert!(error.contains("phase=retained_lens"), "{error}");
    renderer
        .draw_layer_rasters(&ideal, &[&retained], target.view(), 640, 480)
        .unwrap();
    p.layers[0].sampling.motion_blur = Some(render_sampling::Shutter {
        frames: 1.,
        samples: 4,
    });
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &p.composition_id,
        0.5,
        false,
        None,
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    let mut task = ExposureFrame::new(&mut renderer, scenes, 640, 480).unwrap();
    let mut rejected = None;
    for _ in 0..32 {
        match task.poll(&mut renderer, std::time::Duration::from_secs(1), |_, _| {
            Ok(true)
        }) {
            Err(error) => {
                rejected = Some(error.to_string());
                break;
            }
            Ok(ExposurePoll::Ready(_)) => {
                panic!("unsupported lens exposure must not publish unwarped pixels")
            }
            Ok(_) => {}
        }
    }
    let error = rejected.expect("actual receiver path must reach the lens guard");
    assert!(error.contains("phase=retained_lens"), "{error}");
    drop(task);
    p.layers[0].sampling = Default::default();
    assert!(renderer
        .capture(
            &routing::sample(&p, 0.),
            &renderer.capture_target(640, 480).unwrap()
        )
        .is_ok());
    let root = tempfile::tempdir().unwrap();
    routing::install_mesh(&mut p, root.path());
    let error = renderer
        .materialize_layer(&routing::sample(&p, 0.), 11, 640, 480, false)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("object=11") && error.contains("phase=retained_lens"),
        "{error}"
    );
}
