use motion_effects::image_contract::{ChannelPrecision, DataChannels, ImageContract, ImageMeaning};
use motion_effects::{AlphaMode, WorkingSpace};
use motion_render::{
    ColorOutput, ColorOutputDescriptor, ImageCaptureTarget, OutputTransform, Renderer, ToneMap,
};

fn target(
    renderer: &Renderer,
    w: u32,
    h: u32,
    precision: ChannelPrecision,
    space: WorkingSpace,
    alpha: AlphaMode,
) -> ImageCaptureTarget {
    renderer
        .image_capture_target(
            w,
            h,
            ImageContract::new(ImageMeaning::Color { space, alpha }, precision),
        )
        .unwrap()
}
fn paint(renderer: &Renderer, target: &ImageCaptureTarget, body: &str) {
    let shader=renderer.device.create_shader_module(wgpu::ShaderModuleDescriptor {label:Some("independent output fixture"),
        source:wgpu::ShaderSource::Wgsl(format!("
        @vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{
            let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));return vec4(p[i],0.0,1.0);}}
        @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{ {body} }}").into())});
    let pipeline = renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: motion_render::image_formats::native_format(target.storage()),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger())
        })
        .unwrap();
}
fn clear(renderer: &Renderer, target: &ImageCaptureTarget, value: [f32; 4]) {
    paint(
        renderer,
        target,
        &format!(
            "return vec4<f32>({:?},{:?},{:?},{:?});",
            value[0], value[1], value[2], value[3]
        ),
    );
}
fn submit(renderer: &Renderer, conversion: &ColorOutput, output: &ImageCaptureTarget) -> Vec<u8> {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    conversion
        .encode_image(renderer, &mut encoder, output)
        .unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(conversion.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
        })
        .unwrap();
    renderer.read_image_target(output).unwrap().pixels
}
fn half(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1. } else { -1. };
    let exponent = i32::from((bits >> 10) & 31);
    let mantissa = f64::from(bits & 1023);
    assert!(exponent < 31);
    sign * if exponent == 0 {
        mantissa * 2_f64.powi(-24)
    } else {
        (1024. + mantissa) * 2_f64.powi(exponent - 25)
    }
}
fn pixel(bytes: &[u8], precision: ChannelPrecision) -> [f64; 4] {
    match precision {
        ChannelPrecision::Float16 => std::array::from_fn(|c| {
            half(u16::from_le_bytes(
                bytes[c * 2..c * 2 + 2].try_into().unwrap(),
            ))
        }),
        ChannelPrecision::Float32 => std::array::from_fn(|c| {
            f64::from(f32::from_le_bytes(
                bytes[c * 4..c * 4 + 4].try_into().unwrap(),
            ))
        }),
        _ => panic!("fixture expects floating source bytes"),
    }
}
fn srgb(x: f64) -> f64 {
    if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1. / 2.4) - 0.055
    }
}
fn linear(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn oracle(
    source: [f64; 4],
    space: WorkingSpace,
    input_alpha: AlphaMode,
    output_alpha: AlphaMode,
    transform: OutputTransform,
) -> [u8; 4] {
    if source[3] <= 0. {
        return [0; 4];
    }
    let alpha = source[3].clamp(0., 1.);
    let mut expected = [0; 4];
    expected[3] = (alpha * 255.).round() as u8;
    for c in 0..3 {
        let mut value = source[c]
            / if input_alpha == AlphaMode::Premultiplied {
                source[3]
            } else {
                1.
            };
        if space == WorkingSpace::Srgb {
            value = linear(value);
        }
        value = (value * 2_f64.powf(f64::from(transform.exposure_stops))).max(0.);
        value = match transform.tone_map {
            ToneMap::Clip => value.min(1.),
            ToneMap::Reinhard => value / (1. + value),
        };
        if output_alpha == AlphaMode::Premultiplied {
            value *= alpha;
        }
        expected[c] = (srgb(value) * 255.).round() as u8;
    }
    expected
}
fn close(actual: &[u8], expected: [u8; 4]) {
    for p in actual.chunks_exact(4) {
        assert!(
            p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
            "{p:?} vs {expected:?}"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_color_tone_mapping_exposure_and_alpha_match_independent_linear_oracles() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    assert!(
        !renderer
            .device
            .features()
            .contains(wgpu::Features::FLOAT32_FILTERABLE)
    );
    for precision in [ChannelPrecision::Float16, ChannelPrecision::Float32] {
        for space in [WorkingSpace::Linear, WorkingSpace::Srgb] {
            for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
                let input = target(&renderer, 17, 3, precision, space, alpha);
                clear(&renderer, &input, [2., -0.125, 0.0625, 0.25]);
                let before = renderer.read_image_target(&input).unwrap().pixels;
                let source = pixel(&before, precision);
                for output_alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
                    let output = target(
                        &renderer,
                        17,
                        3,
                        ChannelPrecision::Unorm8,
                        WorkingSpace::Linear,
                        output_alpha,
                    );
                    for transform in [
                        OutputTransform::SDR_CLIP,
                        OutputTransform {
                            tone_map: ToneMap::Reinhard,
                            exposure_stops: 1.,
                        },
                        OutputTransform {
                            tone_map: ToneMap::Clip,
                            exposure_stops: -2.,
                        },
                    ] {
                        let conversion = ColorOutput::for_image(
                            &renderer,
                            &input,
                            ColorOutputDescriptor {
                                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                                alpha: output_alpha,
                                transform,
                            },
                        )
                        .unwrap();
                        close(
                            &submit(&renderer, &conversion, &output),
                            oracle(source, space, alpha, output_alpha, transform),
                        );
                    }
                }
                assert_eq!(
                    renderer.read_image_target(&input).unwrap().pixels,
                    before,
                    "output must not modify working bytes"
                );
            }
        }
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn rgba_and_bgra_surface_formats_encode_srgb_exactly_once() {
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
        let source = target(
            &renderer,
            17,
            3,
            ChannelPrecision::Float32,
            WorkingSpace::Linear,
            AlphaMode::Straight,
        );
        clear(&renderer, &source, [0.5, 0.25, 0.125, 1.]);
        let output = renderer.color_output_target(17, 3, format).unwrap();
        let conversion = ColorOutput::for_image(
            &renderer,
            &source,
            ColorOutputDescriptor {
                format,
                alpha: AlphaMode::Straight,
                transform: OutputTransform::SDR_CLIP,
            },
        )
        .unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        conversion.encode(&renderer, &mut encoder, &output).unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(conversion.texture_resource_ledger());
                output.collect_texture_resources(
                    ledger,
                    "output/test",
                    motion_render::TextureState::Retained,
                );
            })
            .unwrap();
        close(&read_output(&renderer, &output), [188, 137, 99, 255]);
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn float_bilinear_scaling_precedes_tone_mapping_and_retained_bindings_reuse_texture_identity() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.set_submission_tracking(true);
    let input = target(
        &renderer,
        2,
        2,
        ChannelPrecision::Float32,
        WorkingSpace::Linear,
        AlphaMode::Premultiplied,
    );
    paint(
        &renderer,
        &input,
        "
        if p.y<1.0 {if p.x<1.0{return vec4(4.0,0.0,0.0,1.0);}return vec4(0.0,1.0,0.0,1.0);}
        if p.x<1.0{return vec4(0.0,0.0,1.0,1.0);}return vec4(0.0,0.0,0.0,0.5);",
    );
    let output = target(
        &renderer,
        3,
        3,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let conversion = ColorOutput::for_image(
        &renderer,
        &input,
        ColorOutputDescriptor {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha: AlphaMode::Straight,
            transform: OutputTransform::SDR_CLIP,
        },
    )
    .unwrap();
    let mut ledger = input.texture_resource_ledger();
    ledger.merge(conversion.texture_resource_ledger());
    assert_eq!(ledger.resources().count(), 1);
    assert_eq!(ledger.total_known_bytes(), 64);
    let identity = ledger.resources().next().unwrap().id.clone();
    let bytes = submit(&renderer, &conversion, &output);
    for (index, value) in [
        (0, [255, 0, 0, 255]),
        (2, [0, 255, 0, 255]),
        (6, [0, 0, 255, 255]),
        (8, [0, 0, 0, 128]),
        (
            4,
            oracle(
                [1., 0.25, 0.25, 0.875],
                WorkingSpace::Linear,
                AlphaMode::Premultiplied,
                AlphaMode::Straight,
                OutputTransform::SDR_CLIP,
            ),
        ),
    ] {
        close(&bytes[index * 4..index * 4 + 4], value);
    }
    clear(&renderer, &input, [0.25, 0.125, 0., 0.5]);
    let changed = submit(&renderer, &conversion, &output);
    assert_ne!(bytes, changed);
    close(&changed, [188, 137, 0, 128]);
    drop(input);
    assert_eq!(
        conversion
            .texture_resource_ledger()
            .resources()
            .next()
            .unwrap()
            .id,
        identity
    );
    assert_eq!(
        submit(&renderer, &conversion, &output),
        changed,
        "binding must retain the source allocation"
    );
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unpremultiplication_happens_before_quantization_and_alpha_clamping() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = target(
        &renderer,
        3,
        2,
        ChannelPrecision::Float32,
        WorkingSpace::Linear,
        AlphaMode::Premultiplied,
    );
    let output = target(
        &renderer,
        3,
        2,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let conversion = ColorOutput::for_image(
        &renderer,
        &source,
        ColorOutputDescriptor {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha: AlphaMode::Straight,
            transform: OutputTransform::SDR_CLIP,
        },
    )
    .unwrap();
    for alpha in [0.001, 0.01, 0.25, 1., 2.] {
        clear(
            &renderer,
            &source,
            [0.25 * alpha, 0.5 * alpha, 0.75 * alpha, alpha],
        );
        let mut expected = [137, 188, 225, 0];
        expected[3] = (alpha.min(1.) * 255.).round() as u8;
        close(&submit(&renderer, &conversion, &output), expected);
    }
    clear(&renderer, &source, [4., -1., 2., 0.]);
    close(&submit(&renderer, &conversion, &output), [0; 4]);
    let raw = renderer.read_image_target(&source).unwrap();
    assert_eq!(
        pixel(&raw.pixels, ChannelPrecision::Float32),
        [4., -1., 2., 0.]
    );
}

fn scene() -> (motion_render::Scene, motion_effects::Registry) {
    use motion_core::{EffectInstance, Layer, Project};
    use motion_effects::{EffectPackage, PassDefinition, Registry};
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.passes = vec![PassDefinition {
        shader: "shaders/output.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.sdr-output".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("shaders/output.wgsl"),
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4<f32>(2.0,-0.125,0.125,0.25);}",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = std::sync::Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut project = Project::new(17, 17, 30, 60).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "SDR output probe", [34.; 2], [8.5, 8.5, 0.], [1.; 4]);
    layer.effects.push(EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (scene, registry)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_float_scene_preview_and_reusable_png_capture_share_the_explicit_output_policy() {
    let (scene, registry) = scene();
    for (precision, format) in [
        (ChannelPrecision::Float16, wgpu::TextureFormat::Rgba16Float),
        (ChannelPrecision::Float32, wgpu::TextureFormat::Rgba32Float),
    ] {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut renderer = pollster::block_on(Renderer::new(&instance, None, format)).unwrap();
        renderer.set_effect_registry(registry.clone());
        for transform in [
            OutputTransform::SDR_CLIP,
            OutputTransform {
                tone_map: ToneMap::Reinhard,
                exposure_stops: 1.,
            },
        ] {
            let source = renderer.render_target(17, 17).unwrap();
            let preview = target(
                &renderer,
                17,
                17,
                ChannelPrecision::Unorm8,
                WorkingSpace::Linear,
                AlphaMode::Premultiplied,
            );
            let conversion = ColorOutput::for_render_target(
                &renderer,
                &source,
                ColorOutputDescriptor {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    alpha: AlphaMode::Premultiplied,
                    transform,
                },
            )
            .unwrap();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            renderer
                .encode_preview(&scene, &source.view, 17, 17, &mut encoder, None)
                .unwrap();
            conversion
                .encode_image(&renderer, &mut encoder, &preview)
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(conversion.texture_resource_ledger());
                    ledger.merge(preview.texture_resource_ledger());
                })
                .unwrap();
            close(
                &renderer.read_image_target(&preview).unwrap().pixels,
                oracle(
                    [2., -0.125, 0.125, 0.25],
                    WorkingSpace::Linear,
                    AlphaMode::Premultiplied,
                    AlphaMode::Premultiplied,
                    transform,
                ),
            );
            let capture = renderer.sdr_capture_target(17, 17, transform).unwrap();
            let bytes = 17
                * 17
                * (if precision == ChannelPrecision::Float16 {
                    8
                } else {
                    16
                })
                + 17 * 17 * 4;
            assert_eq!(capture.texture_bytes(), bytes);
            assert_eq!(capture.texture_resource_ledger().total_known_bytes(), bytes);
            assert_eq!(capture.readback_bytes(), 256 * 17);
            let ids: Vec<_> = capture
                .texture_resource_ledger()
                .resources()
                .map(|r| r.id.clone())
                .collect();
            for _ in 0..2 {
                let (image, stats) = renderer.capture_sdr(&scene, &capture).unwrap();
                assert!(stats.draw_calls >= 2);
                assert_eq!(
                    image.contract.meaning,
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Straight
                    }
                );
                close(
                    &image.pixels,
                    oracle(
                        [2., -0.125, 0.125, 0.25],
                        WorkingSpace::Linear,
                        AlphaMode::Premultiplied,
                        AlphaMode::Straight,
                        transform,
                    ),
                );
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("float-to-sdr.png");
                image::save_buffer_with_format(
                    &path,
                    &image.pixels,
                    17,
                    17,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .unwrap();
                let png = image::open(&path).unwrap().into_rgba8();
                assert_eq!(png.dimensions(), (17, 17));
                assert_eq!(png.as_raw(), &image.pixels);
                if let Some(directory) = std::env::var_os("MOTION_COLOR_OUTPUT_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    std::fs::copy(
                        &path,
                        directory.join(format!("{precision:?}-{:?}.png", transform.tone_map)),
                    )
                    .unwrap();
                }
                assert_eq!(
                    capture
                        .texture_resource_ledger()
                        .resources()
                        .map(|r| r.id.clone())
                        .collect::<Vec<_>>(),
                    ids
                );
            }
        }
        renderer.check_health().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_transform_and_noncolor_inputs_fail_without_poisoning_the_device() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for exposure in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -24.001, 24.001] {
        let error = renderer
            .sdr_capture_target(
                17,
                3,
                OutputTransform {
                    tone_map: ToneMap::Clip,
                    exposure_stops: exposure,
                },
            )
            .err()
            .unwrap();
        assert!(error.to_string().contains("exposure"));
    }
    for meaning in [
        ImageMeaning::Coverage {},
        ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
    ] {
        let source = renderer
            .image_capture_target(
                17,
                3,
                ImageContract::new(meaning, ChannelPrecision::Float32),
            )
            .unwrap();
        let error = ColorOutput::for_image(
            &renderer,
            &source,
            ColorOutputDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                alpha: AlphaMode::Straight,
                transform: OutputTransform::SDR_CLIP,
            },
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("requires color input"));
    }
    let source = target(
        &renderer,
        17,
        3,
        ChannelPrecision::Float32,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let error = ColorOutput::for_image(
        &renderer,
        &source,
        ColorOutputDescriptor {
            format: wgpu::TextureFormat::Rgba16Float,
            alpha: AlphaMode::Straight,
            transform: OutputTransform::SDR_CLIP,
        },
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("8-bit RGBA or BGRA"));
    assert!(
        renderer
            .sdr_capture_target(0, 1, OutputTransform::SDR_CLIP)
            .is_err()
    );
    assert!(renderer.capture_target(17, 3).is_ok());
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn straight_alpha_and_encoded_inputs_scale_in_linear_premultiplied_space() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for space in [WorkingSpace::Linear, WorkingSpace::Srgb] {
        let input = target(
            &renderer,
            2,
            1,
            ChannelPrecision::Float32,
            space,
            AlphaMode::Straight,
        );
        paint(
            &renderer,
            &input,
            "if p.x<1.0{return vec4(0.5,0.0,0.0,1.0);}return vec4(1.0,1.0,1.0,0.0);",
        );
        let output = target(
            &renderer,
            3,
            1,
            ChannelPrecision::Unorm8,
            WorkingSpace::Linear,
            AlphaMode::Straight,
        );
        let conversion = ColorOutput::for_image(
            &renderer,
            &input,
            ColorOutputDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                alpha: AlphaMode::Straight,
                transform: OutputTransform::SDR_CLIP,
            },
        )
        .unwrap();
        let bytes = submit(&renderer, &conversion, &output);
        let red = if space == WorkingSpace::Linear {
            188
        } else {
            128
        };
        close(&bytes[..4], [red, 0, 0, 255]);
        close(&bytes[4..8], [red, 0, 0, 128]);
        assert_eq!(&bytes[8..], &[0, 0, 0, 0]);
    }
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn output_uniform_admission_and_abandoned_or_submitted_encoders_preserve_reservations() {
    use motion_render::{ResourceBudgetScope, ResourceRejection};
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = target(
        &renderer,
        17,
        3,
        ChannelPrecision::Float32,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let output = target(
        &renderer,
        17,
        3,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    clear(&renderer, &source, [0.5, 0.25, 0.125, 1.]);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let descriptor = ColorOutputDescriptor {
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha: AlphaMode::Straight,
        transform: OutputTransform::SDR_CLIP,
    };
    budget.set_limit(before.admitted_bytes + 31).unwrap();
    let error = ColorOutput::for_image(&renderer, &source, descriptor)
        .err()
        .unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + 32,
            budget_bytes: before.admitted_bytes + 31,
        })
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(
        budget.info().unwrap().live_allocations,
        before.live_allocations
    );
    budget.set_limit(before.admitted_bytes + 32).unwrap();
    for submit in [false, true] {
        let conversion = ColorOutput::for_image(&renderer, &source, descriptor).unwrap();
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            before.admitted_bytes + 32
        );
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        conversion
            .encode_image(&renderer, &mut encoder, &output)
            .unwrap();
        drop(conversion);
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            before.admitted_bytes + 32
        );
        if submit {
            renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
        } else {
            drop(encoder);
        }
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    }
    close(
        &renderer.read_image_target(&output).unwrap().pixels,
        [188, 137, 99, 255],
    );
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_budget_never_accepts_foreign_sources_destinations_encoders_or_in_place_output() {
    let budget = motion_render::AllocationBudget::new(128 << 20).unwrap();
    let renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let foreign = pollster::block_on(Renderer::headless_with_allocation_budget(budget)).unwrap();
    let source = target(
        &renderer,
        1,
        1,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let other = target(
        &foreign,
        1,
        1,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let descriptor = ColorOutputDescriptor {
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha: AlphaMode::Straight,
        transform: OutputTransform::SDR_CLIP,
    };
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let error = ColorOutput::for_image(&renderer, &other, descriptor)
        .err()
        .unwrap();
    assert!(error.to_string().contains("another renderer"));
    let foreign_target = foreign.render_target(1, 1).unwrap();
    let error = ColorOutput::for_render_target(&renderer, &foreign_target, descriptor)
        .err()
        .unwrap();
    assert!(error.to_string().contains("another renderer"));
    drop(foreign_target);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    let conversion = ColorOutput::for_image(&renderer, &source, descriptor).unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(
        conversion
            .encode_image(&renderer, &mut encoder, &source)
            .unwrap_err()
            .to_string()
            .contains("distinct")
    );
    assert!(
        conversion
            .encode_image(&renderer, &mut encoder, &other)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    assert!(
        conversion
            .encode_image(&foreign, &mut encoder, &other)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    let destination = target(
        &renderer,
        1,
        1,
        ChannelPrecision::Unorm8,
        WorkingSpace::Linear,
        AlphaMode::Straight,
    );
    let mut wrong_encoder = foreign
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(
        conversion
            .encode_image(&renderer, &mut wrong_encoder, &destination)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}

fn read_output(renderer: &Renderer, target: &motion_render::RenderTarget) -> Vec<u8> {
    let stride = (target.width * 4).div_ceil(256) * 256;
    let buffer = renderer
        .allocation_context()
        .buffer(&wgpu::BufferDescriptor {
            label: Some("independent output readback"),
            size: u64::from(stride) * u64::from(target.height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        target.view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(target.height),
            },
        },
        wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let mut bytes = Vec::new();
    {
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        for row in mapped.chunks_exact(stride as usize) {
            bytes.extend_from_slice(&row[..target.width as usize * 4]);
        }
    }
    buffer.unmap();
    if matches!(
        target.view.texture().format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    bytes
}
