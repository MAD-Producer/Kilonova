//! Actual vector source and raster target capacity, independent of buffer deltas.
use super::*;
use motion_core::vector::{groups::VectorGroup, ShapeKind, VectorContent, VectorSource};

fn vector(grouped: bool, size: f32) -> VectorContent {
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    vector.fill = Some(motion_core::Track::constant([0., 1., 0., 1.]));
    if grouped {
        vector = VectorGroup::wrap(vector, [size; 2]);
        let VectorSource::Group { group } = &mut vector.source else { unreachable!() };
        // Align half opacity to the actual RGBA8 alpha value. Exactly0.5 is
        // between127/255 and128/255; two sRGB round trips can then put straight
        // green at254 on software Vulkan. Keep every pixel assertion exact.
        group.transform.opacity.value = 100. * (128. / 255.);
    }
    vector
}

fn project(grouped: bool) -> Project {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(77, "vector admission", [32.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Vector { vector: vector(grouped, 32.) };
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

fn resize(project: &mut Project, size: f32) {
    for layer in &mut project.layers {
        let Content::Vector { vector: old } = &layer.content else { unreachable!() };
        let grouped = matches!(old.source, VectorSource::Group { .. });
        layer.size = [size; 2];
        // Wrapping freezes geometry into paths; change their actual geometry,
        // rather than only resizing the containing layer.
        layer.content = Content::Vector { vector: vector(grouped, size) };
    }
}

fn sampled(project: &Project) -> Scene {
    let mut scene = Scene::new(project);
    scene.sample(project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    scene
}

fn meshes(renderer: &mut Renderer, scene: &Scene) -> Vec<crate::vector_mesh::VectorMesh> {
    renderer.effect_gpu.builder.build(scene, &renderer.asset_order, 64, 64, false).unwrap().vectors.clone()
}

fn prepare(renderer: &mut Renderer, scene: &Scene, meshes: &[crate::vector_mesh::VectorMesh]) -> Result<bool> {
    renderer.layer_gpu.prepare_textures(&renderer.device, &renderer.image_layout, &renderer.sampler,
        scene, meshes, &mut renderer.images, &mut renderer.texture_bytes, 0, 128 << 20)
}

#[test]
fn native_vector_sources_msaa_and_group_targets_join_combined_capacity_and_release() {
    for grouped in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(64, 64).unwrap();
        let before = budget.info().unwrap();
        let old_ids = renderer.texture_resource_ledger().resources().map(|t| t.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let pixels = renderer.capture(&sampled(&project(grouped)), &target).unwrap().0;
        let ledger = renderer.texture_resource_ledger();
        let textures = ledger.resources().filter(|t| !old_ids.contains(&t.id))
            .map(|t| t.logical_bytes.unwrap()).sum::<u64>();
        let vector_bytes = ledger.resources().filter(|t| t.references.iter().any(|r|
            matches!(r.category, crate::TextureCategory::VectorSources
                | crate::TextureCategory::VectorRasterScratch | crate::TextureCategory::VectorGroupScratch)))
            .map(|t| t.logical_bytes.unwrap()).sum::<u64>();
        assert!(vector_bytes >= 32 * 32 * 24);
        let uploads = renderer.vector_upload_info().unwrap();
        let buffers = uploads.vertex_capacity_bytes + uploads.opacity_capacity_bytes + uploads.idle_bytes;
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes + textures + buffers);
        let center = (32 * 64 + 32) * 4;
        assert_eq!(&pixels[center..center + 3], &[0, 255, 0]);
        assert!(pixels[center + 3].abs_diff(if grouped { 128 } else { 255 }) <= 1);
        assert_eq!(pixels[3], 0);
        renderer.check_health().unwrap();
        drop(target);
        drop(renderer);
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        assert_eq!(budget.info().unwrap().live_allocations, 0);
    }
}

#[test]
fn every_vector_texture_cut_point_preserves_old_outputs_pools_bytes_and_submitted_pixels() {
    for grouped in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(64, 64).unwrap();
        let mut project = project(grouped);
        let scene = sampled(&project);
        let original = renderer.capture(&scene, &target).unwrap().0;
        let old_meshes = meshes(&mut renderer, &scene);
        let ledger = renderer.texture_resource_ledger();
        let bytes = renderer.texture_bytes;
        let before = budget.info().unwrap();
        resize(&mut project, 64.);
        let changed = sampled(&project);
        let changed_meshes = meshes(&mut renderer, &changed);
        let mesh = &changed_meshes[0];
        let pixel_bytes = u64::from(mesh.width) * u64::from(mesh.height) * 4;
        let mut steps = vec![(pixel_bytes, "source_allocate".to_owned())];
        if grouped {
            for depth in 0..=mesh.scope_depth() {
                steps.push((pixel_bytes * 4, format!("group_msaa_allocate depth={depth}")));
                steps.push((pixel_bytes, format!("group_resolve_allocate depth={depth}")));
            }
        } else {
            steps.extend([(pixel_bytes * 4, "msaa_allocate".into()), (pixel_bytes, "resolve_allocate".into())]);
        }
        let mut staged_bytes = 0;
        for (cost, phase) in steps {
            let limit = before.admitted_bytes + staged_bytes;
            budget.set_limit(limit).unwrap();
            let error = prepare(&mut renderer, &changed, &changed_meshes).unwrap_err();
            assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: limit + cost, budget_bytes: limit,
            }));
            for context in ["vector layer=77 frame=0".to_owned(), format!("phase={phase}"),
                format!("size={}x{}", mesh.width, mesh.height), "format=Rgba8Unorm".into()] {
                assert!(error.to_string().contains(&context), "missing {context}: {error}");
            }
            assert_eq!(renderer.texture_resource_ledger(), ledger);
            assert_eq!(renderer.texture_bytes, bytes);
            assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
            assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
            assert_eq!(renderer.read_target(&target).unwrap(), original);
            staged_bytes += cost;
        }
        budget.set_limit(1).unwrap();
        assert!(!prepare(&mut renderer, &scene, &old_meshes).unwrap());
        assert_eq!(renderer.texture_resource_ledger(), ledger);
        budget.set_limit(8 << 20).unwrap();
        let recovered = renderer.capture(&changed, &target).unwrap().0;
        assert_ne!(recovered, original);
        assert_eq!(&recovered[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3], &[0, 255, 0]);
        assert!(recovered[(32 * 64 + 32) * 4 + 3].abs_diff(if grouped { 128 } else { 255 }) <= 1);
        renderer.check_health().unwrap();
    }
}

