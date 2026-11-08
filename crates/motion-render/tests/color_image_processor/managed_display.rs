use super::*;
use motion_render::{ManagedDisplayPresenter, ManagedDisplayTarget};

pub(super) fn pixels(renderer: &Renderer, texture: &wgpu::Texture) -> Vec<u8> {
    let stride = texture.width().div_ceil(64) * 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("managed display qualification readback"),
        size: u64::from(stride * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
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
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..texture.width() as usize * 4].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}
pub(super) fn surface(renderer: &Renderer, format: wgpu::TextureFormat) -> wgpu::Texture {
    sized_surface(renderer, format, 3, 4)
}
fn sized_surface(
    renderer: &Renderer,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("actual managed SDR display attachment"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
pub(super) fn errors(bytes: &[u8], expected: &[[f32; 4]], format: wgpu::TextureFormat) -> usize {
    assert_eq!(
        bytes.len(),
        expected.len() * 4,
        "compare the entire attachment"
    );
    bytes
        .chunks_exact(4)
        .zip(expected)
        .filter(|(actual, e)| {
            let mut actual: [u8; 4] = (*actual).try_into().unwrap();
            if matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ) {
                actual.swap(0, 2);
            }
            (0..4).any(|c| actual[c].abs_diff((e[c].clamp(0., 1.) * 255.).round() as u8) > 1)
        })
        .count()
}

// CPU interpolation of the immutable native reference, before SDR quantization.
// The original reference values and tolerances are never rewritten.
fn enlarged_reference(source: &[[f32; 4]]) -> Vec<[f32; 4]> {
    assert_eq!(source.len(), 12);
    (0..8)
        .flat_map(|y| {
            (0..6).map(move |x| {
                let px = (x as f32 + 0.5) / 2. - 0.5;
                let py = (y as f32 + 0.5) / 2. - 0.5;
                let left = px.floor() as i32;
                let top = py.floor() as i32;
                let fx = px - px.floor();
                let fy = py - py.floor();
                let pixel = |x: i32, y: i32| source[(y.clamp(0, 3) * 3 + x.clamp(0, 2)) as usize];
                std::array::from_fn(|c| {
                    let upper = pixel(left, top)[c] * (1. - fx) + pixel(left + 1, top)[c] * fx;
                    let lower =
                        pixel(left, top + 1)[c] * (1. - fx) + pixel(left + 1, top + 1)[c] * fx;
                    upper * (1. - fy) + lower * fy
                })
            })
        })
        .collect()
}

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn native_display_surface_codes_match_original_cpu_references_without_double_encoding() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime =
        motion_effects::native_color::NativeColorRuntime::load(&std::path::PathBuf::from(
            std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit runtime"),
        ))
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut compared = 0;
    let mut scaled_compared = 0;
    for (case_index, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, case)| case.processor.phase == ColorProcessorPhase::WorkingToDisplay)
    {
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
        let processor = native.source().compile_image(config, selection).unwrap();
        let input = renderer
            .upload_color_image_f32(
                3,
                4,
                ColorImageContract {
                    space: processor.input_space(),
                    range: CodeValueRange::FULL,
                    alpha: processor.input_alpha(),
                    precision: ChannelPrecision::Float32,
                },
                &case.input,
            )
            .unwrap();
        let display = ManagedDisplayTarget::new(&renderer, &input, &processor).unwrap();
        assert_eq!(display.dimensions(), [3, 4]);
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let presenter = ManagedDisplayPresenter::new(&renderer, &display, format).unwrap();
            let output = surface(&renderer, format);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            display.encode(&renderer, &mut encoder).unwrap();
            presenter.encode(&renderer, &mut encoder, &output).unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    display.collect_texture_resources(ledger);
                    presenter.collect_texture_resources(ledger);
                })
                .unwrap();
            let actual = pixels(&renderer, &output);
            assert_eq!(
                errors(&actual, &case.expected, format),
                0,
                "case={case_index} format={format:?} actual={actual:?}"
            );
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            presenter
                .encode_view(
                    &renderer,
                    &mut encoder,
                    &output,
                    [0., 0., 3., 4.],
                    None,
                    [0.; 4],
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    presenter.collect_texture_resources(ledger)
                })
                .unwrap();
            assert_eq!(
                pixels(&renderer, &output),
                actual,
                "canvas and full-surface paths agree"
            );
            let scaled = sized_surface(&renderer, format, 6, 8);
            let expected_scaled = enlarged_reference(&case.expected);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            presenter.encode(&renderer, &mut encoder, &scaled).unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    presenter.collect_texture_resources(ledger)
                })
                .unwrap();
            let scaled_actual = pixels(&renderer, &scaled);
            assert_eq!(
                errors(&scaled_actual, &expected_scaled, format),
                0,
                "bilinear native display case={case_index} format={format:?}"
            );
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            presenter
                .encode_view(
                    &renderer,
                    &mut encoder,
                    &scaled,
                    [0., 0., 6., 8.],
                    None,
                    [0.; 4],
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    presenter.collect_texture_resources(ledger)
                })
                .unwrap();
            assert_eq!(
                pixels(&renderer, &scaled),
                scaled_actual,
                "scaled canvas uses the same display interpolation"
            );
            scaled_compared += expected_scaled.len();
            if case_index == 2 {
                let double_encoded = "return vec4(mix(12.92*outColor.rgb,1.055*pow(max(outColor.rgb,vec3(0.)),vec3(1./2.4))-0.055,step(vec3(0.0031308),outColor.rgb)),outColor.a);";
                for replacement in ["return inPixel;", double_encoded] {
                    let mut wrong = native.source().clone();
                    assert!(wrong.shader.contains("return outColor;"));
                    wrong.shader = wrong.shader.replacen("return outColor;", replacement, 1);
                    let wrong = wrong.compile_image(config, selection).unwrap();
                    let wrong_display =
                        ManagedDisplayTarget::new(&renderer, &input, &wrong).unwrap();
                    let wrong_presenter =
                        ManagedDisplayPresenter::new(&renderer, &wrong_display, format).unwrap();
                    let mut encoder = renderer
                        .allocation_command_encoder(&Default::default())
                        .unwrap();
                    wrong_display.encode(&renderer, &mut encoder).unwrap();
                    wrong_presenter
                        .encode(&renderer, &mut encoder, &output)
                        .unwrap();
                    renderer
                        .submit_allocated(encoder.finish(), |ledger| {
                            wrong_display.collect_texture_resources(ledger);
                            wrong_presenter.collect_texture_resources(ledger);
                        })
                        .unwrap();
                    assert!(errors(&pixels(&renderer, &output), &case.expected, format) > 0,
                        "actual omitted/double encoding GPU route must fail unchanged CPU/SDR8 oracle");
                }
                let wrong_format = if format.is_srgb() {
                    wgpu::TextureFormat::Rgba8Unorm
                } else {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                };
                let mismatch =
                    ManagedDisplayPresenter::new(&renderer, &display, wrong_format).unwrap();
                let before = pixels(&renderer, &output);
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                assert!(mismatch.encode(&renderer, &mut encoder, &output).is_err());
                for viewport in [
                    [f32::NAN, 0., 3., 4.],
                    [-1., 0., 3., 4.],
                    [0., 0., 0., 4.],
                    [1., 0., 3., 4.],
                ] {
                    assert!(presenter
                        .encode_view(&renderer, &mut encoder, &output, viewport, None, [0.; 4])
                        .is_err());
                }
                assert!(presenter
                    .encode_view(
                        &renderer,
                        &mut encoder,
                        &output,
                        [0., 0., 3., 4.],
                        Some([0., 0., f32::INFINITY, 4.]),
                        [0.; 4]
                    )
                    .is_err());
                assert!(presenter
                    .encode_view(
                        &renderer,
                        &mut encoder,
                        &output,
                        [0., 0., 3., 4.],
                        None,
                        [f32::NAN; 4]
                    )
                    .is_err());
                renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                assert_eq!(
                    pixels(&renderer, &output),
                    before,
                    "refused consumer must not write any pixels"
                );
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                presenter.encode(&renderer, &mut encoder, &output).unwrap();
                renderer
                    .submit_allocated(encoder.finish(), |ledger| {
                        presenter.collect_texture_resources(ledger)
                    })
                    .unwrap();
                assert_eq!(
                    pixels(&renderer, &output),
                    actual,
                    "explicit recovery restores the original result"
                );
            }
            compared += 12;
        }
        if case_index == 2 {
            let budget = renderer.allocation_budget().clone();
            let before = budget.info().unwrap();
            budget.set_limit(before.admitted_bytes + 63).unwrap();
            let refused =
                ManagedDisplayPresenter::new(&renderer, &display, wgpu::TextureFormat::Rgba8Unorm)
                    .err()
                    .unwrap();
            assert!(refused.resource_rejection().is_some());
            assert!(refused.to_string().contains("canvas navigation"));
            let after = budget.info().unwrap();
            assert_eq!(after.admitted_bytes, before.admitted_bytes);
            assert_eq!(after.live_allocations, before.live_allocations);
            budget.set_limit(before.admitted_bytes + 64).unwrap();
            let exact =
                ManagedDisplayPresenter::new(&renderer, &display, wgpu::TextureFormat::Rgba8Unorm)
                    .unwrap();
            assert_eq!(
                budget.info().unwrap().admitted_bytes,
                before.admitted_bytes + 64
            );
            drop(exact);
            assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
            budget.set_limit(before.budget_bytes).unwrap();
            let foreign = pollster::block_on(Renderer::headless()).unwrap();
            assert!(ManagedDisplayPresenter::new(
                &foreign,
                &display,
                wgpu::TextureFormat::Rgba8Unorm
            )
            .is_err());
            assert!(ManagedDisplayTarget::new(&foreign, &input, &processor).is_err());
            let presenter =
                ManagedDisplayPresenter::new(&renderer, &display, wgpu::TextureFormat::Rgba8Unorm)
                    .unwrap();
            let output = surface(&renderer, wgpu::TextureFormat::Rgba8Unorm);
            let mut foreign_encoder = foreign
                .allocation_command_encoder(&Default::default())
                .unwrap();
            assert!(presenter
                .encode(&foreign, &mut foreign_encoder, &output)
                .is_err());
            assert!(presenter
                .encode(&renderer, &mut foreign_encoder, &output)
                .is_err());
            drop(foreign_encoder);
            foreign.check_health().unwrap();
            assert!(ManagedDisplayPresenter::new(
                &renderer,
                &display,
                wgpu::TextureFormat::Rgba16Float
            )
            .is_err());
            let limited = native
                .source()
                .compile_image_with_ranges(
                    config,
                    selection,
                    CodeValueRange::FULL,
                    CodeValueRange::new(0.1, 0.9).unwrap(),
                )
                .unwrap();
            let limited = ManagedDisplayTarget::new(&renderer, &input, &limited).unwrap();
            assert!(ManagedDisplayPresenter::new(
                &renderer,
                &limited,
                wgpu::TextureFormat::Rgba8Unorm
            )
            .is_err());
            let input_case = &reference.cases[0];
            let input_processor = runtime
                .prepare(
                    config,
                    selection,
                    input_case.processor.phase,
                    input_case.processor.input_alpha,
                    input_case.processor.output_alpha,
                )
                .unwrap();
            let input_processor = input_processor
                .source()
                .compile_image(config, selection)
                .unwrap();
            let admitted = renderer.allocation_budget().info().unwrap().admitted_bytes;
            assert!(ManagedDisplayTarget::new(&renderer, &input, &input_processor).is_err());
            assert_eq!(
                renderer.allocation_budget().info().unwrap().admitted_bytes,
                admitted,
                "wrong endpoint refuses before target/pipeline allocation"
            );
        }
    }
    assert_eq!(compared, 384);
    assert_eq!(scaled_compared, 1536);
    renderer.check_health().unwrap();
    println!("managed native display original CPU pixels={compared}, scaled CPU pixels={scaled_compared}; RGBA/BGRA UNORM/sRGB attachments passed");
}
