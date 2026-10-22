use super::*;
use motion_render::{
    AllocationBudgetInfo, AllocationCategory as Category, AllocationFrameOutcome, FrameOperation,
    Renderer,
};

fn bytes(info: &AllocationBudgetInfo, category: Category) -> u64 {
    info.categories
        .iter()
        .find(|entry| entry.category == category)
        .unwrap()
        .admitted_bytes
}
fn list(count: usize) -> PaintList {
    let mut list = PaintList::default();
    for _ in 0..count {
        rect(&mut list, Rect::new(0., 0., 4., 8.), Color::rgb(255, 0, 0));
    }
    for _ in 0..count {
        list.push_glyph(Glyph {
            atlas_size: None,
            uv: [0., 0., 1., 1.],
            position: [4., 0.],
            size: [4., 8.],
            color: Color::rgb(0, 255, 0),
        });
    }
    list
}
fn encode(
    renderer: &Renderer,
    painter: &mut Painter,
    list: &PaintList,
    target: &motion_render::CaptureTarget,
) -> Result<(), PaintError> {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .map_err(|cause| PaintError::native("test encoder", cause))?;
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            multiview_mask: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    painter.render(
        &renderer.device,
        &renderer.queue,
        &mut encoder,
        &target.view,
        list,
        [target.width as f32, target.height as f32],
    )?;
    renderer
        .submit_allocated(encoder.finish(), |_| {})
        .map_err(|cause| PaintError::native("test submit", cause))?;
    Ok(())
}
fn colors(pixels: &[u8]) {
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(
                &pixels[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
                if x < 4 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 255]
                }
            );
        }
    }
}

#[test]
fn startup_refusal_is_atomic_and_actual_ui_allocations_have_disjoint_categories() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let context = renderer.allocation_context();
    let budget = context.budget().clone();
    let baseline = budget.info().unwrap();
    budget.set_limit(baseline.admitted_bytes + 39).unwrap();
    let error = Painter::new_admitted(&context, wgpu::TextureFormat::Rgba8UnormSrgb)
        .err()
        .unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: baseline.admitted_bytes + 40,
            budget_bytes: baseline.admitted_bytes + 39,
        })
    );
    assert!(std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<motion_render::RenderError>()
        .is_some());
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        baseline.admitted_bytes
    );
    assert_eq!(
        budget.info().unwrap().live_allocations,
        baseline.live_allocations
    );
    budget.set_limit(baseline.budget_bytes).unwrap();
    let mut painter = Painter::new_admitted(&context, wgpu::TextureFormat::Rgba8UnormSrgb).unwrap();
    let created = budget.info().unwrap();
    assert_eq!(created.admitted_bytes - baseline.admitted_bytes, 40);
    assert_eq!(
        bytes(&created, Category::UniformBuffers) - bytes(&baseline, Category::UniformBuffers),
        32
    );
    assert_eq!(
        bytes(&created, Category::GeometryBuffers) - bytes(&baseline, Category::GeometryBuffers),
        8
    );
    painter
        .upload_atlas(&renderer.device, &renderer.queue, &[255; 16], 2, 2)
        .unwrap();
    let uploaded = budget.info().unwrap();
    assert_eq!(bytes(&uploaded, Category::UiAtlas), 16);
    assert_eq!(uploaded.admitted_bytes, created.admitted_bytes + 16);
    painter
        .upload_atlas(&renderer.device, &renderer.queue, &[0; 16], 2, 2)
        .unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        uploaded.admitted_bytes
    );
    assert_eq!(painter.bind_group_creations, 2);
    let error = painter
        .upload_atlas(&renderer.device, &renderer.queue, &[0; 15], 2, 2)
        .unwrap_err();
    assert!(error.resource_rejection().is_none());
    assert_eq!(painter.bind_group_creations, 2);
    drop(painter);
    assert_eq!(
        bytes(&budget.info().unwrap(), Category::UiAtlas),
        16,
        "queued atlas writes must retain capacity"
    );
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        baseline.admitted_bytes
    );
    assert_eq!(bytes(&budget.info().unwrap(), Category::UiAtlas), 0);
}

