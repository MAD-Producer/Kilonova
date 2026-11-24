use super::*;
use crate::movie::frame::managed::tests as fixture;
#[cfg(feature = "ffmpeg")]
use motion_core::{Ease, WorkArea};
#[cfg(feature = "ffmpeg")]
use motion_effects::color_config::VerifiedColorConfig;
use motion_effects::{
    color_processor::ColorProcessorPhase, native_color::NativeColorRuntime, AlphaMode,
};
#[cfg(feature = "ffmpeg")]
use motion_host::Session;

#[cfg(feature = "ffmpeg")]
fn cpu(config: &fixture::Config, project: &Project, source: [f32; 4]) -> [u8; 4] {
    let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
        unreachable!()
    };
    let verified = VerifiedColorConfig::new(
        config.manifest.clone(),
        Arc::from(config.config.as_bytes()),
        Default::default(),
    )
    .unwrap();
    let runtime = NativeColorRuntime::load(&fixture::runtime()).unwrap();
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
    let front = input.apply_rgb(source[..3].try_into().unwrap()).unwrap();
    let back = input
        .apply_rgb(project.background[..3].try_into().unwrap())
        .unwrap();
    let working = std::array::from_fn(|c| {
        front[c] * source[3] + back[c] * project.background[3] * (1. - source[3])
    });
    let rgb = output.apply_rgb(working).unwrap();
    [rgb[0], rgb[1], rgb[2], 1.].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
}

#[cfg(feature = "ffmpeg")]
fn pixel(pixels: &[u8], x: usize) -> [u8; 4] {
    pixels[(16 * 64 + x) * 4..(16 * 64 + x + 1) * 4]
        .try_into()
        .unwrap()
}