#[test]
fn later_mesh_refusal_rolls_back_staged_ordinary_and_grouped_targets_together() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let target = renderer.capture_target(64, 64).unwrap();
    let mut project = project(false);
    let mut grouped = self::project(true).layers.remove(0);
    grouped.id = 78;
    project.layers.push(grouped);
    let original = renderer.capture(&sampled(&project), &target).unwrap().0;
    let ledger = renderer.texture_resource_ledger();
    let before = budget.info().unwrap();
    let bytes = renderer.texture_bytes;
    resize(&mut project, 64.);
    let changed = sampled(&project);
    let meshes = meshes(&mut renderer, &changed);
    let cost = u64::from(meshes[0].width) * u64::from(meshes[0].height) * 4;
    // All ordinary targets and both new source outputs fit; first group MSAA refuses.
    let limit = before.admitted_bytes + 7 * cost;
    budget.set_limit(limit).unwrap();
    let error = prepare(&mut renderer, &changed, &meshes).unwrap_err();
    assert!(error.to_string().contains("phase=group_msaa_allocate depth=0"), "{error}");
    assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::RenderAllocations, required_bytes: limit + cost * 4, budget_bytes: limit,
    }));
    assert_eq!(renderer.texture_resource_ledger(), ledger);
    assert_eq!(renderer.texture_bytes, bytes);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    budget.set_limit(8 << 20).unwrap();
    assert_ne!(renderer.capture(&changed, &target).unwrap().0, original);
    renderer.check_health().unwrap();
}

