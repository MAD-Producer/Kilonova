//! Exercise composition rendering, not a direct call to the lens primitive.
use super::*;
use motion_model::{
    tracking::{asset::Observation, shared::SharedData},
    *,
};
use motion_render::Scene;
#[path = "../../../motion-model/tests/support/tracking_camera.rs"]
mod fixture;

pub(super) fn project(rotation: Rotation, k: f64) -> Project {
    let mut p = fixture::camera_fixture();
    let mut raw = (*p.tracking_assets[0].data).clone();
    for sample in &mut raw.tracks[0].samples {
        if let Some(Observation::Camera {
            ref mut calibration,
            ..
        }) = sample.observation
        {
            *calibration = Calibration::new(
                raw.source_extent,
                calibration.focal(),
                calibration.principal(),
                [k, 0., 0.],
                if k == 0. { [0.; 2] } else { [0.001, -0.001] },
            )
            .unwrap();
        }
    }
    p.tracking_assets[0].data = SharedData::new(raw).unwrap();
    let binding = p.camera.tracking.as_mut().unwrap();
    binding.rotation = rotation;
    binding.crop = [5., 4., 310., 232.];
    p.background = [0.; 4];
    for (id, pos, size, color) in [
        (10, [500., 240., 120.], [500., 350.], [0., 1., 0., 1.]),
        (11, [520., 210., 0.], [200., 160.], [1., 0., 0., 1.]),
        (12, [530., 230., -90.], [200., 150.], [0., 0., 1., 0.5]),
        (13, [600., 240., 0.], [12., 400.], [1., 1., 0., 1.]),
        (14, [820., 200., 250.], [250., 200.], [1., 0., 1., 0.5]),
    ] {
        let mut layer = Layer::solid(id, "Independent lens plane", size, pos, color);
        layer.three_d = id != 13;
        p.layers.push(layer);
    }
    p.validate().unwrap();
    p
}
pub(super) fn sample(p: &Project, frame: f64) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
fn composite(back: [f64; 4], front: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|i| {
        if i == 3 {
            front[3] + back[3] * (1. - front[3])
        } else {
            front[i] * front[3] + back[i] * (1. - front[3])
        }
    })
}
fn expected(p: &Project, lens: LensWarp, point: [f64; 2]) -> [u8; 4] {
    // Independent Newton inverse, explicit known camera RDF rays, axis-aligned
    // geometry and source-over. No Scene matrices, picking or LensWarp inverse.
    let ideal = oracle(lens, point);
    let input = lens.plate().inverse(ideal).unwrap();
    let c = lens.calibration();
    let f = c.focal().map(|v| v * lens.zoom());
    let centre = c.principal();
    let ray = [
        (input[0] - centre[0]) / f[0],
        -(input[1] - centre[1]) / f[1],
    ];
    let mut out = [0.; 4];
    let mut start = 0;
    while start < p.layers.len() {
        let end = if p.layers[start].three_d {
            p.layers[start..]
                .iter()
                .position(|l| !l.three_d)
                .map_or(p.layers.len(), |n| start + n)
        } else {
            start + 1
        };
        let mut hits = Vec::new();
        for layer in &p.layers[start..end] {
            let pos = layer.transform.position.value;
            let (xy, half, depth) = if layer.three_d {
                let t = 500. + f64::from(pos[2]);
                let xy = [
                    t * ray[0] - (f64::from(pos[0]) - 320.),
                    t * ray[1] - (240. - f64::from(pos[1])),
                ];
                (
                    xy,
                    if matches!(layer.content, Content::Model { .. }) {
                        [100., 80.]
                    } else {
                        layer.size.map(|v| f64::from(v) * 0.5)
                    },
                    t,
                )
            } else {
                (
                    [point[0] - f64::from(pos[0]), point[1] - f64::from(pos[1])],
                    layer.size.map(|v| f64::from(v) * 0.5),
                    0.,
                )
            };
            if xy[0].abs() < half[0] && xy[1].abs() < half[1] {
                let color = if layer.id == 11 {
                    [1., 0., 0., 1.]
                } else {
                    match layer.content {
                        Content::Solid { color } => color.map(f64::from),
                        _ => panic!("independent fixture color"),
                    }
                };
                hits.push((depth, color));
            }
        }
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, color) in hits {
            out = composite(out, color);
        }
        start = end;
    }
    std::array::from_fn(|i| {
        if i == 3 {
            (out[3] * 255.).round() as u8
        } else if out[3] == 0. {
            0
        } else {
            let v = out[i] / out[3];
            ((if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }) * 255.)
                .round() as u8
        }
    })
}

