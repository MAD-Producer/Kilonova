//! An independent four-colour image checks that exact texel fetches do not
//! replace bilinear filtering or partial coverage on fractional transforms.
use motion_core::{Asset, Content, Layer, Project};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_render::{Renderer, Scene};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn integer_grid_preserves_exact_pixels_while_subpixel_and_scaled_draws_keep_linear_sampling() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(
        &instance,
        None,
        wgpu::TextureFormat::Rgba32Float,
    ))
    .unwrap();
    let mut project = Project::new(4, 4, 30, 60).unwrap();
    project.working_precision = ChannelPrecision::Float32;
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "independent primary-colour grid",
        [2.; 2],
        [2., 2., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 7 };
    project.layers.push(layer);
    project.assets.push(Asset {
        id: 7,
        path: "assets/authored-grid.png".into(),
        width: 2,
        height: 2,
    });
    renderer
        .upload_image(
            7,
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )
        .unwrap();
    let target = renderer
        .image_capture_target(
            4,
            4,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                ChannelPrecision::Float32,
            ),
        )
        .unwrap();
    let mut sample = |project: &Project| {
        let mut scene = Scene::new(project).unwrap();
        scene
            .sample(project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        renderer.draw(&scene, target.view(), 4, 4).unwrap();
        let raw = renderer.read_image_target(&target).unwrap();
        raw.pixels
            .chunks_exact(16)
            .map(|p| {
                std::array::from_fn::<_, 4, _>(|c| {
                    f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>()
    };
    let close = |actual: [f32; 4], expected: [f32; 4]| {
        for c in 0..4 {
            assert!(
                (actual[c] - expected[c]).abs() < 1e-6,
                "actual={actual:?} expected={expected:?}"
            );
        }
    };
    let integer = sample(&project);
    for (index, expected) in [
        (5, [1., 0., 0., 1.]),
        (6, [0., 1., 0., 1.]),
        (9, [0., 0., 1., 1.]),
        (10, [1.; 4]),
    ] {
        close(integer[index], expected);
    }
    project.layers[0].transform.position.value[0] += 0.25;
    project.layers[0].transform.position.value[1] += 0.25;
    let subpixel = sample(&project);
    close(subpixel[5], [0.5625, 0., 0., 0.5625]);
    close(subpixel[6], [0.1875, 0.5625, 0., 0.75]);
    close(subpixel[7], [0., 0.1875, 0., 0.1875]);
    project.layers[0].transform.position.value = [2., 2., 0.];
    project.layers[0].transform.scale.value[0] = 200.;
    project.layers[0].transform.scale.value[1] = 200.;
    let scaled = sample(&project);
    close(scaled[5], [0.625, 0.25, 0.25, 1.]);
    assert!(
        scaled[5][1] > 0.2,
        "negative control: nearest sampling must not pass a scaled draw"
    );
    if let Some(directory) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::PathBuf::from(directory).join("pixel-grid-sampling.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"integer":integer,"subpixel":subpixel,"scaled":scaled}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