#[test]
fn refused_atlas_and_two_buffer_growth_preserve_old_pixels_then_recover() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let context = renderer.allocation_context();
    let budget = context.budget().clone();
    let mut painter = Painter::new_admitted(&context, wgpu::TextureFormat::Rgba8UnormSrgb).unwrap();
    let target = renderer.capture_target(8, 8).unwrap();
    painter
        .upload_atlas(&renderer.device, &renderer.queue, &[255; 4], 1, 1)
        .unwrap();
    encode(&renderer, &mut painter, &list(1), &target).unwrap();
    let expected = renderer.read_target(&target).unwrap();
    colors(&expected);
    let before = budget.info().unwrap();
    let bind_count = painter.bind_group_creations;
    budget.set_limit(before.admitted_bytes + 15).unwrap();
    let error = painter
        .upload_atlas(&renderer.device, &renderer.queue, &[0; 16], 2, 2)
        .unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert_eq!(painter.atlas_size, [1, 1]);
    assert_eq!(painter.bind_group_creations, bind_count);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    encode(&renderer, &mut painter, &list(1), &target).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), expected);

    let (solid_size, glyph_size) = (painter.solid_buffer.size(), painter.glyph_buffer.size());
    let solid_growth = (600 * std::mem::size_of::<SolidVertex>() as u64).next_power_of_two();
    let glyph_growth = (600 * std::mem::size_of::<GlyphVertex>() as u64).next_power_of_two();
    budget
        .set_limit(before.admitted_bytes + solid_growth + glyph_growth - 1)
        .unwrap();
    let error = encode(&renderer, &mut painter, &list(100), &target).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + solid_growth + glyph_growth,
            budget_bytes: before.admitted_bytes + solid_growth + glyph_growth - 1,
        })
    );
    assert_eq!(
        (painter.solid_buffer.size(), painter.glyph_buffer.size()),
        (solid_size, glyph_size)
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    budget.set_limit(before.budget_bytes).unwrap();
    encode(&renderer, &mut painter, &list(100), &target).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    painter
        .upload_atlas(&renderer.device, &renderer.queue, &[255; 16], 2, 2)
        .unwrap();
    encode(&renderer, &mut painter, &list(1), &target).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    assert_eq!(painter.bind_group_creations, bind_count + 1);
    // Production transfers ownership of dirty CPU bitmaps. Keep exactly that
    // one intent after refusal, avoid repeated GPU allocation attempts while
    // still short of capacity, then recover without another producer upload.
    let before_pending = budget.info().unwrap();
    budget
        .set_limit(before_pending.admitted_bytes + 63)
        .unwrap();
    let failure = painter
        .upload_atlas_owned(&renderer.device, &renderer.queue, vec![255; 64], 4, 4)
        .unwrap_err();
    assert_eq!(painter.pending_atlas.as_ref().unwrap().pixels.len(), 64);
    let refused = budget.info().unwrap();
    for _ in 0..60 {
        assert_eq!(
            painter
                .retry_atlas(&renderer.device, &renderer.queue)
                .unwrap_err()
                .to_string(),
            failure.to_string()
        );
        assert_eq!(
            budget.info().unwrap().peak_admitted_bytes,
            refused.peak_admitted_bytes
        );
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            refused.admitted_bytes
        );
    }
    budget.set_limit(before.budget_bytes).unwrap();
    painter
        .retry_atlas(&renderer.device, &renderer.queue)
        .unwrap();
    assert!(painter.pending_atlas.is_none());
    assert_eq!(painter.atlas_size, [4, 4]);
    encode(&renderer, &mut painter, &list(1), &target).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
}

