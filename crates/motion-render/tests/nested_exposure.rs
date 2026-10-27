use motion_core::{render_sampling::Shutter, *};
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureSceneStep, ExposureScenes},
    ExposureFrame, ExposurePoll, ExposureStage, Renderer, Scene, TextureCategory,
};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

fn reference(id: u64, composition: &str) -> Layer {
    let mut layer = Layer::solid(id, "nested reference", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Composition {
        clip: CompositionClip::new(composition.into()),
    };
    layer
}
fn composition(id: &str, fps: u32, layers: Vec<Layer>) -> Composition {
    Composition {
        id: id.into(),
        name: "independent clocks".into(),
        width: 64,
        height: 64,
        fps,
        frames: 90,
        work_area: motion_core::WorkArea::full(90),
        guides: Vec::new(),
        background: [0.; 4],
        camera: Camera::new(64, 64),
        layers,
        expressions: vec![],
    }
}
fn fixture(deep: bool) -> Project {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.background = [0.; 4];
    let mut red = Layer::solid(1, "red shutter", [8.; 2], [32., 16., 0.], [1., 0., 0., 0.7]);
    let mut green = Layer::solid(
        2,
        "green center clock",
        [8.; 2],
        [32., 48., 0.],
        [0., 1., 0., 1.],
    );
    for layer in [&mut red, &mut green] {
        for (frame, x) in [(0, 8.), (30, 56.)] {
            layer
                .transform
                .position
                .upsert(
                    frame,
                    [x, layer.transform.position.value[1], 0.],
                    Ease::Linear,
                )
                .unwrap();
        }
    }
    red.sampling.motion_blur = Some(Shutter {
        frames: 2.,
        samples: 4,
    });
    project
        .compositions
        .push(composition("comp-leaf", 60, vec![red, green]));
    let mut outer = reference(1, if deep { "comp-middle" } else { "comp-leaf" });
    if deep {
        outer.sampling.motion_blur = Some(Shutter {
            frames: 1.5,
            samples: 2,
        });
        let mut middle = reference(1, "comp-leaf");
        middle.sampling.motion_blur = Some(Shutter {
            frames: 1.,
            samples: 2,
        });
        project
            .compositions
            .push(composition("comp-middle", 24, vec![middle]));
        let mut repeated = outer.clone();
        repeated.id = 3;
        repeated.transform.opacity.value = 0.4;
        if let Content::Composition { clip } = &mut repeated.content {
            clip.source_start_frame = 3;
        }
        project.layers.push(repeated);
    }
    project.layers.push(outer);
    project.validate().unwrap();
    project
}
fn task(
    renderer: &mut Renderer,
    p: &Project,
    frame: f64,
    preview: bool,
) -> ExposureFrame<ExpressionEvaluator> {
    let document = ExposureDocument::new(Arc::new(p.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &p.composition_id,
        frame,
        preview,
        None,
        ExpressionEvaluator,
    )
    .unwrap();
    ExposureFrame::new(renderer, scenes, p.width, p.height).unwrap()
}
fn finish(
    renderer: &mut Renderer,
    task: &mut ExposureFrame<ExpressionEvaluator>,
) -> motion_render::RenderStats {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let ExposurePoll::Ready(stats) = task
            .poll(renderer, Duration::from_millis(8), |_, scene| {
                assert!(
                    scene.nested.is_empty(),
                    "callback receives only the current composition's media"
                );
                Ok(true)
            })
            .unwrap()
        {
            return stats;
        }
        assert!(Instant::now() < deadline);
    }
}
fn close(actual: &[u8], expected: &[u8], tolerance: u8) {
    assert_eq!(actual.len(), expected.len());
    let (index, max) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .map(|(index,(a, b))| (index,a.abs_diff(*b)))
        .max_by_key(|(_,difference)| *difference)
        .unwrap();
    assert!(
        max <= tolerance,
        "maximum channel difference {max} exceeds {tolerance} pixel={} actual={:?} expected={:?}",
        index/4, &actual[index/4*4..index/4*4+4], &expected[index/4*4..index/4*4+4]
    );
}

// Independent, deliberately expensive oracle: recursively read each child
// back to CPU and re-upload it as an ordinary image. Production never does this.
// The clocks/rasters use the public lower-level protocol, not ExposureFrame.
fn oracle(
    renderer: &mut Renderer,
    document: &ExposureDocument,
    composition: &str,
    frame: f64,
    preview: bool,
    serial: &mut u64,
) -> Vec<u8> {
    let mut scenes = ExposureScenes::new(
        document.clone(),
        composition,
        frame,
        preview,
        None,
        ExpressionEvaluator,
    )
    .unwrap();
    let mut target = renderer
        .exposure_target(64, 64, &scenes.phase_weights().collect::<Vec<_>>())
        .unwrap();
    let mut rasters = vec![];
    loop {
        match scenes.current().unwrap() {
            ExposureSceneStep::Receiver(receiver) => {
                if receiver.layer().is_some_and(|layer| !layer.adjustment) {
                    if let Some(source) = receiver.scene {
                        let context = oracle_context(renderer, document, source, preview, serial);
                        if let Some((raster, _)) = renderer
                            .materialize_layer(&context, receiver.object, 64, 64, preview)
                            .unwrap()
                        {
                            rasters.push(raster);
                        }
                    }
                }
                scenes.finish_receiver().unwrap();
            }
            ExposureSceneStep::PhaseReady { .. } => {
                let center = scenes.center_scene().unwrap();
                let context = oracle_context(renderer, document, center, preview, serial);
                renderer
                    .accumulate_exposure_phase(
                        &mut target,
                        &context,
                        &rasters.iter().collect::<Vec<_>>(),
                        preview,
                    )
                    .unwrap();
                rasters.clear();
                scenes.finish_phase().unwrap();
            }
            ExposureSceneStep::Complete => break,
        }
    }
    renderer.capture_exposure(&target).unwrap()
}
fn oracle_context(
    renderer: &mut Renderer,
    document: &ExposureDocument,
    scene: &Scene,
    preview: bool,
    serial: &mut u64,
) -> Scene {
    let mut result = scene.clone();
    result.nested.clear();
    for node in &scene.nested {
        let pixels = oracle(
            renderer,
            document,
            &node.composition,
            node.scene.frame,
            preview,
            serial,
        );
        // read_target already returns straight-alpha sRGB, which upload_image
        // accepts. A second unpremultiplication brightens translucent colors.
        *serial += 1;
        renderer.upload_image(*serial, 64, 64, &pixels).unwrap();
        let layer = result
            .layers
            .iter_mut()
            .find(|layer| layer.id == node.layer)
            .unwrap();
        layer.composition = false;
        layer.asset = Some(*serial);
    }
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn independent_oracle_uploads_straight_alpha_capture_without_brightening_the_child() {
    let mut project = fixture(false);
    let red = &mut project.compositions[0].layers[0];
    red.content = Content::Solid {color:[0.15,0.35,0.65,0.4]};
    red.transform.position = Track::constant([32.,16.,0.]);
    red.sampling.motion_blur = None;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project,6.5,None,&ExpressionEvaluator).unwrap();
    let target = renderer.capture_target(64,64).unwrap();
    let expected = renderer.capture(&scene,&target).unwrap().0;
    assert!(expected.chunks_exact(4).any(|pixel|pixel[0]>0 && pixel[2]>pixel[0] && pixel[3]>0 && pixel[3]<255));
    let mut oracle_renderer = pollster::block_on(Renderer::headless()).unwrap();
    let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
    let actual = oracle(&mut oracle_renderer,&document,&project.composition_id,6.5,false,&mut 1000);
    close(&actual,&expected,2);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn recursive_mixed_rates_parent_shutters_and_repeated_refs_match_independent_readback_oracle() {
    let project = fixture(true);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut frame = task(&mut renderer, &project, 6.5, false);
    let started = Instant::now();
    let stats = finish(&mut renderer, &mut frame);
    let actual = renderer.capture_exposure(frame.target()).unwrap();
    let elapsed = started.elapsed().as_micros();
    assert!(frame.progress().work_units > 30);
    let mut oracle_renderer = pollster::block_on(Renderer::headless()).unwrap();
    let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
    let expected = oracle(
        &mut oracle_renderer,
        &document,
        &project.composition_id,
        6.5,
        false,
        &mut 1000,
    );
    close(&actual, &expected, 2);
    let mut sharp = project.clone();
    for composition in &mut sharp.compositions {
        for layer in &mut composition.layers {
            layer.sampling.motion_blur = None;
        }
    }
    for layer in &mut sharp.layers {
        layer.sampling.motion_blur = None;
    }
    let mut sharp_frame = task(&mut renderer, &sharp, 6.5, false);
    finish(&mut renderer, &mut sharp_frame);
    let sharp = renderer.capture_exposure(sharp_frame.target()).unwrap();
    assert_ne!(actual, sharp);
    let support = |pixels: &[u8]| pixels.chunks_exact(4).filter(|p| p[0] > 20).count();
    assert!(support(&actual) > support(&sharp));
    if let Some(directory) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (name, pixels) in [
            ("nested-exposure", &actual),
            ("nested-sharp", &sharp),
            ("nested-oracle", &expected),
        ] {
            image::save_buffer(
                directory.join(format!("{name}.png")),
                pixels,
                64,
                64,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        std::fs::write(directory.join("nested-cost.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "capture_us":elapsed,"work_units":frame.progress().work_units,"draw_calls":stats.draw_calls,
            "width":64,"height":64,"adapter":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),
            "tracked_texture_peak":stats.texture_bytes,"scope":"persistent frame poll plus GPU capture; excludes initialization, oracle and file writes"})).unwrap()).unwrap();
    }
    drop(frame);
    drop(sharp_frame);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn child_clip_can_enter_its_shutter_while_center_is_empty_and_green_stays_at_center() {
    let mut project = fixture(false);
    let child = &mut project.compositions[0];
    child.fps = 30;
    child.layers[0].timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 40,
        offset_frame: 0,
    });
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut frame = task(&mut renderer, &project, 9.5, false);
    finish(&mut renderer, &mut frame);
    let actual = renderer.capture_exposure(frame.target()).unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 9.5, None, &ExpressionEvaluator)
        .unwrap();
    assert_eq!(scene.nested[0].scene.layers.len(), 1);
    let target = renderer.capture_target(64, 64).unwrap();
    let center = renderer.capture(&scene, &target).unwrap().0;
    assert!(actual.chunks_exact(4).any(|p| p[0] > 20));
    assert!(!center.chunks_exact(4).any(|p| p[0] > 20));
    for y in 40..56 {
        let start = y * 64 * 4;
        assert_eq!(
            &actual[start..start + 64 * 4],
            &center[start..start + 64 * 4]
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn resolved_child_waits_for_parent_media_without_restarting_and_transfers_texture_identity() {
    let project = fixture(false);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut frame = task(&mut renderer, &project, 6.5, false);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        frame
            .poll(&mut renderer, Duration::from_millis(8), |_, scene| {
                Ok(scene.composition_id != project.composition_id)
            })
            .unwrap();
        if frame.texture_resource_ledger().resources().any(|resource| {
            resource
                .references
                .iter()
                .any(|r| r.owner.contains("nested/") && r.owner.ends_with("/resolved"))
        }) {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    let progress = frame.progress();
    let bytes = renderer.exposure_texture_bytes();
    let ids: HashSet<_> = frame
        .texture_resource_ledger()
        .resources()
        .filter(|resource| {
            resource
                .references
                .iter()
                .any(|r| r.owner.contains("nested/") && r.owner.ends_with("/resolved"))
        })
        .map(|resource| resource.id.clone())
        .collect();
    assert_eq!(ids.len(), 1);
    for _ in 0..60 {
        let mut child_calls = 0;
        assert!(matches!(
            frame
                .poll(&mut renderer, Duration::from_millis(8), |_, scene| {
                    if scene.composition_id == project.composition_id {
                        Ok(false)
                    } else {
                        child_calls += 1;
                        Ok(true)
                    }
                })
                .unwrap(),
            ExposurePoll::Pending(_)
        ));
        assert_eq!(child_calls, 0);
        assert_eq!(frame.progress(), progress);
        assert_eq!(renderer.exposure_texture_bytes(), bytes);
    }
    finish(&mut renderer, &mut frame);
    let imported: HashSet<_> = renderer
        .texture_resource_ledger()
        .resources()
        .filter(|resource| {
            resource
                .references
                .iter()
                .any(|r| r.category == TextureCategory::NestedCompositions)
        })
        .map(|resource| resource.id.clone())
        .collect();
    assert!(
        ids.is_subset(&imported),
        "resolve/import must transfer the same GPU texture, not copy pixels"
    );
    assert_eq!(
        renderer.exposure_texture_bytes(),
        frame.target().texture_bytes()
    );
    drop(frame);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_nested_source_budget_refusal_is_terminal_and_explicit_new_task_recovers() {
    let project = fixture(false);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let normal = renderer.memory_policy();
    renderer
        .set_memory_policy(motion_render::resource_policy::MemoryPolicy {
            source_texture_budget_bytes: 64,
            ..normal
        })
        .unwrap();
    let mut frame = task(&mut renderer, &project, 6.5, false);
    let deadline = Instant::now() + Duration::from_secs(15);
    let error = loop {
        if let Err(error) = frame.poll(&mut renderer, Duration::from_millis(8), |_, _| Ok(true)) {
            break error;
        }
        assert!(Instant::now() < deadline);
    };
    assert!(error.to_string().contains("nested_install"), "{error}");
    assert!(
        error.to_string().contains("composition=comp-leaf")
            && error.to_string().contains("sample_frame=13"),
        "{error}"
    );
    assert!(matches!(
        error.resource_rejection(),
        Some(motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::SourceTextures,
            ..
        })
    ));
    assert_eq!(frame.progress().stage, ExposureStage::Failed);
    assert_eq!(
        renderer.exposure_texture_bytes(),
        frame.target().texture_bytes()
    );
    renderer.set_memory_policy(normal).unwrap();
    for _ in 0..60 {
        assert!(frame
            .poll(&mut renderer, Duration::from_millis(8), |_, _| panic!(
                "terminal task retried"
            ))
            .is_err());
    }
    assert!(renderer.capture_exposure(frame.target()).is_err());
    assert!(frame
        .poll(&mut renderer, Duration::ZERO, |_, _| panic!(
            "terminal task retried"
        ))
        .unwrap_err()
        .to_string()
        .contains("nested_install"));
    drop(frame);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut replacement = task(&mut renderer, &project, 6.5, false);
    finish(&mut renderer, &mut replacement);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn nested_effects_parent_matte_adjustment_preview_and_bgra_match_readback_oracle() {
    let mut project = fixture(false);
    let package = motion_effects::builtin::package().unwrap();
    let effect = |name: &str, id| {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|effect| effect.id == name)
            .unwrap();
        EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        )
    };
    project.compositions[0].layers[0]
        .effects
        .push(effect("gaussian_blur", 1));
    let mut matte = Layer::solid(
        9,
        "hidden matte",
        [52.; 2],
        [32., 32., 0.],
        [1., 1., 1., 0.8],
    );
    matte.visible = false;
    project.layers[0].track_matte = Some(motion_core::compositing::TrackMatte {
        source: 9,
        mode: motion_core::compositing::MatteMode::Alpha,
        hide_source: true,
    });
    project.layers[0].sampling.motion_blur = Some(Shutter {
        frames: 1.,
        samples: 2,
    });
    project.layers.push(matte);
    let mut adjustment = Layer::solid(10, "center adjustment", [64.; 2], [32., 32., 0.], [1.; 4]);
    adjustment.content = Content::Adjustment;
    adjustment.effects.push(effect("tint", 2));
    project.layers.push(adjustment);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut bgra = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ))
    .unwrap();
    let mut rgba = pollster::block_on(Renderer::headless()).unwrap();
    for preview in [false, true] {
        let mut frame = task(&mut bgra, &project, 6.5, preview);
        finish(&mut bgra, &mut frame);
        let actual = bgra.capture_exposure(frame.target()).unwrap();
        let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
        let expected = oracle(
            &mut rgba,
            &document,
            &project.composition_id,
            6.5,
            preview,
            &mut 3000,
        );
        close(&actual, &expected, 2);
        let mut reference = task(&mut rgba, &project, 6.5, preview);
        finish(&mut rgba, &mut reference);
        close(
            &actual,
            &rgba.capture_exposure(reference.target()).unwrap(),
            1,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn child_resolve_is_charged_to_shared_temporal_budget_before_allocation() {
    let mut project = fixture(false);
    project.compositions[0].width = 2048;
    project.compositions[0].height = 2048;
    project.compositions[0].camera = Camera::new(2048, 2048);
    project.compositions[0].layers.truncate(1);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut frame = task(&mut renderer, &project, 6.5, false);
    let deadline = Instant::now() + Duration::from_secs(20);
    let error = loop {
        if let Err(error) = frame.poll(&mut renderer, Duration::from_millis(8), |_, _| Ok(true)) {
            break error;
        }
        assert!(Instant::now() < deadline);
    };
    assert!(
        matches!(
            error.resource_rejection(),
            Some(motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::ExposureTargets,
                ..
            })
        ),
        "{error}"
    );
    assert!(error.to_string().contains("nested_exposure"), "{error}");
    assert_eq!(frame.progress().stage, ExposureStage::Failed);
    assert_eq!(
        renderer.exposure_texture_bytes(),
        frame.target().texture_bytes()
    );
    assert_eq!(
        frame.texture_resource_ledger().total_known_bytes(),
        frame.target().texture_bytes()
    );
    drop(frame);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dropping_parent_cancels_all_pending_nested_gpu_owners_without_restarting_work() {
    let project = fixture(true);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut frame = task(&mut renderer, &project, 6.5, false);
    let mut calls = 0;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(matches!(
            frame
                .poll(&mut renderer, Duration::from_millis(8), |_, scene| {
                    assert_eq!(scene.composition_id, "comp-leaf");
                    calls += 1;
                    Ok(false)
                })
                .unwrap(),
            ExposurePoll::Pending(_)
        ));
        if calls > 0 {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    assert!(renderer.exposure_texture_bytes() >= frame.target().texture_bytes() * 3);
    assert_eq!(
        frame.texture_resource_ledger().total_known_bytes(),
        renderer.exposure_texture_bytes()
    );
    let progress = frame.progress();
    for _ in 0..60 {
        frame
            .poll(&mut renderer, Duration::from_millis(8), |_, _| Ok(false))
            .unwrap();
        assert_eq!(frame.progress(), progress);
    }
    assert!(renderer.capture_exposure(frame.target()).is_err());
    drop(frame);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let mut replacement = task(&mut renderer, &project, 6.5, false);
    finish(&mut renderer, &mut replacement);
    drop(replacement);
    assert_eq!(renderer.exposure_texture_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn all_recursive_scopes_share_one_poll_budget_and_tiny_slices_eventually_complete() {
    let project = fixture(true);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut expected = None;
    for slice in [Duration::from_secs(1), Duration::from_nanos(1)] {
        let mut frame = task(&mut renderer, &project, 6.5, false);
        let mut polls = 0;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let previous = frame.progress().work_units;
            let mut calls = 0;
            let result = frame
                .poll(&mut renderer, slice, |_, _| {
                    calls += 1;
                    Ok(true)
                })
                .unwrap();
            let completed = frame.progress().work_units - previous;
            assert!(
                completed <= 128 && calls <= 128,
                "recursive scopes received separate budgets"
            );
            if slice.as_nanos() == 1 {
                assert!(completed <= 1 && calls <= 1);
            }
            polls += 1;
            if matches!(result, ExposurePoll::Ready(_)) {
                break;
            }
            assert!(Instant::now() < deadline && polls < 2000);
        }
        assert!(frame.progress().work_units > 128);
        assert!(polls > 1);
        let actual = renderer.capture_exposure(frame.target()).unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected);
        } else {
            expected = Some(actual);
        }
    }
}
