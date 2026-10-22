//! Native retained receiver allocation controls.
use super::*;

#[test]
fn constant_receiver_native_texture_joins_combined_admission_and_releases_with_its_owner() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.camera.created = false;
    project.layers.push(Layer::solid(77, "receiver", [16.; 2], [32., 32., 0.], [1.; 4]));
    project.validate().unwrap();
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let before = budget.info().unwrap();
    let raster = renderer.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
    let [width, height] = raster.size();
    let actual_bytes = u64::from(width) * u64::from(height) * 4;
    assert_eq!([width, height], [1, 1]);
    assert_eq!(actual_bytes, 4);
    assert_eq!(raster.texture_bytes(), actual_bytes);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + actual_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 1);
    drop(raster);
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    renderer.check_health().unwrap();
}

fn drain(renderer: &Renderer) {
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.submit_tracked(encoder.finish(), |_| {});
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn image_scene(renderer: &mut Renderer, matte: bool) -> Scene {
    renderer.upload_image(7, 32, 32, &[0, 255, 0, 255].repeat(32 * 32)).unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.background = [0.; 4];
    project.camera.created = false;
    project.assets.push(Asset { id: 7, path: "assets/retained-fixture.png".into(), width: 32, height: 32 });
    project.layers = vec![Layer::solid(78, "receiver", [32.; 2], [32., 32., 0.], [1.; 4])];
    project.layers[0].content = Content::Image { asset: 7 };
    if matte {
        project.layers.insert(0, Layer::solid(77, "coverage", [32.; 2], [32., 32., 0.], [1.; 4]));
        project.layers[0].visible = false;
        project.layers[1].track_matte = Some(motion_core::compositing::TrackMatte {
            source: 77, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
        });
    }
    project.validate().unwrap();
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

fn draw(renderer: &mut Renderer, scene: &Scene, raster: &LayerRaster, target: &CaptureTarget) -> Vec<u8> {
    renderer.draw_layer_rasters(scene, &[raster], &target.view, 64, 64).unwrap();
    renderer.read_target(target).unwrap()
}

#[test]
fn local_image_and_matte_cut_points_rollback_both_budgets_and_keep_previous_pixels() {
    for matte in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let scene = image_scene(&mut renderer, matte);
        let old = renderer.materialize_layer(&scene, 78, 64, 64, false).unwrap().unwrap().0;
        assert_eq!(old.size(), [32, 32]);
        assert_eq!(old.texture_bytes(), if matte { 8192 } else { 4096 });
        let target = renderer.capture_target(64, 64).unwrap();
        let pixels = draw(&mut renderer, &scene, &old, &target);
        let center = (32 * 64 + 32) * 4;
        assert_eq!(&pixels[center..center + 4], &[0, 255, 0, 255]);
        assert_eq!(pixels[3], 0);
        // Drawing retained coverage releases the materialization working mattes.
        // Rewarm that earlier stage before cutting the independent local copy.
        let working = renderer.materialize_layer(&scene, 78, 64, 64, false).unwrap().unwrap().0;
        drop(working);
        drain(&renderer);
        let category = renderer.exposure_texture_bytes();
        let budget = renderer.allocation_budget().clone();
        let before = budget.info().unwrap();
        for slot in 0..if matte { 2 } else { 1 } {
            let limit = before.admitted_bytes + slot * 4096;
            budget.set_limit(limit).unwrap();
            let error = renderer.materialize_layer(&scene, 78, 64, 64, false).err().unwrap();
            assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: limit + 4096, budget_bytes: limit,
            }));
            let phase = if slot == 0 { "phase=local_image_allocate" } else { "phase=local_matte_allocate" };
            for context in ["composition=comp-main frame=0 object=78", phase, if slot == 0 { "size=32x32" } else { "size=64x64" }] {
                assert!(error.to_string().contains(context), "missing {context}: {error}");
            }
            assert_eq!(renderer.exposure_texture_bytes(), category);
            assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
            assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
            assert_eq!(old.size(), [32, 32]);
            assert_eq!(renderer.read_target(&target).unwrap(), pixels);
        }
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.materialize_layer(&scene, 78, 64, 64, false).unwrap().unwrap().0;
        assert_eq!(recovered.size(), old.size());
        assert_eq!(draw(&mut renderer, &scene, &recovered, &target), pixels);
        assert_eq!(renderer.exposure_texture_bytes(), category + old.texture_bytes());
        drop(recovered);
        let working = renderer.materialize_layer(&scene, 78, 64, 64, false).unwrap().unwrap().0;
        drop(working);
        drain(&renderer);
        assert_eq!(renderer.exposure_texture_bytes(), category);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        renderer.check_health().unwrap();
    }
}

