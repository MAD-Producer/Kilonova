use super::Presenter;
use crate::{RenderTarget, Renderer, Scene};

fn fill(renderer: &mut Renderer, target: &RenderTarget, color: [f32; 4]) {
    let mut project = motion_core::Project::new(8, 8, 30, 1).unwrap();
    project.background = color;
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.draw(&scene, &target.view, 8, 8).unwrap();
}

fn assert_pixels(renderer: &Renderer, presenter: &Presenter, expected: [u8; 4]) {
    for navigation in [false, true] {
        let output = renderer.capture_target(8, 8).unwrap();
        if navigation {
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            presenter
                .encode_view(
                    &renderer.queue,
                    &mut encoder,
                    &output.view,
                    None,
                    [0., 0., 8., 8.],
                    Some([0., 0., 8., 8.]),
                    [0.; 4],
                )
                .unwrap();
            renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        } else {
            presenter.draw(renderer, &output.view).unwrap();
        }
        let pixels = renderer.read_target(&output).unwrap();
        assert_eq!(pixels.len(), 8 * 8 * 4);
        assert!(
            pixels.chunks_exact(4).all(|pixel| pixel == expected),
            "navigation={navigation} expected={expected:?}"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn repeated_source_views_reuse_both_bindings_but_pixels_and_new_sources_still_update() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let first = renderer.render_target(8, 8).unwrap();
    let second = renderer.render_target(8, 8).unwrap();
    fill(&mut renderer, &first, [1., 0., 0., 1.]);
    fill(&mut renderer, &second, [0., 0., 1., 1.]);
    let mut presenter =
        Presenter::new_admitted(&renderer, &first.view, renderer.target_format).unwrap();
    assert_pixels(&renderer, &presenter, [255, 0, 0, 255]);
    let group = presenter.group.clone();
    let view_group = presenter.view_group.clone();
    let before = renderer.allocation_budget().info().unwrap();
    let alias = first.view.clone();
    for _ in 0..120 {
        presenter.set_admitted_source(&renderer.device, &alias);
        assert_eq!(presenter.group, group);
        assert_eq!(presenter.view_group, view_group);
        assert!(presenter.source_allocation.is_some());
    }
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before.admitted_bytes
    );
    fill(&mut renderer, &first, [0., 1., 0., 1.]);
    presenter.set_admitted_source(&renderer.device, &first.view);
    assert_eq!(presenter.group, group);
    assert_eq!(presenter.view_group, view_group);
    assert_pixels(&renderer, &presenter, [0, 255, 0, 255]);
    presenter.set_admitted_source(&renderer.device, &second.view);
    assert_ne!(presenter.group, group);
    assert_ne!(presenter.view_group, view_group);
    assert_pixels(&renderer, &presenter, [0, 0, 255, 255]);
    let group = presenter.group.clone();
    let view_group = presenter.view_group.clone();
    presenter.set_source(&renderer.device, &second.view);
    assert!(
        presenter.source_allocation.is_none(),
        "raw input relinquishes the lease even on a hit"
    );
    assert_eq!(presenter.group, group);
    assert_eq!(presenter.view_group, view_group);
    presenter.set_admitted_source(&renderer.device, &second.view);
    assert!(presenter.source_allocation.is_some());
    presenter.set_admitted_source(&renderer.device, &first.view);
    assert_ne!(presenter.group, group);
    assert_ne!(presenter.view_group, view_group);
    assert_pixels(&renderer, &presenter, [0, 255, 0, 255]);
    // A distinct view of the same texture is a different binding identity.
    let group = presenter.group.clone();
    let view_group = presenter.view_group.clone();
    let alternate = first.texture().create_view(&Default::default());
    presenter.set_source(&renderer.device, &alternate);
    assert_ne!(presenter.group, group);
    assert_ne!(presenter.view_group, view_group);
    assert!(presenter.source_allocation.is_none());
    assert_pixels(&renderer, &presenter, [0, 255, 0, 255]);
}