#[test]
#[cfg(feature = "ffmpeg")]
#[ignore = "requires pinned native OCIO runtime, actual GPU and FFmpeg"]
fn managed_mp4_uses_frozen_project_output_and_real_codec_colour_metadata() {
    let reference = fixture::reference();
    let mut checked = 0;
    for (index, config) in reference.configs.iter().enumerate() {
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let root = tempfile::tempdir().unwrap();
            let mut project = fixture::project(config, root.path(), ColorTransfer::Srgb, alpha);
            project.frames = 6;
            project.work_area = WorkArea::full(6);
            let changed = [0.2, 0.8, 0.4, 0.3];
            project.layers[0]
                .transform
                .opacity
                .upsert(0, 1., Ease::Hold)
                .unwrap();
            project.layers[0]
                .transform
                .opacity
                .upsert(3, 0.5, Ease::Hold)
                .unwrap();
            project.validate().unwrap();
            let expected_first = cpu(config, &project, [0.2, 0.8, 0.4, 0.6]);
            let expected_last = cpu(config, &project, changed);
            let expected_image = cpu(
                config,
                &project,
                [
                    16384. / 65535.,
                    32768. / 65535.,
                    8192. / 65535.,
                    32768. / 65535.,
                ],
            );
            assert_ne!(expected_first, expected_last);
            let platform = Arc::new(fixture::NativePlatform(fixture::runtime()));
            let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                ChannelPrecision::Float32,
                Default::default(),
            ))
            .unwrap();
            match std::env::var("WGPU_BACKEND").as_deref() {
                Ok("vulkan") => {
                    assert_eq!(renderer.adapter.get_info().backend, wgpu::Backend::Vulkan)
                }
                Ok("dx12") => assert_eq!(renderer.adapter.get_info().backend, wgpu::Backend::Dx12),
                _ => {}
            }
            let control = ExportControl::default();
            let mut managed = ManagedMovie::prepare(
                &mut renderer,
                &project,
                root.path(),
                &*platform,
                8380,
                &control,
            )
            .unwrap();
            let initial_reads = renderer.image_readback_info().completed_calls;
            let mut session = Session::new(project.clone(), root.path().into(), platform).unwrap();
            session.frame = 2.5;
            let before = session.snapshot().unwrap();
            let plan = FrozenMovie::capture_plan(&session, &control)
                .unwrap()
                .with_observation_request(8381);
            let resource = &project.color_management.resources().unwrap()[0];
            let config_path = root.path().join(&resource.path);
            fs::rename(&config_path, root.path().join("retained.ocio")).unwrap();
            fs::write(&config_path, b"invalid live config replacement").unwrap();
            let image_path = root.path().join("assets/source.png");
            fs::rename(&image_path, root.path().join("retained.png")).unwrap();
            fs::write(&image_path, b"invalid live image replacement").unwrap();
            for frame in 0..6 {
                let actual = managed
                    .render(&mut renderer, &project, frame as f64, &control)
                    .unwrap();
                for (x, expected) in [
                    (
                        16,
                        if frame < 3 {
                            expected_first
                        } else {
                            expected_last
                        },
                    ),
                    (48, expected_image),
                ] {
                    let actual = pixel(&actual, x);
                    assert!((0..4).all(|c| actual[c].abs_diff(expected[c]) <= 1),
                        "raw config={index} alpha={alpha:?} frame={frame}: {actual:?} != {expected:?}");
                    checked += 1;
                }
            }
            assert_eq!(
                renderer.image_readback_info().completed_calls,
                initial_reads + 6
            );
            assert_eq!(session.snapshot().unwrap(), before);
            drop(session);
            let output = root.path().join("managed.mp4");
            let frozen = plan.freeze(&control).unwrap();
            let result = frozen.export(&output, &control, |_| {}).unwrap();
            assert_eq!(result.frames, 6);
            let description = result.color_output.unwrap();
            assert_eq!(
                description["rgbTransport"]["matte"],
                "working_black_before_output"
            );
            assert_eq!(description["encoded"]["transfer"], "iec61966_2_1");
            assert_eq!(description["encoded"]["hdr"], false);
            let (first, last) = super::super::tests::decoded_pixels(&output);
            for (pixels, expected) in [(&first, expected_first), (&last, expected_last)] {
                let actual = pixel(pixels, 16);
                assert!(
                    (0..4).all(|c| actual[c].abs_diff(expected[c]) <= 4),
                    "encoded config={index} alpha={alpha:?}: {actual:?} != {expected:?}"
                );
            }
            let input = ffmpeg_next::format::input(&output).unwrap();
            let stream = input
                .streams()
                .best(ffmpeg_next::media::Type::Video)
                .unwrap();
            let video = ffmpeg_next::codec::Context::from_parameters(stream.parameters())
                .unwrap()
                .decoder()
                .video()
                .unwrap();
            assert_eq!(
                video.color_primaries(),
                ffmpeg_next::color::Primaries::BT709
            );
            assert_eq!(
                video.color_transfer_characteristic(),
                ffmpeg_next::color::TransferCharacteristic::IEC61966_2_1
            );
            let (samples, _) = super::super::tests::decoded_audio_rms(&output);
            assert!((9600..=10_240).contains(&samples));
            if let Some(directory) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
                let directory = PathBuf::from(directory).join("managed-mp4").join(format!(
                    "{}-{index}-{alpha:?}",
                    std::env::var("WGPU_BACKEND").unwrap_or_else(|_| "default".into())
                ));
                fs::create_dir_all(&directory).unwrap();
                fs::copy(&output, directory.join("output.mp4")).unwrap();
                for (name, pixels) in [("first.png", first), ("last.png", last)] {
                    image::save_buffer(
                        directory.join(name),
                        &pixels,
                        64,
                        32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                fs::write(directory.join("reference.json"), serde_json::to_vec_pretty(
                    &serde_json::json!({"expectedFirst":expected_first,"expectedLast":expected_last,
                        "expectedImage":expected_image,"colorOutput":description,"audioSamples":samples})
                ).unwrap()).unwrap();
            }
        }
    }
    assert_eq!(checked, 72);
    println!("managed MP4: 3 ACES configs; straight/premult output; 72 raw CPU/GPU cards; frozen config/image; six decoded frames and AAC; explicit SDR tags");
}

#[test]
#[cfg(feature = "ffmpeg")]
#[ignore = "requires pinned native OCIO runtime, actual GPU and FFmpeg"]
fn managed_mp4_rejects_mismatched_output_and_cancels_without_publication() {
    let reference = fixture::reference();
    let root = tempfile::tempdir().unwrap();
    let mut project = fixture::project(
        &reference.configs[0],
        root.path(),
        ColorTransfer::Linear,
        AlphaMode::Straight,
    );
    project.frames = 6;
    project.work_area = WorkArea::full(6);
    let platform = Arc::new(fixture::NativePlatform(fixture::runtime()));
    let session = Session::new(project, root.path().into(), platform).unwrap();
    let before = session.snapshot().unwrap();
    let output = root.path().join("existing.mp4");
    fs::write(&output, b"existing destination").unwrap();
    let control = ExportControl::default();
    let frozen = FrozenMovie::from_session_cancellable(&session, &control).unwrap();
    let error = frozen.export(&output, &control, |_| {}).err().unwrap();
    assert!(
        error.contains("MP4 colour phase=output_contract"),
        "{error}"
    );
    assert_eq!(fs::read(&output).unwrap(), b"existing destination");
    assert_eq!(session.snapshot().unwrap(), before);

    let root = tempfile::tempdir().unwrap();
    let project = fixture::project(
        &reference.configs[0],
        root.path(),
        ColorTransfer::Srgb,
        AlphaMode::Straight,
    );
    let session = Session::new(
        project,
        root.path().into(),
        Arc::new(fixture::NativePlatform(fixture::runtime())),
    )
    .unwrap();
    let output = root.path().join("cancelled.mp4");
    fs::write(&output, b"keep original").unwrap();
    let control = ExportControl::default();
    let frozen = FrozenMovie::from_session_cancellable(&session, &control).unwrap();
    let error = frozen
        .export(&output, &control, |value| {
            if value > 0.1 {
                control.cancel();
            }
        })
        .err()
        .unwrap();
    assert_eq!(error, "movie export cancelled");
    assert_eq!(fs::read(&output).unwrap(), b"keep original");
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".motion-movie-")));
}

