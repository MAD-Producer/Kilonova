use motion_effects::image_contract::{
    ChannelPrecision, DataChannels, ImageContract, ImageMeaning, ImageStorage, ImageUsage,
};
use motion_render::{image_formats, Renderer};

// Use raw floating-point readback: the PNG capture path intentionally converts
// to display color and UNORM8 and cannot validate a numeric/HDR resource.
fn sampled_pixel(renderer: &Renderer, storage: ImageStorage, bytes: &[u8]) -> [f32; 4] {
    assert_eq!(bytes.len() as u64, storage.bytes_per_pixel());
    let capabilities = image_formats::ImageCapabilities::query(&renderer.adapter, &renderer.device);
    capabilities
        .require(
            storage,
            ImageUsage {
                sampled: true,
                copy_dst: true,
                ..Default::default()
            },
        )
        .unwrap();
    capabilities
        .require(
            ImageStorage::Rgba32Float,
            ImageUsage {
                render: true,
                copy_src: true,
                ..Default::default()
            },
        )
        .unwrap();
    let extent = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let source = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("semantic image contract source"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: image_formats::native_format(storage),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    renderer.queue.write_texture(
        source.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: None,
            rows_per_image: None,
        },
        extent,
    );
    let source_view = source.create_view(&Default::default());
    let output = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("unquantized semantic image readback"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&Default::default());
    let input_layout = renderer
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("numeric nonfiltering input"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
    let input = renderer
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("semantic image input"),
            layout: &input_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source_view),
            }],
        });
    let layout = renderer
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("semantic image copy"),
            bind_group_layouts: &[Some(&input_layout)],
            immediate_size: 0,
        });
    let shader = renderer
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("semantic image textureLoad probe"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(positions[index], 0., 1.);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(0), 0);
}
"#
                .into(),
            ),
        });
    let pipeline = renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("semantic image probe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output.format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("semantic float pixel"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("semantic image readback"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &input, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        extent,
    );
    renderer.submit_tracked(encoder.finish(), |_| {});
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");
    receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let pixel = std::array::from_fn(|channel| {
        f32::from_le_bytes(mapped[channel * 4..channel * 4 + 4].try_into().unwrap())
    });
    drop(mapped);
    readback.unmap();
    renderer.check_health().unwrap();
    pixel
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn numeric_and_coverage_samples_do_not_decode_srgb_or_premultiply_alpha() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scalar = ImageContract::new(
        ImageMeaning::Data {
            channels: DataChannels::Scalar,
        },
        ChannelPrecision::Unorm8,
    );
    let rgba = ImageContract::new(
        ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
        ChannelPrecision::Unorm8,
    );
    let coverage = ImageContract::new(ImageMeaning::Coverage {}, ChannelPrecision::Unorm8);
    let expected = 128. / 255.;
    for contract in [scalar, coverage] {
        let sample = sampled_pixel(&renderer, contract.preferred_storage().unwrap(), &[128]);
        assert!(
            (sample[0] - expected).abs() < 0.00001,
            "{contract:?}: {sample:?}"
        );
    }
    let sample = sampled_pixel(&renderer, rgba.preferred_storage().unwrap(), &[128; 4]);
    for value in sample {
        assert!((value - expected).abs() < 0.00001, "{sample:?}");
    }
    let vector2 = sampled_pixel(&renderer, ImageStorage::Rg8Unorm, &[128, 64]);
    assert!((vector2[0] - expected).abs() < 0.00001, "{vector2:?}");
    assert!((vector2[1] - 64. / 255.).abs() < 0.00001, "{vector2:?}");
    assert_eq!(&vector2[2..], &[0., 1.]);
    let color = sampled_pixel(&renderer, ImageStorage::Rgba8UnormSrgb, &[128; 4]);
    let decoded = ((expected + 0.055) / 1.055_f32).powf(2.4);
    // GPU sRGB conversion has implementation-defined approximation error.
    for value in &color[..3] {
        assert!((*value - decoded).abs() < 0.001, "{color:?}");
    }
    assert!(
        (color[3] - expected).abs() < 0.00001,
        "sRGB must not alter alpha: {color:?}"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn half_and_full_float_samples_preserve_negative_and_hdr_channels() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    // Exact IEEE-754 binary16 values: 2, -1, 0.5 and 0.25.
    let half: Vec<u8> = [0x4000_u16, 0xbc00, 0x3800, 0x3400]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let full: Vec<u8> = [2.0_f32, -1., 0.5, 0.25]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    for (storage, bytes) in [
        (ImageStorage::Rgba16Float, half),
        (ImageStorage::Rgba32Float, full),
    ] {
        assert_eq!(
            sampled_pixel(&renderer, storage, &bytes),
            [2., -1., 0.5, 0.25],
            "{storage:?}"
        );
        let narrow = if storage == ImageStorage::Rgba16Float {
            [ImageStorage::R16Float, ImageStorage::Rg16Float]
        } else {
            [ImageStorage::R32Float, ImageStorage::Rg32Float]
        };
        for (index, storage) in narrow.into_iter().enumerate() {
            let sample = sampled_pixel(
                &renderer,
                storage,
                &bytes[..storage.bytes_per_pixel() as usize],
            );
            assert_eq!(
                sample,
                if index == 0 {
                    [2., 0., 0., 1.]
                } else {
                    [2., -1., 0., 1.]
                },
                "{storage:?}"
            );
        }
    }
    // This value is representable in binary32 but rounds to 1 in binary16.
    let precise: Vec<u8> = [1.0001_f32, -0.0001, 4., 1.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(
        sampled_pixel(&renderer, ImageStorage::Rgba32Float, &precise),
        [1.0001, -0.0001, 4., 1.]
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn enabled_device_capabilities_negotiate_four_precisions_without_downcasting() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let usage = ImageUsage {
        sampled: true,
        render: true,
        copy_src: true,
        copy_dst: true,
        ..Default::default()
    };
    let capabilities = image_formats::ImageCapabilities::query(&renderer.adapter, &renderer.device);
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let contract = ImageContract::new(
            ImageMeaning::Data {
                channels: DataChannels::Vector4,
            },
            precision,
        );
        let requested = contract.preferred_storage().unwrap();
        let result = capabilities.negotiate(contract, usage);
        if precision == ChannelPrecision::Unorm16 && capabilities.require(requested, usage).is_err()
        {
            assert!(
                result.is_err(),
                "unsupported UNORM16 cannot become another precision"
            );
            let cause = capabilities.require(requested, usage).unwrap_err();
            assert_eq!(cause.storage, requested);
            assert_ne!(cause.missing, ImageUsage::default());
            continue;
        }
        let storage = result.unwrap();
        assert_eq!(storage, contract.preferred_storage().unwrap());
        assert_eq!(storage.capacity(3, 7), Some(21 * storage.bytes_per_pixel()));
    }
    for storage in [
        ImageStorage::R32Float,
        ImageStorage::Rg32Float,
        ImageStorage::Rgba32Float,
    ] {
        let available = image_formats::capabilities(&renderer.adapter, &renderer.device, storage);
        if !renderer
            .device
            .features()
            .contains(wgpu::Features::FLOAT32_FILTERABLE)
        {
            assert!(!available.linear_filter);
        }
        if !renderer
            .device
            .features()
            .contains(wgpu::Features::FLOAT32_BLENDABLE)
        {
            assert!(!available.blend);
        }
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn integer16_samples_preserve_uniform_steps_distinct_from_half_float() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let capabilities = image_formats::ImageCapabilities::query(&renderer.adapter, &renderer.device);
    let usage = ImageUsage {
        sampled: true,
        copy_dst: true,
        ..Default::default()
    };
    let values = [32768_u16, 32769, 65535, 1];
    let bytes: Vec<_> = values.into_iter().flat_map(u16::to_le_bytes).collect();
    for storage in [
        ImageStorage::R16Unorm,
        ImageStorage::Rg16Unorm,
        ImageStorage::Rgba16Unorm,
    ] {
        if let Err(cause) = capabilities.require(storage, usage) {
            assert!(!renderer
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM));
            assert_eq!(cause.storage, storage);
            assert!(cause.missing.sampled && cause.missing.copy_dst);
            continue;
        }
        let channels = (storage.bytes_per_pixel() / 2) as usize;
        let sample = sampled_pixel(&renderer, storage, &bytes[..channels * 2]);
        for channel in 0..channels {
            let expected = f32::from(values[channel]) / 65535.;
            assert!(
                (sample[channel] - expected).abs() < 0.0000001,
                "{storage:?}: {sample:?}"
            );
        }
        if channels >= 2 {
            assert!(
                sample[1] > sample[0],
                "adjacent integer16 values cannot collapse to half/8-bit: {sample:?}"
            );
        }
        assert_eq!(&sample[channels..], &([0., 0., 0., 1.][channels..]));
    }
    println!(
        "actual semantic format probe adapter={:?} enabled_features={:?}",
        renderer.adapter.get_info(),
        renderer.device.features()
    );
    renderer.check_health().unwrap();
}
