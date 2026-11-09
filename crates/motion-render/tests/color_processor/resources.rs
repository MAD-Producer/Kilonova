//! Prepare the real production native runtime from verified resource bytes,
//! then compare both GPU consumers with the separate native file-reader oracle.
use super::*;
use motion_effects::{native_color::NativeColorRuntime, AlphaMode};
#[path = "../../../motion-effects/tests/support/native_resources.rs"]
mod fixture;

#[test]
#[ignore = "requires fresh native file-resource oracle and actual GPU"]
fn native_file_resources_match_cpu_in_compute_and_float_image_paths() {
    let manifest = std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
    );
    let runtime = NativeColorRuntime::load(&manifest).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    println!("native resources adapter={:?}", renderer.adapter_info);
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12),
        _ => {}
    }
    let reference: Reference = serde_json::from_slice(include_bytes!(
        "../../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap();
    let mut checked = 0;
    for case in fixture::cases() {
        let config = fixture::config(&case);
        let selection = fixture::selection(&config);
        for phase in [
            ColorProcessorPhase::InputToWorking,
            ColorProcessorPhase::WorkingToDisplay,
            ColorProcessorPhase::WorkingToOutput,
        ] {
            for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
                let native = runtime
                    .prepare(&config, &selection, phase, alpha, alpha)
                    .unwrap();
                let source = native.source();
                let pixels: Vec<_> = case
                    .input
                    .iter()
                    .map(|value| {
                        let mut value = *value;
                        if alpha == AlphaMode::Premultiplied {
                            for c in 0..3 {
                                value[c] *= value[3];
                            }
                        }
                        value
                    })
                    .collect();
                let actual = run(
                    &renderer,
                    &source.compile(&config, &selection).unwrap(),
                    &pixels,
                );
                let image =
                    super::cubes::image(&mut renderer, source, &config, &selection, &pixels);
                assert_eq!(image.len(), 12);
                for (path, result) in [("compute", &actual[..12]), ("image", image.as_slice())] {
                    for (index, (got, want)) in result.iter().zip(&case.expected).enumerate() {
                        for c in 0..3 {
                            let want = want[c]
                                * if alpha == AlphaMode::Premultiplied {
                                    want[3]
                                } else {
                                    1.
                                };
                            let tolerance = reference.absolute_tolerance
                                + reference.relative_tolerance * want.abs();
                            assert!((got[c]-want).abs()<=tolerance,"{path} {phase:?} {alpha:?} pixel={index} c={c} got={} want={want} tolerance={tolerance}",got[c]);
                        }
                        assert_eq!(got[3].to_bits(), case.input[index][3].to_bits());
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 720);
    println!("native verified-resource CPU/GPU pixels={checked}; three phases; two alpha modes; no oracle files remain");
}
