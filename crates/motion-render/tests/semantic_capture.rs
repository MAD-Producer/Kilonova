use motion_effects::image_contract::{
    ChannelPrecision as P, ImageContract, ImageMeaning as M, ImageStorage as S,
};
use motion_render::Renderer;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn named_numeric_capture_preserves_unbounded_depth_weight_and_signed_normals_at_zero_alpha() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let probes = [
        (
            M::Weight { normalized: true },
            P::Unorm8,
            S::R8Unorm,
            0.25,
            vec![64],
        ),
        (
            M::Weight { normalized: false },
            P::Float16,
            S::R16Float,
            1.5,
            0x3e00_u16.to_le_bytes().to_vec(),
        ),
        (
            M::RelativeDepth {},
            P::Float16,
            S::R16Float,
            0.5,
            0x3800_u16.to_le_bytes().to_vec(),
        ),
        (
            M::LinearDepth {},
            P::Float32,
            S::R32Float,
            2.5,
            2.5_f32.to_le_bytes().to_vec(),
        ),
        (
            M::Normal {},
            P::Float16,
            S::Rgba16Float,
            -1.,
            [0xbc00_u16, 0, 0, 0]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
    ];
    for (meaning, precision, storage, red, expected) in probes {
        let contract = ImageContract::new(meaning, precision);
        let target = renderer.image_capture_target(17, 3, contract).unwrap();
        assert_eq!(target.storage(), storage);
        assert_eq!(target.texture_bytes(), 17 * 3 * storage.bytes_per_pixel());
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("numeric semantic storage without transfer or alpha conversion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: red,
                            g: 0.,
                            b: 0.,
                            a: 0.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(target.texture_resource_ledger());
            })
            .unwrap();
        let image = renderer.read_image_target(&target).unwrap();
        assert_eq!(image.contract, contract);
        assert_eq!(image.storage, storage);
        for pixel in image.pixels.chunks_exact(expected.len()) {
            assert_eq!(pixel, expected, "{meaning:?} {precision:?}");
        }
        renderer.check_health().unwrap();
    }
}
