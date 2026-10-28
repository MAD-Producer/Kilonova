//! Native precision, independently projected UVs, and source-revision admission.
use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_four_precisions_and_msaa_match_isolated_sources() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        for samples in [1, 4] {
            let root = tempfile::tempdir().unwrap();
            let project = textured_project(root.path(), samples);
            let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                precision,
                motion_render::AllocationBudget::new(128 << 20).unwrap(),
            ))
            .unwrap();
            prepare(&mut renderer, &project, root.path());
            let scene = sampled(&project, 0.);
            let target = renderer.render_target(96, 64).unwrap();
            let stats = renderer.draw(&scene, &target.view, 96, 64).unwrap();
            assert_eq!(stats.draw_calls, 3);
            let (format, bytes) = match precision {
                Unorm8 => (wgpu::TextureFormat::Rgba8UnormSrgb, 4),
                Unorm16 => (wgpu::TextureFormat::Rgba16Unorm, 8),
                Float16 => (wgpu::TextureFormat::Rgba16Float, 8),
                Float32 => (wgpu::TextureFormat::Rgba32Float, 16),
            };
            assert_eq!(target.view.texture().format(), format);
            let mut expected = Vec::new();
            for (id, range) in [(30, 0..bytes), (1, bytes..2 * bytes)] {
                let mut isolated = project.clone();
                isolated.layers.retain(|layer| layer.id == id);
                let reference = renderer.render_target(96, 64).unwrap();
                renderer
                    .draw(&sampled(&isolated, 0.), &reference.view, 96, 64)
                    .unwrap();
                expected.extend_from_slice(&two_native_pixels(&renderer, &reference)[range]);
            }
            assert!(expected.iter().any(|v| *v != 0));
            assert_eq!(
                two_native_pixels(&renderer, &target),
                expected,
                "actual texture/model depth must preserve {precision:?} samples={samples}"
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_alpha_changes_keep_fallback_and_restore_opaque_depth() {
    let root = tempfile::tempdir().unwrap();
    let project = textured_project(root.path(), 4);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &project, root.path());
    let opaque = capture(&mut renderer, &project);
    assert_eq!(pixel(&opaque, 56), &[255, 0, 0, 255]);
    for alpha in [128, 0] {
        let pixels = [0, 0, 255, alpha].repeat(8);
        renderer.upload_image(700, 4, 2, &pixels).unwrap();
        let actual = capture(&mut renderer, &project);
        let mut independent = pollster::block_on(Renderer::headless()).unwrap();
        prepare(&mut independent, &project, root.path());
        independent.upload_image(700, 4, 2, &pixels).unwrap();
        assert_eq!(
            actual,
            capture(&mut independent, &project),
            "transparent sources must retain the independent existing composition path"
        );
        if alpha == 0 {
            let mut mesh = project.clone();
            mesh.layers.retain(|layer| layer.id == 1);
            assert_eq!(actual, capture(&mut independent, &mesh));
        }
    }
    renderer
        .upload_image(700, 4, 2, &[0, 0, 255, 255].repeat(8))
        .unwrap();
    let recovered = capture(&mut renderer, &project);
    assert_eq!(pixel(&recovered, 40), &[0, 0, 255, 255]);
    assert_eq!(pixel(&recovered, 56), &[255, 0, 0, 255]);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_vertex_and_copy_on_write_budget_refuse_then_recover() {
    let root = tempfile::tempdir().unwrap();
    let mut project = textured_project(root.path(), 4);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let initial_memory = renderer.texture_memory_breakdown();
    prepare(&mut renderer, &project, root.path());
    let scene = sampled(&project, 0.);
    let target = renderer.capture_target(96, 64).unwrap();
    let original = renderer.capture(&scene, &target).unwrap().0;
    let budget = renderer.allocation_budget().clone();
    let used = budget.info().unwrap().admitted_bytes;
    // A referenced 4x2 RGBA source needs a new 32-byte admitted revision.
    budget.set_limit(used + 32 - 1).unwrap();
    let error = renderer
        .upload_image(700, 4, 2, &[0, 0, 255, 255].repeat(8))
        .unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error.to_string().contains("phase=allocate"));
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    budget.set_limit(128 << 20).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, original);
    let used = budget.info().unwrap().admitted_bytes;
    // Textured vertices carry clip XYZW, linear RGBA, and perspective UV.
    budget.set_limit(used + 6 * 40 - 1).unwrap();
    let error = renderer.draw(&scene, &target.view, 96, 64).unwrap_err();
    assert!(error.resource_rejection().is_some());
    assert!(error
        .to_string()
        .contains("immutable shared plane vertices"));
    assert_eq!(budget.info().unwrap().admitted_bytes, used);
    budget.set_limit(128 << 20).unwrap();
    assert_eq!(renderer.read_target(&target).unwrap(), original);
    renderer
        .upload_image(700, 4, 2, &[0, 0, 255, 255].repeat(8))
        .unwrap();
    let recovered = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(pixel(&recovered, 40), &[0, 0, 255, 255]);
    assert_eq!(pixel(&recovered, 56), &[255, 0, 0, 255]);
    project.layers.clear();
    project.model_assets.clear();
    project.assets.clear();
    renderer.configure_assets(&project, root.path()).unwrap();
    capture(&mut renderer, &project);
    assert!(block_depth_targets(&renderer).is_empty());
    let memory = renderer.texture_memory_breakdown();
    assert_eq!(memory.model_sources, 0);
    assert_eq!(
        memory.static_images, initial_memory.static_images,
        "static source bytes must return to the empty-catalog baseline"
    );
    assert_eq!(
        memory.system_textures, initial_memory.system_textures,
        "system textures, including the permanent white source, remain owned"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_parent_camera_uv_and_depth_match_independent_world_rays() {
    let root = tempfile::tempdir().unwrap();
    let mut project = textured_project(root.path(), 4);
    let colors = [
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
        [0, 255, 255, 255],
    ];
    let mut pixels = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            pixels.extend_from_slice(&colors[usize::from(x >= 8) + 2 * usize::from(y >= 8)]);
        }
    }
    image::save_buffer(
        root.path().join(&project.assets[0].path),
        &pixels,
        16,
        16,
        image::ColorType::Rgba8,
    )
    .unwrap();
    project.assets[0].width = 16;
    project.assets[0].height = 16;
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
    prepare(&mut renderer, &project, root.path());
    let scene = sampled(&project, 0.);
    let plane = scene.layers.iter().find(|l| l.id == 30).unwrap();
    let mesh = scene.layers.iter().find(|l| l.id == 1).unwrap();
    let plane_world = plane.model.as_dmat4();
    let mesh_world = mesh.model.as_dmat4() * glam::DMat4::from_scale(glam::DVec3::splat(20.));
    let inverse = scene.camera.view_projection.as_dmat4().inverse();
    let frame = capture(&mut renderer, &project);
    let mut checked = [0usize; 5];
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
            let Some(b) = hit(origin, direction, plane_world, [32., 20.], 0., 3.) else {
                continue;
            };
            let local = plane_world
                .inverse()
                .transform_point3(origin + direction * b);
            let uv = [local.x / 64. + 0.5, 0.5 - local.y / 40.];
            if (a - b).abs() < 2. || uv.iter().any(|v| (v - 0.5).abs() < 0.08) {
                continue;
            }
            let index = if a < b {
                4
            } else {
                usize::from(uv[0] >= 0.5) + 2 * usize::from(uv[1] >= 0.5)
            };
            let expected = if index == 4 {
                [255, 0, 0, 255]
            } else {
                colors[index]
            };
            assert_eq!(
                &frame[(y * 96 + x) * 4..][..4],
                &expected,
                "independent projected texture/depth x={x} y={y} uv={uv:?} mesh_t={a} plane_t={b}"
            );
            checked[index] += 1;
        }
    }
    assert!(
        checked[..4].iter().sum::<usize>() >= 30 && checked[4] >= 30,
        "both surfaces need ray coverage: {checked:?}"
    );
    assert!(
        checked[..4].iter().filter(|n| **n > 0).count() >= 2,
        "multiple image regions must be checked: {checked:?}"
    );
    assert_eq!(project, authored);
}
