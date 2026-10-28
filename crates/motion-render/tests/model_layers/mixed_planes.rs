//! Actual project mesh/plane intersections, with an independent f64 ray oracle.
use super::*;

#[path = "textured_planes.rs"]
mod textured_planes;

#[path = "transparent_planes.rs"]
mod transparent_planes;

fn mixed_project(root: &std::path::Path, samples: u32) -> Project {
    let mut project = crossing_project(root, samples, false);
    project.layers.truncate(1);
    let mut plane = flat(30, [64., 40.], [48., 32.], [0., 1., 0., 1.]);
    plane.three_d = true;
    project.layers.push(plane);
    project.validate().unwrap();
    project
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mixed_plane_mesh_intersections_use_fragment_depth_in_both_authored_orders() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let project = mixed_project(root.path(), samples);
        motion_core::storage::save(root.path(), &project).unwrap();
        let mut restored = motion_core::storage::load(root.path()).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.configure_assets(&restored, root.path()).unwrap();
        for source in &restored.model_assets {
            std::fs::remove_file(root.path().join(&source.path)).unwrap();
        }
        let first = capture(&mut renderer, &restored);
        assert_eq!(
            pixel(&first, 40),
            &[0, 255, 0, 255],
            "left ray must hit the nearer real plane"
        );
        assert_eq!(
            pixel(&first, 56),
            &[255, 0, 0, 255],
            "right ray must hit the nearer real mesh"
        );
        restored.layers.reverse();
        assert_eq!(
            capture(&mut renderer, &restored),
            first,
            "mesh/plane occlusion must not depend on authored layer order"
        );
        restored.layers.reverse();
        assert_eq!(restored, project);
    }
}

