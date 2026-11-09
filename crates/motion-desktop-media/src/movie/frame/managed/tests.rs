use super::*;
use motion_core::{Asset, Content, Layer, color_management::*};
use motion_effects::{
    color_config::*, color_processor::ColorProcessorPhase, native_color::NativeColorRuntime,
};
use motion_host::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use serde::Deserialize;
#[path = "tests/resources.rs"]
mod resources;

#[derive(Deserialize)]
struct Reference {
    configs: Vec<Config>,
}
#[derive(Deserialize)]
struct Config {
    manifest: ColorConfigManifest,
    config: String,
    selection: ColorSelection,
}

struct NativePlatform(PathBuf);
impl Platform for NativePlatform {
    fn name(&self) -> &'static str {
        "native-png-test"
    }
    fn color_runtime_manifest(&self) -> motion_host::Result<Option<PathBuf>> {
        Ok(Some(self.0.clone()))
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("static PNG must not probe video")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("static PNG must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("static PNG must not decode audio")
    }
    fn media_capabilities(
        &self,
        _: Option<&VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("PNG export must not need a window")
    }
}
fn runtime() -> PathBuf {
    std::env::var_os("MOTION_TEST_OCIO_RUNTIME")
        .expect("explicit native OCIO runtime")
        .into()
}
fn reference() -> Reference {
    serde_json::from_str(include_str!(
        "../../../../../motion-effects/tests/color_processor/reference.json"
    ))
    .unwrap()
}
fn signal(
    config: &Config,
    space: &SpaceReference,
    alpha: AlphaMode,
    transfer: ColorTransfer,
) -> ColorSignal {
    let name = config.manifest.declared_color_space(space).unwrap();
    let item = &config.manifest.spaces[name];
    let ap1 = name == "ACEScg";
    ColorSignal {
        encoding: item.encoding.clone(),
        transfer,
        primaries: if ap1 {
            [[0.713, 0.293], [0.165, 0.83], [0.128, 0.044]]
        } else {
            [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]]
        },
        white_point: if ap1 {
            [0.32168, 0.33767]
        } else {
            [0.3127, 0.3290]
        },
        reference: item.reference,
        range: SignalRange::Full {},
        alpha,
    }
}
fn project(config: &Config, root: &Path, transfer: ColorTransfer, alpha: AlphaMode) -> Project {
    let mut p = Project::new(64, 32, 30, 8).unwrap();
    let mut selection = config.selection.clone();
    let space = SpaceReference::Space {
        name: if transfer == ColorTransfer::Linear {
            "ACEScg"
        } else {
            "sRGB Encoded Rec.709 (sRGB)"
        }
        .into(),
    };
    selection.output = OutputSelection::Space {
        space: space.clone(),
    };
    let input = signal(
        config,
        &selection.input,
        AlphaMode::Straight,
        ColorTransfer::Srgb,
    );
    let settings = OcioProject {
        manifest: config.manifest.clone(),
        selection: selection.clone(),
        default_input: input.clone(),
        authored_input: ColorInput {
            space: selection.input.clone(),
            signal: input.clone(),
        },
        working: signal(
            config,
            &selection.working,
            AlphaMode::Premultiplied,
            ColorTransfer::Linear,
        ),
        display: ColorSignal {
            reference: ReferenceSpace::Display,
            ..input
        },
        output: signal(config, &space, alpha, transfer),
        sources: vec![],
    };
    p.color_management = Arc::new(ProjectColorManagement::Ocio {
        settings: Box::new(settings),
    });
    p.working_precision = ChannelPrecision::Float32;
    p.background = [0.08, 0.1, 0.2, 0.25];
    p.layers.push(Layer::solid(
        1,
        "native output card",
        [32., 32.],
        [16., 16., 0.],
        [0.2, 0.8, 0.4, 0.6],
    ));
    let mut image = Layer::solid(2, "sixteen-bit source", [32., 32.], [48., 16., 0.], [1.; 4]);
    image.content = Content::Image { asset: 1 };
    p.layers.push(image);
    p.assets.push(Asset {
        id: 1,
        path: "assets/source.png".into(),
        width: 2,
        height: 2,
    });
    let resource = &p.color_management.resources().unwrap()[0];
    let path = root.join(&resource.path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &config.config).unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_pixel(
        2,
        2,
        image::Rgba([16384, 32768, 8192, 32768]),
    )
    .save(root.join("assets/source.png"))
    .unwrap();
    p.validate().unwrap();
    p
}
fn expected(config: &Config, p: &Project) -> [[u8; 4]; 2] {
    let ProjectColorManagement::Ocio { settings } = p.color_management.as_ref() else {
        unreachable!()
    };
    let verified = VerifiedColorConfig::new(
        config.manifest.clone(),
        Arc::from(config.config.as_bytes()),
        Default::default(),
    )
    .unwrap();
    let runtime = NativeColorRuntime::load(&runtime()).unwrap();
    let input = runtime
        .prepare(
            &verified,
            &settings.selection,
            ColorProcessorPhase::InputToWorking,
            AlphaMode::Straight,
            AlphaMode::Premultiplied,
        )
        .unwrap();
    let output = runtime
        .prepare(
            &verified,
            &settings.selection,
            ColorProcessorPhase::WorkingToOutput,
            AlphaMode::Premultiplied,
            settings.output.alpha,
        )
        .unwrap();
    let background = input
        .apply_rgb(p.background[..3].try_into().unwrap())
        .unwrap();
    let encoded = [
        [0.2, 0.8, 0.4, 0.6],
        [
            16384. / 65535.,
            32768. / 65535.,
            8192. / 65535.,
            32768. / 65535.,
        ],
    ];
    encoded.map(|source| {
        let front = input.apply_rgb(source[..3].try_into().unwrap()).unwrap();
        let a = source[3] + p.background[3] * (1. - source[3]);
        let working = std::array::from_fn(|c| {
            (front[c] * source[3] + background[c] * p.background[3] * (1. - source[3])) / a
        });
        let rgb = output.apply_rgb(working).unwrap();
        [rgb[0], rgb[1], rgb[2], a].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
    })
}

