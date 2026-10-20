use glam::{DVec3, DVec4, Mat4, Vec3};
use motion_core::{EffectInstance, Layer, Project};
use motion_render::{DrawLayer, PlaneCompositor, Renderer, Scene};

fn fixture(effect: bool) -> Scene {
    let mut p = Project::new(256, 256, 30, 60).unwrap();
    p.version = 21;
    p.background = [0., 0., 0., 1.];
    for (id, angle, color) in [(1, 55., [1., 0., 0., 1.]), (2, -45., [0., 0., 1., 1.])] {
        let mut layer = Layer::solid(
            id,
            "authored intersecting plane",
            [300.; 2],
            [128., 128., 0.],
            color,
        );
        layer.three_d = true;
        layer.transform.rotation.value[1] = angle;
        p.layers.push(layer);
    }
    if effect {
        let package = motion_effects::builtin::package().unwrap();
        let def = package
            .manifest
            .effects
            .iter()
            .find(|d| d.id == "brightness_contrast")
            .unwrap();
        p.layers[0].effects.push(EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            def,
            [300.; 2],
        ));
        p.rebuild_plugin_dependencies();
    }
    let mut scene = Scene::new(&p);
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let red = camera(false, Vec3::new(0., 0., 300.), Vec3::ZERO, 50.);
    scene.camera.view_projection = red;
    scene.camera.eye = Vec3::new(0., 0., 300.);
    scene.camera.target = Vec3::ZERO;
    for layer in &mut scene.layers {
        layer.view_projection = if layer.id == 1 {
            red
        } else {
            camera(false, Vec3::new(55., 0., 300.), Vec3::new(10., 0., 0.), 60.)
        };
    }
    scene
}
fn camera(orthographic: bool, eye: Vec3, target: Vec3, fov: f32) -> Mat4 {
    let p = if orthographic {
        Mat4::orthographic_rh(-140., 140., -140., 140., 0.5, 100000.)
    } else {
        Mat4::perspective_rh(fov.to_radians(), 1., 0.5, 100000.)
    };
    p * Mat4::look_at_rh(eye, target, Vec3::Y)
}
fn hit(layer: &DrawLayer, x: usize, y: usize) -> Option<f64> {
    let xy = [(x as f64 + 0.5) / 128. - 1., 1. - (y as f64 + 0.5) / 128.];
    let inverse = layer.view_projection.as_dmat4().inverse();
    let point = |z| {
        let h = inverse * DVec4::new(xy[0], xy[1], z, 1.);
        h.truncate() / h.w
    };
    let model = layer.model.as_dmat4();
    let a = model.inverse().transform_point3(point(0.));
    let b = model.inverse().transform_point3(point(1.));
    let t = -a.z / (b.z - a.z);
    if !(0. ..=1.).contains(&t) {
        return None;
    }
    let local = a.lerp(b, t);
    // Keep the independent reference away from rasterization edge tolerances.
    if local.x.abs() > f64::from(layer.size[0]) * 0.45
        || local.y.abs() > f64::from(layer.size[1]) * 0.45
    {
        return None;
    }
    let clip =
        layer.view_projection.as_dmat4() * model * DVec3::new(local.x, local.y, 0.).extend(1.);
    let z = clip.z / clip.w;
    (clip.w > 0. && (0. ..=1.).contains(&z)).then_some(z)
}
fn srgb(value: f32) -> u8 {
    ((if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }) * 255.)
        .round() as u8
}

