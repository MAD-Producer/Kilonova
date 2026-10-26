use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, Registry, WorkingSpace,
};
use motion_render::{Renderer, Scene};
#[path = "support/import_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

fn scene(project: &motion_core::Project) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn checked_numeric_png_and_private_byte_imports_drive_actual_pixels_in_four_precisions() {
    let mut rows = Vec::new();
    for private in [false, true] {
        let package = fixture::package(private);
        for precision in reference::MODES {
            let mut project = fixture::project(&package, precision);
            let format = match precision {
                ChannelPrecision::Unorm8 => wgpu::TextureFormat::Rgba8UnormSrgb,
                ChannelPrecision::Unorm16 => wgpu::TextureFormat::Rgba16Unorm,
                ChannelPrecision::Float16 => wgpu::TextureFormat::Rgba16Float,
                ChannelPrecision::Float32 => wgpu::TextureFormat::Rgba32Float,
            };
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
            let mut registry = Registry::new_with_builtins().unwrap();
            registry.insert(package.clone()).unwrap();
            renderer.set_effect_registry(registry);
            let contract = ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                precision,
            );
            let [w, h] = fixture::SIZE;
            let target = renderer.image_capture_target(w, h, contract).unwrap();
            renderer.preflight_effects(&scene(&project), w, h).unwrap();
            let stats = renderer
                .draw(&scene(&project), target.view(), w, h)
                .unwrap();
            let raw = renderer.read_image_target(&target).unwrap();
            let actual = reference::linear(&raw);
            let mut worst = 0_f64;
            for y in 0..h {
                for x in 0..w {
                    let expected = if private {
                        [0.5, 0.5, 0.5, 1.]
                    } else {
                        fixture::PIXELS[((y % 2) * 3 + x % 3) as usize].map(|v| f64::from(v) / 255.)
                    };
                    let expected = reference::store(expected, precision);
                    for c in 0..4 {
                        let encode = |v| {
                            if precision == ChannelPrecision::Unorm8 && c < 3 {
                                reference::encode(v)
                            } else {
                                v
                            }
                        };
                        worst = worst.max(
                            (encode(actual[(y * w + x) as usize][c]) - encode(expected[c])).abs(),
                        );
                    }
                }
            }
            let limit = match precision {
                ChannelPrecision::Unorm8 => 1. / 255. + 1e-8,
                ChannelPrecision::Unorm16 => 2. / 65535.,
                ChannelPrecision::Float16 => 0.001,
                ChannelPrecision::Float32 => 0.000002,
            };
            assert!(
                worst <= limit,
                "private={private} precision={precision:?} error={worst} limit={limit}"
            );
            let bytes = if private { 1024 } else { 24 };
            assert_eq!(
                renderer.texture_memory_breakdown().plugin_resources,
                bytes,
                "two logical consumers share one actual resource allocation"
            );
            let builds = renderer.effect_pipeline_builds();
            project.layers[0].effects[1]
                .params
                .get_mut("edge")
                .unwrap()
                .track
                .value[0] = 3.;
            renderer
                .draw(&scene(&project), target.view(), w, h)
                .unwrap();
            assert_eq!(
                renderer.read_image_target(&target).unwrap().pixels,
                raw.pixels
            );
            assert_eq!(renderer.effect_pipeline_builds(), builds);
            rows.push(serde_json::json!({"private":private,"precision":precision,"worst_error":worst,"limit":limit,"plugin_bytes":bytes,"draw_calls":stats.draw_calls,"pipeline_builds":builds,"numeric_reuse":true}));
        }
    }
    if let Some(dir) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join("package-image-imports.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
