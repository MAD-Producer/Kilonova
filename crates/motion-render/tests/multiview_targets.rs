use motion_core::{Content, Layer, Project};
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{
    multiview::{ViewFramePresenter, ViewFrameTargets, ViewPlacement},
    AllocationEncoder, Presenter, RenderError, RenderTarget, Renderer, Scene, ViewCamera,
};
#[path = "multiview_targets/inspection.rs"]
mod inspection;

const COLORS: [[f64; 4]; 4] = [
    [1., 0., 0., 1.],
    [0., 1., 0., 1.],
    [0., 0., 1., 1.],
    [1., 1., 0., 1.],
];

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_single_view_outputs_share_the_group_budget_without_overflow() {
    let mut renderer = new_renderer(ChannelPrecision::Unorm8);
    let mut targets = ViewFrameTargets::default();
    let mut policy = renderer.memory_policy();
    let reserved = 123;
    let required = 2 * 8 * 8 * 4 + reserved;
    policy.preview_target_budget_bytes = required - 1;
    renderer.set_memory_policy(policy).unwrap();
    let error = targets
        .update_with_reserved(&mut renderer, &[[8, 8]], reserved, |_, _, _, _| {
            panic!("combined target refusal must precede encoding")
        })
        .unwrap_err();
    assert!(
        matches!(error.resource_rejection(), Some(motion_render::ResourceRejection::Budget {
        scope: motion_render::ResourceBudgetScope::PreviewTarget,
        required_bytes, budget_bytes,
    }) if *required_bytes == required && *budget_bytes == required - 1)
    );
    assert!(targets.is_empty());
    policy.preview_target_budget_bytes = required;
    renderer.set_memory_policy(policy).unwrap();
    assert!(targets
        .update_with_reserved(
            &mut renderer,
            &[[8, 8]],
            reserved,
            |_, _, target, encoder| {
                clear(encoder, target, COLORS[0]);
                Ok(true)
            }
        )
        .unwrap());
    let original = bytes(&renderer, targets.output(0).unwrap());
    let error = targets
        .update_with_reserved(&mut renderer, &[[8, 8]], u64::MAX, |_, _, _, _| {
            panic!("overflowed reservation must be refused")
        })
        .unwrap_err();
    assert!(matches!(
        error.resource_rejection(),
        Some(motion_render::ResourceRejection::Budget {
            required_bytes: u64::MAX,
            ..
        })
    ));
    assert_eq!(bytes(&renderer, targets.output(0).unwrap()), original);
}

