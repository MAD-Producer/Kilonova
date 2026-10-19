use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{EffectPackage, Registry};
use motion_graph::*;
use motion_render::{Renderer, Scene};
use std::sync::Arc;

// Independent pixel-area oracle: clip the projected quad to the pixel box,
// rather than duplicating the GPU edge/derivative approximation.
fn area_in_pixel(quad: &[[f64; 2]], x: usize, y: usize) -> f64 {
    let mut polygon = quad.to_vec();
    for (axis, bound, lower) in [
        (0, x as f64, true),
        (0, x as f64 + 1., false),
        (1, y as f64, true),
        (1, y as f64 + 1., false),
    ] {
        let before = std::mem::take(&mut polygon);
        if before.is_empty() {
            return 0.;
        }
        let mut a = *before.last().unwrap();
        for b in before {
            let inside = |p: [f64; 2]| {
                if lower {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            if inside(a) != inside(b) {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                polygon.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
            if inside(b) {
                polygon.push(b);
            }
            a = b;
        }
    }
    if polygon.len() < 3 {
        return 0.;
    }
    let mut a = *polygon.last().unwrap();
    let mut sum = 0.;
    for b in polygon {
        sum += a[0] * b[1] - a[1] * b[0];
        a = b;
    }
    (sum.abs() * 0.5).clamp(0., 1.)
}

#[test]
fn aligned_planes_preserve_the_last_row_and_column_of_odd_viewports() {
    use motion_core::{
        compositing::*,
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    for (width, height) in [
        (1, 1),
        (1, 3),
        (2, 3),
        (3, 2),
        (5, 7),
        (7, 5),
        (8, 8),
        (9, 6),
        (6, 9),
    ] {
        for case in ["plain", "mask", "alpha_matte", "luma_matte", "blend"] {
            let mut project = Project::new(width, height, 30, 60).unwrap();
            project.version = 10;
            project.camera.created = false;
            project.background = [0.; 4];
            let size = [width as f32, height as f32];
            let center = [size[0] / 2., size[1] / 2., 0.];
            let mut layer = Layer::solid(1, "Aligned edge", size, center, [1.; 4]);
            if case == "mask" {
                layer.masks.push(LayerMask::new(
                    1,
                    VectorPath {
                        id: 1,
                        closed: true,
                        nodes: [[0., 0.], [size[0], 0.], size, [0., size[1]]]
                            .into_iter()
                            .enumerate()
                            .map(|(i, p)| PathNode {
                                id: i as u64 + 1,
                                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                            })
                            .collect(),
                    },
                ));
            }
            if case.ends_with("matte") {
                let mut source = Layer::solid(2, "Opaque matte", size, center, [1.; 4]);
                source.visible = false;
                project.layers.push(source);
                layer.track_matte = Some(TrackMatte {
                    source: 2,
                    hide_source: true,
                    mode: if case == "alpha_matte" {
                        MatteMode::Alpha
                    } else {
                        MatteMode::Luma
                    },
                });
            }
            if case == "blend" {
                layer.blend = LayerBlend {
                    mode: BlendMode::Multiply,
                    space: BlendSpace::Linear,
                };
            }
            project.layers.push(layer);
            let mut scene = Scene::new(&project);
            scene
                .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let target = renderer.capture_target(width, height).unwrap();
            let pixels = renderer.capture(&scene, &target).unwrap().0;
            assert_eq!(pixels.len(), width as usize * height as usize * 4);
            for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                // Every pixel box is fully inside the aligned opaque plane.
                // This includes partial 2x2 derivative quads at odd extents.
                assert!(
                    pixel.iter().all(|&v| v.abs_diff(255) <= 1),
                    "{case} {width}x{height} ({},{}): {pixel:?}; adapter={:?}",
                    i % width as usize,
                    i / width as usize,
                    renderer.adapter_info
                );
            }
            assert!(renderer.gpu_error().is_none());
        }
    }
}

#[test]
fn magnified_sdf_output_is_rasterized_at_display_density() {
    let folder = tempfile::tempdir().unwrap();
    let mut graph =
        GraphDocument::from_json(include_str!("../examples/solid-color.msgraph")).unwrap();
    graph.format_version = 2;
    let mut circle = Node::new("circle", NodeOp::SdfCircle);
    circle
        .input_defaults
        .insert("center".into(), InputValue::Vec2([32.; 2]));
    circle
        .input_defaults
        .insert("radius".into(), InputValue::Float(14.));
    let mut color = Node::new("color", NodeOp::CombineColor);
    color
        .input_defaults
        .insert("r".into(), InputValue::Float(1.));
    color
        .input_defaults
        .insert("g".into(), InputValue::Float(0.));
    color
        .input_defaults
        .insert("b".into(), InputValue::Float(0.5));
    graph.nodes = vec![
        Node::new("position", NodeOp::Position),
        circle,
        Node::new("fill", NodeOp::SdfFill),
        color,
        Node::new("output", NodeOp::ColorOutput),
    ];
    graph.connections = vec![
        Connection::new("position", "value", "circle", "position"),
        Connection::new("circle", "distance", "fill", "distance"),
        Connection::new("fill", "coverage", "color", "a"),
        Connection::new("color", "color", "output", "color"),
    ];
    let report = export_msfx(
        &graph,
        &CompileOptions::default(),
        &folder.path().join("circle.msfx"),
    )
    .unwrap();
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(512, 512, 30, 60).unwrap();
    project.camera.created = false;
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "Magnified circle",
        [64.; 2],
        [256.25, 256.25, 0.],
        [1.; 4],
    );
    layer.transform.scale.value = [650., 650., 100.];
    layer.transform.rotation.value = [0., 0., 3.7];
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(512, 512).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    if let Some(directory) = std::env::var_os("MOTION_OUTPUT_AA_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            directory.join("magnified-sdf.png"),
            &pixels,
            512,
            512,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let mut error = 0.;
    let mut wide_edge = 0;
    for y in 0..512 {
        for x in 0..512 {
            let distance = ((x as f64 + 0.5 - 256.25).powi(2) + (y as f64 + 0.5 - 256.25).powi(2))
                .sqrt()
                - 91.;
            // Independent 16x16 subpixel integration of the analytic circle.
            // Only boundary pixels need numerical integration.
            let expected = if distance.abs() > std::f64::consts::FRAC_1_SQRT_2 {
                if distance < 0. {
                    1.
                } else {
                    0.
                }
            } else {
                let mut covered = 0;
                for sy in 0..16 {
                    for sx in 0..16 {
                        let dx = x as f64 + (sx as f64 + 0.5) / 16. - 256.25;
                        let dy = y as f64 + (sy as f64 + 0.5) / 16. - 256.25;
                        covered += usize::from(dx * dx + dy * dy <= (14_f64 * 6.5).powi(2));
                    }
                }
                covered as f64 / 256.
            };
            let alpha = pixels[(y * 512 + x) * 4 + 3];
            error += (f64::from(alpha) / 255. - expected).abs();
            wide_edge += usize::from(distance.abs() > 1.5 && alpha > 2 && alpha < 253);
        }
    }
    eprintln!("magnified circle area error={error}, blurred pixels beyond 1.5px={wide_edge}");
    assert!(
        error < 100.,
        "magnification must not stretch the 64px effect raster: {error}"
    );
    assert_eq!(
        wide_edge, 0,
        "SDF edge must stay confined to the display pixel footprint"
    );
}

#[test]
fn rotated_graph_output_has_pixel_coverage_instead_of_a_binary_quad_edge() {
    // Exercise the limiting one-dimensional integral as well as diagonal edges.
    // The expected coverage remains independent pixel/polygon clipping.
    for angle in [0., 0.000001, -0.000001, 3.7, 45., 89.99999] {
        check_rotated_graph_output_coverage(angle);
    }
}

fn check_rotated_graph_output_coverage(angle: f32) {
    let folder = tempfile::tempdir().unwrap();
    let graph = GraphDocument::from_json(include_str!("../examples/solid-color.msgraph")).unwrap();
    let report = export_msfx(
        &graph,
        &CompileOptions::default(),
        &folder.path().join("output.msfx"),
    )
    .unwrap();
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(256, 192, 30, 60).unwrap();
    project.camera.created = false;
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "Graph output",
        [144., 104.],
        [128.25, 96.25, 0.],
        [1.; 4],
    );
    layer.transform.rotation.value = [0., 0., angle];
    layer.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [144., 104.],
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let layer = &scene.layers[0];
    let matrix = (layer.view_projection * layer.model)
        .as_dmat4()
        .to_cols_array_2d();
    let quad: Vec<_> = [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]]
        .into_iter()
        .map(|p| {
            let point = [p[0] * 144., p[1] * 104., 0., 1.];
            let clip: [f64; 4] =
                std::array::from_fn(|row| (0..4).map(|col| matrix[col][row] * point[col]).sum());
            [
                (clip[0] / clip[3] * 0.5 + 0.5) * 256.,
                (0.5 - clip[1] / clip[3] * 0.5) * 192.,
            ]
        })
        .collect();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(256, 192).unwrap();
    let pixels = renderer.capture(&scene, &target).unwrap().0;
    if let Some(directory) = std::env::var_os("MOTION_OUTPUT_AA_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            directory.join(if angle == 3.7 {
                "rotated-node-output.png".into()
            } else {
                format!("rotated-node-output-{angle}.png")
            }),
            &pixels,
            256,
            192,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let mut partial = 0;
    let mut error = 0.;
    let mut binary_error = 0.;
    for y in 0..192 {
        for x in 0..256 {
            let expected = area_in_pixel(&quad, x, y);
            let alpha = pixels[(y * 256 + x) * 4 + 3];
            partial += usize::from(alpha > 0 && alpha < 255);
            error += (f64::from(alpha) / 255. - expected).abs();
            binary_error += ((if expected >= 0.5 { 1. } else { 0. }) - expected).abs();
        }
    }
    eprintln!(
        "angle={angle}, partial={partial}, coverage error={error}, binary error={binary_error}"
    );
    assert!(
        partial > 200,
        "rotated node output still has hard single-sample edges: {partial}"
    );
    assert!(
        error < binary_error * 0.25,
        "coverage must agree with pixel area: {error} vs {binary_error}"
    );
}

