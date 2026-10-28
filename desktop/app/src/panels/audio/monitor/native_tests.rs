use super::*;
use crate::*;
use std::sync::Arc;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn source(root: &std::path::Path) {
    let count = 96_000u32;
    let mut wav = vec![];
    wav.extend(b"RIFF");
    wav.extend((36 + count * 2).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(48_000u32.to_le_bytes());
    wav.extend(96_000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((count * 2).to_le_bytes());
    for i in 0..count {
        let amplitude = (0.15 + 0.6 * (i as f32 / 4800.).sin().abs()) * 16_000.;
        let sample = ((i as f32 * std::f32::consts::TAU * 440. / 48_000.).sin() * amplitude) as i16;
        wav.extend(sample.to_le_bytes());
    }
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/probe.wav"), &wav).unwrap();
    let mut project = motion_core::Project::new(64, 64, 30, 90).unwrap();
    project.audio_assets.push(motion_core::AudioAsset {
        id: 1,
        path: "assets/probe.wav".into(),
        mime: "audio/wav".into(),
        bytes: wav.len() as u64,
        track: 0,
        sample_rate: 48_000,
        channels: 1,
        sample_frames: u64::from(count),
        duration_us: 2_000_000,
    });
    let mut layer = Layer::solid(7, "Audio / 音频", [0.; 2], [0.; 3], [0.; 4]);
    let mut audio = motion_core::AudioClip::new(1);
    audio.source_offset_us = 250_000;
    layer.content = motion_core::Content::Audio { audio };
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 15,
        out_frame: 60,
        offset_frame: 15,
    });
    layer.audio_levels = Some(motion_core::AudioLevels::default());
    layer.audio_levels.as_mut().unwrap().balance = 0.3;
    project.layers.push(layer);
    motion_core::storage::save(root, &project).unwrap();
}

#[test]
#[ignore = "requires Windows native GPU capture; explicitly run for audio monitor acceptance"]
fn native_audio_monitor_captures_real_source_waveform_both_owners_languages_and_scales() {
    let temp = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_AUDIO_MONITOR_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().join("evidence"));
    std::fs::create_dir_all(&output).unwrap();
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        let root = temp.path().join(language);
        source(&root);
        for floating in [false, true] {
            for scale in [1f32, 1.5, 2.] {
                let size = if floating {
                    [320., 600.]
                } else if scale == 1. {
                    [1600., 1000.]
                } else {
                    [900., 600.]
                };
                let mut shell = Shell::new(root.clone(), None, Some(locale)).unwrap();
                shell.home = None;
                shell.workspace.root = motion_ui::dock::Node::dock(Panel::Audio.dock());
                shell.panels.select_only(Some(7));
                shell.scale = scale;
                let pixels = [(size[0] * scale) as u32, (size[1] * scale) as u32];
                #[allow(deprecated)]
                let window = Arc::new(
                    events
                        .create_window(
                            Window::default_attributes()
                                .with_visible(false)
                                .with_inner_size(winit::dpi::PhysicalSize::new(
                                    pixels[0], pixels[1],
                                )),
                        )
                        .unwrap(),
                );
                let reply = shell
                    .engine
                    .attach_async(window.clone(), pixels[0], pixels[1])
                    .unwrap();
                shell.state = crate::native_test_support::receive(
                    &mut events,
                    &reply,
                    Duration::from_secs(60),
                )
                .unwrap()
                .unwrap();
                shell.window = Some(window);
                shell.attached = true;
                let before = shell.engine.state().unwrap()["project"].clone();
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut ui = loop {
                    shell.atlas.set_pixel_ratio(scale);
                    let (paint, actions) = if floating {
                        let mut paint = shell.engine.take_paint();
                        paint.stylesheet = shell.panels.styles.clone();
                        let actions = crate::panels::floating_chrome(
                            &mut paint,
                            &mut shell.atlas,
                            Panel::Audio,
                            size,
                            &mut shell.panels,
                            &shell.state,
                            &Input::default(),
                        );
                        (paint, actions)
                    } else {
                        shell.draw_commands(size)
                    };
                    assert!(actions.is_empty(), "{actions:?}");
                    let monitor = &mut shell.panels.auxiliary.audio;
                    monitor.waveform.poll();
                    assert!(
                        monitor.waveform.error.is_none(),
                        "{:?}",
                        monitor.waveform.error
                    );
                    if !monitor.waveform.values.is_empty() {
                        // Real callback PCM probe, not a connected physical device.
                        monitor.levels =
                            crate::audio_output::meter_probe(&[[0.5, -0.25], [0.1, 0.05]]);
                        break shell.ui_frame(paint, size, true);
                    }
                    assert!(Instant::now() < deadline, "waveform preparation stalled");
                    std::thread::sleep(Duration::from_millis(5));
                };
                // Paint once more after the real asynchronous waveform and callback
                // levels are available, so the captured frame contains both.
                let mut paint = shell.engine.take_paint();
                paint.stylesheet = shell.panels.styles.clone();
                let actions = if floating {
                    crate::panels::floating_chrome(
                        &mut paint,
                        &mut shell.atlas,
                        Panel::Audio,
                        size,
                        &mut shell.panels,
                        &shell.state,
                        &Input::default(),
                    )
                } else {
                    crate::panels::chrome(
                        &mut paint,
                        &mut shell.atlas,
                        size,
                        &shell.workspace.root,
                        &mut shell.panels,
                        &shell.state,
                        &Input::default(),
                    )
                };
                assert!(actions.is_empty());
                ui.paint = paint;
                ui.atlas = Some(AtlasUpload {
                    pixels: shell.atlas.pixel_data().to_vec(),
                    width: shell.atlas.config().width,
                    height: shell.atlas.config().height,
                });
                let name = format!(
                    "audio-monitor-{language}-{}-{}x.png",
                    if floating {
                        "floating-route"
                    } else {
                        "main-maximized"
                    },
                    scale
                );
                crate::native_test_support::capture_ui(
                    &mut events,
                    &shell.engine,
                    ui,
                    output.join(&name),
                )
                .unwrap();
                let image = image::open(output.join(&name)).unwrap().into_rgba8();
                assert!(
                    image.pixels().filter(|p| p[2] > 100 && p[2] > p[0]).count() > 100,
                    "actual waveform/control accents must be visible"
                );
                assert_eq!(
                    shell.engine.state().unwrap()["project"],
                    before,
                    "viewing and cache preparation cannot edit the project"
                );
            }
        }
    }
}
