use motion_core::{Layer, Project};
use motion_render::Scene;
use motion_render::{Presenter, Renderer};
#[test]
fn android_presentation_shader_is_valid_on_an_actual_gpu() {
    let mut r = pollster::block_on(Renderer::headless()).unwrap();
    let mut p = Project::new(64, 64, 30, 180).unwrap();
    p.layers.push(Layer::solid(
        1,
        "color",
        [64.0; 2],
        [32.0, 32.0, 0.0],
        [0.4, 0.6, 0.8, 1.0],
    ));
    let mut scene = Scene::new(&p);
    scene.sample(&p, 0.0, None, &motion_core::ExpressionEvaluator).unwrap();
    let source = r.capture_target(64, 64).unwrap();
    r.draw(&scene, &source.view, 64, 64).unwrap();
    let output = r.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    let presenter = Presenter::new(&r, &source.view, wgpu::TextureFormat::Rgba8Unorm);
    presenter.draw(&r, &view);
    r.device.poll(wgpu::Maintain::Wait);
    r.check_health().unwrap();
}

#[test]
fn presentation_validation_failures_are_visible_to_the_test() {
    let r = pollster::block_on(Renderer::headless()).unwrap();
    let source = r.capture_target(64, 64).unwrap();
    let output = r.capture_target(64, 64).unwrap();
    // The capture target is sRGB. An Unorm-only pipeline is deliberately
    // incompatible, exercising the same error callback as Presenter::draw.
    let presenter = Presenter::new(&r, &source.view, wgpu::TextureFormat::Rgba8Unorm);
    presenter.draw(&r, &output.view);
    r.device.poll(wgpu::Maintain::Wait);
    let error = r.check_health().unwrap_err();
    assert!(error.to_string().contains("format"), "{error}");
}
