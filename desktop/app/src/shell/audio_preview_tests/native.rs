use super::*;
use std::io::Write;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn present(shell: &mut Shell, events: &mut EventLoop<()>, size: [f32; 2]) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, size, true);
        let reply = shell.present_ui_async(ui).unwrap();
        if crate::native_test_support::receive(events, &reply, Duration::from_secs(60))
            .unwrap()
            .unwrap()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "composition presentation never completed"
        );
    }
}

fn project(root: &std::path::Path) {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.name = "Audio preview probe".into();
    let mut visual = Layer::solid(
        2,
        "Independent visual probe",
        [32.; 2],
        [32., 32., 0.],
        [0.1, 0.5, 0.8, 1.],
    );
    visual.transform.position.set_animated(0, true).unwrap();
    visual
        .transform
        .position
        .set_at(59, [48., 32., 0.])
        .unwrap();
    project.layers.push(visual);
    let asset = motion_core::AudioAsset {
        id: 1,
        path: "assets/probe.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 96_000,
        duration_us: 2_000_000,
    };
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join(&asset.path), [0]).unwrap();
    let cache = motion_media::cache_path(root, &asset).unwrap();
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut pcm = std::fs::File::create(cache).unwrap();
    // Device callbacks consume genuine prepared PCM; keep the local probe silent.
    pcm.write_all(&vec![0u8; 96_000 * 2 * 4]).unwrap();
    let mut audio = Layer::solid(1, "Audio probe", [0.; 2], [0.; 3], [0.; 4]);
    audio.content = motion_core::Content::Audio {
        audio: motion_core::AudioClip::new(1),
    };
    project.audio_assets.push(asset);
    project.layers.push(audio);
    motion_core::storage::save(root, &project).unwrap();
}

