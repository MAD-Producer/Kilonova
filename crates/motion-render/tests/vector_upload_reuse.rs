use motion_core::{
    vector::{groups::*, *},
    Content, Layer, Project, Track,
};
use motion_render::{Renderer, Scene};

fn group(color: [f32; 4]) -> VectorContent {
    let source = VectorGroup::wrap(VectorContent::shape(ShapeKind::Rectangle), [16.; 2]);
    let VectorSource::Group { group } = source.source else {
        unreachable!()
    };
    let mut child = *group;
    child.id = 5;
    if let GroupItem::Geometry { id, vector, .. } = &mut child.items[0] {
        *id = 6;
        vector.fill = Some(Track::constant(color));
    }
    child.items.push(GroupItem::Fill {
        id: 7,
        name: "Fill".into(),
        color: Track::constant(color),
        fill_rule: FillRule::NonZero,
        composite: Composite::Below,
    });
    child.transform.opacity.value = 50.;
    let root = VectorGroup {
        id: 1,
        name: "Root".into(),
        transform: Default::default(),
        items: vec![GroupItem::Group {
            group: Box::new(child),
        }],
    };
    VectorContent {
        source: VectorSource::Group {
            group: Box::new(root),
        },
        fill: None,
        stroke: None,
        trim: None,
        fill_rule: FillRule::NonZero,
    }
}
fn fixture() -> Project {
    let mut project = Project::new(96, 64, 30, 60).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.background = [0.; 4];
    for (id, x, color) in [
        (1, 16., [1., 0., 0., 1.]),
        (2, 48., [0., 1., 0., 1.]),
        (3, 80., [0., 0., 1., 1.]),
    ] {
        let mut layer = Layer::solid(id, "Vector", [16.; 2], [x, 32., 0.], [1.; 4]);
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        vector.fill = Some(Track::constant(color));
        if id != 1 {
            vector = group(color);
        }
        layer.content = Content::Vector { vector };
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}
fn capture(renderer: &mut Renderer, project: &Project) -> Vec<u8> {
    let mut scene = Scene::new(project);
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    renderer.capture(&scene, &target).unwrap().0
}
fn opacity(project: &mut Project, index: usize, value: f32) {
    let Content::Vector { vector } = &mut project.layers[index].content else {
        unreachable!()
    };
    let VectorSource::Group { group } = &mut vector.source else {
        unreachable!()
    };
    group.transform.opacity.value = value;
}

fn native_vector_bytes(renderer: &Renderer) -> u64 {
    renderer.texture_resource_ledger().resources().filter(|t| t.references.iter().any(|r|
        matches!(r.category, motion_render::TextureCategory::VectorSources
            | motion_render::TextureCategory::VectorRasterScratch
            | motion_render::TextureCategory::VectorGroupScratch)))
        .map(|t| t.logical_bytes.unwrap()).sum()
}

#[test]
fn grouped_vector_buffers_share_admission_and_release_after_real_capture() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let mut project = fixture();
    capture(&mut renderer, &project);
    let first = renderer.vector_upload_info().unwrap();
    assert!(first.vertex_capacity_bytes > 0 && first.opacity_capacity_bytes > 0 && first.idle_bytes > 0);
    assert_eq!(first.pending_buffers, 0);
    // Shared totals now include actual native textures as well as buffers.
    // Measure their descriptors independently; keep exact total/release checks.
    let native = native_vector_bytes(&renderer);
    assert!(native > 0);
    assert_eq!(budget.info().unwrap().admitted_bytes,
        motion_render::FIXED_RENDERER_ALLOCATION_BYTES + first.vertex_capacity_bytes + first.opacity_capacity_bytes + first.idle_bytes + native);
    opacity(&mut project, 1, 25.);
    let pixels = capture(&mut renderer, &project);
    assert!(pixels[(32 * 96 + 48) * 4 + 3].abs_diff(32) <= 2);
    let next = renderer.vector_upload_info().unwrap();
    assert_eq!(next.destination_buffers_created_total, first.destination_buffers_created_total);
    assert_eq!(next.staging_buffers_created_total, first.staging_buffers_created_total);
    assert_eq!(native_vector_bytes(&renderer), native);
    assert_eq!(budget.info().unwrap().admitted_bytes,
        motion_render::FIXED_RENDERER_ALLOCATION_BYTES + next.vertex_capacity_bytes + next.opacity_capacity_bytes + next.idle_bytes + native);
    renderer.clear_assets();
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, motion_render::FIXED_RENDERER_ALLOCATION_BYTES);
    drop(renderer);
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}

#[test]
fn refused_vector_reupload_keeps_completed_output_and_recovers_real_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget().clone();
    let mut project = fixture();
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let original = renderer.capture(&scene, &target).unwrap().0;
    // Keep the admitted source/raster textures warm and force a larger vertex
    // upload, preserving this test's original vertex refusal and recovery site.
    project.layers[0].content = Content::Vector {
        vector: VectorContent::shape(ShapeKind::Plus),
    };
    let created = renderer.vector_upload_info().unwrap().destination_buffers_created_total;
    let limit = budget.info().unwrap().admitted_bytes;
    budget.set_limit(limit).unwrap();
    opacity(&mut project, 1, 25.);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator).unwrap();
    let failure = renderer.capture(&scene, &target).unwrap_err();
    assert!(matches!(failure.resource_rejection(), Some(motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::RenderAllocations,
        required_bytes, budget_bytes,
    }) if *budget_bytes == limit && *required_bytes > limit));
    for context in ["vector layer=1 frame=0 phase=vertices", "phase=allocate label=reusable vector raster triangles"] {
        assert!(failure.to_string().contains(context), "{failure}");
    }
    assert_eq!(renderer.vector_upload_info().unwrap().destination_buffers_created_total, created);
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    assert!(renderer.gpu_error().is_none());
    budget.set_limit(8 << 20).unwrap();
    let recovered = renderer.capture(&scene, &target).unwrap().0;
    assert_ne!(recovered, original);
    assert!(recovered[(32 * 96 + 48) * 4 + 3].abs_diff(32) <= 2);
    assert_eq!(renderer.vector_upload_info().unwrap().pending_buffers, 0);
}