#[test]
fn independently_sampled_cameras_sort_intersections_by_projected_depth_in_direct_and_effect_paths()
{
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(256, 256).unwrap();
    for effect in [false, true] {
        let mut scene = fixture(effect);
        for orthographic in [false, true] {
            let blue = scene.layers.iter_mut().find(|l| l.id == 2).unwrap();
            blue.view_projection = camera(
                orthographic,
                Vec3::new(55., 0., 300.),
                Vec3::new(10., 0., 0.),
                60.,
            );
            for swapped in [false, true] {
                if swapped {
                    scene.layers.reverse();
                }
                for translucent in [false, true] {
                    for layer in &mut scene.layers {
                        layer.opacity = if translucent {
                            if layer.id == 1 {
                                0.35
                            } else {
                                0.65
                            }
                        } else {
                            1.
                        };
                    }
                    let pixels = renderer.capture(&scene, &target).unwrap().0;
                    let mut nearer = [0, 0];
                    for y in (32..224).step_by(16) {
                        for x in (32..224).step_by(16) {
                            let Some(a) = hit(&scene.layers[0], x, y) else {
                                continue;
                            };
                            let Some(b) = hit(&scene.layers[1], x, y) else {
                                continue;
                            };
                            if (a - b).abs() < 1e-9 {
                                continue;
                            }
                            let (far, near) = if a > b {
                                (&scene.layers[0], &scene.layers[1])
                            } else {
                                (&scene.layers[1], &scene.layers[0])
                            };
                            nearer[(near.id - 1) as usize] += 1;
                            let mut expected = [0u8, 0, 0, 255];
                            for (channel, id) in [(0, 1), (2, 2)] {
                                let value = if near.id == id {
                                    near.opacity
                                } else if far.id == id {
                                    far.opacity * (1. - near.opacity)
                                } else {
                                    0.
                                };
                                expected[channel] = srgb(value);
                            }
                            let actual = &pixels[4 * (y * 256 + x)..4 * (y * 256 + x) + 4];
                            assert!(actual.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=3),
                            "effect={effect} ortho={orthographic} swapped={swapped} translucent={translucent} point={x},{y} depths={a},{b} actual={actual:?} expected={expected:?}");
                        }
                    }
                    assert!(nearer.iter().sum::<usize>() > 20);
                    if !orthographic {
                        assert!(
                            nearer.iter().all(|count| *count > 4),
                            "must exercise both occlusion orders: {nearer:?}"
                        );
                    }
                }
            }
        }
    }
    renderer.check_health().unwrap();
}

fn gradient(renderer: &mut Renderer) {
    let pixels: Vec<_> = (0..64)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                [
                    if (x / 8 + y / 8) % 2 == 0 { 255 } else { 0 },
                    x * 4,
                    y * 4,
                    255,
                ]
            })
        })
        .collect();
    renderer.upload_image(7, 64, 64, &pixels).unwrap();
}
fn diff_context(a: &[u8], b: &[u8]) -> String {
    let (index, _) = a
        .iter()
        .zip(b)
        .enumerate()
        .max_by_key(|(_, (a, b))| a.abs_diff(**b))
        .unwrap();
    let pixel = index / 4;
    format!(
        "max_diff={} at {},{} channel={} a={:?} b={:?} changed_over_one={}",
        a[index].abs_diff(b[index]),
        pixel % 256,
        pixel / 256,
        index % 4,
        &a[pixel * 4..pixel * 4 + 4],
        &b[pixel * 4..pixel * 4 + 4],
        a.iter().zip(b).filter(|(a, b)| a.abs_diff(**b) > 1).count()
    )
}
#[test]
fn split_perspective_uvs_match_an_unsplit_textured_plane_without_changing_the_vertex_abi() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    gradient(&mut renderer);
    let target = renderer.capture_target(256, 256).unwrap();
    let mut scene = fixture(false);
    scene.background = [0.; 4];
    for l in &mut scene.layers {
        if l.id == 1 {
            l.asset = Some(7);
            l.color = [1.; 4];
        } else {
            l.opacity = 0.;
        }
    }
    let mut single = scene.clone();
    single.layers.retain(|l| l.id == 1);
    let reference = renderer.capture(&single, &target).unwrap().0;
    let actual = renderer.capture(&scene, &target).unwrap().0;
    let mut compositor = PlaneCompositor::new();
    compositor.prepare(&scene).unwrap();
    assert!(
        compositor.vertices.len() > 12,
        "intersection must split geometry"
    );
    assert!(
        actual
            .iter()
            .zip(&reference)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "projected split must preserve perspective UVs: {}",
        diff_context(&actual, &reference)
    );
    assert!(reference.chunks_exact(4).filter(|p| p[3] > 0).count() > 1000);
    let mut builder = motion_render::effect_plan::PlanBuilder::new(Default::default()).unwrap();
    let plan = builder.build(&scene, &[0, 7], 256, 256, true).unwrap();
    let mut bytes = vec![0; plan.buffer_bytes(&scene)];
    plan.write(&scene, &mut bytes).unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        motion_render::effect_plan::PLAN_VERSION
    );
    assert_eq!(motion_render::effect_plan::PLAN_VERSION, 9);
    assert_eq!(std::mem::size_of::<motion_render::PlaneVertex>(), 20);
    renderer.check_health().unwrap();
}

