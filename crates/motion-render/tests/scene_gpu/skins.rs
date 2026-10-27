use super::morph::floats;
use super::*;
use motion_render::scene_gpu::{ScenePass, ScenePassFrame, ScenePassInstance};

fn influences(doc: &mut Value, resources: &mut BTreeMap<String, Vec<u8>>, primitive: usize) {
    let bytes = resources.get_mut("mesh.bin").unwrap();
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let offset = bytes.len();
    bytes.extend_from_slice(&[0u8, 1, 0, 0].repeat(3));
    let views = doc["bufferViews"].as_array_mut().unwrap();
    let view = views.len();
    views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":12}));
    let accessors = doc["accessors"].as_array_mut().unwrap();
    let j = accessors.len();
    accessors.push(json!({"bufferView":view,"componentType":5121,"count":3,"type":"VEC4"}));
    doc["buffers"][0]["byteLength"] = json!(bytes.len());
    let w = floats(
        doc,
        resources,
        "VEC4",
        &[0.25, 0.75, 0., 0.].repeat(3),
        None,
    );
    doc["meshes"][0]["primitives"][primitive]["attributes"]["JOINTS_0"] = json!(j);
    doc["meshes"][0]["primitives"][primitive]["attributes"]["WEIGHTS_0"] = json!(w);
}
fn source(material: Value, morph: bool) -> Arc<motion_scene::Scene> {
    fixture_with(vec![material], &[(0., 0)], None, false, |doc, res| {
        influences(doc, res, 0);
        // The skin ignores this singular instantiating matrix, not the joints.
        doc["nodes"] = json!([{"mesh":0,"skin":0,"scale":[0.,0.,0.],"translation":[100.,0.,0.]},{"children":[2]},{"translation":[0.8,0.,0.4]}]);
        doc["skins"] = json!([{"joints":[1,2],"skeleton":1}]);
        doc["scenes"][0]["nodes"] = json!([0, 1]);
        if morph {
            let delta = [0.2, 0., 0.];
            let d = floats(doc, res, "VEC3", &[delta; 3].concat(), Some((delta, delta)));
            doc["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":d}]);
            doc["meshes"][0]["weights"] = json!([1.]);
        }
        let t = floats(doc, res, "SCALAR", &[0., 1.], None);
        doc["accessors"][t]["min"] = json!([0.]);
        doc["accessors"][t]["max"] = json!([1.]);
        let p = floats(doc, res, "VEC3", &[0.8, 0., 0.4, -0.8, 0., -0.4], None);
        doc["animations"] = json!([{"samplers":[{"input":t,"output":p}],"channels":[{"sampler":0,"target":{"node":2,"path":"translation"}}]}]);
    })
}
fn sample(seconds: f64) -> motion_scene::AnimationSample {
    motion_scene::AnimationSample {
        animation: 0,
        seconds,
    }
}
fn plain(renderer: &Renderer, position: [f32; 3], material: Value) -> GpuScene {
    GpuScene::upload(
        renderer,
        REQUEST,
        fixture_with(vec![material], &[(0., 0)], None, false, |doc, _| {
            doc["nodes"][0]["translation"] = json!(position)
        }),
        SceneGpuLimits::default(),
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_joint_morph_pixels_match_independent_geometry_in_four_precisions_and_pending_frames() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let renderer = gpu_renderer(precision);
        let a = renderer.render_target(SIZE, SIZE).unwrap();
        let b = renderer.render_target(SIZE, SIZE).unwrap();
        let r = renderer.render_target(SIZE, SIZE).unwrap();
        let source = source(emissive([1., 0., 0.], 1., "OPAQUE"), true);
        let before = source.clone();
        let mut gpu = GpuScene::upload(
            &renderer,
            REQUEST,
            source.clone(),
            SceneGpuLimits::default(),
        )
        .unwrap();
        for samples in [1, 4] {
            let mut f = frame();
            f.samples = samples;
            f.animation = Some(sample(0.));
            let mut first = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            assert_eq!(
                gpu.encode(&renderer, REQUEST, &mut first, &a, &f)
                    .unwrap()
                    .deformation_bytes,
                3 * 72
            );
            f.animation = Some(sample(1.));
            let mut second = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            gpu.encode(&renderer, REQUEST, &mut second, &b, &f).unwrap();
            renderer.submit_allocated(first.finish(), |_| {}).unwrap();
            renderer.submit_allocated(second.finish(), |_| {}).unwrap();
            f.animation = None;
            let mut reference = plain(
                &renderer,
                [-0.4, 0., -0.3],
                emissive([1., 0., 0.], 1., "OPAQUE"),
            );
            reference.draw(&renderer, REQUEST, &r, &f).unwrap();
            assert_eq!(
                pixels(&renderer, &b, precision).pixels,
                pixels(&renderer, &r, precision).pixels,
                "{precision:?}/{samples}/last"
            );
            let mut reference = plain(
                &renderer,
                [0.8, 0., 0.3],
                emissive([1., 0., 0.], 1., "OPAQUE"),
            );
            reference.draw(&renderer, REQUEST, &r, &f).unwrap();
            assert_eq!(
                pixels(&renderer, &a, precision).pixels,
                pixels(&renderer, &r, precision).pixels,
                "{precision:?}/{samples}/first"
            );
            assert_ne!(
                pixels(&renderer, &a, precision).pixels,
                pixels(&renderer, &b, precision).pixels
            );
        }
        assert_eq!(source, before);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_skin_depth_and_transparency_use_the_deformed_world_centre() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut red = GpuScene::upload(
        &renderer,
        REQUEST,
        source(emissive([1., 0., 0.], 0.5, "BLEND"), false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut blue = plain(
        &renderer,
        [0., 0., 0.],
        emissive([0., 0., 1.], 0.5, "BLEND"),
    );
    let mut pass = ScenePass::new(&renderer, 8).unwrap();
    for (seconds, expected) in [(0., [0.5, 0., 0.25, 0.75]), (1., [0.25, 0., 0.5, 0.75])] {
        let mut a = ScenePassInstance::new(0, REQUEST, 0, 1.);
        a.animation = Some(sample(seconds));
        a.lights = &[];
        a.ambient = [0.; 3];
        a.world = Mat4::from_translation(Vec3::new(if seconds == 0. { -0.6 } else { 0.6 }, 0., 0.));
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
        near8(
            center8(&pixels(&renderer, &target, ChannelPrecision::Unorm8)),
            expected,
        );
        let depth = super::shared::depth(&renderer, pass.depth().unwrap());
        assert!(
            depth.iter().all(|&z| z == 1.),
            "blended skin must not write hardware depth"
        );
    }
    let mut opaque = GpuScene::upload(
        &renderer,
        REQUEST,
        source(emissive([1., 0., 0.], 1., "OPAQUE"), false),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut a = ScenePassInstance::new(0, REQUEST, 0, 1.);
    a.animation = Some(sample(0.));
    a.lights = &[];
    a.ambient = [0.; 3];
    a.world = Mat4::from_translation(Vec3::new(-0.6, 0., 0.));
    pass.draw(
        &renderer,
        REQUEST,
        &target,
        &mut [&mut opaque],
        &[a],
        &ScenePassFrame::new(frame().camera),
    )
    .unwrap();
    let depth = super::shared::depth(&renderer, pass.depth().unwrap());
    assert!(
        (depth[(SIZE / 2 * SIZE + SIZE / 2) as usize] - (3. - 0.3 - 0.1) / (10. - 0.1)).abs()
            < 1e-6
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn skin_budget_refusal_preserves_pixels_and_abandoned_buffers_release_capacity() {
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let source = source(emissive([1.; 3], 1., "OPAQUE"), true);
    let mut gpu = GpuScene::upload(
        &renderer,
        REQUEST,
        source.clone(),
        SceneGpuLimits::default(),
    )
    .unwrap();
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
    assert!(err
        .to_string()
        .contains("node=0 mesh=0 primitive=0 phase=skin"));
    assert!(err.to_string().contains("immutable scene skin vertices"));
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
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    gpu.draw(&renderer, REQUEST, &target, &f).unwrap();
    assert_ne!(
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels,
        before.pixels
    );
    let mut limited = GpuScene::upload(
        &renderer,
        REQUEST,
        source,
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
fn shared_mesh_different_skins_and_unskinned_primitive_keep_independent_transforms() {
    let scene = fixture_with(
        vec![
            emissive([1., 0., 0.], 1., "OPAQUE"),
            emissive([0., 0., 1.], 1., "OPAQUE"),
        ],
        &[(0., 0), (0.1, 1)],
        None,
        false,
        |doc, res| {
            influences(doc, res, 0);
            // Tiny red copies on either side; blue unskinned primitives use mesh-node transforms.
            doc["nodes"] = json!([{"mesh":0,"skin":0,"translation":[0.,1.2,0.],"scale":[0.25,0.25,0.25]},
            {"mesh":0,"skin":1,"translation":[0.,-1.2,0.],"scale":[0.25,0.25,0.25]},
            {"translation":[-0.8,0.,0.],"scale":[0.25,0.25,0.25],"children":[3]}, {},
            {"translation":[0.8,0.,0.],"scale":[0.25,0.25,0.25],"children":[5]},{}]);
            doc["skins"] = json!([{"joints":[2,3]},{"joints":[4,5]}]);
            doc["scenes"][0]["nodes"] = json!([0, 1, 2, 4]);
        },
    );
    let renderer = gpu_renderer(ChannelPrecision::Unorm8);
    let target = renderer.render_target(SIZE, SIZE).unwrap();
    let mut gpu = GpuScene::upload(&renderer, REQUEST, scene, SceneGpuLimits::default()).unwrap();
    let mut f = frame();
    f.samples = 4;
    assert_eq!(
        gpu.draw(&renderer, REQUEST, &target, &f)
            .unwrap()
            .deformation_bytes,
        2 * 3 * 72
    );
    let actual = pixels(&renderer, &target, ChannelPrecision::Unorm8);
    let reference = fixture_with(
        vec![
            emissive([1., 0., 0.], 1., "OPAQUE"),
            emissive([0., 0., 1.], 1., "OPAQUE"),
        ],
        &[(0., 0), (0.1, 1)],
        None,
        false,
        |doc, _| {
            let blue = doc["meshes"][0]["primitives"][1].clone();
            doc["meshes"][0]["primitives"].as_array_mut().unwrap().pop();
            doc["meshes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"primitives":[blue]}));
            doc["nodes"] = json!([{"mesh":0,"translation":[-0.8,0.,0.],"scale":[0.25,0.25,0.25]},{"mesh":0,"translation":[0.8,0.,0.],"scale":[0.25,0.25,0.25]},
            {"mesh":1,"translation":[0.,1.2,0.],"scale":[0.25,0.25,0.25]},{"mesh":1,"translation":[0.,-1.2,0.],"scale":[0.25,0.25,0.25]}]);
            doc["scenes"][0]["nodes"] = json!([0, 1, 2, 3]);
        },
    );
    let mut reference =
        GpuScene::upload(&renderer, REQUEST, reference, SceneGpuLimits::default()).unwrap();
    reference.draw(&renderer, REQUEST, &target, &f).unwrap();
    assert_eq!(
        actual.pixels,
        pixels(&renderer, &target, ChannelPrecision::Unorm8).pixels
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn skinned_normal_maps_match_independent_nonuniform_basis_and_unsupported_poses_keep_pixels() {
    fn basis(skin: bool, scale: [f32; 3]) -> Arc<motion_scene::Scene> {
        fixture_with(
            vec![
                json!({"pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0,"roughnessFactor":1},"normalTexture":{"index":0}}),
            ],
            &[(0., 0)],
            Some(png8(1, 1, &[255, 200, 128, 255])),
            true,
            |doc, res| {
                let n = floats(doc, res, "VEC3", &[0.6, 0., 0.8].repeat(3), None);
                let t = floats(doc, res, "VEC4", &[1., 0., 0., -1.].repeat(3), None);
                doc["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(n);
                doc["meshes"][0]["primitives"][0]["attributes"]["TANGENT"] = json!(t);
                if skin {
                    influences(doc, res, 0);
                    doc["nodes"] = json!([{"mesh":0,"skin":0,"scale":[0.,0.,0.]},{"scale":scale,"children":[2]},{}]);
                    doc["skins"] = json!([{"joints":[1,2]}]);
                    doc["scenes"][0]["nodes"] = json!([0, 1]);
                } else {
                    doc["nodes"][0]["scale"] = json!(scale);
                }
            },
        )
    }
    let renderer = gpu_renderer(ChannelPrecision::Float32);
    let a = renderer.render_target(SIZE, SIZE).unwrap();
    let b = renderer.render_target(SIZE, SIZE).unwrap();
    let mut skin = GpuScene::upload(
        &renderer,
        REQUEST,
        basis(true, [2., 1., 0.5]),
        SceneGpuLimits::default(),
    )
    .unwrap();
    let mut reference = GpuScene::upload(
        &renderer,
        REQUEST,
        basis(false, [2., 1., 0.5]),
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
    skin.draw(&renderer, REQUEST, &a, &f).unwrap();
    reference.draw(&renderer, REQUEST, &b, &f).unwrap();
    let actual = center_linear(&pixels(&renderer, &a, ChannelPrecision::Float32));
    let expected = center_linear(&pixels(&renderer, &b, ChannelPrecision::Float32));
    assert!(
        actual[0] > 0.01,
        "normal-mapped skin shading must be visible"
    );
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 2e-5, "{actual:?} != {expected:?}");
    }
    let before = pixels(&renderer, &a, ChannelPrecision::Float32).pixels;
    for scale in [[0., 1., 1.], [-1., 1., 1.]] {
        let mut bad = GpuScene::upload(
            &renderer,
            REQUEST,
            basis(true, scale),
            SceneGpuLimits::default(),
        )
        .unwrap();
        let err = bad.draw(&renderer, REQUEST, &a, &f).unwrap_err();
        assert!(err.to_string().contains("phase=skin"));
        assert!(err
            .to_string()
            .contains("singular or reflected effective skin transform"));
        assert_eq!(
            pixels(&renderer, &a, ChannelPrecision::Float32).pixels,
            before
        );
    }
    skin.draw(&renderer, REQUEST, &a, &f).unwrap();
    assert_eq!(
        pixels(&renderer, &a, ChannelPrecision::Float32).pixels,
        before
    );
}
