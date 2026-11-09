//! Frozen working/display images compared on actual SDR attachments.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use motion_render::{ManagedDisplayPresenter, ManagedDisplayTarget, ViewChannel};

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn managed_snapshots_freeze_both_endpoints_and_compare_independent_cells() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime = NativeColorRuntime::load(&std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").unwrap(),
    ))
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        _ => {}
    }
    eprintln!("managed snapshot adapter={:?}", renderer.adapter_info);
    let mut checked = 0;
    let mut wrong_slots = 0;
    let mut failed_replacements = 0;
    let foreign = pollster::block_on(Renderer::headless()).unwrap();
    let mut prior = None;
    for case in reference.cases.iter().filter(|c| {
        c.processor.phase == ColorProcessorPhase::WorkingToDisplay
            && c.processor.input_alpha == AlphaMode::Premultiplied
    }) {
        let (config, selection) = &configs[case.config];
        let native = runtime
            .prepare(
                config,
                selection,
                case.processor.phase,
                case.processor.input_alpha,
                case.processor.output_alpha,
            )
            .unwrap();
        let processor = native
            .source()
            .compile_display_over_black(config, selection)
            .unwrap();
        let old = case.input.clone();
        let new: Vec<_> = old
            .iter()
            .map(|p| [p[2] * 0.5, p[0] * 0.25, p[1] * 0.75, 1. - p[3]])
            .collect();
        let colors = |input: &[[f32; 4]]| {
            input
                .iter()
                .map(|p| {
                    let rgb = native
                        .apply_rgb(if p[3] > 0. {
                            [p[0], p[1], p[2]]
                        } else {
                            [0.; 3]
                        })
                        .unwrap();
                    [rgb[0], rgb[1], rgb[2], 1.]
                })
                .collect::<Vec<_>>()
        };
        let old_color = colors(&old);
        let new_color = colors(&new);
        let source = renderer
            .upload_color_image_f32(
                3,
                4,
                ColorImageContract {
                    space: processor.input_space(),
                    range: processor.input_range(),
                    alpha: processor.input_alpha(),
                    precision: ChannelPrecision::Float32,
                },
                &old,
            )
            .unwrap();
        let display = ManagedDisplayTarget::new(&renderer, &source, &processor).unwrap();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let update = |pixels: &[[f32; 4]]| {
                renderer.queue.write_texture(
                    source.image().view().texture().as_image_copy(),
                    bytemuck::cast_slice(pixels),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(48),
                        rows_per_image: Some(4),
                    },
                    wgpu::Extent3d {
                        width: 3,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                );
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                display.encode(&renderer, &mut encoder).unwrap();
                renderer
                    .submit_allocated(encoder.finish(), |ledger| {
                        display.collect_texture_resources(ledger)
                    })
                    .unwrap();
            };
            update(&old);
            let prototype = ManagedDisplayPresenter::new(&renderer, &display, format).unwrap();
            let reads = renderer.image_readback_info();
            let snapshot = prototype.capture_snapshot(&renderer, &display).unwrap();
            assert_eq!(snapshot.dimensions(), [3, 4]);
            assert_eq!(renderer.image_readback_info(), reads);
            assert!(prototype
                .check_snapshot(&foreign, &snapshot)
                .unwrap_err()
                .to_string()
                .contains("another renderer"));
            let alternate = if format == wgpu::TextureFormat::Rgba8Unorm {
                wgpu::TextureFormat::Bgra8Unorm
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            };
            let other_format = snapshot.for_format(&renderer, alternate).unwrap();
            assert!(prototype
                .check_snapshot(&renderer, &other_format)
                .unwrap_err()
                .to_string()
                .contains("surface format"));
            if format == wgpu::TextureFormat::Rgba8Unorm {
                if let Some(previous) = prior.take() {
                    assert!(prototype
                        .check_snapshot(&renderer, &previous)
                        .unwrap_err()
                        .to_string()
                        .contains("display program"));
                }
                prior = Some(snapshot.for_format(&renderer, format).unwrap());
            }
            update(&new);
            let live: Vec<_> = (0..4)
                .map(|_| prototype.rebind(&renderer, &display).unwrap())
                .collect();
            let output = managed_display::sized_surface(&renderer, format, 12, 4);
            let viewport = |i: usize| [(i * 3) as f32, 0., 3., 4.];
            for (slot, split, reason) in [
                (4, 0.5, "below four"),
                (0, f32::NAN, "comparison split"),
                (0, -0.1, "comparison split"),
                (0, 1.1, "comparison split"),
            ] {
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                let error = live[0]
                    .encode_compared_region_view_load(
                        &renderer,
                        &mut encoder,
                        &output,
                        viewport(0),
                        None,
                        [0., 0., 0., 1.],
                        ViewChannel::Red,
                        None,
                        Some((&snapshot, slot, split)),
                        false,
                    )
                    .unwrap_err();
                assert!(error.to_string().contains(reason), "{error}");
            }
            for phase in 0..5 {
                let channel = |i: usize| {
                    if phase == 4 {
                        ViewChannel::Color
                    } else {
                        [
                            ViewChannel::Red,
                            ViewChannel::Green,
                            ViewChannel::Blue,
                            ViewChannel::Alpha,
                        ][(i + phase) % 4]
                    }
                };
                for split in [0., 0.5, 1.] {
                    for roi in [false, true] {
                        let region = roi.then_some([0., 0.25, 0.75, 0.5]);
                        let mut expected = Vec::new();
                        for y in 0..4 {
                            for x in 0..12 {
                                let i = x / 3;
                                let index = y * 3 + x % 3;
                                let u = (x % 3) as f32 / 3. + 0.5 / 3.;
                                let v = y as f32 / 4. + 0.5 / 4.;
                                expected.push(if roi && (u >= 0.75 || v < 0.25 || v >= 0.75) {
                                    [0., 0., 0., 1.]
                                } else if channel(i) == ViewChannel::Color {
                                    if u >= split {
                                        old_color[index]
                                    } else {
                                        new_color[index]
                                    }
                                } else {
                                    let value = if u >= split { old[index] } else { new[index] }
                                        [channel(i) as usize - 1]
                                        .clamp(0., 1.);
                                    [value, value, value, 1.]
                                });
                            }
                        }
                        let draw = |wrong: bool| {
                            let mut encoder = renderer
                                .allocation_command_encoder(&Default::default())
                                .unwrap();
                            for (i, presenter) in live.iter().enumerate() {
                                presenter
                                    .encode_compared_region_view_load(
                                        &renderer,
                                        &mut encoder,
                                        &output,
                                        viewport(i),
                                        None,
                                        [0., 0., 0., 1.],
                                        channel(i),
                                        region,
                                        Some((&snapshot, if wrong { 0 } else { i }, split)),
                                        i != 0,
                                    )
                                    .unwrap();
                            }
                            renderer
                                .submit_allocated(encoder.finish(), |ledger| {
                                    display.collect_texture_resources(ledger);
                                    snapshot.collect_texture_resources(ledger);
                                    for p in &live {
                                        p.collect_texture_resources(ledger);
                                    }
                                })
                                .unwrap();
                            managed_display::pixels(&renderer, &output)
                        };
                        assert_eq!(
                            managed_display::errors(&draw(false), &expected, format),
                            0,
                            "format={format:?} phase={phase} split={split} roi={roi}"
                        );
                        checked += expected.len();
                        if phase == 0 && split == 0. && !roi {
                            assert!(managed_display::errors(&draw(true),&expected,format)>0,
                                "sharing frozen uniform slots must fail the exact per-cell reference");
                            wrong_slots += 1;
                        }
                    }
                }
            }
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            let budget = renderer.allocation_budget().clone();
            let before = budget.info().unwrap();
            // Refuse before the first allocation, then after admitting one
            // complete Float32 working texture. Neither may leak a candidate.
            for allowance in [0, 3 * 4 * 16] {
                budget.set_limit(before.admitted_bytes + allowance).unwrap();
                assert!(prototype.capture_snapshot(&renderer, &display).is_err());
                budget.set_limit(before.budget_bytes).unwrap();
                assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
                failed_replacements += 1;
            }
            let capture = snapshot.for_format(&renderer, format).unwrap();
            let alpha_output = managed_display::surface(&renderer, format);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            capture
                .encode(&renderer, &mut encoder, &alpha_output, ViewChannel::Alpha)
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    capture.collect_texture_resources(ledger)
                })
                .unwrap();
            // The failed replacement must not destroy or rebind the previous copy.
            let alpha: Vec<_> = old
                .iter()
                .map(|p| {
                    let a = p[3];
                    [a, a, a, 1.]
                })
                .collect();
            assert_eq!(
                managed_display::errors(
                    &managed_display::pixels(&renderer, &alpha_output),
                    &alpha,
                    format
                ),
                0
            );
        }
    }
    renderer.check_health().unwrap();
    assert_eq!(checked, 23_040);
    assert_eq!(wrong_slots, 16);
    assert_eq!(failed_replacements, 32);
    eprintln!("managed snapshot pixels={checked} shared_slot_controls={wrong_slots} refused_replacements={failed_replacements}");
}