#[test]
#[ignore = "requires pinned native OCIO runtime and actual GPU"]
fn opaque_project_output_preserves_hdr_working_values_and_separates_alpha_and_display_programs() {
    let reference = fixture::reference();
    let mut checked = 0;
    for config in &reference.configs {
        let root = tempfile::tempdir().unwrap();
        let project = fixture::project(
            config,
            root.path(),
            ColorTransfer::Srgb,
            AlphaMode::Premultiplied,
        );
        let observation = Observation::new(8382);
        let prepared = color_preparation::prepare_native_authored_project(
            &project,
            root.path(),
            &fixture::runtime(),
            &observation,
        )
        .unwrap()
        .unwrap();
        let program = color_preparation::prepare_native_opaque_output(
            &prepared.colors,
            &fixture::runtime(),
            &observation,
        )
        .unwrap();
        assert!(program.is_opaque_output());
        assert!(!program.is_opaque_display());
        assert!(!prepared.colors.output().is_opaque_output());
        assert_ne!(program.key(), prepared.colors.output().key());
        let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
            unreachable!()
        };
        let runtime = NativeColorRuntime::load(&fixture::runtime()).unwrap();
        let native = runtime
            .prepare(
                prepared.colors.config(),
                &settings.selection,
                ColorProcessorPhase::WorkingToOutput,
                AlphaMode::Premultiplied,
                settings.output.alpha,
            )
            .unwrap();
        let input_native = runtime
            .prepare(
                prepared.colors.config(),
                &settings.selection,
                ColorProcessorPhase::InputToWorking,
                AlphaMode::Premultiplied,
                AlphaMode::Premultiplied,
            )
            .unwrap();
        assert!(input_native
            .source()
            .compile_output_over_black(prepared.colors.config(), &settings.selection)
            .is_err());
        let straight = runtime
            .prepare(
                prepared.colors.config(),
                &settings.selection,
                ColorProcessorPhase::WorkingToOutput,
                AlphaMode::Straight,
                settings.output.alpha,
            )
            .unwrap();
        assert!(straight
            .source()
            .compile_output_over_black(prepared.colors.config(), &settings.selection)
            .is_err());
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            ChannelPrecision::Float32,
            Default::default(),
        ))
        .unwrap();
        let values = [
            [-0.2, 0.8, 2., 0.5],
            [3., 0.5, -0.1, 1.],
            [9., 2., -1., 0.],
            [0.1, 0.2, 0.3, 0.25],
        ];
        let working = renderer
            .upload_color_image_f32(4, 1, prepared.authored.contract(), &values)
            .unwrap();
        let output = renderer
            .color_image_target(
                4,
                1,
                ColorImageContract {
                    space: program.output_space(),
                    range: program.output_range(),
                    alpha: program.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let conversion =
            ColorProcessorOutput::new(&renderer, &working, &program, output.contract()).unwrap();
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        conversion.encode(&renderer, &mut encoder, &output).unwrap();
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(working.texture_resource_ledger());
                ledger.merge(output.texture_resource_ledger());
                ledger.merge(conversion.texture_resource_ledger());
            })
            .unwrap();
        let read = renderer.read_color_image_target(&output).unwrap();
        for (index, bytes) in read.pixels.chunks_exact(16).enumerate() {
            let actual: [f32; 4] = std::array::from_fn(|c| {
                f32::from_le_bytes(bytes[c * 4..c * 4 + 4].try_into().unwrap())
            });
            let rgb = native
                .apply_rgb(if values[index][3] > 0. {
                    values[index][..3].try_into().unwrap()
                } else {
                    [0.; 3]
                })
                .unwrap();
            let expected = [rgb[0], rgb[1], rgb[2], 1.];
            assert!(
                (0..4).all(|c| actual[c].is_finite()
                    && (actual[c] - expected[c]).abs() <= 0.0001 + 0.0002 * expected[c].abs()),
                "sample={index}: {actual:?} != {expected:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 12);
    println!("opaque project output: 12 independent CPU/GPU HDR/negative/transparent references; ordinary alpha endpoint unchanged");
}