#[test]
fn retained_receiver_capacity_is_shared_across_renderers_and_refusal_restores_category_count() {
    let budget = crate::AllocationBudget::new(16 << 20).unwrap();
    let mut first = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let mut second = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let probe = first.capture_target(1, 1).unwrap();
    assert_eq!(probe.texture.format(), wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.layers.push(Layer::solid(77, "receiver", [16.; 2], [32., 32., 0.], [1.; 4]));
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    drain(&first);
    drain(&second);
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes + 4).unwrap();
    let retained = first.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
    let error = second.materialize_layer(&scene, 77, 64, 64, false).err().unwrap();
    assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::RenderAllocations,
        required_bytes: before.admitted_bytes + 8, budget_bytes: before.admitted_bytes + 4,
    }));
    assert!(error.to_string().contains("size=1x1 format=Rgba8UnormSrgb phase=local_constant_allocate"), "{error}");
    assert_eq!(first.exposure_texture_bytes(), 4);
    assert_eq!(second.exposure_texture_bytes(), 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 4);
    drop(retained);
    drain(&first);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    let recovered = second.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
    assert_eq!(recovered.texture_bytes(), 4);
    drop(recovered);
    drain(&second);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    first.check_health().unwrap();
    second.check_health().unwrap();
}

#[test]
fn retained_local_image_and_matte_commands_keep_both_allocations_until_their_own_retirement() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = image_scene(&mut renderer, true);
    let warm = renderer.materialize_layer(&scene, 78, 64, 64, false).unwrap().unwrap().0;
    drop(warm);
    drain(&renderer);
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    for mode in 0..3 {
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        let raster = renderer.copy_layer_raster(&scene, 78, [64, 64], [64, 64], false, &mut encoder).unwrap();
        assert_eq!(raster.texture_bytes(), 8192);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 8192);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations + 2);
        enum Work { Encoding(crate::AllocationEncoder), Finished(crate::AllocatedCommandBuffer) }
        let work = if mode == 0 { Work::Encoding(encoder) } else { Work::Finished(encoder.finish()) };
        drop(raster);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + 8192);
        let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
        match work {
            Work::Encoding(encoder) => drop(encoder),
            Work::Finished(buffer) if mode == 2 => { renderer.submit_allocated(buffer, |_| {}).unwrap(); drain(&renderer); },
            Work::Finished(buffer) => drop(buffer),
        }
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
        drop(later);
    }
    renderer.check_health().unwrap();
}

#[test]
fn retained_native_receiver_outlives_renderer_without_releasing_its_capacity_early() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let device = renderer.device.clone();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.layers.push(Layer::solid(77, "receiver", [16.; 2], [32., 32., 0.], [1.; 4]));
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let raster = renderer.materialize_layer(&scene, 77, 64, 64, false).unwrap().unwrap().0;
    drain(&renderer);
    drop(renderer);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(raster.size(), [1, 1]);
    assert_eq!(budget.info().unwrap().admitted_bytes, 4);
    assert_eq!(budget.info().unwrap().live_allocations, 1);
    drop(raster);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
}
