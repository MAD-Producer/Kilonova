//! Actual native mesh/material/depth pixels, with independent analytical goldens.
use glam::{Mat4, Vec3};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_model::{CameraPose, ProjectionKind};
use motion_render::{
    scene_gpu::{GpuScene, SceneFrame, SceneGpuError, SceneGpuLimits, SceneLight, SceneRequest},
    AllocationBudget, RenderTarget, Renderer,
};
use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io, sync::Arc};

const SIZE: u32 = 65;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn scene_native_access_rejects_a_foreign_thread_before_allocation_and_recovers() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let source = fixture(vec![json!({"emissiveFactor":[1.,0.,0.]})], &[(0., 0)], None, true);
    let gpu = GpuScene::upload(&renderer, REQUEST, source, SceneGpuLimits::default()).unwrap();
    let (renderer, target, mut gpu) = std::thread::spawn(move || {
        let mut gpu = gpu;
        let before = renderer.allocation_budget().info().unwrap();
        let error = gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap_err();
        assert!(error.to_string().contains("owning thread"));
        assert_eq!(renderer.allocation_budget().info().unwrap().admitted_bytes, before.admitted_bytes);
        (renderer, target, gpu)
    }).join().unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    assert!(pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels.iter().any(|b| *b != 0));
}
const REQUEST: SceneRequest = SceneRequest {
    request: 8703,
    source: 87,
};
struct Assets(BTreeMap<String, Vec<u8>>);
impl AssetResolver for Assets {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        let bytes = self.0.get(path).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "original fixture resource absent")
        })?;
        if bytes.len() > limit {
            return Err(io::Error::other("fixture resource exceeds allowance"));
        }
        Ok(bytes.clone())
    }
}
fn fixture(
    materials: Vec<Value>,
    primitives: &[(f32, usize)],
    texture: Option<Vec<u8>>,
    normals: bool,
) -> Arc<motion_scene::Scene> {
    fixture_with(materials, primitives, texture, normals, |_, _| {})
}
fn fixture_with(
    materials: Vec<Value>,
    primitives: &[(f32, usize)],
    texture: Option<Vec<u8>>,
    normals: bool,
    change: impl FnOnce(&mut Value, &mut BTreeMap<String, Vec<u8>>),
) -> Arc<motion_scene::Scene> {
    let mut bytes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut draws = Vec::new();
    for &(z, material) in primitives {
        let position = [-0.9, -0.9, z, 0.9, -0.9, z, 0., 0.9, z];
        let normal = [0., 0., 1., 0., 0., 1., 0., 0., 1.];
        let uv = [0.25, 0.25, 0.25, 0.25, 0.25, 0.25];
        let first = accessors.len();
        for (values, kind, bounds) in [
            (
                &position[..],
                "VEC3",
                Some(([-0.9, -0.9, z], [0.9, 0.9, z])),
            ),
            (&normal[..], "VEC3", None),
            (&uv[..], "VEC2", None),
        ] {
            let offset = bytes.len();
            bytes.extend(values.iter().flat_map(|v| f32::to_le_bytes(*v)));
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes.len()-offset}));
            let mut accessor =
                json!({"bufferView":views.len()-1,"componentType":5126,"count":3,"type":kind});
            if let Some((min, max)) = bounds {
                accessor["min"] = json!(min);
                accessor["max"] = json!(max);
            }
            accessors.push(accessor);
        }
        let mut attributes = json!({"POSITION":first,"TEXCOORD_0":first+2});
        if normals {
            attributes["NORMAL"] = json!(first + 1);
        }
        draws.push(json!({"attributes":attributes,"material":material}));
    }
    let mut document = json!({"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":bytes.len()}],"bufferViews":views,"accessors":accessors,
        "materials":materials,"meshes":[{"primitives":draws}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
    let mut resources = BTreeMap::from([("mesh.bin".into(), bytes)]);
    if let Some(texture) = texture {
        document["images"] = json!([{"uri":"texture.png"}]);
        document["textures"] = json!([{"source":0,"sampler":0}]);
        document["samplers"] =
            json!([{"magFilter":9728,"minFilter":9728,"wrapS":33071,"wrapT":33071}]);
        resources.insert("texture.png".into(), texture);
    }
    change(&mut document, &mut resources);
    Arc::new(
        motion_scene::import(
            ImportContext {
                request: 8701,
                source: 87,
            },
            &serde_json::to_vec(&document).unwrap(),
            &mut Assets(resources),
            &ImportLimits::default(),
        )
        .unwrap(),
    )
}
fn gpu_renderer(precision: ChannelPrecision) -> Renderer {
    pollster::block_on(Renderer::headless_with_working_precision(
        precision,
        AllocationBudget::new(128 << 20).unwrap(),
    ))
    .unwrap()
}
fn frame() -> SceneFrame<'static> {
    let eye = Vec3::new(0., 0., 3.);
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let mut f = SceneFrame::new(
        CameraPose {
            eye,
            target: Vec3::ZERO,
            projection: ProjectionKind::Orthographic,
            view_projection: Mat4::orthographic_rh(-1., 1., -1., 1., 0.1, 10.) * view,
        },
        0,
        1.,
    );
    f.lights = &[];
    f.ambient = [0.; 3];
    f
}
fn pixels(
    renderer: &Renderer,
    target: &RenderTarget,
    precision: ChannelPrecision,
) -> motion_render::CapturedImage {
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        precision,
    );
    let storage = contract.preferred_storage().unwrap();
    let row_bytes = SIZE * storage.bytes_per_pixel() as u32;
    let stride =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let context = renderer.allocation_context();
    let buffer = context
        .buffer(&wgpu::BufferDescriptor {
            label: Some("Scene acceptance format-preserving readback"),
            size: u64::from(stride) * u64::from(SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        target.view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
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
    let pixels = mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..row_bytes as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    motion_render::CapturedImage {
        contract,
        storage,
        width: SIZE,
        height: SIZE,
        pixels,
    }
}
fn center8(image: &motion_render::CapturedImage) -> [u8; 4] {
    let index = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    image.pixels[index..index + 4].try_into().unwrap()
}
fn srgb(value: f64) -> u8 {
    ((if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }) * 255.)
        .round() as u8
}
fn near8(actual: [u8; 4], linear: [f64; 4]) {
    let expected = [
        srgb(linear[0]),
        srgb(linear[1]),
        srgb(linear[2]),
        (linear[3] * 255.).round() as u8,
    ];
    for i in 0..4 {
        assert!(
            actual[i].abs_diff(expected[i]) <= 1,
            "channel {i}: {actual:?} expected {expected:?}"
        );
    }
}
fn emissive(rgb: [f32; 3], alpha: f32, mode: &str) -> Value {
    json!({"pbrMetallicRoughness":{"baseColorFactor":[0,0,0,alpha],"metallicFactor":0,"roughnessFactor":1},"emissiveFactor":rgb,"alphaMode":mode})
}
fn png8(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(rgba)
            .unwrap();
    }
    out
}
fn drain(renderer: &Renderer) {
    let encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn metallic_roughness_matches_an_independent_direct_light_solution() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    for (base, metallic, expected) in [
        ([1., 0., 0.], 0., [0.97, 0.01, 0.01, 1.]),
        ([0.2, 0.4, 0.8], 1., [0.05, 0.1, 0.2, 1.]),
    ] {
        let scene = fixture(
            vec![
                json!({"pbrMetallicRoughness":{"baseColorFactor":[base[0],base[1],base[2],1],"metallicFactor":metallic,"roughnessFactor":1}}),
            ],
            &[(0., 0)],
            None,
            true,
        );
        let mut gpu =
            GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
        let mut f = frame();
        let lights = [SceneLight::Directional {
            direction: [0., 0., -1.],
            color: [1.; 3],
            intensity: std::f32::consts::PI,
        }];
        f.lights = &lights;
        let stats = gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
        assert_eq!(stats.draws, 1);
        assert_eq!(stats.triangles, 1);
        near8(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            expected,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn depth_alpha_mask_and_blend_use_actual_pixels() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    for (materials, order, expected) in [
        (
            vec![
                emissive([1., 0., 0.], 1., "OPAQUE"),
                emissive([0., 0., 1.], 1., "OPAQUE"),
            ],
            vec![(0.2, 0), (0., 1)],
            [1., 0., 0., 1.],
        ),
        (
            vec![
                emissive([1., 0., 0.], 1., "OPAQUE"),
                emissive([0., 0., 1.], 1., "OPAQUE"),
            ],
            vec![(0., 1), (0.2, 0)],
            [1., 0., 0., 1.],
        ),
        (
            vec![
                emissive([1., 0., 0.], 0.5, "BLEND"),
                emissive([0., 0., 1.], 0.5, "BLEND"),
            ],
            vec![(0.3, 0), (0., 1)],
            [0.5, 0., 0.25, 0.75],
        ),
        (
            vec![emissive([1., 0., 0.], 0.25, "MASK")],
            vec![(0., 0)],
            [0.; 4],
        ),
        (
            vec![emissive([1., 0., 0.], 0.75, "MASK")],
            vec![(0., 0)],
            [1., 0., 0., 1.],
        ),
        (
            vec![
                emissive([0., 1., 0.], 1., "OPAQUE"),
                emissive([1., 0., 0.], 0.5, "BLEND"),
            ],
            vec![(0.3, 0), (0., 1)],
            [0., 1., 0., 1.],
        ),
    ] {
        let mut gpu = GpuScene::upload(
            &renderer,
            REQUEST,
            fixture(materials, &order, None, true),
            SceneGpuLimits::default(),
        )
        .unwrap();
        gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
        near8(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            expected,
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn encoded_srgb_texture_has_a_linear_emissive_result() {
    let mut material = emissive([1.; 3], 1., "OPAQUE");
    material["emissiveTexture"] = json!({"index":0});
    let scene = fixture(
        vec![material],
        &[(0., 0)],
        Some(png8(
            2,
            2,
            &[
                128, 64, 32, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
            ],
        )),
        true,
    );
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    let center = center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8));
    assert_eq!(
        center,
        [128, 64, 32, 255],
        "sRGB input must not be double-decoded"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn obj_classic_unlit_material_is_distinct_from_pbr_and_generates_flat_normals() {
    let obj = b"mtllib paint.mtl\nusemtl Paint\nv -0.9 -0.9 0\nv 0.9 -0.9 0\nv 0 0.9 0\nf 1 2 3";
    let scene = motion_scene::obj::import_obj(
        ImportContext {
            request: 8701,
            source: 87,
        },
        obj,
        &mut Assets(BTreeMap::from([(
            "paint.mtl".into(),
            b"newmtl Paint\nKd 0.2 0.4 0.8\nillum 0".to_vec(),
        )])),
        &motion_scene::obj::ObjOptions::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        Arc::new(scene),
        SceneGpuLimits::default(),
    )
    .unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0.2, 0.4, 0.8, 1.],
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_encodes_before_submission_keep_uniforms_and_reservations_independent() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    drain(&renderer);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let scene = fixture(
        vec![emissive([1., 0., 0.], 1., "OPAQUE")],
        &[(0., 0)],
        None,
        false,
    );
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    gpu.encode(&renderer, REQUEST, &mut encoder, &a, &frame())
        .unwrap();
    let mut moved = frame();
    moved.world = Mat4::from_translation(Vec3::new(3., 0., 0.));
    gpu.encode(&renderer, REQUEST, &mut encoder, &b, &moved)
        .unwrap();
    let admitted = renderer.allocation_budget().info().unwrap().admitted_bytes;
    drop(gpu);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        admitted,
        "pending encoder owns the scene reservations"
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    near8(
        center8(&pixels(&renderer, &a, ChannelPrecision::Unorm8)),
        [1., 0., 0., 1.],
    );
    near8(
        center8(&pixels(&renderer, &b, ChannelPrecision::Unorm8)),
        [0.; 4],
    );
    drop((a, b));
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_owner_budget_and_corrupt_image_failures_remain_explicit() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let scene = fixture(
        vec![emissive([1.; 3], 1., "OPAQUE")],
        &[(0., 0)],
        None,
        true,
    );
    let error = GpuScene::upload(
        &renderer,
        REQUEST,
        scene.clone(),
        SceneGpuLimits {
            resident_bytes: 1,
            ..SceneGpuLimits::default()
        },
    )
    .err()
    .unwrap();
    assert!(matches!(error, SceneGpuError::ResidentBudget { .. }));
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let other = gpu_renderer(ChannelPrecision::Unorm8);
    assert!(gpu
        .draw(&other, REQUEST, &target, &frame())
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    let mut bad = frame();
    bad.world = Mat4::from_scale(Vec3::ZERO);
    assert!(gpu
        .draw(&renderer, REQUEST, &target, &bad)
        .unwrap_err()
        .to_string()
        .contains("singular"));
    bad = frame();
    bad.world = Mat4::perspective_rh(1., 1., 0.1, 10.);
    assert!(gpu
        .draw(&renderer, REQUEST, &target, &bad)
        .unwrap_err()
        .to_string()
        .contains("affine"));
    bad = frame();
    bad.camera.view_projection = Mat4::ZERO;
    assert!(gpu
        .draw(&renderer, REQUEST, &target, &bad)
        .unwrap_err()
        .to_string()
        .contains("invertible"));
    let mut m = emissive([1.; 3], 1., "OPAQUE");
    m["emissiveTexture"] = json!({"index":0});
    let corrupt = fixture(
        vec![m],
        &[(0., 0)],
        Some(b"\x89PNG\r\n\x1a\n".to_vec()),
        true,
    );
    assert!(matches!(
        GpuScene::upload(&renderer, REQUEST, corrupt, SceneGpuLimits::default())
            .err()
            .unwrap(),
        SceneGpuError::Image { image: 0, .. }
    ));
}

fn center_linear(image: &motion_render::CapturedImage) -> [f64; 4] {
    use motion_effects::image_contract::ImageStorage;
    let unit = image.storage.bytes_per_pixel() as usize;
    let offset = (SIZE / 2 * SIZE + SIZE / 2) as usize * unit;
    let pixel = &image.pixels[offset..offset + unit];
    std::array::from_fn(|i| match image.storage {
        ImageStorage::Rgba8UnormSrgb => {
            let v = f64::from(pixel[i]) / 255.;
            if i == 3 {
                v
            } else if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        ImageStorage::Rgba16Unorm => {
            f64::from(u16::from_le_bytes(
                pixel[i * 2..i * 2 + 2].try_into().unwrap(),
            )) / 65535.
        }
        ImageStorage::Rgba16Float => {
            let bits = u16::from_le_bytes(pixel[i * 2..i * 2 + 2].try_into().unwrap());
            let exponent = (bits >> 10) & 31;
            assert_ne!(exponent, 31, "finite scene output required");
            let mantissa = f64::from(bits & 1023);
            let value = if exponent == 0 {
                mantissa * 2f64.powi(-24)
            } else {
                (1. + mantissa / 1024.) * 2f64.powi(i32::from(exponent) - 15)
            };
            if bits & 0x8000 != 0 {
                -value
            } else {
                value
            }
        }
        ImageStorage::Rgba32Float => f64::from(f32::from_le_bytes(
            pixel[i * 4..i * 4 + 4].try_into().unwrap(),
        )),
        other => panic!("unexpected color storage {other:?}"),
    })
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn four_working_precisions_preserve_authored_values_and_hdr() {
    for (precision, tolerance) in [
        (ChannelPrecision::Unorm8, 0.005),
        (ChannelPrecision::Unorm16, 1. / 65535.),
        (ChannelPrecision::Float16, 0.0005),
        (ChannelPrecision::Float32, 1e-6),
    ] {
        let renderer = gpu_renderer(precision);
        let target = renderer.render_target(SIZE, SIZE).unwrap();
        let scene = fixture(
            vec![emissive([0.123456, 0.4321, 0.765432], 1., "OPAQUE")],
            &[(0., 0)],
            None,
            true,
        );
        let mut gpu =
            GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
        gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
        let actual = center_linear(&pixels(&renderer, &target, precision));
        for (a, expected) in actual.into_iter().zip([0.123456, 0.4321, 0.765432, 1.]) {
            assert!(
                (a - expected).abs() <= tolerance,
                "{precision:?}: {actual:?}"
            );
        }
        if matches!(
            precision,
            ChannelPrecision::Float16 | ChannelPrecision::Float32
        ) {
            // HDR comes from actual irradiance, not an out-of-spec emissive factor.
            let scene = fixture(
                vec![
                    json!({"pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0,"roughnessFactor":1}}),
                ],
                &[(0., 0)],
                None,
                true,
            );
            let mut gpu =
                GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
            let mut f = frame();
            let light = [SceneLight::Directional {
                direction: [0., 0., -1.],
                color: [1.; 3],
                intensity: std::f32::consts::PI * 4.,
            }];
            f.lights = &light;
            gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
            let actual = center_linear(&pixels(&renderer, &target, precision));
            for &value in &actual[..3] {
                assert!(
                    (value - 3.88).abs() <= tolerance * 4.,
                    "{precision:?}: {actual:?}"
                );
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn four_sample_resolve_produces_fractional_edge_coverage() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        fixture(
            vec![emissive([1.; 3], 1., "OPAQUE")],
            &[(0., 0)],
            None,
            true,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut f = frame();
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    let single = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    assert!(single
        .pixels
        .chunks_exact(4)
        .all(|p| p[3] == 0 || p[3] == 255));
    f.samples = 4;
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    let multi = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    let edge: Vec<_> = multi
        .pixels
        .chunks_exact(4)
        .filter(|p| p[3] > 0 && p[3] < 255)
        .collect();
    assert!(!edge.is_empty(), "MSAA must change actual edge coverage");
    for p in edge {
        // Native UNORM resolves may choose either adjacent storage code. Test
        // the exact quarter-coverage interval, not one tie-rounding direction.
        let scaled = u32::from(p[3]) * 4;
        assert!(
            (1..=3).any(|samples| scaled.abs_diff(samples * 255) <= 3),
            "{p:?}"
        );
    }
    assert_eq!(center8(&multi), [255; 4]);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn reflected_world_retains_front_faces_and_double_sided_back_faces() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    for double in [false, true] {
        let mut material = emissive([0., 1., 0.], 1., "OPAQUE");
        material["doubleSided"] = json!(double);
        let mut gpu = GpuScene::upload(
            &renderer,
            REQUEST,
            fixture(vec![material], &[(0., 0)], None, true),
            SceneGpuLimits::default(),
        )
        .unwrap();
        let mut f = frame();
        f.world = Mat4::from_scale(Vec3::new(-1., 1., 1.));
        gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
        assert_eq!(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            [0, 255, 0, 255]
        );
        f.world = Mat4::from_rotation_y(std::f32::consts::PI);
        gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
        assert_eq!(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            if double { [0, 255, 0, 255] } else { [0; 4] }
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn png16_retains_sub_byte_color_precision_without_a_silent_downgrade() {
    let mut texture = Vec::new();
    let values = [32769u16, 17001, 53003, 65535];
    {
        let mut encoder = png::Encoder::new(&mut texture, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(
                &values
                    .into_iter()
                    .flat_map(u16::to_be_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
    }
    let mut material = emissive([1.; 3], 1., "OPAQUE");
    material["emissiveTexture"] = json!({"index":0});
    let source = fixture(vec![material], &[(0., 0)], Some(texture), true);
    let renderer = gpu_renderer(ChannelPrecision::Float32);
    assert!(renderer
        .device
        .features()
        .contains(wgpu::Features::FLOAT32_FILTERABLE));
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        source.clone(),
        SceneGpuLimits::default(),
    )
    .unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    let actual = center_linear(&pixels(&renderer, &target, ChannelPrecision::Float32));
    for i in 0..3 {
        let encoded = f64::from(values[i]) / 65535.;
        let expected = ((encoded + 0.055) / 1.055).powf(2.4);
        assert!(
            (actual[i] - expected).abs() < 2e-7,
            "channel {i}: {actual:?}"
        );
        let byte = (encoded * 255.).round() / 255.;
        let quantized = ((byte + 0.055) / 1.055).powf(2.4);
        assert!(
            (actual[i] - quantized).abs() > 1e-4,
            "8-bit downgrade must visibly differ"
        );
    }
    let baseline = gpu_renderer(ChannelPrecision::Unorm8);
    if !baseline
        .device
        .features()
        .contains(wgpu::Features::FLOAT32_FILTERABLE)
    {
        assert!(matches!(
            GpuScene::upload(&baseline, REQUEST, source, SceneGpuLimits::default()),
            Err(SceneGpuError::Unsupported {
                capability:
                    "16-bit image requires filterable Float32 storage; no precision downgrade",
                ..
            })
        ));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn material_data_maps_use_linear_channels_and_the_selected_uv_set() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    // Metallic is B, roughness is G, R must not influence either value.
    let material = json!({"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.4,0.8,1],"metallicFactor":1,"roughnessFactor":1,"metallicRoughnessTexture":{"index":0}}});
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        fixture(
            vec![material],
            &[(0., 0)],
            Some(png8(1, 1, &[7, 255, 255, 255])),
            true,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let lights = [SceneLight::Directional {
        direction: [0., 0., -1.],
        color: [1.; 3],
        intensity: std::f32::consts::PI,
    }];
    let mut f = frame();
    f.lights = &lights;
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0.05, 0.1, 0.2, 1.],
    );
    let material = json!({"pbrMetallicRoughness":{"baseColorFactor":[0.5,0.5,0.5,1],"metallicFactor":0},"occlusionTexture":{"index":0,"strength":0.5}});
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        fixture(
            vec![material],
            &[(0., 0)],
            Some(png8(1, 1, &[64, 255, 0, 255])),
            true,
        ),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut f = frame();
    f.ambient = [std::f32::consts::PI; 3];
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    let expected = 0.5 * (0.5 + 0.5 * 64. / 255.);
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [expected, expected, expected, 1.],
    );
    let mut material = emissive([1.; 3], 1., "OPAQUE");
    material["emissiveTexture"] = json!({"index":0,"texCoord":1});
    let source = fixture_with(
        vec![material],
        &[(0., 0)],
        Some(png8(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255])),
        true,
        |document, resources| {
            let buffer = resources.get_mut("mesh.bin").unwrap();
            let offset = buffer.len();
            for value in [0.75f32, 0.25, 0.75, 0.25, 0.75, 0.25] {
                buffer.extend(value.to_le_bytes());
            }
            document["buffers"][0]["byteLength"] = json!(buffer.len());
            let views = document["bufferViews"].as_array_mut().unwrap();
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":24}));
            let accessors = document["accessors"].as_array_mut().unwrap();
            let accessor = accessors.len();
            accessors.push(json!({"bufferView":view,"componentType":5126,"count":3,"type":"VEC2"}));
            document["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_1"] = json!(accessor);
        },
    );
    let mut gpu = GpuScene::upload(&renderer, REQUEST, source, SceneGpuLimits::default()).unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    assert_eq!(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [0, 255, 0, 255]
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn minified_odd_texture_includes_the_last_texel_in_linear_light() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut image = vec![0; 3 * 3 * 4];
    for p in image.chunks_exact_mut(4) {
        p[3] = 255;
    }
    image[8 * 4..9 * 4].copy_from_slice(&[255; 4]);
    let mut material = emissive([1.; 3], 1., "OPAQUE");
    material["emissiveTexture"] = json!({"index":0});
    let source = fixture_with(
        vec![material],
        &[(0., 0)],
        Some(png8(3, 3, &image)),
        true,
        |document, resources| {
            document["samplers"][0]["minFilter"] = json!(9984);
            let view = document["accessors"][2]["bufferView"].as_u64().unwrap() as usize;
            let offset = document["bufferViews"][view]["byteOffset"]
                .as_u64()
                .unwrap() as usize;
            let buffer = resources.get_mut("mesh.bin").unwrap();
            for (i, value) in [0f32, 0., 128., 0., 0., 128.].into_iter().enumerate() {
                buffer[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        },
    );
    let mut gpu = GpuScene::upload(&renderer, REQUEST, source, SceneGpuLimits::default()).unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    // A cropped 2x2 average would be zero, encoded-space averaging 28 codes.
    // Exact area integration over all nine texels gives 1/9 in linear light.
    near8(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [1. / 9., 1. / 9., 1. / 9., 1.],
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn obj_normal_map_rotates_the_actual_lambert_response_with_its_uv_frame() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let obj = b"mtllib paint.mtl\nusemtl Paint\nv -0.9 -0.9 0\nv 0.9 -0.9 0\nv 0 0.9 0\nvt 0 0\nvt 1 0\nvt 0.5 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1";
    for mapped in [false, true] {
        let mut mtl = b"newmtl Paint\nKd 0.5 0.5 0.5\nillum 1\n".to_vec();
        if mapped {
            mtl.extend(b"norm normal.png\n");
        }
        let source = motion_scene::obj::import_obj(
            ImportContext {
                request: 8701,
                source: 87,
            },
            obj,
            &mut Assets(BTreeMap::from([
                ("paint.mtl".into(), mtl),
                ("normal.png".into(), png8(1, 1, &[255, 128, 128, 255])),
            ])),
            &Default::default(),
            &ImportLimits::default(),
        )
        .unwrap();
        let mut gpu = GpuScene::upload(
            &renderer,
            REQUEST,
            Arc::new(source),
            SceneGpuLimits::default(),
        )
        .unwrap();
        for reflected in [false, true] {
            let mut f = frame();
            if reflected {
                f.world = Mat4::from_scale(Vec3::new(-1., 1., 1.));
            }
            let lights = [SceneLight::Directional {
                direction: [if reflected { 1. } else { -1. }, 0., 0.],
                color: [1.; 3],
                intensity: 1.,
            }];
            f.lights = &lights;
            gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
            let expected = if mapped {
                0.5 / (1f64 + 2. / (255. * 255.)).sqrt()
            } else {
                0.
            };
            near8(
                center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
                [expected, expected, expected, 1.],
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn image_decode_and_edge_refusals_retire_partial_uploads_and_recover() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    drain(&renderer);
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut material = emissive([1.; 3], 1., "OPAQUE");
    material["emissiveTexture"] = json!({"index":0});
    let source = fixture(
        vec![material],
        &[(0., 0)],
        Some(png8(2, 2, &[255; 16])),
        true,
    );
    let cause = GpuScene::upload(
        &renderer,
        REQUEST,
        source.clone(),
        SceneGpuLimits {
            decoded_image_bytes: 1,
            ..Default::default()
        },
    )
    .err()
    .unwrap();
    assert!(cause.resource_rejection().is_some(), "{cause}");
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
    let cause = GpuScene::upload(
        &renderer,
        REQUEST,
        source.clone(),
        SceneGpuLimits {
            max_image_edge: 1,
            ..Default::default()
        },
    )
    .err()
    .unwrap();
    assert!(matches!(
        cause,
        SceneGpuError::Invalid {
            phase: "image",
            object: 0,
            ..
        }
    ));
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
    let mut gpu = GpuScene::upload(&renderer, REQUEST, source, Default::default()).unwrap();
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    gpu.draw(&renderer, REQUEST, &target, &frame()).unwrap();
    assert_eq!(
        center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
        [255; 4]
    );
    drop((gpu, target));
    drain(&renderer);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        baseline
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn explicit_mip_upload_checks_owner_and_bounds_before_queue_entry() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let other = gpu_renderer(ChannelPrecision::Unorm8);
    let context = renderer.allocation_context();
    let texture = context
        .texture(&wgpu::TextureDescriptor {
            label: Some("Scene explicit mip upload acceptance"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 3,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .unwrap();
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(8),
        rows_per_image: Some(2),
    };
    let size = wgpu::Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    assert!(context
        .write_texture_mip(&texture, 3, &[255; 16], layout, size)
        .unwrap_err()
        .to_string()
        .contains("mip is out of range"));
    assert!(other
        .allocation_context()
        .write_texture_mip(&texture, 1, &[255; 16], layout, size)
        .unwrap_err()
        .to_string()
        .contains("another renderer"));
    context
        .write_texture_mip(&texture, 1, &[255; 16], layout, size)
        .unwrap();
    drop(texture);
    drain(&renderer);
}
