use super::*;
use motion_effects::image_contract::ChannelPrecision;

fn context() -> CompositionFrameContext {
    CompositionFrameContext {
        document_generation: 1,
        device_generation: 2,
        content: [3; 32],
        output: [4; 32],
    }
}
fn key(source: &RenderTarget, frame: f64) -> CompositionFrameKey {
    CompositionFrameKey::new(
        context(),
        frame,
        source.width,
        source.height,
        source.texture().format(),
    )
    .unwrap()
}
fn clear(renderer: &Renderer, source: &RenderTarget, color: wgpu::Color) {
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("composition cache control"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &source.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
}
fn read(renderer: &Renderer, target: &RenderTarget) -> Vec<u8> {
    let row = target.width * target.texture().format().block_copy_size(None).unwrap();
    let stride =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = crate::allocation_buffer::AllocationBuffer::new(
        &renderer.device,
        renderer.allocation_budget(),
        &wgpu::BufferDescriptor {
            label: Some("cache native oracle"),
            size: u64::from(stride) * u64::from(target.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
    )
    .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        target.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(target.height),
            },
        },
        wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = mapped
        .chunks_exact(stride as usize)
        .flat_map(|r| r[..row as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    renderer.check_health().unwrap();
    pixels
}

#[test]
fn cache_keys_preserve_fractional_time_native_format_and_all_context_components() {
    let make = |ctx, f, format| CompositionFrameKey::new(ctx, f, 4, 4, format);
    let format = wgpu::TextureFormat::Rgba16Float;
    let original = make(context(), 0.25, format).unwrap();
    assert_eq!(original.frame(), 0.25);
    assert_ne!(
        original,
        make(context(), f64::from_bits(0.25f64.to_bits() + 1), format).unwrap()
    );
    assert_eq!(
        make(context(), -0., format).unwrap(),
        make(context(), 0., format).unwrap()
    );
    for f in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(make(context(), f, format).is_err());
    }
    for index in 0..4 {
        let mut ctx = context();
        match index {
            0 => ctx.document_generation += 1,
            1 => ctx.device_generation += 1,
            2 => ctx.content[0] += 1,
            _ => ctx.output[0] += 1,
        }
        assert_ne!(original, make(ctx, 0.25, format).unwrap());
    }
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba16Unorm,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        assert_ne!(original, make(context(), 0.25, format).unwrap());
    }
    assert!(CompositionFrameKey::new(context(), 0., 0, 4, format).is_err());
    assert!(make(context(), 0., wgpu::TextureFormat::Depth32Float).is_err());
    assert!(CompositionFrameCache::new(context(), 1024, 9).is_err());
}

fn native_copy_control(precision: ChannelPrecision) {
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        precision,
        crate::AllocationBudget::default(),
    ))
    .unwrap();
    println!(
        "native cache precision={precision:?} adapter={:?}",
        renderer.adapter_info
    );
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        _ => {}
    }
    let source = renderer.render_target(4, 4).unwrap();
    let color = wgpu::Color {
        r: 2.5,
        g: -0.5,
        b: 0.125,
        a: 0.5,
    };
    clear(&renderer, &source, color);
    let expected = read(&renderer, &source);
    let mut cache = CompositionFrameCache::new(context(), 4096, 2).unwrap();
    let first = key(&source, 0.25);
    assert!(cache.get(&renderer, first).unwrap().is_none());
    assert!(cache.store(&renderer, first, &source).unwrap());
    clear(&renderer, &source, wgpu::Color::BLUE);
    assert_ne!(read(&renderer, &source), expected);
    let held = cache.get(&renderer, first).unwrap().unwrap();
    assert_eq!(held.format(), source.texture().format());
    assert_eq!(read(&renderer, &held.target), expected);
    match precision {
        ChannelPrecision::Float32 => {
            assert_eq!(f32::from_le_bytes(expected[0..4].try_into().unwrap()), 2.5);
            assert_eq!(f32::from_le_bytes(expected[4..8].try_into().unwrap()), -0.5);
            assert_eq!(
                f32::from_le_bytes(expected[12..16].try_into().unwrap()),
                0.5
            );
        }
        ChannelPrecision::Float16 => {
            assert_eq!(
                u16::from_le_bytes(expected[0..2].try_into().unwrap()),
                0x4100
            );
            assert_eq!(
                u16::from_le_bytes(expected[2..4].try_into().unwrap()),
                0xb800
            );
            assert_eq!(
                u16::from_le_bytes(expected[6..8].try_into().unwrap()),
                0x3800
            );
        }
        _ => {}
    }
    let mut changed = context();
    changed.content[0] += 1;
    cache.invalidate(changed);
    assert!(!cache.store(&renderer, first, &source).unwrap());
    assert!(cache.get(&renderer, first).unwrap().is_none());
    assert_eq!(cache.info().stale, 1);
    assert_eq!(read(&renderer, &held.target), expected);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_copies_independent_pixels_and_keeps_half_float_hdr_and_alpha() {
    for precision in [ChannelPrecision::Unorm8, ChannelPrecision::Float16] {
        native_copy_control(precision);
    }
}