#[test]
#[ignore = "requires pinned native OCIO runtime and actual GPU"]
fn frozen_managed_png_uses_native_output_and_frozen_sources_after_session_close() {
    let reference = reference();
    let mut checked = 0;
    for (config_index, config) in reference.configs.iter().enumerate() {
        for transfer in [ColorTransfer::Linear, ColorTransfer::Srgb] {
            for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
                let root = tempfile::tempdir().unwrap();
                let p = project(config, root.path(), transfer, alpha);
                let cpu = expected(config, &p);
                let mut session = Session::new(
                    p.clone(),
                    root.path().into(),
                    Arc::new(NativePlatform(runtime())),
                )
                .unwrap();
                session.frame = 0.5;
                let before = session.engine.snapshot();
                let control = ExportControl::default();
                let output = root.path().join("output.png");
                let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control)
                    .unwrap()
                    .with_observation_request(8301);
                // Replace paths after capture: retained original handles must
                // win over the new, invalid live configuration and image.
                let resource = &p.color_management.resources().unwrap()[0];
                let config_path = root.path().join(&resource.path);
                fs::rename(&config_path, root.path().join("retained.ocio")).unwrap();
                fs::write(&config_path, b"invalid replacement configuration").unwrap();
                let source = root.path().join("assets/source.png");
                fs::rename(&source, root.path().join("retained.png")).unwrap();
                fs::write(&source, b"invalid replacement image").unwrap();
                assert_eq!(session.engine.snapshot(), before);
                session.frame = 2.75;
                drop(session);
                let result = plan.export(&control).unwrap();
                assert_eq!(result["frame"], 0.5);
                assert_eq!(result["color"]["storage"]["alpha"], "straight");
                let actual = image::open(&output).unwrap().to_rgba8();
                assert_eq!(actual.dimensions(), (64, 32));
                for tile in 0..2 {
                    for y in 4..28 {
                        for x in 4..28 {
                            let got = actual.get_pixel(tile * 32 + x, y).0;
                            assert!(
                                (0..4).all(|c| got[c].abs_diff(cpu[tile as usize][c]) <= 1),
                                "config={config_index} transfer={transfer:?} alpha={alpha:?} tile={tile} actual={got:?} CPU={:?}",
                                cpu[tile as usize]
                            );
                            checked += 1;
                        }
                    }
                }
                let reader =
                    png::Decoder::new(std::io::BufReader::new(fs::File::open(&output).unwrap()))
                        .read_info()
                        .unwrap();
                let info = reader.info();
                let metadata = info
                    .utf8_text
                    .iter()
                    .find(|text| text.keyword == "MotionStudioColor")
                    .unwrap();
                let embedded: serde_json::Value =
                    serde_json::from_str(&metadata.get_text().unwrap()).unwrap();
                assert_eq!(embedded, result["color"]);
                if transfer == ColorTransfer::Srgb {
                    assert_eq!(
                        info.srgb,
                        Some(png::SrgbRenderingIntent::RelativeColorimetric)
                    );
                } else {
                    // png 0.18 decodes file chunks into gama_chunk/chrm_chunk;
                    // source_* are encoder inputs, not populated by its decoder.
                    assert_eq!(info.gama_chunk, Some(png::ScaledFloat::new(1.)));
                    assert_eq!(
                        info.chrm_chunk,
                        Some(png::SourceChromaticities::new(
                            (0.32168, 0.33767),
                            (0.713, 0.293),
                            (0.165, 0.83),
                            (0.128, 0.044)
                        ))
                    );
                    assert!(info.srgb.is_none());
                }
                if let Some(evidence) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
                    let evidence = PathBuf::from(evidence).join("managed-png");
                    fs::create_dir_all(&evidence).unwrap();
                    let name = format!("config-{config_index}-{transfer:?}-{alpha:?}");
                    fs::copy(output, evidence.join(format!("{name}.png"))).unwrap();
                    fs::write(
                        evidence.join(format!("{name}.json")),
                        serde_json::to_vec_pretty(
                            &serde_json::json!({"reference":cpu,"result":result}),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    assert_eq!(checked, 13_824);
    println!(
        "managed PNG native CPU pixels={checked}; 3 configurations; linear and sRGB metadata; straight and premultiplied processors; frozen replaced sources; closed session"
    );
}

#[test]
#[ignore = "requires pinned native OCIO runtime and actual GPU"]
fn managed_png_rejections_preserve_publication_and_protect_configuration_sources() {
    let reference = reference();
    let config = &reference.configs[0];
    let root = tempfile::tempdir().unwrap();
    let mut p = project(
        config,
        root.path(),
        ColorTransfer::Srgb,
        AlphaMode::Straight,
    );
    let ProjectColorManagement::Ocio { settings } = Arc::make_mut(&mut p.color_management) else {
        unreachable!()
    };
    settings.output.transfer = ColorTransfer::ConfigDefined;
    let session = Session::new(
        p.clone(),
        root.path().into(),
        Arc::new(NativePlatform(runtime())),
    )
    .unwrap();
    let output = root.path().join("existing.png");
    fs::write(&output, b"previous output").unwrap();
    let control = ExportControl::default();
    let error = FrameFreezePlan::capture(&session, Some(output.clone()), &control)
        .unwrap()
        .export(&control)
        .unwrap_err();
    assert!(
        error.contains("phase=output_contract") && error.contains("explicit PNG profile"),
        "{error}"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous output");
    let config_path = root
        .path()
        .join(&p.color_management.resources().unwrap()[0].path);
    let before = fs::read(&config_path).unwrap();
    let error = FrameFreezePlan::capture(&session, Some(config_path.clone()), &control)
        .unwrap()
        .export(&control)
        .unwrap_err();
    assert!(error.contains("source"), "{error}");
    assert_eq!(fs::read(config_path).unwrap(), before);
    let cancelled = ExportControl::default();
    let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &cancelled).unwrap();
    cancelled.cancel();
    assert_eq!(plan.export(&cancelled).unwrap_err(), "PNG export cancelled");
    assert_eq!(fs::read(output).unwrap(), b"previous output");
}
