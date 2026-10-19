//! Compare the actual old sampler and new perspective filter on the same
//! renderer, geometry, source and device. Capture costs include GPU readback.
use super::*;
use glam::{Mat4, Vec3};
use motion_core::{Layer, Project};
use std::time::Instant;

fn hardware_pipeline(renderer: &Renderer) -> wgpu::RenderPipeline {
    let source = format!("{}\n{}", include_str!("../plane.wgsl"),
        "fn sample_plane_texture(source:texture_2d<f32>, linear_sampler:sampler, uv:vec2<f32>)->vec4<f32>{return textureSample(source,linear_sampler,uv);}");
    let source = crate::plane_coverage::shader(&source, true);
    let shader = renderer
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reference hardware plane filter"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let layout = renderer
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&renderer.uniform_layout), Some(&renderer.image_layout)],
            immediate_size: 0,
        });
    renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reference hardware plane filter"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: renderer.target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
}
fn fixture(renderer: &mut Renderer) -> Scene {
    let mut p = Project::new(256, 256, 30, 60).unwrap();
    p.background = [0.; 4];
    for (id, angle) in [(1, 55f32), (2, -45f32)] {
        let mut l = Layer::solid(
            id,
            "authored checker plane",
            [300.; 2],
            [128., 128., 0.],
            [1.; 4],
        );
        l.three_d = true;
        l.transform.rotation.value[1] = angle;
        p.layers.push(l);
    }
    let mut scene = Scene::new(&p);
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let camera = |eye, target, fov: f32| {
        Mat4::perspective_rh(fov.to_radians(), 1., 0.5, 100000.)
            * Mat4::look_at_rh(eye, target, Vec3::Y)
    };
    let red = camera(Vec3::new(0., 0., 300.), Vec3::ZERO, 50.);
    scene.camera.view_projection = red;
    for l in &mut scene.layers {
        if l.id == 1 {
            l.view_projection = red;
            l.opacity = 0.;
        } else {
            l.view_projection = camera(Vec3::new(55., 0., 300.), Vec3::new(10., 0., 0.), 60.);
            l.asset = Some(7);
        }
    }
    let pixels: Vec<_> = (0u8..64)
        .flat_map(|y| {
            (0u8..64).flat_map(move |x| {
                [
                    if (x / 8 + y / 8) % 2 == 0 { 255 } else { 0 },
                    x * 4,
                    y * 4,
                    255,
                ]
            })
        })
        .collect();
    renderer.upload_image(7, 64, 64, &pixels).unwrap();
    scene
}
fn max_difference(a: &[u8], b: &[u8]) -> u8 {
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap()
}

#[test]
fn perspective_filter_preserves_split_pixels_without_additional_texture_capacity() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = fixture(&mut renderer);
    let mut single = scene.clone();
    single.layers.retain(|l| l.id == 2);
    let target = renderer.capture_target(256, 256).unwrap();
    let precise = renderer.pipeline.clone();
    let hardware = hardware_pipeline(&renderer);
    let precise_reference = renderer.capture(&single, &target).unwrap().0;
    let precise_split = renderer.capture(&scene, &target).unwrap().0;
    let bytes = renderer.texture_bytes();
    renderer.pipeline = hardware.clone();
    let hardware_reference = renderer.capture(&single, &target).unwrap().0;
    let hardware_split = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(renderer.texture_bytes(), bytes);
    let improved = max_difference(&precise_reference, &precise_split);
    let worst = precise_reference.iter().zip(&precise_split).enumerate()
        .max_by_key(|(_, (a, b))| a.abs_diff(**b)).unwrap().0 / 4;
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, pixels) in [("projected-reference.png", &precise_reference),
            ("projected-split.png", &precise_split)] {
            image::RgbaImage::from_raw(256, 256, pixels.clone()).unwrap()
                .save(folder.join(name)).unwrap();
        }
    }
    assert!(improved <= 3, "perspective split difference {improved} pixel=({}, {}) reference={:?} split={:?}",
        worst % 256, worst / 256, &precise_reference[worst*4..worst*4+4], &precise_split[worst*4..worst*4+4]);
    let baseline = max_difference(&hardware_reference, &hardware_split);
    // Drivers may have higher precision than the observed reference device;
    // do not require a driver defect or a particular speed ratio to pass.
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, pixels) in [
            ("projected-checker.png", precise_split),
            ("projected-checker-hardware.png", hardware_split),
        ] {
            image::RgbaImage::from_raw(256, 256, pixels)
                .unwrap()
                .save(folder.join(name))
                .unwrap();
        }
        let large = renderer.capture_target(1024, 1024).unwrap();
        let mut timings = [Vec::new(), Vec::new()];
        // Alternate first position as well as filter to avoid a fixed ordering
        // advantage. Keep cold captures and every warm capture in the report.
        for iteration in 0..10 {
            for mode in if iteration % 2 == 0 { [0, 1] } else { [1, 0] } {
                renderer.pipeline = if mode == 0 {
                    hardware.clone()
                } else {
                    precise.clone()
                };
                let start = Instant::now();
                renderer.capture(&scene, &large).unwrap();
                timings[mode].push(start.elapsed().as_micros());
                assert_eq!(renderer.texture_bytes(), bytes);
            }
        }
        let cost = serde_json::json!({"width":1024,"height":1024,
            "adapter":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),
            "hardware_capture_us":timings[0],"precise_capture_us":timings[1],
            "hardware_split_max_code_difference":baseline,"precise_split_max_code_difference":improved,
            "renderer_texture_bytes":bytes,"additional_texture_bytes":0,
            "scope":"Same renderer and split-plane fixture; capture includes CPU preparation, GPU rendering and image readback. Not GPU-only timing or production playback FPS. Perspective filter uses up to four textureLoad operations instead of one hardware sampling operation (one load for 1x1 textures); flat draws retain the hardware filter."});
        std::fs::write(
            folder.join("projected-filter-cost.json"),
            serde_json::to_vec_pretty(&cost).unwrap(),
        )
        .unwrap();
    }
    renderer.pipeline = precise;
    renderer.check_health().unwrap();
}
