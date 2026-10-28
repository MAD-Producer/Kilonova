//! Real mixed geometry must test alpha coverage against fragment depth.
use super::*;

fn alpha_project(root: &std::path::Path, samples: u32, image: bool) -> Project {
    let mut project = mixed_project(root, samples);
    if image {
        let rgba = [0, 255, 0, 128].repeat(8);
        image::save_buffer(
            root.join("assets/alpha-plane.png"),
            &rgba,
            4,
            2,
            image::ColorType::Rgba8,
        )
        .unwrap();
        project.assets.push(motion_core::Asset {
            id: 701,
            path: "assets/alpha-plane.png".into(),
            width: 4,
            height: 2,
        });
        project.layers[1].content = Content::Image { asset: 701 };
    } else {
        project.layers[1].content = Content::Solid {
            color: [0., 1., 0., 0.5],
        };
    }
    project.validate().unwrap();
    project
}

fn prepare_alpha(renderer: &mut Renderer, project: &Project, root: &std::path::Path) {
    renderer.configure_assets(project, root).unwrap();
    assert!(renderer
        .prepare_scene_assets(
            &sampled(project, 0.),
            motion_render::image_resources::Resolution::Full,
            false
        )
        .unwrap());
}

fn check_opaque_mesh_depth(image: bool) {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let authored = alpha_project(root.path(), samples, image);
        motion_core::storage::save(root.path(), &authored).unwrap();
        let mut project = motion_core::storage::load(root.path()).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare_alpha(&mut renderer, &project, root.path());
        for asset in &project.model_assets {
            std::fs::remove_file(root.path().join(&asset.path)).unwrap();
        }
        for asset in &project.assets {
            std::fs::remove_file(root.path().join(&asset.path)).unwrap();
        }
        let mut forced_front = project.clone();
        forced_front.layers[1].three_d = false;
        let front_reference = capture(&mut renderer, &forced_front);
        let mut mesh = project.clone();
        mesh.layers.retain(|layer| layer.id == 1);
        let mesh_reference = capture(&mut renderer, &mesh);
        let actual = capture(&mut renderer, &project);
        assert_eq!(
            pixel(&actual, 40),
            pixel(&front_reference, 40),
            "near alpha plane must blend over the farther real mesh"
        );
        assert_eq!(
            pixel(&actual, 56),
            pixel(&mesh_reference, 56),
            "near opaque mesh must hide the farther alpha plane"
        );
        project.layers.reverse();
        assert_eq!(
            capture(&mut renderer, &project),
            actual,
            "physical alpha/opaque depth cannot follow authored order"
        );
        project.layers.reverse();
        assert_eq!(project, authored);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn transparent_plane_solid_obeys_real_mesh_fragment_depth_in_both_orders() {
    check_opaque_mesh_depth(false);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn transparent_plane_image_obeys_real_mesh_fragment_depth_in_both_orders() {
    check_opaque_mesh_depth(true);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn transparent_plane_joins_the_global_mesh_alpha_queue_in_linear_color() {
    use motion_effects::image_contract::ChannelPrecision::Float32;
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let mut project = crossing_project(root.path(), samples, true);
        let mut plane = flat(30, [64., 40.], [48., 32.], [0., 1., 0., 0.5]);
        plane.three_d = true;
        project.layers.push(plane);
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            Float32,
            motion_render::AllocationBudget::new(128 << 20).unwrap(),
        ))
        .unwrap();
        prepare_alpha(&mut renderer, &project, root.path());
        let target = renderer.render_target(96, 64).unwrap();
        // Measure each independent source on this GPU: sRGB material sampling
        // can differ from the ideal endpoint by an ULP even in Float32 storage.
        // Compute the physical source-over result on CPU, retaining exact bytes.
        let mut expected = [0f32; 8];
        for id in [2, 30, 1] {
            let mut isolated = project.clone();
            isolated.layers.retain(|layer| layer.id == id);
            renderer
                .draw(&sampled(&isolated, 0.), &target.view, 96, 64)
                .unwrap();
            let source: Vec<f32> = two_native_pixels(&renderer, &target)
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            for pixel in [0, 4] {
                for channel in 0..4 {
                    expected[pixel + channel] = source[pixel + channel]
                        + expected[pixel + channel] * (1. - source[pixel + 3]);
                }
            }
        }
        assert!(expected[0] > expected[1] && expected[1] > expected[2] && expected[2] > 0.);
        assert_eq!(expected[3], 0.875);
        renderer
            .draw(&sampled(&project, 0.), &target.view, 96, 64)
            .unwrap();
        let actual = two_native_pixels(&renderer, &target);
        // Independent linear source-over: far blue, middle green, near red.
        assert_eq!(
            actual,
            bytemuck::cast_slice::<f32, u8>(&expected),
            "alpha planes and meshes need one global order, with no extra premultiplication"
        );
        let layers = project.layers.clone();
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            project.layers = order.map(|i| layers[i].clone()).to_vec();
            renderer
                .draw(&sampled(&project, 0.), &target.view, 96, 64)
                .unwrap();
            assert_eq!(
                two_native_pixels(&renderer, &target),
                actual,
                "alpha source order must be physical for {order:?}"
            );
        }
        let mut empty = Project::new(96, 64, 30, 61).unwrap();
        empty.background = [0.; 4];
        renderer.configure_assets(&empty, root.path()).unwrap();
        renderer
            .draw(&sampled(&empty, 0.), &target.view, 96, 64)
            .unwrap();
        assert_eq!(two_native_pixels(&renderer, &target), vec![0; 32]);
    }
}

#[path = "transparent_plane_contracts.rs"]
mod contracts;