#[test]
fn vector_native_commands_retain_capacity_until_their_own_retirement() {
    for grouped in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let budget = renderer.allocation_budget().clone();
        let target = renderer.capture_target(64, 64).unwrap();
        let scene = sampled(&project(grouped));
        renderer.capture(&scene, &target).unwrap();
        let source = &renderer.images[&TextureKey::Vector(77)];
        let (width, height) = source.size;
        let mut copy = renderer.capture_target(width, height).unwrap();
        // The fixture receives copies; ordinary product capture targets are
        // sources and intentionally do not need COPY_DST.
        let texture = renderer.device.create_admitted_texture(&wgpu::TextureDescriptor {
            label: Some("native vector command copy fixture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }, &budget).unwrap();
        copy.view = texture.admitted_view().unwrap();
        copy.texture = texture;
        let extent = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
        encoder.copy_texture_to_texture(source._texture.as_image_copy(), copy.texture.as_image_copy(), extent);
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        let expected = renderer.read_target(&copy).unwrap();
        assert!(expected[((height / 2 * width + width / 2) * 4 + 3) as usize]
            .abs_diff(if grouped { 128 } else { 255 }) <= 1);
        renderer.clear_assets();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let before = budget.info().unwrap();
        for mode in 0..3 {
            // Complete uploads before opening the lifetime scope. Their pending
            // staging ownership is tested separately by vector_upload tests.
            renderer.capture(&scene, &target).unwrap();
            let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
            encoder.copy_texture_to_texture(renderer.images[&TextureKey::Vector(77)]._texture.as_image_copy(),
                copy.texture.as_image_copy(), extent);
            let held = budget.info().unwrap();
            assert!(held.admitted_bytes > before.admitted_bytes + 32 * 32 * 24);
            renderer.clear_assets();
            assert_eq!(budget.info().unwrap().admitted_bytes, held.admitted_bytes);
            // An encoder opened after owner retirement must not renew old work.
            let later = renderer.allocation_command_encoder(&Default::default()).unwrap();
            match mode {
                0 => drop(encoder),
                1 => drop(encoder.finish()),
                _ => { renderer.submit_allocated(encoder.finish(), |_| {}).unwrap(); }
            }
            renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
            assert_eq!(budget.info().unwrap().live_allocations, before.live_allocations);
            if mode == 2 { assert_eq!(renderer.read_target(&copy).unwrap(), expected); }
            drop(later);
        }
        renderer.check_health().unwrap();
    }
}

#[test]
fn native_vector_source_alias_survives_renderer_teardown_without_releasing_its_capacity() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let scene = sampled(&project(false));
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap();
    let source = renderer.images.remove(&TextureKey::Vector(77)).unwrap();
    renderer.texture_bytes -= source.bytes;
    let alias = source._texture.admitted_view().unwrap();
    let source_bytes = source.bytes;
    renderer.clear_assets();
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    drop(source);
    drop(target);
    drop(renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, source_bytes);
    assert_eq!(budget.info().unwrap().live_allocations, 1);
    assert_eq!(u64::from(alias.texture().width()) * u64::from(alias.texture().height()) * 4, source_bytes);
    drop(alias);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
    assert_eq!(budget.info().unwrap().live_allocations, 0);
}

#[test]
fn two_renderers_share_vector_capacity_and_failed_staging_leaves_no_reservation() {
    let budget = crate::AllocationBudget::new(16 << 20).unwrap();
    let mut first = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let mut second = pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let scene = sampled(&project(false));
    let first_meshes = meshes(&mut first, &scene);
    let second_meshes = meshes(&mut second, &scene);
    let before = budget.info().unwrap();
    let cost = u64::from(first_meshes[0].width) * u64::from(first_meshes[0].height) * 4;
    let limit = before.admitted_bytes + cost * 6;
    budget.set_limit(limit).unwrap();
    assert!(prepare(&mut first, &scene, &first_meshes).unwrap());
    assert_eq!(budget.info().unwrap().admitted_bytes, limit);
    let first_ledger = first.texture_resource_ledger();
    let second_ledger = second.texture_resource_ledger();
    let error = prepare(&mut second, &scene, &second_meshes).unwrap_err();
    assert_eq!(error.resource_rejection(), Some(&ResourceRejection::Budget {
        scope: ResourceBudgetScope::RenderAllocations, required_bytes: limit + cost, budget_bytes: limit,
    }));
    assert!(error.to_string().contains("vector layer=77 frame=0"), "{error}");
    assert!(error.to_string().contains("phase=source_allocate"), "{error}");
    assert_eq!(first.texture_resource_ledger(), first_ledger);
    assert_eq!(second.texture_resource_ledger(), second_ledger);
    assert_eq!(budget.info().unwrap().admitted_bytes, limit);
    first.clear_assets();
    first.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert!(prepare(&mut second, &scene, &second_meshes).unwrap());
    budget.set_limit(16 << 20).unwrap();
    let target = second.capture_target(64, 64).unwrap();
    let pixels = second.capture(&scene, &target).unwrap().0;
    assert_eq!(&pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4], &[0, 255, 0, 255]);
    first.check_health().unwrap(); second.check_health().unwrap();
    drop(target); drop(first); drop(second);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}
