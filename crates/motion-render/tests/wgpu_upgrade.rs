use motion_core::Project;
use motion_render::{RenderError, Renderer, Scene};
use std::error::Error as _;

#[test]
fn unavailable_adapter_keeps_the_original_request_cause() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let error = match pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )) {
        Err(error) => error,
        Ok(_) => panic!("an instance with no backends acquired an adapter"),
    };
    assert!(matches!(error, RenderError::Adapter(_)), "{error}");
    let source = error.source().expect("adapter request cause was discarded");
    assert!(error.to_string().contains(&source.to_string()));
}

#[test]
fn scoped_shader_rejection_is_reported_and_does_not_poison_later_rendering() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scope = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let invalid = renderer
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("controlled-wgpu-upgrade-invalid-shader"),
            source: wgpu::ShaderSource::Wgsl("invalid WGSL".into()),
        });
    let result = scope.pop();
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let error = pollster::block_on(result).expect("shader rejection was lost");
    assert!(error
        .to_string()
        .contains("controlled-wgpu-upgrade-invalid-shader"));
    drop(invalid);
    assert!(renderer.gpu_error().is_none());

    let project = Project::demo();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 72., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    assert!(renderer
        .capture(&scene, &target)
        .unwrap()
        .0
        .iter()
        .any(|v| *v != 0));
    assert!(renderer.gpu_error().is_none());
}
