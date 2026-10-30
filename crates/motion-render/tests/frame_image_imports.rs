//! Real uploads, typed coverage, independent pixels, retained allocation IDs,
//! refusal before target replacement and recovery on the same GPU owner.
use motion_core::{
    masks::LayerMask,
    vector::{PathNode, VectorPath},
    EffectInstance, Track,
};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
};
use motion_render::{Renderer, Scene, TextureCategory, TextureResource};
use std::sync::Arc;
#[allow(dead_code)]
#[path = "support/import_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/precision_golden.rs"]
mod reference;

fn sample(p: &motion_core::Project) -> Scene {
    let mut s = Scene::new(p).unwrap();
    s.sample(p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s
}
fn renderer(precision: ChannelPrecision, package: Arc<EffectPackage>) -> Renderer {
    let format = match precision {
        ChannelPrecision::Unorm8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        ChannelPrecision::Unorm16 => wgpu::TextureFormat::Rgba16Unorm,
        ChannelPrecision::Float16 => wgpu::TextureFormat::Rgba16Float,
        ChannelPrecision::Float32 => wgpu::TextureFormat::Rgba32Float,
    };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    renderer.set_effect_registry(registry);
    renderer
}
fn target(r: &Renderer, precision: ChannelPrecision) -> motion_render::ImageCaptureTarget {
    r.image_capture_target(
        fixture::SIZE[0],
        fixture::SIZE[1],
        ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            precision,
        ),
    )
    .unwrap()
}
fn resources(r: &Renderer, category: TextureCategory) -> Vec<TextureResource> {
    r.texture_resource_ledger()
        .resources()
        .filter(|t| {
            t.references
                .iter()
                .any(|reference| reference.category == category)
        })
        .cloned()
        .collect()
}
fn compare(
    raw: &motion_render::CapturedImage,
    precision: ChannelPrecision,
    expected: impl Fn(u32, u32) -> Option<[f64; 4]>,
) -> f64 {
    let actual = reference::linear(raw);
    let [w, h] = fixture::SIZE;
    let mut worst = 0_f64;
    let mut worst_pixel = None;
    for y in 0..h {
        for x in 0..w {
            if let Some(expected) = expected(x, y) {
                let expected = reference::store(expected, precision);
                for c in 0..4 {
                    let encode = |v| {
                        if precision == ChannelPrecision::Unorm8 && c < 3 {
                            reference::encode(v)
                        } else {
                            v
                        }
                    };
                    let error =
                        (encode(actual[(y * w + x) as usize][c]) - encode(expected[c])).abs();
                    if error > worst {
                        worst = error;
                        worst_pixel = Some((x, y, c, actual[(y * w + x) as usize], expected));
                    }
                }
            }
        }
    }
    let limit = match precision {
        ChannelPrecision::Unorm8 => 2. / 255. + 1e-8,
        ChannelPrecision::Unorm16 => 3. / 65535.,
        ChannelPrecision::Float16 => 0.001,
        ChannelPrecision::Float32 => 0.000002,
    };
    assert!(
        worst <= limit,
        "precision={precision:?} worst={worst} limit={limit} worst_pixel={worst_pixel:?}"
    );
    worst
}
fn write(name: &str, rows: &[serde_json::Value]) {
    if let Some(dir) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join(name),
            serde_json::to_vec_pretty(rows).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn dynamic_private_values_keep_companion_pixels_and_pipeline_identity_in_four_precisions() {
    let package = fixture::dynamic_package();
    let mut rows = vec![];
    for precision in reference::MODES {
        let mut p = fixture::project(&package, precision);
        for (i, effect) in p.layers[0].effects.iter_mut().enumerate() {
            let mut values = package.manifest.effects[0]
                .shader_data
                .as_ref()
                .unwrap()
                .values(Some(&package.files["assets/pixels.png"]))
                .unwrap();
            values.texture[0] = (i + 1) as f32 * 0.25;
            effect.shader_data = Some(Arc::new(values));
        }
        let mut r = renderer(precision, package.clone());
        let target = target(&r, precision);
        let [w, h] = fixture::SIZE;
        let mut builds = None;
        let mut companion = None;
        for value in [0.5_f32, 0.75, 0.25, 0.5] {
            Arc::make_mut(p.layers[0].effects[1].shader_data.as_mut().unwrap()).texture[0] = value;
            r.preflight_effects(&sample(&p), w, h).unwrap();
            r.draw(&sample(&p), target.view(), w, h).unwrap();
            let raw = r.read_image_target(&target).unwrap();
            let worst = compare(&raw, precision, |x, y| {
                let mut pixel =
                    fixture::PIXELS[((y % 2) * 3 + x % 3) as usize].map(|v| f64::from(v) / 255.);
                for c in &mut pixel[..3] {
                    *c *= f64::from(value);
                }
                Some(pixel)
            });
            let native = resources(&r, TextureCategory::PluginResources);
            let image = native
                .iter()
                .find(|t| t.width == 3 && t.height == 2)
                .unwrap();
            assert_eq!(image.format, "Rgba8Unorm");
            assert_eq!(image.logical_bytes, Some(24));
            assert_eq!(
                native
                    .iter()
                    .filter(|t| t.width == 256 && t.height == 1)
                    .count(),
                3,
                "one package default plus two distinct current-instance resources"
            );
            for texture in &native {
                assert_eq!(texture.format, "Rgba8Unorm");
            }
            if let Some(builds) = builds {
                assert_eq!(r.effect_pipeline_builds(), builds);
            } else {
                builds = Some(r.effect_pipeline_builds());
            }
            if let Some(old) = &companion {
                assert_eq!(image, old);
            } else {
                companion = Some(image.clone());
            }
            // A malformed current sample must preserve prior output and owner
            // health, then allow the exact same renderer to recover.
            let mut bad = sample(&p);
            Arc::make_mut(bad.effects[1].shader_data.as_mut().unwrap())
                .texture
                .push(0.);
            assert!(r.preflight_effects(&bad, w, h).is_err());
            assert_eq!(r.read_image_target(&target).unwrap().pixels, raw.pixels);
            r.draw(&sample(&p), target.view(), w, h).unwrap();
            assert_eq!(r.read_image_target(&target).unwrap().pixels, raw.pixels);
            r.check_health().unwrap();
            rows.push(serde_json::json!({"precision":precision,"value":value,"worst_error":worst,
                "companion":image,"resources":native,"native_pipeline_builds":builds,"refusal_and_recovery":true}));
        }
    }
    write("dynamic-frame-imports.json", &rows);
}

fn curve_package() -> Arc<EffectPackage> {
    let dir = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut def = manifest
        .effects
        .iter()
        .find(|d| d.id == "curves")
        .unwrap()
        .clone();
    def.working_space = WorkingSpace::Linear;
    def.alpha_mode = AlphaMode::Premultiplied;
    def.resources.clear();
    def.passes = vec![PassDefinition {
        shader: "shaders/curve.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "test.typed-curve-mask-import".into();
    manifest.effects = vec![def];
    std::fs::create_dir(dir.path().join("shaders")).unwrap();
    std::fs::write(dir.path().join("shaders/curve.wgsl"),
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return curve_lookup(vec4(0.4,0.5,0.6,1.0))*sample_input(p).a;}").unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(dir.path()).unwrap()).unwrap(),
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn curve_and_mask_imports_drive_pixels_and_retain_native_backings_in_four_precisions() {
    let package = curve_package();
    let mut rows = vec![];
    for precision in reference::MODES {
        let mut p = fixture::project(&package, precision);
        p.layers[0].effects.truncate(1);
        p.layers[0].effects[0] = EffectInstance::new(
            11,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            p.layers[0].size,
        );
        let path = VectorPath {
            tangents: Default::default(), id: 1,
            closed: true,
            nodes: [[4., 4.], [26., 4.], [26., 20.], [4., 20.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        };
        let mut mask = LayerMask::new(17, path);
        mask.opacity.value = 50.;
        p.layers[0].masks.push(mask);
        p.rebuild_plugin_dependencies();
        let mut r = renderer(precision, package.clone());
        let target = target(&r, precision);
        let [w, h] = fixture::SIZE;
        let mut previous = None;
        let mut builds = None;
        for red in [0.625_f32, 0.25, 0.625] {
            let curve = p.layers[0].effects[0]
                .params
                .get_mut("p0001")
                .unwrap()
                .curve
                .as_mut()
                .unwrap();
            curve.value.channels[1] = vec![[0., red], [1., red]];
            let scene = sample(&p);
            r.preflight_effects(&scene, w, h).unwrap();
            r.draw(&scene, target.view(), w, h).unwrap();
            let raw = r.read_image_target(&target).unwrap();
            let coverage = if precision == ChannelPrecision::Unorm8 {
                128. / 255.
            } else {
                0.5
            };
            let worst = compare(&raw, precision, |x, y| {
                if (5..25).contains(&x) && (5..19).contains(&y) {
                    Some([
                        f64::from(red) * coverage,
                        0.5 * coverage,
                        0.6 * coverage,
                        coverage,
                    ])
                } else if x <= 2 || x >= 28 || y <= 2 || y >= 22 {
                    Some([0.; 4])
                } else {
                    None
                } // Do not substitute a guessed CPU edge rasterizer.
            });
            let lut = resources(&r, TextureCategory::CurveLuts);
            assert_eq!(lut.len(), 1);
            assert_eq!(
                lut[0].format,
                if precision == ChannelPrecision::Unorm8 {
                    "Rgba8Unorm"
                } else {
                    "Rgba32Float"
                }
            );
            assert_eq!(
                lut[0].width,
                if precision == ChannelPrecision::Unorm8 {
                    256
                } else {
                    motion_model::HIGH_PRECISION_CURVE_SAMPLES as u32
                }
            );
            let masks = resources(&r, TextureCategory::MaskOutputs);
            assert_eq!(masks.len(), 1);
            assert_eq!([masks[0].width, masks[0].height], fixture::SIZE);
            assert_eq!(
                masks[0].format,
                match precision {
                    ChannelPrecision::Unorm8 => "R8Unorm",
                    ChannelPrecision::Unorm16 => "R16Unorm",
                    ChannelPrecision::Float16 => "R16Float",
                    ChannelPrecision::Float32 => "R32Float",
                }
            );
            let resources = (lut, masks);
            if let Some(previous) = &previous {
                assert_eq!(&resources, previous);
            } else {
                previous = Some(resources.clone());
            }
            if let Some(builds) = builds {
                assert_eq!(r.effect_pipeline_builds(), builds);
            } else {
                builds = Some(r.effect_pipeline_builds());
            }
            if precision != ChannelPrecision::Unorm8 {
                let mut bad = scene;
                bad.curve_luts.push([[0.; 4]; 256]);
                let error = r.preflight_effects(&bad, w, h).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("high-precision curve sources=1 do not match LUTs=2"),
                    "{error}"
                );
                assert_eq!(r.read_image_target(&target).unwrap().pixels, raw.pixels);
                r.draw(&sample(&p), target.view(), w, h).unwrap();
                assert_eq!(r.read_image_target(&target).unwrap().pixels, raw.pixels);
            }
            r.check_health().unwrap();
            rows.push(serde_json::json!({"precision":precision,"red":red,"worst_error":worst,
                "native_resources":resources,"native_pipeline_builds":builds,"backings_retained":true}));
        }
    }
    write("curve-mask-frame-imports.json", &rows);
}
