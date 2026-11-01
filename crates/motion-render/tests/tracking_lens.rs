//! Actual GPU lens primitives and composition-camera routing controls.
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_model::tracking::{lens::LensWarp, plate::*};
use motion_render::{image_formats, ImageCaptureTarget, LensOutputPipeline, Renderer};
const W: u32 = 67;
const H: u32 = 49;
#[path = "tracking_lens/resources.rs"]
mod resources;
#[path = "tracking_lens/routing.rs"]
mod routing;
#[path = "tracking_lens/ownership.rs"]
mod ownership;
fn contract(precision: ChannelPrecision) -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        precision,
    )
}
fn lens(rotation: Rotation, k: f64) -> LensWarp {
    let extent = Extent::new(64, 48).unwrap();
    LensWarp::new(
        Calibration::new(
            extent,
            [52., 45.],
            [30., 23.],
            [k, 0., 0.],
            if k == 0. { [0.; 2] } else { [0.003, -0.002] },
        )
        .unwrap(),
        PlateTransform::new(
            extent,
            [2., 1., 60., 46.],
            rotation,
            Extent::new(W, H).unwrap(),
        )
        .unwrap(),
        1.,
    )
    .unwrap()
}
fn fill(renderer: &Renderer, target: &ImageCaptureTarget, body: &str) {
    let shader=renderer.device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("independently authored lens probe"),source:wgpu::ShaderSource::Wgsl(format!(
        "@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,1.-y*2.,0.,1.);}}\n@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{ {body} }}").into())});
    let pipeline = renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("independent lens source producer"),
            layout: None,
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
                    format: image_formats::native_format(target.storage()),
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
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
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
        pass.draw(0..3, 0..1);
    }
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })
        .unwrap();
}
fn pattern(rect: [f64; 4], size: [u32; 2]) -> String {
    format!("let q=p.xy/vec2({}.0,{}.0)*vec2({:?},{:?})+vec2({:?},{:?});let uv=q/vec2({}.0,{}.0);\nlet check=(i32(floor(q.x/8.))+i32(floor(q.y/6.)))&1;let a=select(0.25,0.65,check==0);\nif distance(uv,vec2(0.5))>0.65{{return vec4(0.);}}return vec4(a*(0.2+uv.x*0.3),a*(0.2+uv.y*0.3),a*0.25,a);",
        size[0],size[1],rect[2],rect[3],rect[0],rect[1],W,H)
}
fn oracle(lens: LensWarp, pixel: [f64; 2]) -> [f64; 2] {
    let c = lens.calibration();
    let source = lens.plate().inverse(pixel).unwrap();
    let f = c.focal().map(|v| v * lens.zoom());
    let centre = c.principal();
    let d = [
        (source[0] - centre[0]) / f[0],
        (source[1] - centre[1]) / f[1],
    ];
    let mut q = d;
    // Separate Newton/Jacobian oracle. Production uses a certified contraction.
    for _ in 0..14 {
        let [x, y] = q;
        let r = x * x + y * y;
        let [k1, k2, k3] = c.radial();
        let [p1, p2] = c.tangential();
        let a = 1. + k1 * r + k2 * r * r + k3 * r * r * r;
        let b = k1 + 2. * k2 * r + 3. * k3 * r * r;
        let e = [
            x * a + 2. * p1 * x * y + p2 * (r + 2. * x * x) - d[0],
            y * a + p1 * (r + 2. * y * y) + 2. * p2 * x * y - d[1],
        ];
        let j = [
            a + 2. * x * x * b + 2. * p1 * y + 6. * p2 * x,
            2. * x * y * b + 2. * p1 * x + 2. * p2 * y,
            a + 2. * y * y * b + 6. * p1 * y + 2. * p2 * x,
        ];
        let det = j[0] * j[2] - j[1] * j[1];
        q[0] -= (j[2] * e[0] - j[1] * e[1]) / det;
        q[1] -= (j[0] * e[1] - j[1] * e[0]) / det;
    }
    lens.plate()
        .apply([q[0] * f[0] + centre[0], q[1] * f[1] + centre[1]])
        .unwrap()
}
fn bilinear(bytes: &[u8], size: [u32; 2], pixel: [f64; 2]) -> [f64; 4] {
    let x = pixel[0].floor() as i32;
    let y = pixel[1].floor() as i32;
    let fx = pixel[0].fract();
    let fy = pixel[1].fract();
    let fx = if fx < 0. { fx + 1. } else { fx };
    let fy = if fy < 0. { fy + 1. } else { fy };
    let mut out = [0.; 4];
    for (dx, dy, weight) in [
        (0, 0, (1. - fx) * (1. - fy)),
        (1, 0, fx * (1. - fy)),
        (0, 1, (1. - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let [px, py] = [x + dx, y + dy];
        if px < 0 || py < 0 || px >= size[0] as i32 || py >= size[1] as i32 {
            continue;
        }
        let at = (py as usize * size[0] as usize + px as usize) * 4;
        for channel in 0..4 {
            let v = f64::from(bytes[at + channel]) / 255.;
            let linear = if channel == 3 {
                v
            } else if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
            out[channel] += linear * weight;
        }
    }
    // Capture bytes are sRGB; textureLoad and shader interpolation operate in
    // linear light, then the destination attachment applies the transfer.
    std::array::from_fn(|i| {
        let v = out[i];
        255. * if i == 3 {
            v
        } else if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    })
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn actual_gpu_radial_tangential_crop_and_rotation_match_an_independent_inverse_oracle() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let pipeline = LensOutputPipeline::new(
        &renderer,
        image_formats::native_format(image_formats::color_storage(ChannelPrecision::Unorm8)),
    )
    .unwrap();
    let directory = std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from);
    let mut rows = Vec::new();
    for (rotation_id, rotation) in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ]
    .into_iter()
    .enumerate()
    {
        for (sign_id, k) in [-0.025, 0.025].into_iter().enumerate() {
            let lens = lens(rotation, k);
            let raster = lens.raster(1.).unwrap();
            let size = raster.size();
            let rect = raster.rect();
            let source = renderer
                .image_capture_target(size[0], size[1], contract(ChannelPrecision::Unorm8))
                .unwrap();
            let destination = renderer
                .image_capture_target(W, H, contract(ChannelPrecision::Unorm8))
                .unwrap();
            fill(&renderer, &source, &pattern(rect, size));
            let original = renderer.read_image_target(&source).unwrap();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            pipeline
                .encode_images(&renderer, &mut encoder, lens, raster, &source, &destination)
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(source.texture_resource_ledger());
                    ledger.merge(destination.texture_resource_ledger());
                })
                .unwrap();
            let actual = renderer.read_image_target(&destination).unwrap();
            let mut max_error = 0u8;
            let mut identity_difference = 0usize;
            for y in 0..H {
                for x in 0..W {
                    let point = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                    let q = oracle(lens, point);
                    let expected = bilinear(
                        &original.pixels,
                        size,
                        [
                            (q[0] - rect[0]) / rect[2] * f64::from(size[0]) - 0.5,
                            (q[1] - rect[1]) / rect[3] * f64::from(size[1]) - 0.5,
                        ],
                    );
                    let wrong = bilinear(
                        &original.pixels,
                        size,
                        [
                            (point[0] - rect[0]) / rect[2] * f64::from(size[0]) - 0.5,
                            (point[1] - rect[1]) / rect[3] * f64::from(size[1]) - 0.5,
                        ],
                    );
                    let at = (y as usize * W as usize + x as usize) * 4;
                    for c in 0..4 {
                        max_error = max_error
                            .max(actual.pixels[at + c].abs_diff(expected[c].round() as u8));
                    }
                    identity_difference +=
                        usize::from((0..4).any(|c| (expected[c] - wrong[c]).abs() > 3.));
                }
            }
            assert!(
                max_error <= 1,
                "rotation={rotation:?} radial={k} max_error={max_error}"
            );
            assert!(
                identity_difference > 80,
                "fixture must detect skipped distortion: {identity_difference}"
            );
            if let Some(directory) = &directory {
                std::fs::create_dir_all(directory).unwrap();
                image::save_buffer(
                    directory.join(format!("lens-{rotation_id}-{sign_id}.png")),
                    &actual.pixels,
                    W,
                    H,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            rows.push(serde_json::json!({"rotation":format!("{rotation:?}"),"radial":k,"maxByteError":max_error,"detectsIdentityPixels":identity_difference,"sourceSize":size,"sourceRect":rect}));
        }
    }
    if let Some(directory) = directory {
        std::fs::write(
            directory.join("oracle.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn actual_gpu_identity_is_byte_exact_at_all_four_precisions_and_hdr_is_retained() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let identity = lens(Rotation::Clockwise90, 0.);
    let raster = identity.raster(1.).unwrap();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let source = renderer
            .image_capture_target(W, H, contract(precision))
            .unwrap();
        let target = renderer
            .image_capture_target(W, H, contract(precision))
            .unwrap();
        let pipeline =
            LensOutputPipeline::new(&renderer, image_formats::native_format(target.storage()))
                .unwrap();
        let hdr = matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        );
        fill(
            &renderer,
            &source,
            if hdr {
                "return vec4(f32(u32(p.x)%4u)*0.125-0.125,2.+f32(u32(p.y)%4u)*0.25,0.0625,select(0.25,0.5,(u32(p.x)+u32(p.y))%3u==0u));"
            } else {
                "return vec4(f32(u32(p.x)%4u)*0.125,f32(u32(p.y)%4u)*0.125,0.0625,select(0.25,0.5,(u32(p.x)+u32(p.y))%3u==0u));"
            },
        );
        let before = renderer.read_image_target(&source).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        pipeline
            .encode_images(&renderer, &mut encoder, identity, raster, &source, &target)
            .unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(source.texture_resource_ledger());
                ledger.merge(target.texture_resource_ledger());
            })
            .unwrap();
        let after = renderer.read_image_target(&target).unwrap();
        assert_eq!(before.pixels, after.pixels, "identity {precision:?}");
        if precision == ChannelPrecision::Float32 {
            let v: Vec<_> = after.pixels[..16]
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            for (actual, expected) in v.iter().zip([-0.125, 2., 0.0625, 0.5]) {
                assert!((actual - expected).abs() < 0.000001, "{v:?}");
            }
        }
        // Nonlinear remapping of a constant field also retains the same native
        // precision, negative channels, HDR and alpha without a display LUT.
        let distorted = lens(Rotation::Clockwise90, 0.025);
        let r = distorted.raster(1.).unwrap();
        let s = r.size();
        let input = renderer
            .image_capture_target(s[0], s[1], contract(precision))
            .unwrap();
        fill(
            &renderer,
            &input,
            if hdr {
                "return vec4(-0.125,2.500123,0.0625123,0.50003);"
            } else {
                "return vec4(0.125,0.25003,0.0625123,0.50003);"
            },
        );
        let constant = renderer.read_image_target(&input).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        pipeline
            .encode_images(&renderer, &mut encoder, distorted, r, &input, &target)
            .unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(input.texture_resource_ledger());
                ledger.merge(target.texture_resource_ledger());
            })
            .unwrap();
        let image = renderer.read_image_target(&target).unwrap();
        let bpp = image.storage.bytes_per_pixel() as usize;
        for pixel in image.pixels.chunks_exact(bpp) {
            match precision {
                ChannelPrecision::Float32 => {
                    for (i, v) in pixel.chunks_exact(4).enumerate() {
                        let a = f32::from_le_bytes(v.try_into().unwrap());
                        let b = f32::from_le_bytes(
                            constant.pixels[i * 4..i * 4 + 4].try_into().unwrap(),
                        );
                        assert!((a - b).abs() < 0.000001);
                    }
                }
                ChannelPrecision::Float16 => assert_eq!(pixel, &constant.pixels[..bpp]),
                ChannelPrecision::Unorm16 => {
                    for (i, v) in pixel.chunks_exact(2).enumerate() {
                        let a = u16::from_le_bytes(v.try_into().unwrap());
                        let b = u16::from_le_bytes(
                            constant.pixels[i * 2..i * 2 + 2].try_into().unwrap(),
                        );
                        assert!(a.abs_diff(b) <= 1);
                    }
                }
                ChannelPrecision::Unorm8 => {
                    for i in 0..4 {
                        assert!(pixel[i].abs_diff(constant.pixels[i]) <= 1);
                    }
                }
            }
        }
    }
}
