//! Owned texture content and geometry must remain independent across pending frames.
use super::*;

fn textured_project(root: &std::path::Path, samples: u32) -> Project {
    let mut project = mixed_project(root, samples);
    let rgba = [0, 255, 0, 255].repeat(8);
    image::save_buffer(
        root.join("assets/shared-plane.png"),
        &rgba,
        4,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    project.assets.push(motion_core::Asset {
        id: 700,
        path: "assets/shared-plane.png".into(),
        width: 4,
        height: 2,
    });
    project.layers[1].content = Content::Image { asset: 700 };
    project.validate().unwrap();
    project
}

fn prepare(renderer: &mut Renderer, project: &Project, root: &std::path::Path) {
    renderer.configure_assets(project, root).unwrap();
    assert!(renderer
        .prepare_scene_assets(
            &sampled(project, 0.),
            motion_render::image_resources::Resolution::Full,
            false,
        )
        .unwrap());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_owned_image_and_mesh_use_fragment_depth_in_both_orders() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let authored = textured_project(root.path(), samples);
        motion_core::storage::save(root.path(), &authored).unwrap();
        let mut project = motion_core::storage::load(root.path()).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        prepare(&mut renderer, &project, root.path());
        std::fs::remove_file(root.path().join(&project.assets[0].path)).unwrap();
        for asset in &project.model_assets {
            std::fs::remove_file(root.path().join(&asset.path)).unwrap();
        }
        let frame = capture(&mut renderer, &project);
        assert_eq!(pixel(&frame, 40), &[0, 255, 0, 255]);
        assert_eq!(
            pixel(&frame, 56),
            &[255, 0, 0, 255],
            "opaque image plane must not cover a nearer mesh"
        );
        project.layers.reverse();
        assert_eq!(capture(&mut renderer, &project), frame);
        project.layers.reverse();
        assert_eq!(project, authored);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn textured_plane_reupload_keeps_earlier_pending_frame_pixels_and_capacity() {
    let root = tempfile::tempdir().unwrap();
    let project = textured_project(root.path(), 4);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    prepare(&mut renderer, &project, root.path());
    let scene = sampled(&project, 0.);
    let first = renderer.capture_target(96, 64).unwrap();
    let last = renderer.capture_target(96, 64).unwrap();
    let green = [0, 255, 0, 255].repeat(8);
    let blue = [0, 0, 255, 255].repeat(8);
    let expected_first = renderer.capture(&scene, &first).unwrap().0;
    renderer.upload_image(700, 4, 2, &blue).unwrap();
    let expected_last = renderer.capture(&scene, &last).unwrap().0;
    assert_ne!(expected_first, expected_last);
    renderer.upload_image(700, 4, 2, &green).unwrap();
    let mut a = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scene, &first.view, 96, 64, &mut a, None)
        .unwrap();
    renderer.upload_image(700, 4, 2, &blue).unwrap();
    let mut b = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&scene, &last.view, 96, 64, &mut b, None)
        .unwrap();
    renderer.submit_allocated(b.finish(), |_| {}).unwrap();
    renderer.submit_allocated(a.finish(), |_| {}).unwrap();
    assert_eq!(
        renderer.read_target(&first).unwrap(),
        expected_first,
        "source reupload must not mutate an already encoded image plane"
    );
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

#[path = "textured_plane_contracts.rs"]
mod contracts;