#[test]
fn changing_vertex_data_and_opacity_reuses_buffers_without_cross_layer_overwrites() {
    let mut project = fixture();
    opacity(&mut project, 1, 25.);
    opacity(&mut project, 2, 75.);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    eprintln!("vector upload adapter: {:?}", renderer.adapter_info);
    capture(&mut renderer, &project);
    let warm = renderer.vector_upload_info().unwrap();
    let texture_bytes = renderer.texture_bytes();
    assert_eq!(warm.destination_buffers_created_total, 3); // vertex grows once from flat to grouped; one opacity buffer
    eprintln!(
        "warm vector uploads: {}",
        serde_json::to_string(&warm).unwrap()
    );
    assert_eq!(warm.pending_buffers, 0);
    capture(&mut renderer, &project);
    assert_eq!(
        renderer
            .vector_upload_info()
            .unwrap()
            .encoded_upload_bytes_total,
        warm.encoded_upload_bytes_total
    );
    for (iteration, alpha) in [40., 75., 25., 90., 40.].into_iter().enumerate() {
        opacity(&mut project, 1, alpha);
        opacity(&mut project, 2, 100. - alpha);
        let Content::Vector { vector } = &mut project.layers[0].content else {
            unreachable!()
        };
        vector.fill = Some(Track::constant(if iteration % 2 == 0 {
            [1., 0., 0., 1.]
        } else {
            [1., 1., 0., 1.]
        }));
        let pixels = capture(&mut renderer, &project);
        let at = |x: usize| &pixels[(32 * 96 + x) * 4..(32 * 96 + x) * 4 + 4];
        assert_eq!(at(16)[0], 255);
        assert_eq!(at(16)[1], if iteration % 2 == 0 { 0 } else { 255 });
        assert_eq!(at(16)[2], 0);
        assert_eq!(at(16)[3], 255);
        assert_eq!(at(48)[0], 0);
        assert!(at(48)[1] >= 250);
        assert_eq!(at(48)[2], 0);
        assert!(at(48)[3].abs_diff((alpha * 0.5 * 2.55).round() as u8) <= 2);
        assert_eq!(at(80)[0], 0);
        assert_eq!(at(80)[1], 0);
        assert!(at(80)[2] >= 250);
        assert!(at(80)[3].abs_diff(((100. - alpha) * 0.5 * 2.55).round() as u8) <= 2);
        let info = renderer.vector_upload_info().unwrap();
        assert_eq!(
            info.destination_buffers_created_total,
            warm.destination_buffers_created_total
        );
        assert_eq!(
            info.staging_buffers_created_total,
            warm.staging_buffers_created_total
        );
        assert_eq!(info.pending_buffers, 0);
        assert!(info.encoded_upload_bytes_total > warm.encoded_upload_bytes_total);
        assert_eq!(renderer.texture_bytes(), texture_bytes);
    }
    eprintln!(
        "steady vector uploads: {}",
        serde_json::to_string(&renderer.vector_upload_info().unwrap()).unwrap()
    );
    renderer.clear_assets();
    let released = renderer.vector_upload_info().unwrap();
    assert_eq!(
        (
            released.vertex_capacity_bytes,
            released.opacity_capacity_bytes,
            released.idle_bytes
        ),
        (0, 0, 0)
    );
    assert!(released.destination_buffers_created_total >= warm.destination_buffers_created_total);
}

#[test]
fn distinct_encodes_in_one_submission_keep_their_own_vector_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut project = fixture();
    let target = || {
        renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 96,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let first = target();
    let second = target();
    let first_view = first.create_view(&Default::default());
    let second_view = second.create_view(&Default::default());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let mut scene = Scene::new(&project);
    opacity(&mut project, 1, 25.);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer
        .encode(&scene, &first_view, 96, 64, &mut encoder, None)
        .unwrap();
    // Copy both results before readback, independently of the renderer's cached
    // vector textures which deliberately change during the second encode.
    let stride = 512u32;
    let bytes = u64::from(stride) * 64;
    let output = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytes * 2,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        first.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    opacity(&mut project, 1, 75.);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer
        .encode(&scene, &second_view, 96, 64, &mut encoder, None)
        .unwrap();
    encoder.copy_texture_to_buffer(
        second.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: bytes,
                bytes_per_row: Some(stride),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    output
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let pixels = output.slice(..).get_mapped_range().unwrap();
    let alpha = (32 * stride + 48 * 4 + 3) as usize;
    assert!(pixels[alpha].abs_diff(32) <= 2);
    assert!(pixels[bytes as usize + alpha].abs_diff(96) <= 2);
    assert_eq!(renderer.vector_upload_info().unwrap().pending_buffers, 0);
}
