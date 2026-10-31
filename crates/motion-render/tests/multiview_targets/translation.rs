use super::bytes;
use motion_render::{Presenter, Renderer, ViewChannel};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn translated_canvas_pixels_match_origin_for_filtered_patterns_channels_and_formats() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    eprintln!("translation fixture adapter: {:?}", renderer.adapter_info);
    let source = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("translation fixture with high-frequency color and alpha"),
        size: wgpu::Extent3d {
            width: 37,
            height: 29,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pattern: Vec<u8> = (0..29u32)
        .flat_map(|y| {
            (0..37u32).flat_map(move |x| {
                [
                    ((19 * x + 37 * y) % 256) as u8,
                    ((71 * x + 11 * y) % 256) as u8,
                    ((7 * x + 97 * y) % 256) as u8,
                    ((31 * x + 13 * y) % 256) as u8,
                ]
            })
        })
        .collect();
    renderer.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pattern,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(37 * 4),
            rows_per_image: Some(29),
        },
        wgpu::Extent3d {
            width: 37,
            height: 29,
            depth_or_array_layers: 1,
        },
    );
    let source_view = source.create_view(&Default::default());
    let mut comparisons = 0;
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let small = renderer.color_output_target(37, 29, format).unwrap();
        let large = renderer.color_output_target(249, 197, format).unwrap();
        let mut presenter = Presenter::new(&renderer, &source_view, format);
        for channel in [
            ViewChannel::Color,
            ViewChannel::Red,
            ViewChannel::Green,
            ViewChannel::Blue,
            ViewChannel::Alpha,
        ] {
            presenter
                .set_view_channel(&renderer.queue, channel)
                .unwrap();
            for image in [
                None,
                Some([-12., -9., 53.65, 42.05]),
                Some([3.25, 2.5, 29.25, 21.5]),
            ] {
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                presenter
                    .encode_view(
                        &renderer.queue,
                        &mut encoder,
                        &small.view,
                        None,
                        [0., 0., 37., 29.],
                        image,
                        [0., 0., 0., 1.],
                    )
                    .unwrap();
                renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                let expected = bytes(&renderer, &small);
                if format == wgpu::TextureFormat::Rgba8UnormSrgb
                    && channel == ViewChannel::Color
                    && image.is_none()
                {
                    assert_eq!(
                        expected, pattern,
                        "identity must retain the original uploaded texels"
                    );
                }
                for [x, y] in [[1u32, 3u32], [98, 98], [199, 165]] {
                    let translated =
                        image.map(|[ix, iy, w, h]| [ix + x as f32, iy + y as f32, w, h]);
                    let mut encoder = renderer
                        .allocation_command_encoder(&Default::default())
                        .unwrap();
                    presenter
                        .encode_view(
                            &renderer.queue,
                            &mut encoder,
                            &large.view,
                            None,
                            [x as f32, y as f32, 37., 29.],
                            translated,
                            [0., 0., 0., 1.],
                        )
                        .unwrap();
                    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
                    let actual = bytes(&renderer, &large);
                    for row in 0..29usize {
                        let offset = ((y as usize + row) * 249 + x as usize) * 4;
                        assert_eq!(&actual[offset..offset+37*4], &expected[row*37*4..(row+1)*37*4],
                            "format={format:?} channel={channel:?} image={image:?} origin=({x},{y}) row={row}");
                    }
                    comparisons += 1;
                }
            }
        }
        // A channel/geometry update must not alter the ordinary encode binding.
        if format == wgpu::TextureFormat::Rgba8UnormSrgb {
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            presenter.encode(&mut encoder, &small.view, None, None, [0., 0., 0., 1.]);
            renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
            assert_eq!(
                bytes(&renderer, &small),
                pattern,
                "ordinary presentation must retain color and alpha after navigated channel draws"
            );
        }
    }
    assert_eq!(comparisons, 180);
    renderer.check_health().unwrap();
}
