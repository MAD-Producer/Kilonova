use motion_core::{Layer, Project};
use motion_render::Scene;
use motion_render::{Presenter, Renderer};

#[test]
fn canvas_zoom_pan_and_fit_crop_pixels_inside_a_valid_surface_viewport() {
    let r = pollster::block_on(Renderer::headless()).unwrap();
    let texture = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("navigation red/green source"),
        size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixels: Vec<u8> = (0..64).flat_map(|i|
        if i%8 < 4 { [255,0,0,255] } else { [0,255,0,255] }).collect();
    r.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0,
        origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, &pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(32), rows_per_image: Some(8) },
        wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 });
    let source = texture.create_view(&Default::default());
    let presenter = Presenter::new(&r, &source, wgpu::TextureFormat::Rgba8UnormSrgb);
    let output = r.capture_target(64,64).unwrap();
    let render = |image| {
        let mut encoder = r.device.create_command_encoder(&Default::default());
        presenter.encode_view(&r.queue, &mut encoder, &output.view, None,
            [8.,8.,48.,48.], Some(image), [0.,0.,1.,1.]);
        r.submit_tracked(encoder.finish(), |_| {});
        let pixels = r.read_target(&output).unwrap();
        r.check_health().unwrap();
        pixels
    };
    let pixel = |pixels: &[u8], x: usize, y: usize| pixels[(y*64+x)*4..(y*64+x)*4+4].to_vec();
    let fit = render([8.,8.,48.,48.]);
    assert_eq!(pixel(&fit,16,32), [255,0,0,255]);
    assert_eq!(pixel(&fit,48,32), [0,255,0,255]);
    let zoom = render([8.,8.,96.,48.]);
    assert_eq!(pixel(&zoom,40,32), [255,0,0,255]);
    let pan = render([-40.,8.,96.,48.]);
    assert_eq!(pixel(&pan,24,32), [0,255,0,255]);
    let small = render([20.,20.,24.,24.]);
    assert_eq!(pixel(&small,10,32), [0,0,255,255]);
    assert_eq!(pixel(&small,24,32), [255,0,0,255]);
    assert_eq!(pixel(&small,40,32), [0,255,0,255]);
    assert_eq!(pixel(&small,60,32), [0,0,255,255]);
}
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
    r.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
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
    r.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    let error = r.check_health().unwrap_err();
    assert!(error.to_string().contains("format"), "{error}");
}