#[test]
fn dropped_painter_remains_in_frame_peak_until_real_gpu_work_retires() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let context = renderer.allocation_context();
    let budget = context.budget().clone();
    let target = renderer.capture_target(8, 8).unwrap();
    let baseline = budget.info().unwrap();
    let frame = budget
        .begin_frame(9901, FrameOperation::Present)
        .unwrap()
        .unwrap();
    let mut painter = Painter::new_admitted(&context, wgpu::TextureFormat::Rgba8UnormSrgb).unwrap();
    painter
        .upload_atlas(&renderer.device, &renderer.queue, &[255; 4], 1, 1)
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    painter
        .render(
            &renderer.device,
            &renderer.queue,
            &mut encoder,
            &target.view,
            &list(1),
            [8.; 2],
        )
        .unwrap();
    let encoded = budget.info().unwrap();
    assert!(encoded.admitted_bytes > baseline.admitted_bytes);
    assert_eq!(bytes(&encoded, Category::UiAtlas), 4);
    drop(painter);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        encoded.admitted_bytes,
        "unsubmitted commands and writes retain dropped UI resources"
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    frame.finish(AllocationFrameOutcome::Rendered);
    colors(&renderer.read_target(&target).unwrap());
    let retired = budget.info().unwrap();
    assert_eq!(retired.admitted_bytes, baseline.admitted_bytes);
    assert_eq!(retired.live_allocations, baseline.live_allocations);
    let windows = budget.frame_peaks().unwrap();
    assert!(windows.active.is_empty());
    let completed = windows
        .recent
        .iter()
        .find(|window| window.request == 9901)
        .unwrap();
    let atlas = completed
        .categories
        .iter()
        .find(|category| category.category == Category::UiAtlas)
        .unwrap();
    assert_eq!(atlas.start_admitted_bytes, 0);
    assert_eq!(atlas.peak_admitted_bytes, 4);
    assert_eq!(atlas.end_admitted_bytes, Some(0));
}

#[test]
fn admitted_text_matches_legacy_pixels_through_real_atlas_growth_and_multiple_dpis() {
    use crate::{
        text::TextAtlas,
        ui::{self, Align},
    };
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let context = renderer.allocation_context();
    for ratio in [1., 1.5, 2.] {
        let mut atlas = TextAtlas::from_system_language(14., true)
            .expect("installed UI font required for admitted text regression");
        atlas.set_pixel_ratio(ratio);
        let mut raw = Painter::new(&renderer.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut admitted =
            Painter::new_admitted(&context, wgpu::TextureFormat::Rgba8UnormSrgb).unwrap();
        let reference = renderer.capture_target(512, 128).unwrap();
        let target = renderer.capture_target(512, 128).unwrap();
        let mut list = PaintList::default();
        ui::text_sized(
            &mut list,
            &mut atlas,
            Rect::new(16., 8., 480., 48.),
            "Retained glyphs: AaWg 0123456789",
            Color::grey(235),
            Align::Left,
            24.,
        );
        list.clip_to(Rect::new(24., 0., 480., 128.));
        assert!(!list.glyphs.is_empty());
        let initial = atlas.config();
        for grow in [false, true] {
            if grow {
                for size in 48..512 {
                    atlas.glyph_at('W', size as f32 / 2.);
                    if atlas.config().width > initial.width {
                        break;
                    }
                }
                assert!(
                    atlas.config().width > initial.width,
                    "exercise real atlas growth at ratio={ratio}"
                );
                let mut added = PaintList::default();
                ui::text_sized(
                    &mut added,
                    &mut atlas,
                    Rect::new(16., 72., 480., 48.),
                    "New glyphs after atlas growth",
                    Color::grey(235),
                    Align::Left,
                    24.,
                );
                list.append(added);
            }
            let config = atlas.config();
            raw.upload_atlas(
                &renderer.device,
                &renderer.queue,
                atlas.pixel_data(),
                config.width,
                config.height,
            )
            .unwrap();
            admitted
                .upload_atlas_owned(
                    &renderer.device,
                    &renderer.queue,
                    atlas.pixel_data().to_vec(),
                    config.width,
                    config.height,
                )
                .unwrap();
            encode(&renderer, &mut raw, &list, &reference).unwrap();
            encode(&renderer, &mut admitted, &list, &target).unwrap();
            let expected = renderer.read_target(&reference).unwrap();
            assert!(expected.iter().any(|byte| *byte != 0));
            assert_eq!(
                renderer.read_target(&target).unwrap(),
                expected,
                "text pixels changed at ratio={ratio}, grow={grow}"
            );
        }
    }
}