#[test]
fn homogeneous_clipping_handles_camera_plane_crossings_and_culls_normal_outside_geometry() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    gradient(&mut renderer);
    let target = renderer.capture_target(256, 256).unwrap();
    let mut scene = fixture(false);
    scene.layers.retain(|l| l.id == 1);
    scene.background = [0.; 4];
    scene.layers[0].asset = Some(7);
    scene.layers[0].color = [1.; 4];
    scene.layers[0].model = Mat4::from_rotation_y(70f32.to_radians());
    scene.layers[0].view_projection = camera(false, Vec3::new(0., 0., 50.), Vec3::ZERO, 60.);
    let mut hardware_clipped = scene.clone();
    hardware_clipped.camera.view_projection = scene.layers[0].view_projection;
    let reference = renderer.capture(&hardware_clipped, &target).unwrap().0;
    let actual = renderer.capture(&scene, &target).unwrap().0;
    assert!(
        actual
            .iter()
            .zip(&reference)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "eye/near clipping must agree with the original GPU clipping path: {}",
        diff_context(&actual, &reference)
    );
    assert!(actual.chunks_exact(4).filter(|p| p[3] > 0).count() > 1000);
    scene.layers[0].model = Mat4::from_translation(Vec3::new(0., 0., 1000.));
    assert!(renderer
        .capture(&scene, &target)
        .unwrap()
        .0
        .iter()
        .all(|v| *v == 0));
    scene.layers[0].view_projection = Mat4::ZERO;
    let error = renderer.capture(&scene, &target).unwrap_err();
    assert!(error
        .to_string()
        .contains("layer 1 phase=exposure_projection: nonfinite or singular camera"));
    scene.layers[0].model = Mat4::from_rotation_y(70f32.to_radians());
    scene.layers[0].view_projection = camera(false, Vec3::new(0., 0., 50.), Vec3::ZERO, 60.);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, actual);
    renderer.check_health().unwrap();
}

#[test]
fn split_textured_plane_retains_local_coordinates_and_perspective_interpolation() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    gradient(&mut renderer);
    let target = renderer.capture_target(256, 256).unwrap();
    let mut scene = fixture(false);
    scene.background = [0.; 4];
    // The second plane is genuinely split by the first plane's BSP node.
    // The existing test above also protects exact unsplit frustum clipping.
    for layer in &mut scene.layers {
        if layer.id == 2 {
            layer.asset = Some(7);
            layer.color = [1.; 4];
        } else {
            layer.opacity = 0.;
        }
    }
    let mut compositor = PlaneCompositor::new();
    compositor.prepare(&scene).unwrap();
    let layer_index = scene.layers.iter().position(|l| l.id == 2).unwrap();
    let layer = &scene.layers[layer_index];
    let vertices: Vec<_> = compositor
        .batches
        .iter()
        .filter(|b| b.layer == layer_index)
        .flat_map(|b| &compositor.vertices[b.vertices.start as usize..b.vertices.end as usize])
        .collect();
    assert!(vertices.len() > 6, "textured plane must actually split");
    let inverse = layer.model.as_dmat4().inverse();
    for vertex in &vertices {
        let local = inverse.transform_point3(DVec3::from_array(vertex.position.map(f64::from)));
        assert!(local.z.abs() < 1e-5, "split vertex left the source plane");
        let expected = [
            local.x / f64::from(layer.size[0]) + 0.5,
            0.5 - local.y / f64::from(layer.size[1]),
        ];
        assert!(
            expected
                .iter()
                .zip(vertex.uv)
                .all(|(a, b)| (a - f64::from(b)).abs() < 1e-7),
            "split vertex changed the source UV"
        );
    }
    let mut single = scene.clone();
    single.layers.retain(|l| l.id == 2);
    let reference = renderer.capture(&single, &target).unwrap().0;
    let actual = renderer.capture(&scene, &target).unwrap().0;
    // f32 GPU triangle setup can shift the high-contrast checker interpolation
    // slightly when a new edge is introduced. Keep the existing compositor's
    // three-code color bound, plus a tighter one-code bound on the UV ramps.
    assert!(
        actual
            .iter()
            .zip(&reference)
            .all(|(a, b)| a.abs_diff(*b) <= 3),
        "split checker: {}",
        diff_context(&actual, &reference)
    );
    assert!(
        actual
            .chunks_exact(4)
            .zip(reference.chunks_exact(4))
            .all(|(a, b)| a[1].abs_diff(b[1]) <= 1 && a[2].abs_diff(b[2]) <= 1 && a[3] == b[3]),
        "split ramp/coverage: {}",
        diff_context(&actual, &reference)
    );
    renderer.check_health().unwrap();
}
