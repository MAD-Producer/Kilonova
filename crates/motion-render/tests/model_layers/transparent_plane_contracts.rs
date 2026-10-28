//! Alpha coverage, precision and queued image revisions on actual hardware.
use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_plane_four_precisions_and_msaa_match_independent_depth_and_blend() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        for samples in [1, 4] {
            for image in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let project = alpha_project(root.path(), samples, image);
                let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                    precision,
                    motion_render::AllocationBudget::new(128 << 20).unwrap(),
                ))
                .unwrap();
                prepare_alpha(&mut renderer, &project, root.path());
                let target = renderer.render_target(96, 64).unwrap();
                renderer
                    .draw(&sampled(&project, 0.), &target.view, 96, 64)
                    .unwrap();
                let actual = two_native_pixels(&renderer, &target);
                let bytes = actual.len() / 2;
                let mut front = project.clone();
                front.layers[1].three_d = false;
                let reference = renderer.render_target(96, 64).unwrap();
                renderer
                    .draw(&sampled(&front, 0.), &reference.view, 96, 64)
                    .unwrap();
                let mut expected = two_native_pixels(&renderer, &reference)[..bytes].to_vec();
                let mut mesh = project.clone();
                mesh.layers.retain(|layer| layer.id == 1);
                renderer
                    .draw(&sampled(&mesh, 0.), &reference.view, 96, 64)
                    .unwrap();
                expected.extend_from_slice(&two_native_pixels(&renderer, &reference)[bytes..]);
                assert!(expected.iter().any(|v| *v != 0));
                assert_eq!(
                    actual, expected,
                    "alpha geometry precision={precision:?} samples={samples} image={image}"
                );
            }
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_plane_image_holes_do_not_write_depth_or_hide_opaque_surfaces() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let mut project = alpha_project(root.path(), samples, true);
        let mut back = flat(31, [64., 40.], [48., 32.], [0., 0., 1., 1.]);
        back.three_d = true;
        back.transform.position.set_at(0, [48., 32., 20.]).unwrap();
        project.layers.push(back);
        let row = [
            [0, 255, 0, 0],
            [0, 255, 0, 0],
            [0, 255, 0, 0],
            [0, 255, 0, 0],
            [0, 255, 0, 255],
            [0, 255, 0, 255],
            [0, 255, 0, 255],
            [0, 255, 0, 255],
        ]
        .concat();
        project.assets[0].width = 8;
        image::save_buffer(
            root.path().join(&project.assets[0].path),
            &row.repeat(2),
            8,
            2,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare_alpha(&mut renderer, &project, root.path());
        let frame = capture(&mut renderer, &project);
        assert_eq!(
            pixel(&frame, 24),
            &[0, 0, 255, 255],
            "zero-alpha image must reveal the rear plane"
        );
        assert_eq!(
            pixel(&frame, 40),
            &[255, 0, 0, 255],
            "zero-alpha image must reveal a farther real mesh"
        );
        assert_eq!(
            pixel(&frame, 56),
            &[255, 0, 0, 255],
            "nearer real mesh must cover an opaque image texel"
        );
        assert_eq!(
            pixel(&frame, 72),
            &[0, 255, 0, 255],
            "opaque texel must cover the rear plane"
        );
        project.layers.reverse();
        assert_eq!(capture(&mut renderer, &project), frame);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_plane_opacity_and_pending_revisions_survive_reverse_and_abandonment() {
    let root = tempfile::tempdir().unwrap();
    let mut project = alpha_project(root.path(), 4, true);
    project.layers[1].transform.opacity.set_at(0, 0.5).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare_alpha(&mut renderer, &project, root.path());
    let first = renderer.capture_target(96, 64).unwrap();
    let last = renderer.capture_target(96, 64).unwrap();
    let scene = sampled(&project, 0.);
    let expected_first = renderer.capture(&scene, &first).unwrap().0;
    let green = [0, 255, 0, 128].repeat(8);
    let blue = [0, 0, 255, 64].repeat(8);
    renderer.upload_image(701, 4, 2, &blue).unwrap();
    let expected_last = renderer.capture(&scene, &last).unwrap().0;
    assert_ne!(expected_first, expected_last);
    renderer.upload_image(701, 4, 2, &green).unwrap();
    let mut a = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scene, &first.view, 96, 64, &mut a, None)
        .unwrap();
    renderer.upload_image(701, 4, 2, &blue).unwrap();
    let mut b = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scene, &last.view, 96, 64, &mut b, None)
        .unwrap();
    renderer.submit_allocated(b.finish(), |_| {}).unwrap();
    renderer.submit_allocated(a.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&first).unwrap(), expected_first);
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    let used = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scene, &last.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        used
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_plane_opaque_coplanar_tie_retains_the_existing_mesh_winner() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let mut project = mixed_project(root.path(), samples);
        let source = b"mtllib paint.mtl\nv -0.8 -0.6 0\nv 0.8 -0.6 0\nv 0.8 0.6 0\nv -0.8 0.6 0\nusemtl Paint\nf 1 2 3 4\n";
        let imported = motion_scene::obj::import_obj(
            ImportContext {
                request: 8720,
                source: 7,
            },
            source,
            &mut Paint("newmtl Paint\nKd 1 0 0\nillum 0\n".into()),
            &Default::default(),
            &ImportLimits::default(),
        )
        .unwrap();
        let frozen = imported
            .freeze(
                source,
                motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
            )
            .unwrap();
        let asset = project.model_assets.iter_mut().find(|a| a.id == 7).unwrap();
        std::fs::write(root.path().join(&asset.path), &frozen).unwrap();
        asset.bytes = frozen.len() as u64;
        asset.sha256 = format!("{:x}", Sha256::digest(&frozen));
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare_alpha(&mut renderer, &project, root.path());
        let frame = capture(&mut renderer, &project);
        assert_eq!(pixel(&frame, 40), &[255, 0, 0, 255]);
        assert_eq!(pixel(&frame, 56), &[255, 0, 0, 255]);
        project.layers.reverse();
        assert_eq!(capture(&mut renderer, &project), frame);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn alpha_plane_budget_and_invalid_opacity_refuse_before_output_then_recover() {
    for image in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let project = alpha_project(root.path(), 4, image);
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare_alpha(&mut renderer, &project, root.path());
        let scene = sampled(&project, 0.);
        let target = renderer.capture_target(96, 64).unwrap();
        let original = renderer.capture(&scene, &target).unwrap().0;
        let budget = renderer.allocation_budget().clone();
        let used = budget.info().unwrap().admitted_bytes;
        let vertex_bytes = if image { 240 } else { 192 };
        budget.set_limit(used + vertex_bytes - 1).unwrap();
        let error = renderer.draw(&scene, &target.view, 96, 64).unwrap_err();
        assert!(error.resource_rejection().is_some());
        assert!(error
            .to_string()
            .contains("immutable shared plane vertices"));
        assert_eq!(budget.info().unwrap().admitted_bytes, used);
        budget.set_limit(128 << 20).unwrap();
        assert_eq!(renderer.read_target(&target).unwrap(), original);
        for opacity in [2.5, f32::NAN] {
            let mut invalid = scene.clone();
            let plane = invalid.layers.iter_mut().find(|l| l.id == 30).unwrap();
            plane.id = u64::MAX;
            plane.opacity = opacity;
            let error = renderer.draw(&invalid, &target.view, 96, 64).unwrap_err();
            assert!(error
                .to_string()
                .contains("object=18446744073709551615 phase=shared_plane"));
            assert_eq!(renderer.read_target(&target).unwrap(), original);
        }
        assert_eq!(renderer.capture(&scene, &target).unwrap().0, original);
    }
}