fn new_renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        Default::default(),
    ))
    .unwrap();
    if let Some(change) = renderer.prepare_working_precision(precision).unwrap() {
        renderer.commit_working_precision(change).unwrap();
    }
    renderer
}
fn clear(encoder: &mut AllocationEncoder, target: &RenderTarget, color: [f64; 4]) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("independent view fixture"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: color[0],
                    g: color[1],
                    b: color[2],
                    a: color[3],
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}
fn update_colors(renderer: &mut Renderer, targets: &mut ViewFrameTargets, sizes: &[[u32; 2]]) {
    assert!(targets
        .update(renderer, sizes, |_, index, target, encoder| {
            clear(encoder, target, COLORS[index]);
            Ok(true)
        })
        .unwrap());
}
fn bytes(renderer: &Renderer, target: &RenderTarget) -> Vec<u8> {
    let texture = target.view.texture();
    let row = target.width * texture.format().block_copy_size(None).unwrap();
    let stride = row.div_ceil(256) * 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("view transaction test readback"),
        size: u64::from(stride * target.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
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
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = (0..target.height)
        .flat_map(|y| mapped[(y * stride) as usize..(y * stride + row) as usize].to_vec())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_and_four_cells_preserve_each_others_pixels_on_one_output() {
    let mut renderer = new_renderer(ChannelPrecision::Unorm8);
    let mut targets = ViewFrameTargets::default();
    let mut presenter = ViewFramePresenter::default();
    let output = renderer.render_target(66, 66).unwrap();
    let placements: Vec<_> = (0..4)
        .map(|i| ViewPlacement {
            viewport: [(i % 2 * 34) as f32, (i / 2 * 34) as f32, 32., 32.],
            image: None,
        })
        .collect();
    for count in [1, 2, 4, 2, 1] {
        update_colors(&mut renderer, &mut targets, &vec![[32; 2]; count]);
        assert_eq!(targets.len(), count);
        assert!(targets.output(count).is_none());
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        presenter
            .encode(
                &renderer,
                &targets,
                &mut encoder,
                &output.view,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [66, 66],
                &placements[..count],
                [0., 0., 0., 1.],
            )
            .unwrap();
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        let pixels = bytes(&renderer, &output);
        for (i, color) in COLORS[..count].iter().enumerate() {
            let offset = ((i / 2 * 34 + 16) * 66 + (i % 2 * 34 + 16)) * 4;
            let expected: Vec<u8> = color.iter().map(|v| (v * 255.) as u8).collect();
            assert_eq!(&pixels[offset..offset + 4], expected.as_slice(), "view={i}");
        }
        assert_eq!(
            &pixels[(33 * 66 + 33) * 4..(33 * 66 + 33) * 4 + 4],
            &[0, 0, 0, 255]
        );
        if count == 4 {
            // The old single-view clear semantics must visibly fail this group.
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            for (i, placement) in placements.iter().enumerate() {
                let old = Presenter::new_admitted(
                    &renderer,
                    &targets.output(i).unwrap().view,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                )
                .unwrap();
                old.encode_view(
                    &renderer.queue,
                    &mut encoder,
                    &output.view,
                    None,
                    placement.viewport,
                    None,
                    [0., 0., 0., 1.],
                )
                .unwrap();
            }
            renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
            assert_ne!(
                bytes(&renderer, &output),
                pixels,
                "negative control must detect clearing earlier cells"
            );
            if let Some(path) = std::env::var_os("MOTION_MULTIVIEW_TARGET_ARTIFACTS") {
                let root = std::path::PathBuf::from(path);
                std::fs::create_dir_all(&root).unwrap();
                image::save_buffer(
                    root.join("four-cells.png"),
                    &pixels,
                    66,
                    66,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn pending_and_failed_later_cells_keep_the_whole_previous_group_and_recover() {
    let mut renderer = new_renderer(ChannelPrecision::Float16);
    let mut targets = ViewFrameTargets::default();
    assert!(targets.is_empty());
    assert!(!targets
        .update(&mut renderer, &[[16; 2]; 2], |_, _, _, _| Ok(false))
        .unwrap());
    assert!(targets.is_empty());
    assert_eq!(targets.texture_bytes(), 0);
    update_colors(&mut renderer, &mut targets, &[[16; 2]; 2]);
    let old: Vec<_> = (0..2)
        .map(|i| bytes(&renderer, targets.output(i).unwrap()))
        .collect();
    let old_ids: Vec<_> = (0..2)
        .map(|i| targets.output(i).unwrap().view.texture().clone())
        .collect();
    for failure in [false, true] {
        let mut calls = vec![];
        let result = targets.update(&mut renderer, &[[24; 2]; 4], |_, i, target, encoder| {
            calls.push(i);
            clear(encoder, target, COLORS[3 - i]);
            if i == 1 {
                if failure {
                    Err(RenderError::Invalid(
                        "source=73 phase=decode: controlled failure".into(),
                    ))
                } else {
                    Ok(false)
                }
            } else {
                Ok(true)
            }
        });
        if failure {
            let RenderError::Invalid(cause) = result.unwrap_err() else {
                panic!("callback failure changed its error variant")
            };
            assert_eq!(cause, "source=73 phase=decode: controlled failure");
        } else {
            assert!(!result.unwrap());
        }
        assert_eq!(calls, vec![0, 1]);
        assert_eq!(targets.len(), 2);
        for i in 0..2 {
            assert_eq!(targets.output(i).unwrap().view.texture(), &old_ids[i]);
            assert_eq!(bytes(&renderer, targets.output(i).unwrap()), old[i]);
        }
    }
    // Unwinding must preserve the published group and the original panic.
    // Catch only in this test; production keeps its existing worker/hook policy.
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        targets.update(&mut renderer, &[[24; 2]; 4], |_, i, target, encoder| {
            clear(encoder, target, COLORS[3 - i]);
            if i == 1 {
                std::panic::panic_any("source=74 phase=encode: controlled panic");
            }
            Ok(true)
        })
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"source=74 phase=encode: controlled panic")
    );
    assert_eq!(targets.len(), 2);
    for i in 0..2 {
        assert_eq!(bytes(&renderer, targets.output(i).unwrap()), old[i]);
    }
    let mut retained = motion_render::TextureResourceLedger::default();
    targets.collect_texture_resources(&mut retained);
    assert!(
        retained.resources().count() <= 4 * 2 * 2,
        "panic must leave bounded front/candidate ownership"
    );
    update_colors(&mut renderer, &mut targets, &[[24; 2]; 4]);
    assert_eq!(targets.len(), 4);
    for i in 0..4 {
        assert_eq!(
            [
                targets.output(i).unwrap().width,
                targets.output(i).unwrap().height
            ],
            [24; 2]
        );
        assert_eq!(
            &bytes(&renderer, targets.output(i).unwrap())[..4],
            COLORS[i]
                .iter()
                .map(|value| (value * 255.) as u8)
                .collect::<Vec<_>>()
                .as_slice()
        );
    }
    targets.clear();
    assert!(targets.is_empty());
    assert_eq!(targets.texture_bytes(), 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn combined_budget_rejects_before_encoding_and_preserves_all_fronts() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = new_renderer(precision);
        let mut targets = ViewFrameTargets::default();
        update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        let exact = targets.texture_bytes();
        let mut policy = renderer.memory_policy();
        policy.preview_target_budget_bytes = exact;
        renderer.set_memory_policy(policy).unwrap();
        // The already allocated, exact group still fits after lowering policy.
        update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        let before: Vec<_> = (0..4)
            .map(|i| bytes(&renderer, targets.output(i).unwrap()))
            .collect();
        policy.preview_target_budget_bytes = exact - 1;
        renderer.set_memory_policy(policy).unwrap();
        let mut invoked = false;
        let error = targets
            .update(&mut renderer, &[[16; 2]; 4], |_, _, _, _| {
                invoked = true;
                Ok(true)
            })
            .unwrap_err();
        assert!(
            !invoked,
            "the combined budget must reject before entering a cell"
        );
        assert!(
            matches!(error,RenderError::Resource {rejection:motion_render::ResourceRejection::Budget {
            scope:motion_render::ResourceBudgetScope::PreviewTarget,required_bytes,budget_bytes},..}
            if required_bytes==exact && budget_bytes==exact-1),
            "{error:?}"
        );
        for i in 0..4 {
            assert_eq!(bytes(&renderer, targets.output(i).unwrap()), before[i]);
        }
        policy.preview_target_budget_bytes = exact;
        renderer.set_memory_policy(policy).unwrap();
        update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        assert_eq!(targets.texture_bytes(), exact);
        let native = motion_render::image_formats::native_format(
            motion_render::image_formats::color_storage(precision),
        );
        assert_eq!(
            targets.working_output(0).unwrap().view.texture().format(),
            native
        );
        assert_eq!(
            targets.output(0).unwrap().view.texture().format(),
            wgpu::TextureFormat::Rgba8UnormSrgb
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn camera_views_and_independent_canvas_transforms_match_single_cell_oracles() {
    let mut project = Project::new(96, 96, 30, 60).unwrap();
    project.background = [0., 0., 0., 1.];
    for (id, z) in [(1, -100.), (2, 100.)] {
        let mut camera = Layer::camera(id, "Camera", 96, 96);
        if let Content::Camera { camera } = &mut camera.content {
            camera.position.value = [48., 48., z];
        }
        project.layers.push(camera);
    }
    for (id, x, z, color) in [
        (7, 38., -20., [1., 0., 0., 1.]),
        (8, 58., 20., [0., 0., 1., 1.]),
    ] {
        let mut layer = Layer::solid(id, "Plane", [30.; 2], [x, 48., z], color);
        layer.three_d = true;
        project.layers.push(layer);
    }
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 12.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let views = scene
        .fork_views(&[
            ViewCamera::Camera(1),
            ViewCamera::Camera(2),
            ViewCamera::Camera(1),
            ViewCamera::Camera(2),
        ])
        .unwrap();
    let mut renderer = new_renderer(ChannelPrecision::Unorm8);
    let mut targets = ViewFrameTargets::default();
    assert!(targets
        .update(&mut renderer, &[[96; 2]; 4], |r, i, target, encoder| {
            r.encode(&views[i], &target.view, 96, 96, encoder, None)?;
            Ok(true)
        })
        .unwrap());
    let placements: Vec<_> = (0..4)
        .map(|i| {
            let x = (i % 2 * 98) as f32;
            let y = (i / 2 * 98) as f32;
            let scale = 1. + i as f32 * 0.15;
            ViewPlacement {
                viewport: [x, y, 96., 96.],
                image: Some([
                    x - 4. * i as f32,
                    y - 3. * i as f32,
                    96. * scale,
                    96. * scale,
                ]),
            }
        })
        .collect();
    let output = renderer.render_target(194, 194).unwrap();
    let mut presenter = ViewFramePresenter::default();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    presenter
        .encode(
            &renderer,
            &targets,
            &mut encoder,
            &output.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [194, 194],
            &placements,
            [0., 0., 0., 1.],
        )
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let combined = bytes(&renderer, &output);
    let mut individual = vec![];
    for (i, placement) in placements.iter().enumerate() {
        let oracle = renderer.render_target(96, 96).unwrap();
        let single = Presenter::new_admitted(
            &renderer,
            &targets.output(i).unwrap().view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
        .unwrap();
        let mut image = placement.image.unwrap();
        image[0] -= placement.viewport[0];
        image[1] -= placement.viewport[1];
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        single
            .encode_view(
                &renderer.queue,
                &mut encoder,
                &oracle.view,
                None,
                [0., 0., 96., 96.],
                Some(image),
                [0., 0., 0., 1.],
            )
            .unwrap();
        renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        let expected = bytes(&renderer, &oracle);
        for y in 0..96 {
            let offset =
                ((placement.viewport[1] as usize + y) * 194 + placement.viewport[0] as usize) * 4;
            assert_eq!(
                &combined[offset..offset + 96 * 4],
                &expected[y * 96 * 4..(y + 1) * 96 * 4],
                "cell={i} row={y}"
            );
        }
        individual.push(expected);
    }
    assert_ne!(
        individual[0], individual[1],
        "fixture must detect a shared camera"
    );
    assert_ne!(
        individual[0], individual[2],
        "fixture must detect a shared canvas transform"
    );
    if let Some(path) = std::env::var_os("MOTION_MULTIVIEW_TARGET_ARTIFACTS") {
        let root = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&root).unwrap();
        image::save_buffer(
            root.join("four-camera-canvases.png"),
            &combined,
            194,
            194,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_layouts_foreign_owners_and_bad_geometry_are_rejected_before_draw() {
    let mut renderer = new_renderer(ChannelPrecision::Unorm8);
    let mut targets = ViewFrameTargets::default();
    for sizes in [
        vec![],
        vec![[16; 2]; 3],
        vec![[16; 2]; 5],
        vec![[0, 16]],
        vec![[16, 0]],
        vec![[u32::MAX, 16]],
    ] {
        assert!(targets
            .update(&mut renderer, &sizes, |_, _, _, _| panic!(
                "invalid layout encoded"
            ))
            .is_err());
        assert!(targets.is_empty());
        assert_eq!(targets.texture_bytes(), 0);
    }
    update_colors(&mut renderer, &mut targets, &[[16; 2]; 2]);
    let output = renderer.render_target(32, 16).unwrap();
    let mut presenter = ViewFramePresenter::default();
    let placements = [
        ViewPlacement {
            viewport: [0., 0., 16., 16.],
            image: None,
        },
        ViewPlacement {
            viewport: [16., 0., 16., 16.],
            image: None,
        },
    ];
    for bad in [
        ViewPlacement {
            viewport: [15., 0., 16., 16.],
            image: None,
        },
        ViewPlacement {
            viewport: [16., 0., 17., 16.],
            image: None,
        },
        ViewPlacement {
            viewport: [f32::NAN, 0., 16., 16.],
            image: None,
        },
        ViewPlacement {
            image: Some([0., 0., 0., 16.]),
            ..placements[1]
        },
    ] {
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        assert!(presenter
            .encode(
                &renderer,
                &targets,
                &mut encoder,
                &output.view,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [32, 16],
                &[placements[0], bad],
                [0., 0., 0., 1.]
            )
            .is_err());
    }
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    presenter
        .encode(
            &renderer,
            &targets,
            &mut encoder,
            &output.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [32, 16],
            &placements,
            [0., 0., 0., 1.],
        )
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let mut other = new_renderer(ChannelPrecision::Unorm8);
    assert!(targets
        .update(&mut other, &[[16; 2]; 2], |_, _, _, _| panic!(
            "foreign target encoded"
        ))
        .is_err());
    assert_eq!(targets.len(), 2);
    let mut encoder = other
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(presenter
        .encode(
            &other,
            &targets,
            &mut encoder,
            &output.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [32, 16],
            &placements,
            [0., 0., 0., 1.]
        )
        .is_err());
    let mut foreign_targets = ViewFrameTargets::default();
    update_colors(&mut other, &mut foreign_targets, &[[16; 2]; 2]);
    let foreign_output = other.render_target(32, 16).unwrap();
    let mut encoder = other
        .allocation_command_encoder(&Default::default())
        .unwrap();
    let error = presenter
        .encode(
            &other,
            &foreign_targets,
            &mut encoder,
            &foreign_output.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [32, 16],
            &placements,
            [0., 0., 0., 1.],
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("presenter belongs to another renderer"));
    presenter.clear();
    presenter
        .encode(
            &other,
            &foreign_targets,
            &mut encoder,
            &foreign_output.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [32, 16],
            &placements,
            [0., 0., 0., 1.],
        )
        .unwrap();
    other.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer.check_health().unwrap();
    other.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn precision_switches_and_warm_updates_keep_display_pixels_and_bounded_targets() {
    let mut renderer = new_renderer(ChannelPrecision::Unorm8);
    let mut targets = ViewFrameTargets::default();
    let mut previous = vec![];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Float32,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Unorm8,
    ] {
        if let Some(change) = renderer.prepare_working_precision(precision).unwrap() {
            renderer.commit_working_precision(change).unwrap();
        }
        update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        let displayed: Vec<_> = (0..4)
            .map(|i| bytes(&renderer, targets.output(i).unwrap()))
            .collect();
        if !previous.is_empty() {
            assert_eq!(displayed, previous);
        }
        previous = displayed;
        let native = motion_render::image_formats::native_format(
            motion_render::image_formats::color_storage(precision),
        );
        for i in 0..4 {
            assert_eq!(
                targets.working_output(i).unwrap().view.texture().format(),
                native
            );
        }
        // Warm both slots after a format change; subsequent healthy updates
        // must reuse native/output allocations and perform no CPU readback.
        update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let before = renderer.allocation_budget().info().unwrap();
        let reads = renderer.image_readback_info();
        for _ in 0..3 {
            update_colors(&mut renderer, &mut targets, &[[16; 2]; 4]);
        }
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let after = renderer.allocation_budget().info().unwrap();
        assert_eq!(after.admitted_bytes, before.admitted_bytes);
        assert_eq!(after.live_allocations, before.live_allocations);
        assert_eq!(
            renderer.image_readback_info().submitted_calls,
            reads.submitted_calls
        );
        let mut ledger = motion_render::TextureResourceLedger::default();
        targets.collect_texture_resources(&mut ledger);
        assert_eq!(ledger.total_known_bytes(), targets.texture_bytes());
    }
}