#[test]
#[ignore = "manual native four-precision cache storage checkpoint; requires optional adapter features"]
fn all_four_native_cache_precisions_preserve_exact_storage_without_output_conversion() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        native_copy_control(precision);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_lru_quota_disable_layout_and_foreign_owner_controls() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer.render_target(4, 4).unwrap();
    clear(&renderer, &source, wgpu::Color::RED);
    let keys = [key(&source, 1.), key(&source, 2.), key(&source, 3.)];
    let mut cache = CompositionFrameCache::new(context(), 128, 2).unwrap();
    assert!(cache.store(&renderer, keys[0], &source).unwrap());
    assert!(cache.store(&renderer, keys[1], &source).unwrap());
    let held = cache.get(&renderer, keys[0]).unwrap().unwrap();
    assert!(cache.store(&renderer, keys[2], &source).unwrap());
    assert!(cache.get(&renderer, keys[1]).unwrap().is_none());
    assert!(cache.get(&renderer, keys[0]).unwrap().is_some());
    assert_eq!(cache.info().resident_bytes, 128);
    assert_eq!(cache.info().evicted, 1);
    let invalid = CompositionFrameKey::new(context(), 4., 8, 4, source.texture().format()).unwrap();
    assert!(cache.store(&renderer, invalid, &source).is_err());
    assert_eq!(cache.info().resident_frames, 2);
    assert!(cache.set_limits(64, 9).is_err());
    assert_eq!(cache.info().resident_frames, 2);
    let foreign = pollster::block_on(Renderer::headless()).unwrap();
    assert!(cache.get(&foreign, keys[0]).is_err());
    assert!(cache.store(&foreign, keys[0], &source).is_err());
    cache.set_limits(0, 0).unwrap();
    assert_eq!(cache.info().resident_bytes, 0);
    assert!(!cache.store(&renderer, keys[0], &source).unwrap());
    assert_eq!(read(&renderer, &held.target)[0], 255);
    cache.set_limits(64, 1).unwrap();
    assert!(cache.store(&renderer, keys[0], &source).unwrap());
    let mut changed = context();
    changed.device_generation += 1;
    cache.invalidate(changed);
    let new_source = foreign.render_target(4, 4).unwrap();
    clear(&foreign, &new_source, wgpu::Color::GREEN);
    let new_key =
        CompositionFrameKey::new(changed, 0., 4, 4, new_source.texture().format()).unwrap();
    assert!(cache.store(&foreign, new_key, &new_source).unwrap());
    // An ordinary late completion from the old device is not a foreign-owner fault.
    assert!(!cache.store(&renderer, keys[0], &source).unwrap());
    assert!(cache.get(&renderer, keys[0]).unwrap().is_none());
    assert!(cache.get(&foreign, new_key).unwrap().is_some());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_shared_admission_refusal_and_evicted_consumer_leases_retire_honestly() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer.render_target(4, 4).unwrap();
    clear(&renderer, &source, wgpu::Color::RED);
    let expected = read(&renderer, &source);
    let budget = renderer.allocation_budget();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let baseline = budget.info().unwrap().admitted_bytes;
    let old_limit = budget.info().unwrap().budget_bytes;
    budget.set_limit(baseline).unwrap();
    let mut cache = CompositionFrameCache::new(context(), 64, 1).unwrap();
    let k = key(&source, 0.);
    let failure = cache.store(&renderer, k, &source).unwrap_err();
    assert!(failure.resource_rejection().is_some(), "{failure}");
    assert_eq!(cache.info().refused, 1);
    assert_eq!(cache.info().resident_frames, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline);
    budget.set_limit(old_limit).unwrap();
    assert_eq!(read(&renderer, &source), expected);
    assert!(cache.store(&renderer, k, &source).unwrap());
    let held = cache.get(&renderer, k).unwrap().unwrap();
    cache.clear();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline + 64);
    // This is an actual encoded consumer, not a fabricated cache-only count.
    let buffer = crate::allocation_buffer::AllocationBuffer::new(
        &renderer.device,
        budget,
        &wgpu::BufferDescriptor {
            label: Some("evicted cache consumer"),
            size: 1024,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
    )
    .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        held.target.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    drop(held);
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline + 64 + 1024);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline + 1024);
    drop(buffer);
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_rejects_mutated_public_layout_before_allocating_or_submitting() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut source = renderer.render_target(4, 4).unwrap();
    let mut cache = CompositionFrameCache::new(context(), 4096, 2).unwrap();
    let before = renderer.allocation_budget().info().unwrap();
    source.width = 8;
    let failure = cache
        .store(&renderer, key(&source, 0.), &source)
        .unwrap_err();
    assert!(
        failure.to_string().contains("native source layout"),
        "{failure}"
    );
    assert_eq!(renderer.allocation_budget().info().unwrap(), before);
    assert_eq!(cache.info().resident_frames, 0);
    renderer.check_health().unwrap();
    source.width = 4;
    clear(&renderer, &source, wgpu::Color::GREEN);
    assert!(cache.store(&renderer, key(&source, 0.), &source).unwrap());
    assert_eq!(
        read(
            &renderer,
            cache
                .get(&renderer, key(&source, 0.))
                .unwrap()
                .unwrap()
                .source()
        )[1],
        255
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_has_a_distinct_budget_category_even_after_eviction() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer.render_target(4, 4).unwrap();
    clear(&renderer, &source, wgpu::Color::RED);
    let mut cache = CompositionFrameCache::new(context(), 64, 1).unwrap();
    assert!(cache.store(&renderer, key(&source, 0.), &source).unwrap());
    let held = cache.get(&renderer, key(&source, 0.)).unwrap().unwrap();
    let mut ledger = crate::TextureResourceLedger::default();
    held.collect_texture_resources(&mut ledger);
    assert_eq!(ledger.total_known_bytes(), 64);
    assert!(ledger.resources().all(|r| r
        .references
        .iter()
        .all(|r| r.category == crate::TextureCategory::CompositionFrames)));
    let category_bytes = || {
        renderer
            .allocation_budget()
            .info()
            .unwrap()
            .categories
            .into_iter()
            .find(|c| c.category == crate::AllocationCategory::CompositionFrames)
            .unwrap()
            .admitted_bytes
    };
    cache.clear();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(cache.info().resident_bytes, 0);
    assert_eq!(category_bytes(), 64);
    drop(held);
    assert_eq!(category_bytes(), 0);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_cache_hits_and_misses_preserve_a_real_queue_validation_failure() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer.render_target(4, 4).unwrap();
    clear(&renderer, &source, wgpu::Color::RED);
    let mut cache = CompositionFrameCache::new(context(), 64, 1).unwrap();
    let cached = key(&source, 0.);
    assert!(cache.store(&renderer, cached, &source).unwrap());
    assert!(cache.get(&renderer, cached).unwrap().is_some());
    let info = cache.info();
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("composition cache native failure control"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Real wgpu validation, after a successfully submitted cache copy. Do not
    // fabricate or clear the renderer's stored cause to manufacture recovery.
    renderer.queue.write_buffer(&buffer, 4, &[0; 16]);
    let original = renderer
        .gpu_error()
        .expect("native write must fail validation");
    assert!(
        original.contains("write_buffer") && original.contains("16"),
        "{original}"
    );
    for candidate in [cached, key(&source, 0.25)] {
        let failure = match cache.get(&renderer, candidate) {
            Err(failure) => failure,
            Ok(_) => panic!("cache lookup hid the native GPU failure"),
        };
        let message = failure.to_string();
        assert!(message.contains(&original), "{message}");
        assert!(message.contains("phase=lookup"), "{message}");
        assert!(message.contains("document=1 device=2"), "{message}");
        assert!(
            message.contains(&format!("frame={} size=4x4", candidate.frame())),
            "{message}"
        );
        assert!(std::error::Error::source(&failure)
            .unwrap()
            .to_string()
            .contains(&original));
    }
    assert_eq!(cache.info().hits, info.hits);
    assert_eq!(cache.info().misses, info.misses);
    assert_eq!(renderer.gpu_error().as_deref(), Some(original.as_str()));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_static_and_animated_compositions_replay_full_fractional_pixels_and_alpha() {
    use motion_model::{Ease, Keyframe, Layer, Project};
    for animated in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            ChannelPrecision::Float16,
            crate::AllocationBudget::default(),
        ))
        .unwrap();
        let target = renderer.render_target(32, 32).unwrap();
        let mut project = Project::new(32, 32, 30, 30).unwrap();
        project.background = [0.; 4];
        let mut layer = Layer::solid(
            1,
            "cache pixel control",
            [24.; 2],
            [16., 16., 0.],
            [0.8, 0.2, 0.4, 0.75],
        );
        if animated {
            layer.transform.opacity.keys = [(0, 0.2), (2, 0.9)]
                .into_iter()
                .map(|(frame, value)| Keyframe {
                    auto_bezier: false,
                    frame,
                    value,
                    ease: Ease::Linear,
                    curve: None,
                    spatial: None,
                })
                .collect();
        }
        project.layers.push(layer);
        let mut scene = Scene::new(&project).unwrap();
        let mut cache =
            CompositionFrameCache::new(context(), target.texture_bytes() * 4, 4).unwrap();
        let mut expected = Vec::new();
        let frames = [0.25, 0.75, 1.25, 1.75];
        for frame in frames {
            let k = key(&target, frame);
            assert!(cache.get(&renderer, k).unwrap().is_none());
            scene
                .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            renderer.draw(&scene, &target.view, 32, 32).unwrap();
            let pixels = read(&renderer, &target);
            assert!(pixels
                .chunks_exact(8)
                .any(|p| u16::from_le_bytes([p[6], p[7]]) != 0));
            assert!(cache.store(&renderer, k, &target).unwrap());
            expected.push(pixels);
        }
        if animated {
            // Both members of each pair have the same integer frame. A cache
            // that truncates time would fail the independent full-pixel oracle.
            assert_ne!(expected[0], expected[1]);
            assert_ne!(expected[2], expected[3]);
        } else {
            assert!(expected.windows(2).all(|pair| pair[0] == pair[1]));
        }
        for index in [3, 0, 2, 1] {
            let frame = frames[index];
            let hit = cache.get(&renderer, key(&target, frame)).unwrap().unwrap();
            assert_eq!(hit.key().frame(), frame);
            assert_eq!(read(&renderer, hit.source()), expected[index]);
            // Independently re-render with the cache disabled, including alpha.
            scene
                .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            renderer.draw(&scene, &target.view, 32, 32).unwrap();
            assert_eq!(read(&renderer, &target), expected[index]);
        }
        let info = cache.info();
        assert_eq!(
            (info.hits, info.misses, info.installed, info.evicted),
            (4, 4, 4, 0)
        );
        assert_eq!(info.peak_resident_bytes, 4 * target.texture_bytes());
        cache.set_limits(0, 0).unwrap();
        assert_eq!(cache.info().resident_bytes, 0);
        assert_eq!(cache.info().peak_resident_bytes, info.peak_resident_bytes);
        assert!(!cache.store(&renderer, key(&target, 0.25), &target).unwrap());
    }
}
