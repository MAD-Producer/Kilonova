//! Numeric working components remain separate from nonlinear, opaque display values.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use motion_render::{ManagedDisplayPresenter, ManagedDisplayTarget, ViewChannel};

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn managed_components_keep_pre_matte_values_independent_cells_and_color_restoration() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime = NativeColorRuntime::load(&std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
    ))
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        _ => {}
    }
    eprintln!("managed component adapter={:?}", renderer.adapter_info);
    let mut checked = 0;
    let mut false_display_detected = 0;
    let mut shared_uniform_detected = 0;
    for (ci, case) in reference.cases.iter().enumerate().filter(|(_, c)| {
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
        let mut input = case.input.clone();
        input[0] = [0.75, 0.25, 0.5, 0.];
        let expected_color: Vec<_> = input
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
            .collect();
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
                &input,
            )
            .unwrap();
        let display = ManagedDisplayTarget::new(&renderer, &source, &processor).unwrap();
        let before = read(&renderer, &source);
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let prototype = ManagedDisplayPresenter::new(&renderer, &display, format).unwrap();
            for count in [1, 2, 4] {
                let columns = if count == 1 { 1 } else { 2 };
                let rows = if count == 4 { 2 } else { 1 };
                let presenters: Vec<_> = (0..count)
                    .map(|_| prototype.rebind(&renderer, &display).unwrap())
                    .collect();
                let output =
                    managed_display::sized_surface(&renderer, format, 3 * columns, 4 * rows);
                let viewport =
                    |i: u32| [(i % columns * 3) as f32, (i / columns * 4) as f32, 3., 4.];
                for phase in 0..5 {
                    let channel = |i: u32| {
                        if phase == 4 {
                            ViewChannel::Color
                        } else {
                            [
                                ViewChannel::Red,
                                ViewChannel::Green,
                                ViewChannel::Blue,
                                ViewChannel::Alpha,
                            ][((phase + i) % 4) as usize]
                        }
                    };
                    let mut expected = Vec::new();
                    for y in 0..4 * rows {
                        for x in 0..3 * columns {
                            let cell = y / 4 * columns + x / 3;
                            let pixel = (y % 4 * 3 + x % 3) as usize;
                            expected.push(if channel(cell) == ViewChannel::Color {
                                expected_color[pixel]
                            } else {
                                let v = input[pixel][channel(cell) as usize - 1].clamp(0., 1.);
                                [v, v, v, 1.]
                            });
                        }
                    }
                    let reads = renderer.image_readback_info();
                    let mut encoder = renderer
                        .allocation_command_encoder(&Default::default())
                        .unwrap();
                    display.encode(&renderer, &mut encoder).unwrap();
                    for (i, presenter) in presenters.iter().enumerate() {
                        presenter
                            .encode_inspected_region_view_load(
                                &renderer,
                                &mut encoder,
                                &output,
                                viewport(i as u32),
                                None,
                                [0., 0., 0., 1.],
                                channel(i as u32),
                                None,
                                i != 0,
                            )
                            .unwrap();
                    }
                    renderer
                        .submit_allocated(encoder.finish(), |ledger| {
                            display.collect_texture_resources(ledger);
                            for p in &presenters {
                                p.collect_texture_resources(ledger);
                            }
                        })
                        .unwrap();
                    assert_eq!(renderer.image_readback_info(), reads);
                    let pixels = managed_display::pixels(&renderer, &output);
                    assert_eq!(
                        managed_display::errors(&pixels, &expected, format),
                        0,
                        "case={ci} format={format:?} count={count} phase={phase}"
                    );
                    checked += expected.len();
                    if count == 4 && phase == 0 {
                        // Deliberately false source contract, only for this real
                        // faulty GPU producer: display codes stand in for working values.
                        let wrong_source = renderer
                            .upload_color_image_f32(3, 4, source.contract(), &expected_color)
                            .unwrap();
                        let wrong_display = display.rebind(&renderer, &wrong_source).unwrap();
                        let wrong: Vec<_> = (0..4)
                            .map(|_| prototype.rebind(&renderer, &wrong_display).unwrap())
                            .collect();
                        let mut encoder = renderer
                            .allocation_command_encoder(&Default::default())
                            .unwrap();
                        for (i, p) in wrong.iter().enumerate() {
                            p.encode_inspected_region_view_load(
                                &renderer,
                                &mut encoder,
                                &output,
                                viewport(i as u32),
                                None,
                                [0., 0., 0., 1.],
                                channel(i as u32),
                                None,
                                i != 0,
                            )
                            .unwrap();
                        }
                        renderer
                            .submit_allocated(encoder.finish(), |ledger| {
                                wrong_display.collect_texture_resources(ledger);
                                for p in &wrong {
                                    p.collect_texture_resources(ledger);
                                }
                            })
                            .unwrap();
                        assert!(
                            managed_display::errors(
                                &managed_display::pixels(&renderer, &output),
                                &expected,
                                format
                            ) > 0
                        );
                        false_display_detected += 1;
                        // Actual faulty draw: one writable uniform shared by all cells.
                        let mut encoder = renderer
                            .allocation_command_encoder(&Default::default())
                            .unwrap();
                        for i in 0..4 {
                            prototype
                                .encode_inspected_region_view_load(
                                    &renderer,
                                    &mut encoder,
                                    &output,
                                    viewport(i),
                                    None,
                                    [0., 0., 0., 1.],
                                    channel(i),
                                    None,
                                    i != 0,
                                )
                                .unwrap();
                        }
                        renderer
                            .submit_allocated(encoder.finish(), |ledger| {
                                prototype.collect_texture_resources(ledger)
                            })
                            .unwrap();
                        assert!(
                            managed_display::errors(
                                &managed_display::pixels(&renderer, &output),
                                &expected,
                                format
                            ) > 0
                        );
                        shared_uniform_detected += 1;
                    }
                }
            }
        }
        assert_eq!(
            read(&renderer, &source),
            before,
            "inspection changed working pixels"
        );
    }
    renderer.check_health().unwrap();
    assert_eq!(checked, 6720);
    assert_eq!(false_display_detected, 16);
    assert_eq!(shared_uniform_detected, 16);
    eprintln!(
        "managed numeric component pixels={checked} wrong_display={false_display_detected} shared_uniform={shared_uniform_detected}"
    );
}