#[test]
fn coverage_is_shared_by_plain_masked_matted_and_blended_planes() {
    use motion_core::{
        compositing::*,
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(256, 192).unwrap();
    for case in [
        "plain",
        "axis",
        "mask",
        "mask_alpha_matte",
        "mask_blend",
        "alpha_matte",
        "luma_matte",
        "blend",
        "opacity",
        "perspective",
        "reflection",
    ] {
        let mut project = Project::new(256, 192, 30, 60).unwrap();
        project.version = 10;
        project.camera.created = false;
        project.background = [0.; 4];
        let mut layer = Layer::solid(1, "Edge", [144., 104.], [128.25, 96.25, 0.], [1.; 4]);
        layer.transform.rotation.value[2] = if case == "axis" { 0. } else { 3.7 };
        let opacity = if case == "opacity" { 0.5 } else { 1. };
        layer.transform.opacity.value = opacity;
        if case == "reflection" {
            layer.transform.scale.value[0] = -100.;
        }
        if case == "perspective" {
            project.camera.created = true;
            layer.three_d = true;
            layer.transform.rotation.value = [25., 36., 3.7];
        }
        if case.starts_with("mask") {
            layer.masks.push(LayerMask::new(
                1,
                VectorPath {
                    id: 1,
                    closed: true,
                    nodes: [[0., 0.], [72., 0.], [72., 104.], [0., 104.]]
                        .into_iter()
                        .enumerate()
                        .map(|(i, p)| PathNode {
                            id: i as u64 + 1,
                            geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                        })
                        .collect(),
                },
            ));
        }
        if case.ends_with("matte") {
            let mut source = Layer::solid(2, "Matte", [256., 192.], [128., 96., 0.], [1.; 4]);
            source.visible = false;
            project.layers.push(source);
            layer.track_matte = Some(TrackMatte {
                source: 2,
                hide_source: true,
                mode: if case.contains("alpha_matte") {
                    MatteMode::Alpha
                } else {
                    MatteMode::Luma
                },
            });
        }
        if case.ends_with("blend") {
            layer.blend = LayerBlend {
                mode: BlendMode::Multiply,
                space: BlendSpace::Linear,
            };
        }
        project.layers.push(layer);
        let mut scene = Scene::new(&project);
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let layer = scene.layers.iter().find(|l| l.id == 1).unwrap();
        let matrix = (layer.view_projection * layer.model)
            .as_dmat4()
            .to_cols_array_2d();
        // A partial mask also guards against reusing the clamped UV from a
        // 1x1 solid-color image when sampling its full-sized mask texture.
        let right = if case.starts_with("mask") { 0. } else { 0.5 };
        let quad: Vec<_> = [[-0.5, 0.5], [-0.5, -0.5], [right, -0.5], [right, 0.5]]
            .into_iter()
            .map(|p| {
                let point = [p[0] * 144., p[1] * 104., 0., 1.];
                let clip: [f64; 4] =
                    std::array::from_fn(|r| (0..4).map(|c| matrix[c][r] * point[c]).sum());
                [
                    (clip[0] / clip[3] * 0.5 + 0.5) * 256.,
                    (0.5 - clip[1] / clip[3] * 0.5) * 192.,
                ]
            })
            .collect();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        // Rgba8 quantization can place the 0.5 tie at either adjacent byte.
        // Check opacity independently, then compare edge area at that measured
        // interior level so thousands of opaque interior pixels cannot dominate.
        let interior = pixels.chunks_exact(4).map(|p| p[3]).max().unwrap();
        assert!(interior.abs_diff((opacity * 255.).round() as u8) <= 1);
        let mut error = 0.;
        let mut binary_error = 0.;
        let mut partial = 0;
        for y in 0..192 {
            for x in 0..256 {
                let expected = area_in_pixel(&quad, x, y);
                let pixel = &pixels[(y * 256 + x) * 4..(y * 256 + x + 1) * 4];
                let expected_byte = (expected * f64::from(interior)).round();
                error += (f64::from(pixel[3]) - expected_byte).abs() / 255.;
                binary_error += (expected - if expected >= 0.5 { 1. } else { 0. }).abs();
                partial += usize::from(
                    expected > 0.
                        && expected < 1.
                        && pixel[3] > 0
                        && f64::from(pixel[3]) < 255. * f64::from(opacity),
                );
                // Capture exports straight-alpha PNG bytes. With premultiplied
                // coverage applied to RGB and alpha together, white stays white
                // after readback (rather than leaving dark edge halos).
                if pixel[3] > 16 {
                    assert!(
                        pixel[..3].iter().all(|&c| c >= 248),
                        "{case} ({x},{y}): {pixel:?}"
                    );
                } else if pixel[3] == 0 {
                    assert_eq!(&pixel[..3], &[0; 3]);
                }
            }
        }
        eprintln!("{case}: partial={partial}, area error={error}, binary={binary_error}");
        assert!(partial > 150, "{case} has hard edges: {partial}");
        assert!(
            error < binary_error * 0.25 + 2.,
            "{case}: {error} vs {binary_error}"
        );
        assert!(renderer.gpu_error().is_none());
    }
}

#[test]
fn soft_sphere_output_preserves_animation_and_random_seek_at_display_density() {
    let folder = tempfile::tempdir().unwrap();
    let graph =
        GraphDocument::from_json(include_str!("../examples/soft-sphere-motion.msgraph")).unwrap();
    let artifact = std::env::var_os("MOTION_OUTPUT_AA_ARTIFACTS").map(std::path::PathBuf::from);
    let package_path = artifact.as_ref().map_or_else(
        || folder.path().join("sphere.msfx"),
        |root| root.join("plugins/sphere.msfx"),
    );
    let report = export_msfx(&graph, &CompileOptions::default(), &package_path).unwrap();
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(512, 512, 30, 120).unwrap();
    project.camera.created = false;
    project.background = [0.04, 0.05, 0.07, 1.];
    let mut layer = Layer::solid(
        1,
        "Soft sphere / antialiasing",
        [64.; 2],
        [256., 256., 0.],
        [1.; 4],
    );
    layer.transform.scale.value = [650., 650., 100.];
    layer.transform.rotation.value[2] = 3.7;
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    );
    effect.params.get_mut("tone").unwrap().track.value = [0.8, 0.01, 0.9, 1.];
    effect.params.get_mut("band").unwrap().track.value[0] = 0.6;
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    if let Some(root) = &artifact {
        motion_core::storage::save(&root.join("acceptance-project"), &project).unwrap();
    }
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(512, 512).unwrap();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let first = renderer.capture(&scene, &target).unwrap().0;
    let memory = renderer.texture_memory_breakdown().total_bytes();
    scene
        .sample(&project, 15.5, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let later = renderer.capture(&scene, &target).unwrap().0;
    assert!(
        first
            .iter()
            .zip(&later)
            .filter(|(a, b)| a.abs_diff(**b) > 2)
            .count()
            > 1000
    );
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, first);
    assert_eq!(renderer.texture_memory_breakdown().total_bytes(), memory);
    assert!(renderer.gpu_error().is_none());
    if let Some(root) = &artifact {
        image::save_buffer(
            root.join("soft-sphere-node-output.png"),
            &first,
            512,
            512,
            image::ColorType::Rgba8,
        )
        .unwrap();
        image::save_buffer(
            root.join("soft-sphere-node-output-later.png"),
            &later,
            512,
            512,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn magnified_graph_sampling_preserves_source_uvs_and_premultiplied_filtering() {
    use motion_core::{Asset, Content};
    let folder = tempfile::tempdir().unwrap();
    let mut graph =
        GraphDocument::from_json(include_str!("../examples/image-invert.msgraph")).unwrap();
    graph.nodes.retain(|node| node.id != "invert");
    graph
        .connections
        .retain(|c| c.from.node != "invert" && c.to.node != "invert");
    graph
        .connections
        .push(Connection::new("sample", "color", "output", "color"));
    let report = export_msfx(
        &graph,
        &CompileOptions::default(),
        &folder.path().join("identity.msfx"),
    )
    .unwrap();
    let package = Arc::new(EffectPackage::open(&report.path).unwrap());
    let mut registry = Registry::default();
    registry.insert(package.clone()).unwrap();
    let mut project = Project::new(512, 512, 30, 60).unwrap();
    project.camera.created = false;
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "Sample input", [64.; 2], [256.25, 256.25, 0.], [1.; 4]);
    layer.content = Content::Image { asset: 1 };
    layer.transform.scale.value = [650., 650., 100.];
    layer.transform.rotation.value[2] = 3.7;
    project.layers.push(layer);
    project.assets.push(Asset {
        id: 1,
        path: "test.png".into(),
        width: 64,
        height: 64,
    });
    let image: Vec<u8> = (0..64)
        .flat_map(|y| (0..64).flat_map(move |x| [x * 4, y * 4, 150, 64 + x * 2]))
        .collect();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.upload_image(1, 64, 64, &image).unwrap();
    renderer.set_effect_registry(registry);
    let target = renderer.capture_target(512, 512).unwrap();
    let mut scene = Scene::new(&project);
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let reference = renderer.capture(&scene, &target).unwrap().0;
    project.layers[0].effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [64.; 2],
    ));
    project.rebuild_plugin_dependencies();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let actual = renderer.capture(&scene, &target).unwrap().0;
    let mut rgb_error = 0.;
    let mut alpha_error = 0.;
    let mut visible = 0;
    for (a, b) in actual.chunks_exact(4).zip(reference.chunks_exact(4)) {
        alpha_error += f64::from(a[3].abs_diff(b[3]));
        // Straight-alpha readback is poorly conditioned at almost zero alpha;
        // source filtering is evaluated where the source has visible coverage.
        if b[3] > 32 {
            visible += 1;
            rgb_error += a[..3]
                .iter()
                .zip(&b[..3])
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>();
        }
    }
    let rgb_mae = rgb_error / (visible as f64 * 3.);
    let alpha_mae = alpha_error / (512. * 512.);
    eprintln!("magnified identity: RGB MAE={rgb_mae}, alpha MAE={alpha_mae}");
    assert!(visible > 100_000);
    assert!(
        rgb_mae < 2. && alpha_mae < 1.,
        "RGB={rgb_mae}, alpha={alpha_mae}"
    );
    assert!(renderer.gpu_error().is_none());
}