pub(super) fn install_mesh(p: &mut Project, root: &std::path::Path) {
    use motion_scene::{AssetResolver, ImportContext, ImportLimits, ResourceKind};
    struct Material;
    impl AssetResolver for Material {
        fn read(&mut self, path: &str, _: ResourceKind, _: usize) -> std::io::Result<Vec<u8>> {
            assert_eq!(path, "paint.mtl");
            Ok(b"newmtl Red\nKd 1 0 0\nillum 0\n".to_vec())
        }
    }
    let bytes=b"mtllib paint.mtl\nv -1 -0.8 0\nv 1 -0.8 0\nv 1 0.8 0\nv -1 0.8 0\nusemtl Red\nf 1 2 3 4\n";
    let imported = motion_scene::obj::import_obj(
        ImportContext {
            request: 88,
            source: 7,
        },
        bytes,
        &mut Material,
        &Default::default(),
        &ImportLimits::default(),
    )
    .unwrap();
    let frozen = imported
        .freeze(
            bytes,
            motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
        )
        .unwrap();
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::write(root.join("assets/models/lens-quad.mscn"), &frozen).unwrap();
    use sha2::{Digest, Sha256};
    p.model_assets.push(ModelAsset {
        id: 7,
        path: "assets/models/lens-quad.mscn".into(),
        bytes: frozen.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&frozen)),
    });
    p.layers[1].content = Content::Model {
        model: ModelClip {
            asset: 7,
            scene: 0,
            pixels_per_metre: 100.,
            pivot: [0.; 3],
            samples: 4,
            animation: None,
        },
    };
    p.validate().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn camera_mesh_alpha_and_flat_boundaries_match_independent_distorted_rays() {
    let root = tempfile::tempdir().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let mut rows = Vec::new();
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        for k in [0., -0.018, 0.018] {
            let mut p = project(rotation, k);
            install_mesh(&mut p, root.path());
            renderer.configure_assets(&p, root.path()).unwrap();
            std::fs::remove_file(root.path().join(&p.model_assets[0].path)).unwrap();
            let scene = sample(&p, 0.);
            let lens = scene.native_lens().unwrap_or_else(|| {
                let Observation::Camera { calibration, .. } = p.tracking_assets[0].data.tracks[0]
                    .samples[0]
                    .observation
                    .unwrap()
                else {
                    panic!("camera")
                };
                let b = p.camera.tracking.as_ref().unwrap();
                LensWarp::new(
                    calibration,
                    PlateTransform::new(
                        calibration.source(),
                        b.crop,
                        b.rotation,
                        Extent::new(640, 480).unwrap(),
                    )
                    .unwrap(),
                    1.,
                )
                .unwrap()
            });
            let (actual, stats) = renderer.capture(&scene, &target).unwrap();
            assert!(stats.draw_calls >= 5);
            let mut reference = Vec::with_capacity(actual.len());
            for y in 0..480 {
                for x in 0..640 {
                    reference.extend_from_slice(&expected(
                        &p,
                        lens,
                        [x as f64 + 0.5, y as f64 + 0.5],
                    ));
                }
            }
            if let Some(directory) =
                std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from)
            {
                std::fs::create_dir_all(&directory).unwrap();
                for (name, pixels) in [("actual", &actual), ("reference", &reference)] {
                    image::save_buffer(
                        directory.join(format!("route-{rotation:?}-{k}-{name}.png")),
                        pixels,
                        640,
                        480,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            let mut checked = 0;
            let mut max_error = 0u8;
            let mut worst = None;
            // A fixed two-output-pixel edge exclusion covers MSAA and inverse
            // bilinear footprints. Interior color/depth/2D-boundary pixels must
            // still match; changing this band to make a failure pass is invalid.
            for y in 2..478usize {
                for x in 2..638usize {
                    let at = (y * 640 + x) * 4;
                    let color = &reference[at..at + 4];
                    if [-2isize, 0, 2].into_iter().any(|dy| {
                        [-2isize, 0, 2].into_iter().any(|dx| {
                            let i =
                                ((y as isize + dy) as usize * 640 + (x as isize + dx) as usize) * 4;
                            reference[i..i + 4] != *color
                        })
                    }) {
                        continue;
                    }
                    checked += 1;
                    for channel in 0..4 {
                        let error = actual[at + channel].abs_diff(color[channel]);
                        if error > max_error {
                            max_error = error;
                            worst = Some((x, y, actual[at..at + 4].to_vec(), color.to_vec()));
                        }
                    }
                }
            }
            assert!(checked > 250000, "{checked}");
            if max_error > 2 {
                if let Some(directory) =
                    std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from)
                {
                    std::fs::write(
                        directory.join("geometry.txt"),
                        format!("camera={:?}\n layers={:#?}", scene.camera, scene.layers),
                    )
                    .unwrap();
                    let mut isolated = p.clone();
                    isolated.layers.truncate(3);
                    let image = renderer.capture(&sample(&isolated, 0.), &target).unwrap().0;
                    image::save_buffer(
                        directory.join("isolated.png"),
                        &image,
                        640,
                        480,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                    let mut planes = p.clone();
                    planes.layers[1].content = Content::Solid {
                        color: [1., 0., 0., 1.],
                    };
                    let image = renderer.capture(&sample(&planes, 0.), &target).unwrap().0;
                    image::save_buffer(
                        directory.join("planes.png"),
                        &image,
                        640,
                        480,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
            assert!(
                max_error <= 2,
                "rotation={rotation:?} k={k} max_error={max_error} worst={worst:?} eye={:?}",
                scene.camera.eye
            );
            if let Some(directory) =
                std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from)
            {
                std::fs::create_dir_all(&directory).unwrap();
                image::save_buffer(
                    directory.join(format!("route-{rotation:?}-{k}.png")),
                    &actual,
                    640,
                    480,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            rows.push(serde_json::json!({"rotation":format!("{rotation:?}"),"radial":k,"checked":checked,"maxByteError":max_error}));
        }
    }
    if let Some(directory) = std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from)
    {
        std::fs::write(
            directory.join("routing-oracle.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn projections_picking_and_drag_share_the_lens_while_flat_geometry_is_fixed() {
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::HalfTurn,
        Rotation::Clockwise270,
    ] {
        let p = project(rotation, 0.018);
        let scene = sample(&p, 0.);
        let lens = scene.native_lens().unwrap();
        for world in [[480., 200., 0.], [530., 230., -90.], [350., 200., 100.]] {
            let point = scene.project_point(world).unwrap();
            let ideal = oracle(lens, [f64::from(point[0]), f64::from(point[1])]);
            let source = lens.plate().inverse(ideal).unwrap();
            let c = lens.calibration();
            let t = 500. + f64::from(world[2]);
            let expected = [
                (c.focal()[0] * (f64::from(world[0]) - 320.) / t + c.principal()[0]),
                (c.principal()[1] - c.focal()[1] * (240. - f64::from(world[1])) / t),
            ];
            assert!((source[0] - expected[0]).hypot(source[1] - expected[1]) < 0.0001);
            for viewport in [[640, 480], [192, 200], [900, 350]] {
                let delta = [3., -2.];
                let translation = scene.screen_translation(world, delta, viewport).unwrap();
                let moved = scene
                    .project_point(std::array::from_fn(|i| world[i] + translation[i]))
                    .unwrap();
                let scale = (viewport[0] as f32 / 640.).min(viewport[1] as f32 / 480.);
                for i in 0..2 {
                    assert!(((moved[i] - point[i]) * scale - delta[i]).abs() < 0.0002);
                }
            }
        }
        let anchor = scene.project_node(12).unwrap();
        let hit = scene
            .hit_candidates([anchor[0], anchor[1]])
            .unwrap()
            .into_iter()
            .find(|h| h.id == 12)
            .unwrap();
        for uv in hit.uv {
            assert!((uv - 0.5).abs() < 0.00001);
        }
        assert_eq!(scene.project_node(13).unwrap(), [600., 240., 0.5]);
        let registry = motion_effects::Registry::new_with_builtins().unwrap();
        let plane = scene.layers.iter().find(|l| l.id == 12).unwrap();
        assert_eq!(
            motion_render::selection_geometry::polygon(plane, &scene, &registry)
                .unwrap()
                .len(),
            64
        );
        assert!(scene
            .project_point([1e9, 1e9, 0.])
            .unwrap_err()
            .to_string()
            .contains("certified camera domain"));
    }
}
