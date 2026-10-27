use super::*;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn floats(
    doc: &mut Value,
    resources: &mut BTreeMap<String, Vec<u8>>,
    shape: &str,
    values: &[f32],
    bounds: Option<([f32; 3], [f32; 3])>,
) -> usize {
    let bytes = resources.get_mut("mesh.bin").unwrap();
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let offset = bytes.len();
    bytes.extend(values.iter().flat_map(|v| v.to_le_bytes()));
    let views = doc["bufferViews"].as_array_mut().unwrap();
    let view = views.len();
    views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes.len()-offset}));
    let accessors = doc["accessors"].as_array_mut().unwrap();
    let id = accessors.len();
    let width = match shape {
        "SCALAR" => 1,
        "VEC3" => 3,
        "VEC4" => 4,
        _ => panic!("fixture shape"),
    };
    accessors.push(
        json!({"bufferView":view,"componentType":5126,"count":values.len()/width,"type":shape}),
    );
    if let Some((min, max)) = bounds {
        doc["accessors"][id]["min"] = json!(min);
        doc["accessors"][id]["max"] = json!(max);
    }
    doc["buffers"][0]["byteLength"] = json!(bytes.len());
    id
}
fn source(material: Value, delta: [f32; 3], normals: bool) -> Arc<motion_scene::Scene> {
    fixture_with(vec![material], &[(0., 0)], None, normals, |doc, res| {
        let d = floats(
            doc,
            res,
            "VEC3",
            &[delta, delta, delta].concat(),
            Some((delta, delta)),
        );
        doc["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":d}]);
        doc["meshes"][0]["weights"] = json!([0.]);
        // Matrix-authored nodes may animate weights, while their TRS stays fixed.
        doc["nodes"][0]["matrix"] = json!(Mat4::IDENTITY.to_cols_array());
        let t = floats(doc, res, "SCALAR", &[0., 1.], None);
        doc["accessors"][t]["min"] = json!([0]);
        doc["accessors"][t]["max"] = json!([1]);
        let w = floats(doc, res, "SCALAR", &[0., 1.], None);
        doc["animations"] = json!([{"channels":[{"sampler":0,"target":{"node":0,"path":"weights"}}],
            "samplers":[{"input":t,"output":w}]}]);
    })
}
fn sample(seconds: f64) -> motion_scene::AnimationSample {
    motion_scene::AnimationSample {
        animation: 0,
        seconds,
    }
}
fn plain(renderer: &Renderer, translation: [f32; 3], material: Value) -> GpuScene {
    GpuScene::upload(
        renderer,
        REQUEST,
        fixture_with(vec![material], &[(0., 0)], None, false, |doc, _| {
            doc["nodes"][0]["translation"] = json!(translation)
        }),
        SceneGpuLimits::default(),
    )
    .unwrap()
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn morph_native_samples_match_independent_static_geometry_and_keep_pending_frames_immutable() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let a = renderer.render_target(SIZE, SIZE).unwrap();
        let b = renderer.render_target(SIZE, SIZE).unwrap();
        let reference = renderer.render_target(SIZE, SIZE).unwrap();
        let mut gpu = GpuScene::upload(
            &renderer,
            REQUEST,
            source(emissive([1., 0., 0.], 1., "OPAQUE"), [0.65, 0., 0.4], false),
            SceneGpuLimits::default(),
        )
        .unwrap();
        let mut static_gpu = plain(
            &renderer,
            [0.65, 0., 0.4],
            emissive([1., 0., 0.], 1., "OPAQUE"),
        );
        for samples in [1, 4] {
            let mut f = frame();
            f.samples = samples;
            f.animation = Some(sample(0.));
            let mut first = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            gpu.encode(&renderer, REQUEST, &mut first, &a, &f).unwrap();
            f.animation = Some(sample(1.));
            let mut second = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            let stats = gpu.encode(&renderer, REQUEST, &mut second, &b, &f).unwrap();
            assert_eq!(stats.deformation_bytes, 3 * 72);
            renderer.submit_allocated(first.finish(), |_| {}).unwrap();
            renderer.submit_allocated(second.finish(), |_| {}).unwrap();
            f.animation = None;
            static_gpu.draw(&renderer, REQUEST, &reference, &f).unwrap();
            assert_eq!(
                pixels(&renderer, &b, precision).pixels,
                pixels(&renderer, &reference, precision).pixels,
                "{precision:?}/{samples}"
            );
            assert_ne!(
                pixels(&renderer, &a, precision).pixels,
                pixels(&renderer, &b, precision).pixels,
                "later morph must not mutate pending earlier draws"
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_morph_uses_displaced_centres_for_global_transparency_and_real_depth() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut red = GpuScene::upload(
        &renderer,
        REQUEST,
        source(emissive([1., 0., 0.], 0.5, "BLEND"), [0., 0., 1.], false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut blue = plain(
        &renderer,
        [0., 0., 0.5],
        emissive([0., 0., 1.], 0.5, "BLEND"),
    );
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    for (seconds, expected) in [(0., [0.25, 0., 0.5, 0.75]), (1., [0.5, 0., 0.25, 0.75])] {
        let mut a = ScenePassInstance::new(0, REQUEST, 0, 1.);
        a.animation = Some(sample(seconds));
        a.lights = &[];
        a.ambient = [0.; 3];
        let mut b = ScenePassInstance::new(1, REQUEST, 0, 1.);
        b.lights = &[];
        b.ambient = [0.; 3];
        pass.draw(
            &renderer,
            REQUEST,
            &target,
            &mut [&mut red, &mut blue],
            &[a, b],
            &ScenePassFrame::new(frame().camera),
        )
        .unwrap();
        let actual = center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8));
        near8(actual, expected);
        assert!(
            super::shared::depth(&renderer, pass.depth().unwrap())
                .iter()
                .all(|&v| v == 1.),
            "blended morph must not write hardware depth"
        );
    }
    // An opaque displaced primitive writes its actual projected Z, never base-mesh Z.
    let mut opaque = GpuScene::upload(
        &renderer,
        REQUEST,
        source(emissive([1.; 3], 1., "OPAQUE"), [0., 0., 1.], false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut item = ScenePassInstance::new(0, REQUEST, 0, 1.);
    item.animation = Some(sample(1.));
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut opaque],
        &[item],
        &ScenePassFrame::new(frame().camera),
    )
    .unwrap();
    let z = super::shared::depth(&renderer, pass.depth().unwrap());
    assert!((z[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (2. - 0.1) / (10. - 0.1)).abs() < 1e-6);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn morph_admission_refusal_preserves_pixels_and_abandoned_vertices_release_capacity() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let scene = source(emissive([1.; 3], 1., "OPAQUE"), [0.6, 0., 0.], false);
    let mut gpu =
        GpuScene::upload(&renderer, REQUEST, scene.clone(), SceneGpuLimits::default()).unwrap();
    let mut f = frame();
    f.animation = Some(sample(0.));
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    let before = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    let budget = renderer.allocation_budget();
    let used = budget.info().unwrap().admitted_bytes;
    budget.set_limit(used + 3 * 72 - 1).unwrap();
    f.animation = Some(sample(1.));
    let err = gpu.draw(&renderer, REQUEST, &target, &f).unwrap_err();
    assert!(err.resource_rejection().is_some());
    assert!(err.to_string().contains("immutable scene morph vertices"));
    assert!(err
        .to_string()
        .contains("node=0 mesh=0 primitive=0 phase=morph"));
    budget.set_limit(128 << 20).unwrap();
    assert_eq!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        before.pixels
    );
    let used = budget.info().unwrap().admitted_bytes;
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    gpu.encode(&renderer, REQUEST, &mut encoder, &target, &f)
        .unwrap();
    assert!(budget.info().unwrap().admitted_bytes > used + 3 * 72);
    drop(encoder);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        used,
        "abandoned immutable vertices and frame uniforms must retire"
    );
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    assert_ne!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        before.pixels
    );
    let mut limited = GpuScene::upload(
        &renderer,
        REQUEST,
        scene,
        SceneGpuLimits {
            resident_bytes: gpu.resident_bytes() + 3 * 72 - 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(limited
        .draw(&renderer, REQUEST, &target, &f)
        .unwrap_err()
        .to_string()
        .contains("resident"));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn morphed_normal_tangent_directions_match_authored_basis_and_refuse_cancellation() {
    fn basis(morphed: bool, cancel: bool) -> Arc<motion_scene::Scene> {
        fixture_with(
            vec![
                json!({"pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0,"roughnessFactor":1},
            "normalTexture":{"index":0}}),
            ],
            &[(0., 0)],
            Some(png8(1, 1, &[255, 128, 128, 255])),
            true,
            |doc, res| {
                let normal = doc["meshes"][0]["primitives"][0]["attributes"]["NORMAL"]
                    .as_u64()
                    .unwrap() as usize;
                if !morphed {
                    let view = doc["accessors"][normal]["bufferView"].as_u64().unwrap() as usize;
                    let offset = doc["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize;
                    let values = [0.6f32, 0., 0.8, 0.6, 0., 0.8, 0.6, 0., 0.8];
                    res.get_mut("mesh.bin").unwrap()[offset..offset + 36].copy_from_slice(
                        &values
                            .iter()
                            .flat_map(|v| v.to_le_bytes())
                            .collect::<Vec<_>>(),
                    );
                }
                let tangent = if morphed {
                    [1., 0., 0., -1.]
                } else {
                    [
                        std::f32::consts::FRAC_1_SQRT_2,
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.,
                        -1.,
                    ]
                };
                let tangent = floats(
                    doc,
                    res,
                    "VEC4",
                    &[tangent, tangent, tangent].concat(),
                    None,
                );
                doc["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(tangent);
                if morphed {
                    let n = if cancel {
                        [0., 0., -1.]
                    } else {
                        [0.3, 0., -0.6]
                    };
                    let dn = floats(doc, res, "VEC3", &[n, n, n].concat(), None);
                    let dt = floats(
                        doc,
                        res,
                        "VEC3",
                        &[-0.5, 0.5, 0., -0.5, 0.5, 0., -0.5, 0.5, 0.],
                        None,
                    );
                    doc["meshes"][0]["primitives"][0]["targets"] =
                        json!([{"NORMAL":dn,"TANGENT":dt}]);
                    doc["meshes"][0]["weights"] = json!([1.]);
                }
            },
        )
    }
    let renderer = gpu_renderer(ChannelPrecision::Float32);
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let mut m = GpuScene::upload(
        &renderer,
        REQUEST,
        basis(true, false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut r = GpuScene::upload(
        &renderer,
        REQUEST,
        basis(false, false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let lights = [SceneLight::Directional {
        direction: [-0.7, -0.4, -1.],
        color: [1.; 3],
        intensity: 3.,
    }];
    let mut f = frame();
    f.lights = &lights;
    m.draw(&renderer, REQUEST, &a, &f).unwrap();
    r.draw(&renderer, REQUEST, &b, &f).unwrap();
    let actual = center_linear(&pixels(&renderer, &a, ChannelPrecision::Float32));
    let expected = center_linear(&pixels(&renderer, &b, ChannelPrecision::Float32));
    assert!(
        actual[0] > 0.01,
        "actual mapped directional light must be visible"
    );
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 2e-5, "{actual:?} != {expected:?}");
    }
    let before = pixels(&renderer, &a, ChannelPrecision::Float32).pixels;
    let mut bad = GpuScene::upload(
        &renderer,
        REQUEST,
        basis(true, true),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let error = bad.draw(&renderer, REQUEST, &a, &f).unwrap_err();
    assert!(error
        .to_string()
        .contains("zero or nonfinite displaced shading direction"));
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Float32).pixels,
        before
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_mesh_nodes_keep_distinct_authored_morph_weights_in_actual_pixels() {
    let material = emissive([0., 1., 0.], 1., "OPAQUE");
    let scene = fixture_with(
        vec![material.clone()],
        &[(0., 0)],
        None,
        false,
        |doc, res| {
            let delta = [0.65, 0., 0.];
            let id = floats(
                doc,
                res,
                "VEC3",
                &[delta, delta, delta].concat(),
                Some((delta, delta)),
            );
            doc["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":id}]);
            doc["meshes"][0]["weights"] = json!([0.]);
            doc["nodes"] = json!([{"mesh":0,"weights":[1.],"translation":[-0.325,0,0]},{"mesh":0,"weights":[0.],"translation":[-0.325,0,0]}]);
            doc["scenes"][0]["nodes"] = json!([0, 1]);
        },
    );
    let reference = fixture_with(vec![material], &[(0., 0)], None, false, |doc, _| {
        doc["nodes"] =
            json!([{"mesh":0,"translation":[0.325,0,0]},{"mesh":0,"translation":[-0.325,0,0]}]);
        doc["scenes"][0]["nodes"] = json!([0, 1]);
    });
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let mut m = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let mut r = GpuScene::upload(&renderer, REQUEST, reference, SceneGpuLimits::default()).unwrap();
    let stats = m.draw(&renderer, REQUEST, &a, &frame()).unwrap();
    r.draw(&renderer, REQUEST, &b, &frame()).unwrap();
    assert_eq!(stats.draws, 2);
    assert_eq!(stats.deformation_bytes, 3 * 72);
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Unorm8).pixels,
        pixels(&renderer, &b, ChannelPrecision::Unorm8).pixels
    );
}
