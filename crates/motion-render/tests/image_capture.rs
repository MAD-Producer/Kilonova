use motion_effects::image_contract::{
    ChannelPrecision, DataChannels, ImageContract, ImageMeaning, ImageStorage,
};
use motion_effects::{AlphaMode, WorkingSpace};
use motion_render::{image_formats, ImageCaptureTarget, Renderer};

fn clear(renderer: &Renderer, target: &ImageCaptureTarget, color: wgpu::Color) {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("raw capture precision and row alignment"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })
        .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn raw_capture_retains_all_precision_modes_rows_alpha_and_allocation_capacity() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (precision, expected_storage, expected_pixel) in [
        (
            ChannelPrecision::Unorm8,
            ImageStorage::Rgba8Unorm,
            vec![255, 0, 255, 0],
        ),
        (
            ChannelPrecision::Unorm16,
            ImageStorage::Rgba16Unorm,
            [65535_u16, 0, 65535, 0]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        (
            ChannelPrecision::Float16,
            ImageStorage::Rgba16Float,
            [0x4000_u16, 0xbc00, 0x3800, 0x3400]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        (
            ChannelPrecision::Float32,
            ImageStorage::Rgba32Float,
            [2.0_f32, -1., 0.5, 0.25]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
        ),
    ] {
        let contract = ImageContract::new(
            ImageMeaning::Data {
                channels: DataChannels::Vector4,
            },
            precision,
        );
        // Odd width and multiple rows expose incorrect four-byte and row-stride assumptions.
        let target = match renderer.image_capture_target(17, 3, contract) {
            Ok(target) => target,
            Err(cause) if precision == ChannelPrecision::Unorm16 => {
                assert!(
                    !renderer
                        .device
                        .features()
                        .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
                        || !image_formats::capabilities(
                            &renderer.adapter,
                            &renderer.device,
                            expected_storage
                        )
                        .render
                );
                assert!(cause.to_string().contains("format-capabilities"), "{cause}");
                continue;
            }
            Err(cause) => panic!("{precision:?}: {cause}"),
        };
        assert_eq!(target.storage(), expected_storage);
        assert_eq!((target.width(), target.height()), (17, 3));
        assert_eq!(target.contract(), contract);
        let packed = 17 * 3 * expected_storage.bytes_per_pixel();
        assert_eq!(target.texture_bytes(), packed);
        assert_eq!(target.texture_resource_ledger().total_known_bytes(), packed);
        assert_eq!(
            target.readback_bytes(),
            if precision == ChannelPrecision::Float32 {
                1536
            } else {
                768
            }
        );
        // Use exact UNORM endpoints here to isolate raw readback from clear
        // rounding. Nonzero numeric RGB at zero alpha must remain untouched.
        let authored = if matches!(
            precision,
            ChannelPrecision::Unorm8 | ChannelPrecision::Unorm16
        ) {
            wgpu::Color {
                r: 1.,
                g: 0.,
                b: 1.,
                a: 0.,
            }
        } else {
            wgpu::Color {
                r: 2.,
                g: -1.,
                b: 0.5,
                a: 0.25,
            }
        };
        for color in [authored, wgpu::Color::TRANSPARENT] {
            clear(&renderer, &target, color);
            let before = renderer.image_readback_info();
            let result = renderer.read_image_target(&target).unwrap();
            let after = renderer.image_readback_info();
            assert_eq!(after.requested_calls, before.requested_calls + 1);
            assert_eq!(after.completed_calls, before.completed_calls + 1);
            assert_eq!(
                after.submitted_payload_bytes,
                before.submitted_payload_bytes + packed
            );
            assert_eq!(
                after.returned_payload_bytes,
                before.returned_payload_bytes + packed
            );
            assert_eq!(
                after.mapped_buffer_bytes,
                before.mapped_buffer_bytes + target.readback_bytes()
            );
            assert_eq!(after.failed_calls, before.failed_calls);
            assert_eq!(after.in_flight_calls, 0);
            assert_eq!((result.width, result.height), (17, 3));
            assert_eq!(
                (result.contract, result.storage),
                (contract, expected_storage)
            );
            assert_eq!(result.pixels.len() as u64, packed);
            let expected = if color == wgpu::Color::TRANSPARENT {
                vec![0; expected_pixel.len()]
            } else {
                expected_pixel.clone()
            };
            for pixel in result.pixels.chunks_exact(expected.len()) {
                assert_eq!(pixel, expected, "{precision:?}");
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_color_and_scalar_data_readback_preserve_hdr_sign_and_fine_detail() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        ChannelPrecision::Float32,
    );
    let target = renderer.image_capture_target(3, 2, contract).unwrap();
    clear(
        &renderer,
        &target,
        wgpu::Color {
            r: 1.0001_f32 as f64,
            g: -0.0001_f32 as f64,
            b: 4.,
            a: 0.5,
        },
    );
    let result = renderer.read_image_target(&target).unwrap();
    let expected: Vec<u8> = [1.0001_f32, -0.0001, 4., 0.5]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert!(result
        .pixels
        .chunks_exact(16)
        .all(|pixel| pixel == expected));

    let scalar = ImageContract::new(
        ImageMeaning::Data {
            channels: DataChannels::Scalar,
        },
        ChannelPrecision::Float32,
    );
    let target = renderer.image_capture_target(3, 2, scalar).unwrap();
    clear(
        &renderer,
        &target,
        wgpu::Color {
            r: -3.25,
            g: 0.,
            b: 0.,
            a: 1.,
        },
    );
    let result = renderer.read_image_target(&target).unwrap();
    for pixel in result
        .pixels
        .chunks_exact(result.storage.bytes_per_pixel() as usize)
    {
        assert_eq!(f32::from_le_bytes(pixel[..4].try_into().unwrap()), -3.25);
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_capture_requests_fail_before_allocation_and_png_rejects_float_explicitly() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let contract = ImageContract::new(ImageMeaning::Coverage, ChannelPrecision::Float16);
    for (width, height) in [(0, 1), (1, 0), (u32::MAX, 1), (1, u32::MAX)] {
        let error = renderer
            .image_capture_target(width, height, contract)
            .err()
            .unwrap();
        assert!(error.to_string().contains("image capture"), "{error}");
        renderer.check_health().unwrap();
    }
    let mut value = serde_json::to_value(contract).unwrap();
    value["version"] = 2.into();
    let future = serde_json::from_value(value).unwrap();
    let error = renderer.image_capture_target(2, 2, future).err().unwrap();
    assert!(
        error.to_string().contains("UnsupportedVersion(2)"),
        "{error}"
    );
    renderer.check_health().unwrap();

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba16Float,
    ))
    .unwrap();
    let error = renderer.capture_target(3, 2).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("PNG capture does not accept Rgba16Float"),
        "{error}"
    );
    renderer.check_health().unwrap();
    let raw = renderer.image_capture_target(3, 2, contract).unwrap();
    renderer.device.destroy();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");
    let primary = renderer.check_health().unwrap_err().to_string();
    assert_eq!(
        renderer.capture_target(3, 2).err().unwrap().to_string(),
        primary
    );
    assert_eq!(
        renderer
            .image_capture_target(3, 2, contract)
            .err()
            .unwrap()
            .to_string(),
        primary
    );
    assert_eq!(
        renderer.read_image_target(&raw).err().unwrap().to_string(),
        primary
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn raw_capture_admits_texture_and_aligned_staging_and_rolls_back_refused_growth() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let budget = renderer.allocation_budget();
    let baseline = budget.info().unwrap().admitted_bytes;
    let contract = ImageContract::new(
        ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
        ChannelPrecision::Float32,
    );
    let texture_bytes = 17 * 3 * 16;
    let staging_bytes = 512 * 3;
    let total = baseline + texture_bytes + staging_bytes;
    // Texture admission succeeds; the aligned staging reservation is one byte
    // over budget. Both reservations must retire on the refused construction.
    budget.set_limit(total - 1).unwrap();
    let cause = renderer
        .image_capture_target(17, 3, contract)
        .err()
        .unwrap();
    assert_eq!(
        cause.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: total,
            budget_bytes: total - 1,
        })
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline);
    budget.set_limit(total).unwrap();
    let target = renderer.image_capture_target(17, 3, contract).unwrap();
    assert_eq!(target.texture_bytes(), texture_bytes);
    assert_eq!(target.readback_bytes(), staging_bytes);
    assert_eq!(budget.info().unwrap().admitted_bytes, total);
    clear(
        &renderer,
        &target,
        wgpu::Color {
            r: 2.,
            g: -1.,
            b: 0.5,
            a: 0.25,
        },
    );
    assert_eq!(
        renderer.read_image_target(&target).unwrap().pixels.len() as u64,
        texture_bytes
    );
    drop(target);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(budget.info().unwrap().admitted_bytes, baseline);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn foreign_renderer_readback_is_refused_before_gpu_use_without_poisoning_the_target() {
    let owner = pollster::block_on(Renderer::headless()).unwrap();
    let other = pollster::block_on(Renderer::headless()).unwrap();
    let contract = ImageContract::new(ImageMeaning::Coverage, ChannelPrecision::Float16);
    let target = owner.image_capture_target(3, 2, contract).unwrap();
    clear(&owner, &target, wgpu::Color::WHITE);
    let before = other.image_readback_info();
    let error = other.read_image_target(&target).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("target belongs to another renderer"),
        "{error}"
    );
    let after = other.image_readback_info();
    assert_eq!(after.failed_calls, before.failed_calls + 1);
    assert_eq!(after.submitted_calls, before.submitted_calls);
    assert_eq!(after.in_flight_calls, 0);
    let pixels = owner.read_image_target(&target).unwrap();
    assert!(pixels
        .pixels
        .chunks_exact(2)
        .all(|pixel| pixel == 0x3c00_u16.to_le_bytes()));
    owner.check_health().unwrap();
    other.check_health().unwrap();
}