fn hit(
    origin: glam::DVec3,
    direction: glam::DVec3,
    world: glam::DMat4,
    half: [f64; 2],
    slope: f64,
    margin: f64,
) -> Option<f64> {
    let inverse = world.inverse();
    let origin = inverse.transform_point3(origin);
    let direction = inverse.transform_vector3(direction);
    let denominator = direction.z - slope * direction.x;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let t = (slope * origin.x - origin.z) / denominator;
    let point = origin + direction * t;
    (t > 0. && point.x.abs() < half[0] - margin && point.y.abs() < half[1] - margin).then_some(t)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mixed_plane_camera_and_parent_depth_match_independent_world_ray_intersections() {
    let root = tempfile::tempdir().unwrap();
    let mut project = mixed_project(root.path(), 4);
    project.camera.position.set_at(0, [60., 23., -90.]).unwrap();
    project.layers[1]
        .transform
        .rotation
        .set_at(0, [10., 12., -7.])
        .unwrap();
    let mut parent = flat(50, [1.; 2], [48., 32.], [0.; 4]);
    parent.content = Content::Null;
    parent.three_d = true;
    parent.transform.rotation.set_at(0, [0., -8., 4.]).unwrap();
    for layer in &mut project.layers {
        layer.parent = Some(motion_core::ParentLink {
            object: Some(50),
            bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
        });
    }
    project.layers.push(parent);
    project.validate().unwrap();
    let authored = project.clone();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let scene = sampled(&project, 0.);
    let mesh = scene.layers.iter().find(|l| l.id == 1).unwrap();
    let plane = scene.layers.iter().find(|l| l.id == 30).unwrap();
    let mesh_world = mesh.model.as_dmat4() * glam::DMat4::from_scale(glam::DVec3::splat(20.));
    let inverse = scene.camera.view_projection.as_dmat4().inverse();
    let frame = capture(&mut renderer, &project);
    let mut checked = [0usize; 2];
    for y in 0..64usize {
        for x in 0..96usize {
            let ndc = glam::DVec2::new((x as f64 + 0.5) / 48. - 1., 1. - (y as f64 + 0.5) / 32.);
            let near = inverse * glam::DVec4::new(ndc.x, ndc.y, 0., 1.);
            let far = inverse * glam::DVec4::new(ndc.x, ndc.y, 1., 1.);
            let origin = near.truncate() / near.w;
            let direction = (far.truncate() / far.w - origin).normalize();
            let Some(a) = hit(origin, direction, mesh_world, [0.8, 0.6], 0.75, 0.12) else {
                continue;
            };
            let Some(b) = hit(
                origin,
                direction,
                plane.model.as_dmat4(),
                [32., 20.],
                0.,
                3.,
            ) else {
                continue;
            };
            // Test interior rays well away from both silhouettes and the crossing.
            if (a - b).abs() < 2. {
                continue;
            }
            let index = usize::from(a < b);
            let color = if index == 1 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(&frame[(y * 96 + x) * 4..][..4], &color,
                "actual common-camera depth differs from independent world ray x={x} y={y} mesh_t={a} plane_t={b}");
            checked[index] += 1;
        }
    }
    assert!(
        checked.iter().all(|n| *n >= 30),
        "both geometry surfaces need meaningful ray coverage: {checked:?}"
    );
    assert_eq!(project, authored);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mixed_plane_four_precisions_and_msaa_match_independent_surface_colors() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        for samples in [1, 4] {
            let root = tempfile::tempdir().unwrap();
            let project = mixed_project(root.path(), samples);
            let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                precision,
                motion_render::AllocationBudget::new(128 << 20).unwrap(),
            ))
            .unwrap();
            renderer.configure_assets(&project, root.path()).unwrap();
            let scene = sampled(&project, 0.);
            let target = renderer.render_target(96, 64).unwrap();
            let stats = renderer.draw(&scene, &target.view, 96, 64).unwrap();
            assert_eq!(
                stats.draw_calls, 3,
                "one actual mesh, one actual plane and one composition raster"
            );
            let (format, bytes) = match precision {
                Unorm8 => (wgpu::TextureFormat::Rgba8UnormSrgb, 4),
                Unorm16 => (wgpu::TextureFormat::Rgba16Unorm, 8),
                Float16 => (wgpu::TextureFormat::Rgba16Float, 8),
                Float32 => (wgpu::TextureFormat::Rgba32Float, 16),
            };
            assert_eq!(target.view.texture().format(), format);
            let mut reference = Vec::new();
            for (id, range) in [(30, 0..bytes), (1, bytes..bytes * 2)] {
                let mut isolated = project.clone();
                isolated.layers.retain(|l| l.id == id);
                let isolated_target = renderer.render_target(96, 64).unwrap();
                renderer
                    .draw(&sampled(&isolated, 0.), &isolated_target.view, 96, 64)
                    .unwrap();
                let color = two_native_pixels(&renderer, &isolated_target);
                reference.extend_from_slice(&color[range]);
            }
            assert_eq!(reference.len(), bytes * 2);
            assert!(reference.iter().any(|v| *v != 0));
            assert_eq!(two_native_pixels(&renderer, &target), reference,
                "mixed geometry must retain actual working precision: {precision:?} samples={samples}");
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mixed_plane_pending_animated_geometry_remains_immutable_for_reverse_and_abandoned_frames() {
    let root = tempfile::tempdir().unwrap();
    let mut project = mixed_project(root.path(), 4);
    project.layers[1]
        .transform
        .rotation
        .set_animated(0, true)
        .unwrap();
    project.layers[1]
        .transform
        .rotation
        .set_at(60, [0., 28., 8.])
        .unwrap();
    let authored = project.clone();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let first_scene = sampled(&project, 0.);
    let last_scene = sampled(&project, 60.);
    let first = renderer.capture_target(96, 64).unwrap();
    let last = renderer.capture_target(96, 64).unwrap();
    let expected_first = renderer.capture(&first_scene, &first).unwrap().0;
    let expected_last = renderer.capture(&last_scene, &last).unwrap().0;
    assert_ne!(expected_first, expected_last);
    let mut a = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&first_scene, &first.view, 96, 64, &mut a, None)
        .unwrap();
    let mut b = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last_scene, &last.view, 96, 64, &mut b, None)
        .unwrap();
    renderer.submit_allocated(b.finish(), |_| {}).unwrap();
    renderer.submit_allocated(a.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&first).unwrap(), expected_first);
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    let reserved = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last_scene, &last.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        reserved
    );
    assert_eq!(project, authored);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn mixed_plane_exact_vertex_budget_and_invalid_object_refuse_before_recording_then_recover() {
    let root = tempfile::tempdir().unwrap();
    let mut project = mixed_project(root.path(), 4);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let scene = sampled(&project, 0.);
    let target = renderer.capture_target(96, 64).unwrap();
    let original = renderer.capture(&scene, &target).unwrap().0;
    let budget = renderer.allocation_budget().clone();
    let used = budget.info().unwrap().admitted_bytes;
    // Six immutable vertices, each holding clip XYZW and linear RGBA.
    budget.set_limit(used + 6 * 32 - 1).unwrap();
    let error = renderer.draw(&scene, &target.view, 96, 64).unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error
        .to_string()
        .contains("immutable shared plane vertices"));
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    budget.set_limit(128 << 20).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, original);
    let mut invalid = scene.clone();
    let plane = invalid.layers.iter_mut().find(|l| l.id == 30).unwrap();
    plane.id = u64::MAX;
    plane.model.x_axis.x = f32::NAN;
    let error = renderer.draw(&invalid, &target.view, 96, 64).unwrap_err();
    assert!(error
        .to_string()
        .contains("object=18446744073709551615 phase=shared_plane"));
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, original);
    let mut zero = project.clone();
    zero.layers[1]
        .transform
        .scale
        .set_at(0, [0., 100., 100.])
        .unwrap();
    let invisible_plane = capture(&mut renderer, &zero);
    zero.layers.retain(|l| l.id == 1);
    assert_eq!(
        capture(&mut renderer, &zero),
        invisible_plane,
        "zero-area plane is an ordinary empty visual"
    );
    project.layers.retain(|l| l.id == 30);
    project.model_assets.clear();
    renderer.configure_assets(&project, root.path()).unwrap();
    capture(&mut renderer, &project);
    assert!(block_depth_targets(&renderer).is_empty());
    assert_eq!(renderer.texture_memory_breakdown().model_sources, 0);
}
