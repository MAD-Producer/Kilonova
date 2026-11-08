//! Same display identity must never retain an obsolete source interpretation.
use super::*;

pub(super) fn qualify(events: &winit::event_loop::EventLoop<()>) {
    let reference = reference();
    let runtime = NativeColorRuntime::load(&runtime()).unwrap();
    let sample = [32768u16, 16384, 8192, 65535];
    let raw = sample.map(|value| f32::from(value) / 65535.);
    let mut checked = 0;
    for case in reference
        .cases
        .iter()
        .filter(|case| case.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let mut project = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut project).display.transfer = ColorTransfer::Srgb;
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/original.png".into(),
            width: 2,
            height: 2,
        });
        let mut layer = Layer::solid(
            1,
            "source colour reassignment",
            [64.; 2],
            [32., 32., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset: 1 };
        project.layers.push(layer);
        std::fs::create_dir_all(root.path().join("assets")).unwrap();
        let image_path = root.path().join("assets/original.png");
        let write = || {
            image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(2, 2, sample.repeat(4))
                .unwrap()
                .save(&image_path)
                .unwrap()
        };
        write();
        let config = &reference.configs[case.config];
        let verified = VerifiedColorConfig::new(
            config.manifest.clone(),
            Arc::from(config.config.as_bytes()),
            BTreeMap::new(),
        )
        .unwrap();
        let input = runtime
            .prepare(
                &verified,
                &config.selection,
                ColorProcessorPhase::InputToWorking,
                case.processor.input_alpha,
                AlphaMode::Premultiplied,
            )
            .unwrap();
        let display = runtime
            .prepare(
                &verified,
                &config.selection,
                ColorProcessorPhase::WorkingToDisplay,
                AlphaMode::Premultiplied,
                AlphaMode::Premultiplied,
            )
            .unwrap();
        let display_pixel = |rgb| {
            let rgb = display.apply_rgb(rgb).unwrap();
            [rgb[0], rgb[1], rgb[2], 1.].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
        };
        let original = display_pixel(input.apply_rgb([raw[0], raw[1], raw[2]]).unwrap());
        let reassigned = display_pixel([raw[0], raw[1], raw[2]]);
        assert!(
            (0..3).any(|c| original[c].abs_diff(reassigned[c]) > 2),
            "source negative control must be visible"
        );
        let id = motion_host::session::open(
            root.path().to_owned(),
            project.clone(),
            Arc::new(session_installation::NativePlatform(super::super::runtime())),
        )
        .unwrap();
        motion_host::session::with_session(id, |session| {
            attach(session, &events);
            let key = session
                .color_programs()
                .unwrap()
                .surface_display()
                .unwrap()
                .key();
            let before = capture(session, 0.25, &views(1), false);
            checked += check(&before, 1, [original; 2]);
            std::fs::remove_file(&image_path).unwrap();
            let mut next = project.clone();
            let colors = settings(&mut next);
            colors.sources.push(SourceColorAssignment {
                asset: 1,
                interpretation: SourceInterpretation::Color {
                    input: ColorInput {
                        space: colors.selection.working.clone(),
                        signal: colors.working.clone(),
                    },
                },
            });
            let history = session.engine.history_diagnostics();
            let error = session
                .apply_command_batch(vec![Command::SetColorManagement {
                    settings: next.color_management.clone(),
                }])
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    motion_host::Error::ColorPreparation(prep::Error::SurfaceDisplay(_))
                ),
                "{error:?}"
            );
            assert_eq!(session.engine.project(), &project);
            assert_eq!(session.engine.history_diagnostics(), history);
            assert_eq!(session.last_presented_frame, Some(0.25));
            assert_eq!(capture(session, 0.25, &views(1), true), before);
            write();
            session.apply_command_batch(vec![Command::SetColorManagement {
                settings: next.color_management.clone(),
            }])?;
            assert_eq!(
                session
                    .color_programs()
                    .unwrap()
                    .surface_display()
                    .unwrap()
                    .key(),
                key,
                "only the source interpretation changed; display key is insufficient"
            );
            let after = capture(session, 0.5, &views(1), false);
            checked += check(&after, 1, [reassigned; 2]);
            assert_ne!(before, after, "obsolete source interpretation was reused");
            std::fs::remove_file(&image_path).unwrap();
            checked += check(
                &capture(session, 0.75, &views(1), false),
                1,
                [reassigned; 2],
            );
            assert_eq!(
                session.engine.project().color_management,
                next.color_management
            );
            Ok(())
        })
        .unwrap();
        motion_host::session::close(id);
    }
    assert_eq!(checked, 23_040);
    println!(
        "actual source reassignment CPU pixels={checked}; unchanged display key; 6 visible stale-source negative controls; missing image edit atomicity and explicit recovery"
    );
}