#[test]
#[ignore = "requires a real GPU, native windows and an audio output device"]
fn native_audio_preview_menu_and_device_clock_preserve_canvas_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_AUDIO_PREVIEW_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    std::fs::create_dir_all(&output).unwrap();
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        let root = temp.path().join(language);
        project(&root);
        let mut shell = Shell::new(root, None, Some(locale)).unwrap();
        shell.home = None;
        shell.panels.select_only(Some(1));
        shell.workspace.select_preset(workspace::Preset::Default);
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
                )
                .unwrap(),
        );
        let pending = shell
            .engine
            .attach_async(window.clone(), 1600, 1000)
            .unwrap();
        shell.state =
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
                .unwrap()
                .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        let before = shell.engine.state().unwrap();
        let size = [1600., 1000.];
        present(&mut shell, &mut events, size);
        shell.panels.menu = Some(2);
        let (paint, actions) = shell.draw_commands(size);
        assert!(actions.is_empty());
        let view =
            panels::composition_view_styled(shell.workspace.display_root(), size, &paint.styles());
        let ui = shell.ui_frame(paint, size, true);
        crate::native_test_support::capture_ui(
            &mut events,
            &shell.engine,
            ui,
            output.join(format!("audio-preview-menu-{language}.png")),
        )
        .unwrap();
        shell.panels.menu = None;
        // Main and floating panel entry points must emit the same command.
        for floating in [false, true] {
            shell.action(panels::Action::Seek(0.), None).unwrap();
            let (_, reply) = shell.editing.take().unwrap();
            shell.accept_edit_state(
                crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                    .unwrap()
                    .unwrap(),
            );
            present(&mut shell, &mut events, size);
            let route = if floating { "floating-route" } else { "main" };
            let (paint, actions) = shell.draw_commands(size);
            assert!(actions.is_empty());
            let ui = shell.ui_frame(paint, size, true);
            let baseline = output.join(format!("audio-preview-baseline-{language}-{route}.png"));
            crate::native_test_support::capture_ui(
                &mut events,
                &shell.engine,
                ui,
                baseline.clone(),
            )
            .unwrap();
            let initial_image = image::open(baseline).unwrap().into_rgba8();
            assert!(
                ((view.min[1] as u32)..(view.max[1] as u32)).any(|y| ((view.min[0] as u32)
                    ..(view.max[0] as u32))
                    .any(|x| {
                        let p = initial_image.get_pixel(x, y);
                        p[2] > p[0].saturating_add(40) && p[2] > p[1]
                    })),
                "baseline must contain the actual rendered blue probe"
            );
            let initial_renders = shell.engine.state().unwrap()["desktop"]["uiPerformance"]
                ["compositionRenders"]
                .as_u64()
                .unwrap();
            shell.panels.focused = Some(dock::Panel::Timeline);
            let input = Input {
                keys: vec![(Key::NumpadDecimal, Default::default())],
                ..Default::default()
            };
            let actions = if floating {
                let mut paint = PaintList::default();
                panels::floating_chrome(
                    &mut paint,
                    &mut shell.atlas,
                    dock::Panel::Timeline,
                    size,
                    &mut shell.panels,
                    &shell.state,
                    &input,
                )
            } else {
                shell.panels.menu = Some(2);
                let point = [
                    125.,
                    motion_ui::theme::metrics::TITLE_BAR
                        + motion_ui::theme::metrics::MENU_BAR
                        + 4.
                        + 3. * 32.
                        + 15.,
                ];
                shell.input = Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                };
                shell.draw_commands(size).1
            };
            shell.input = Input::default();
            assert!(
                matches!(actions.as_slice(), [panels::Action::ToggleAudioPreview]),
                "{actions:?}"
            );
            shell
                .action(actions.into_iter().next().unwrap(), None)
                .unwrap();
            assert!(
                shell.audio_only_preview && shell.audio.is_some(),
                "{:?}",
                shell.panels.message
            );
            let horizon = Instant::now() + Duration::from_secs(5);
            loop {
                shell.advance_playback().unwrap();
                if let Some((_, reply)) = shell.editing.take() {
                    shell.accept_edit_state(
                        crate::native_test_support::receive(
                            &mut events,
                            &reply,
                            Duration::from_secs(60),
                        )
                        .unwrap()
                        .unwrap(),
                    );
                }
                assert!(shell.panels.message.is_none(), "{:?}", shell.panels.message);
                if shell.frame > 0. {
                    break;
                }
                assert!(Instant::now() < horizon, "device clock never advanced");
                std::thread::sleep(Duration::from_millis(5));
            }
            let (paint, actions) = shell.draw_commands(size);
            assert!(actions.is_empty());
            let ui = shell.ui_frame(paint, size, true);
            assert!(ui.composition.is_some());
            let reply = shell.present_ui_async(ui).unwrap();
            assert!(crate::native_test_support::receive(
                &mut events,
                &reply,
                Duration::from_secs(60)
            )
            .unwrap()
            .unwrap());
            let (paint, actions) = shell.draw_commands(size);
            assert!(actions.is_empty());
            let ui = shell.ui_frame(paint, size, true);
            crate::native_test_support::capture_ui(
                &mut events,
                &shell.engine,
                ui,
                output.join(format!(
                    "audio-preview-running-{language}-{}.png",
                    if floating { "floating-route" } else { "main" }
                )),
            )
            .unwrap();
            let current_image = image::open(output.join(format!(
                "audio-preview-running-{language}-{}.png",
                if floating { "floating-route" } else { "main" }
            )))
            .unwrap()
            .into_rgba8();
            // Sample the actual composition area away from all UI readouts.
            for y in (view.min[1] as u32 + 8)..(view.max[1] as u32 - 8) {
                for x in (view.min[0] as u32 + 8)..(view.max[0] as u32 - 8) {
                    assert_eq!(
                        initial_image.get_pixel(x, y),
                        current_image.get_pixel(x, y),
                        "audio-only preview changed canvas at {x},{y}"
                    );
                }
            }
            assert_eq!(
                shell.engine.state().unwrap()["desktop"]["uiPerformance"]["compositionRenders"]
                    .as_u64()
                    .unwrap(),
                initial_renders
            );
            let began = Instant::now();
            shell
                .action(panels::Action::ToggleAudioPreview, None)
                .unwrap();
            assert!(began.elapsed() < Duration::from_secs(1));
            assert!(!shell.audio_only_preview && shell.audio.is_none() && shell.playback.is_none());
            present(&mut shell, &mut events, size);
            assert_eq!(
                shell.engine.state().unwrap()["desktop"]["uiPerformance"]["compositionRenders"]
                    .as_u64()
                    .unwrap(),
                initial_renders + 1,
                "stopping must render the advanced clock, not reuse the frozen stamp"
            );
            let after = shell.engine.state().unwrap();
            for key in ["project", "revision", "canUndo", "canRedo"] {
                assert_eq!(after[key], before[key], "{key}");
            }
            shell.frame = 0.;
        }
    }
}
