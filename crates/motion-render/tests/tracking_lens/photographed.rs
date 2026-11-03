//! Real-photo calibration and measured corners, through actual native GPU output.
//! Placeholder video metadata is a model fixture, not source decode/relink proof.
use super::*;
use motion_model::Layer;
#[path = "../../../motion-model/tests/support/photographed_camera.rs"]
#[allow(dead_code)]
mod measured;

fn expected(lens: LensWarp, view: &measured::View, pixel: [f64; 2]) -> [u8; 4] {
    // Separate analytic-Jacobian Newton rays, fitted external R/t, and a world
    // plane equation. Do not reuse Scene camera matrices or the product inverse.
    let ideal = super::oracle(lens, pixel);
    let c = lens.calibration();
    let q = [
        (ideal[0] - c.principal()[0]) / c.focal()[0],
        (ideal[1] - c.principal()[1]) / c.focal()[1],
        1.,
    ];
    let r = view.world_to_camera_rotation;
    let t = view.world_to_camera_translation;
    let origin: [f64; 3] = std::array::from_fn(|i| -(0..3).map(|j| r[j][i] * t[j]).sum::<f64>());
    let ray: [f64; 3] = std::array::from_fn(|i| (0..3).map(|j| r[j][i] * q[j]).sum::<f64>());
    let distance = -origin[2] / ray[2];
    let x = origin[0] + distance * ray[0];
    let y = origin[1] + distance * ray[1];
    if distance <= 0. || !(0. ..8.).contains(&x) || !(0. ..5.).contains(&y) {
        [0; 4]
    } else if (x.floor() as i32 + y.floor() as i32) & 1 == 0 {
        [0, 255, 0, 255]
    } else {
        [255, 0, 255, 255]
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn photographed_board_replacement_and_held_out_corners_match_native_gpu_output() {
    let data = measured::Measurements::load();
    let mut project = data.project();
    project.background = [0.; 4];
    for y in 0..5 {
        for x in 0..8 {
            let color = if (x + y) & 1 == 0 {
                [0., 1., 0., 1.]
            } else {
                [1., 0., 1., 1.]
            };
            let mut layer = Layer::solid(
                100 + (y * 8 + x) as u64,
                "Measured board replacement",
                [100.; 2],
                [
                    320. + (x as f32 + 0.5) * 100.,
                    240. + (y as f32 + 0.5) * 100.,
                    0.,
                ],
                color,
            );
            layer.three_d = true;
            project.layers.push(layer);
        }
    }
    project.validate().unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(640, 480).unwrap();
    let mut rows = Vec::new();
    for (frame, view) in data.views.iter().enumerate() {
        let scene = super::routing::sample(&project, frame as f64);
        let lens = scene.native_lens().unwrap();
        let (actual, stats) = renderer.capture(&scene, &target).unwrap();
        assert!(stats.draw_calls >= 40);
        let mut reference = Vec::with_capacity(actual.len());
        for y in 0..480 {
            for x in 0..640 {
                reference.extend_from_slice(&expected(
                    lens,
                    view,
                    [x as f64 + 0.5, y as f64 + 0.5],
                ));
            }
        }
        let mut checked = 0;
        let mut interior = 0;
        let mut max_error = 0u8;
        for y in 2..478usize {
            for x in 2..638usize {
                let at = (y * 640 + x) * 4;
                let color = &reference[at..at + 4];
                // Same fixed two-pixel edge exclusion as existing lens routing
                // tests, to cover MSAA plus inverse bilinear filtering.
                if [-2isize, 0, 2].into_iter().any(|dy| {
                    [-2isize, 0, 2].into_iter().any(|dx| {
                        let offset =
                            ((y as isize + dy) as usize * 640 + (x as isize + dx) as usize) * 4;
                        reference[offset..offset + 4] != *color
                    })
                }) {
                    continue;
                }
                checked += 1;
                if color[3] > 0 {
                    interior += 1;
                }
                for ch in 0..4 {
                    max_error = max_error.max(actual[at + ch].abs_diff(color[ch]));
                }
            }
        }
        // Layer transforms and Scene::project_point take authored composition
        // coordinates. Canonical board RDF [x,y,z] maps to engine RUB world
        // [100*x,-100*y,-100*z], then to authored [320+100*x,240+100*y,100*z].
        let mut residuals = Vec::new();
        for &index in &view.validation_indices {
            let [x, y, z] = data.object_points[index];
            let pixel = scene
                .project_point([
                    320. + (x * 100.) as f32,
                    240. + (y * 100.) as f32,
                    (z * 100.) as f32,
                ])
                .unwrap();
            let external = view.projected[index];
            assert!(
                (f64::from(pixel[0]) - external[0]).hypot(f64::from(pixel[1]) - external[1])
                    < 0.0003,
                "{} {index}: actual={pixel:?} external={external:?}",
                view.name
            );
            let measured = data.measured_corners[&view.name][index];
            residuals
                .push((f64::from(pixel[0]) - measured[0]).hypot(f64::from(pixel[1]) - measured[1]));
        }
        let rms = (residuals.iter().map(|e| e * e).sum::<f64>() / 27.).sqrt();
        let max = residuals.into_iter().fold(0f64, f64::max);
        if let Some(dir) = std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from) {
            std::fs::create_dir_all(&dir).unwrap();
            for (name, pixels) in [("actual", &actual), ("reference", &reference)] {
                image::save_buffer(
                    dir.join(format!("photographed-{}-{name}.png", view.name)),
                    pixels,
                    640,
                    480,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        assert!(
            checked > 200000 && interior > 3000,
            "{} checked={checked} interior={interior}",
            view.name
        );
        assert!(max_error <= 2, "{} max_byte_error={max_error}", view.name);
        assert!(rms <= 1. && max <= 2.5, "{} rms={rms} max={max}", view.name);
        rows.push(
            serde_json::json!({"name":view.name,"checked":checked,"interior":interior,
            "maxByteError":max_error,"heldOutRms":rms,"heldOutMax":max,
            "derivativeBound":lens.derivative_bound(),"certificateRadius":lens.radius()}),
        );
    }
    if let Some(dir) = std::env::var_os("MOTION_LENS_ARTIFACTS").map(std::path::PathBuf::from) {
        std::fs::write(
            dir.join("photographed-oracle.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
