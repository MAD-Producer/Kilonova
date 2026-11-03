//! Real menu/dialog input, native GPU, serial worker and a second-process reopen.
use super::*;
use motion_core::{
    layer_time::{LayerTimeAction, TimeDirection, TimeStretch},
    AudioAsset, AudioClip, Content, Layer, LayerTimeline, Project,
};
use motion_ui::input::Modifiers;
use winit::platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows};
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn draw(shell: &mut Shell, input: Input) -> PaintList {
    shell.input = input;
    let (paint, actions) = shell.draw_commands([900., 600.]);
    shell.input = Input::default();
    for action in actions {
        shell.action(action, None).unwrap();
    }
    paint
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    window: u64,
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = window;
    ui.views = None;
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    let image = image::open(path).unwrap();
    let ratio = if window == 0 {
        shell.scale
    } else {
        shell
            .floating
            .values()
            .find(|f| f.key == window)
            .unwrap()
            .scale
    };
    assert_eq!(
        (image.width(), image.height()),
        ((size[0] * ratio) as u32, (size[1] * ratio) as u32)
    );
}
fn apply(shell: &mut Shell, events: &mut EventLoop<()>) {
    assert_eq!(shell.actions.len(), 1);
    let action = shell.actions.pop_front().unwrap();
    shell.action(action, None).unwrap();
    let (_, pending) = shell.editing.take().unwrap();
    let state = crate::native_test_support::receive(events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.accept_edit_state(state);
    shell.present_due = false;
}
struct FloatDraw<'a> {
    shell: &'a mut Shell,
    id: WindowId,
    called: bool,
}
impl ApplicationHandler for FloatDraw<'_> {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl FloatDraw<'_> {
    fn draw(&mut self, e: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, e).unwrap();
        }
    }
}
fn floating(shell: &mut Shell, events: &mut EventLoop<()>, id: WindowId, input: Input) {
    shell.floating.get_mut(&id).unwrap().input = input;
    shell.present_due = false;
    let mut host = FloatDraw {
        shell,
        id,
        called: false,
    };
    events.pump_app_events(Some(Duration::ZERO), &mut host);
    assert!(host.called);
    if let Some(pending) = shell.rendering.remove(&1) {
        crate::native_test_support::receive(events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
}
fn record(path: PathBuf, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

#[test]
#[ignore = "manual layer-time native menu/dialog GPU, worker and independent reopen"]
fn layer_time_native_menus_drafts_transactions_owners_and_reopen() {
    const NAME:&str="shell::layer_time::native_tests::layer_time_native_menus_drafts_transactions_owners_and_reopen";
    const CHILD: &str = "MOTION_LAYER_TIME_EDITOR_NATIVE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let project = root.join("project");
        if std::env::var_os("MOTION_LAYER_TIME_EDITOR_REOPEN").is_some() {
            let engine =
                engine::Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
            let s = engine.state().unwrap();
            let saved: Project =
                serde_json::from_slice(&std::fs::read(root.join("saved-project.json")).unwrap())
                    .unwrap();
            let actual: Project = serde_json::from_value(s["project"].clone()).unwrap();
            assert_eq!(actual, saved);
            assert_eq!(s["history"]["undoEntries"], 0);
            record(root.join("reopened.json"), &s);
            record(
                root.join("reopen-process.json"),
                &serde_json::json!({"pid":std::process::id()}),
            );
            engine.request_shutdown().unwrap();
            return;
        }
        let mut p = Project::new(640, 360, 30, 240).unwrap();
        let mut l = Layer::solid(7, "Audio time", [0.; 2], [0.; 3], [1.; 4]);
        l.content = Content::Audio {
            audio: AudioClip::new(1),
        };
        l.timeline = Some(LayerTimeline {
            in_frame: 10,
            out_frame: 90,
            offset_frame: 10,
        });
        let count = 384000u32;
        let size = count * 4;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + size).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(48000u32.to_le_bytes());
        wav.extend(192000u32.to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(size.to_le_bytes());
        wav.resize(44 + size as usize, 0);
        p.audio_assets.push(AudioAsset {
            id: 1,
            path: "assets/tone.wav".into(),
            bytes: wav.len() as u64,
            mime: "audio/wav".into(),
            track: 0,
            sample_rate: 48000,
            channels: 2,
            sample_frames: count as u64,
            duration_us: 8_000_000,
        });
        p.layers.push(l);
        std::fs::create_dir_all(project.join("assets")).unwrap();
        std::fs::write(project.join("assets/tone.wav"), wav).unwrap();
        motion_core::storage::save(&project, &p).unwrap();
        let mut shell = Shell::new(project, None, Some(Locale::Zh)).unwrap();
        shell.home = None;
        let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
                )
                .unwrap(),
        );
        let attached = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
        crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.window = Some(window.clone());
        shell.attached = true;
        shell.panels.select_only(Some(7));
        shell.panels.focused = Some(dock::Panel::Timeline);
        let baseline = shell.state.clone();
        record(root.join("baseline.json"), &baseline);
        record(
            root.join("capture-process.json"),
            &serde_json::json!({"pid":std::process::id()}),
        );
        for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
            shell.workspace.locale = locale;
            shell.panels.locale = locale;
            draw(&mut shell, Input::default());
            shell
                .panels
                .layer_actions
                .open(Some(7), [60., 240.], &shell.state);
            draw(&mut shell, Input::default());
            draw(&mut shell, key(Key::Down));
            draw(&mut shell, key(Key::Right));
            let paint = draw(&mut shell, Input::default());
            capture(
                &mut shell,
                &mut events,
                paint,
                [900., 600.],
                0,
                root.join(format!("{lang}-context-time.png")),
            );
            draw(&mut shell, key(Key::Enter));
            assert!(
                shell.dialog.is_some(),
                "context time settings opens a dialog"
            );
            assert_eq!(shell.dialog.as_ref().unwrap().modal_owner(), Some(0));
            let paint = draw(&mut shell, Input::default());
            capture(
                &mut shell,
                &mut events,
                paint,
                [900., 600.],
                0,
                root.join(format!("{lang}-time-dialog.png")),
            );
            for _ in 0..5 {
                draw(&mut shell, key(Key::Tab));
            }
            let paint = draw(&mut shell, key(Key::Space));
            capture(
                &mut shell,
                &mut events,
                paint,
                [900., 600.],
                0,
                root.join(format!("{lang}-remap-options.png")),
            );
            draw(&mut shell, key(Key::Down));
            draw(&mut shell, key(Key::Enter));
            assert!(shell.dialog.is_some());
            draw(&mut shell, key(Key::Tab));
            draw(&mut shell, key(Key::Tab));
            draw(
                &mut shell,
                Input {
                    text: "2.25".into(),
                    ..Default::default()
                },
            );
            let paint = draw(&mut shell, Input::default());
            capture(
                &mut shell,
                &mut events,
                paint,
                [900., 600.],
                0,
                root.join(format!("{lang}-remap-edit.png")),
            );
            assert_eq!(
                shell.engine.state().unwrap()["project"],
                baseline["project"]
            );
            draw(&mut shell, key(Key::Enter));
            draw(&mut shell, key(Key::Enter));
            assert!(shell.dialog.is_none());
            apply(&mut shell, &mut events);
            assert_eq!(shell.state["history"]["undoEntries"], 1);
            let actual: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
            assert_eq!(
                actual.layers[0].source_time_sample(10.).unwrap().frame,
                2.25
            );
            shell.engine.save().unwrap();
            std::fs::write(
                root.join("saved-project.json"),
                serde_json::to_vec_pretty(&actual).unwrap(),
            )
            .unwrap();
            record(root.join(format!("{lang}-after.json")), &shell.state);
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"], baseline["project"]);
            assert_eq!(shell.state["history"]["undoEntries"], 0);
            record(root.join(format!("{lang}-undo.json")), &shell.state);
        }
        shell.workspace.locale = Locale::Zh;
        shell.panels.locale = Locale::Zh;
        shell
            .open_layer_time(
                crate::layer_clipboard::KeyframeSource::of(&shell.state),
                vec![7],
            )
            .unwrap();
        draw(&mut shell, key(Key::Tab));
        draw(
            &mut shell,
            Input {
                text: "150".into(),
                ..Default::default()
            },
        );
        draw(&mut shell, key(Key::Enter));
        draw(&mut shell, key(Key::Enter));
        apply(&mut shell, &mut events);
        let actual: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        assert_eq!(
            actual.layers[0]
                .time_mapping
                .as_ref()
                .unwrap()
                .clock
                .stretch(),
            TimeStretch::new(3, 2).unwrap()
        );
        assert_eq!(
            actual.layers[0]
                .time_mapping
                .as_ref()
                .unwrap()
                .clock
                .direction(),
            TimeDirection::Forward
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
        #[allow(deprecated)]
        let float = Arc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1080, 780)),
                )
                .unwrap(),
        );
        shell
            .engine
            .float_window(1, float.clone(), 1080, 780)
            .unwrap();
        let id = float.id();
        shell.floating.insert(
            id,
            FloatWindow {
                window: float,
                key: 1,
                panel: dock::Panel::Timeline,
                input: Default::default(),
                paint_input: Default::default(),
                pointer_position: Default::default(),
                scale: 1.5,
            },
        );
        shell.dialog = Some(
            dialogs::Dialog::layer_time(
                &shell.state,
                crate::layer_clipboard::KeyframeSource::of(&shell.state),
                vec![7],
                1,
            )
            .unwrap(),
        );
        draw(&mut shell, key(Key::Escape));
        assert!(
            shell.dialog.is_some(),
            "main window cannot cancel the floating owner's dialog"
        );
        floating(&mut shell, &mut events, id, Input::default());
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        shell.atlas.set_pixel_ratio(1.5);
        panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            dock::Panel::Timeline,
            [720., 520.],
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        shell.dialog.as_mut().unwrap().render(
            &mut paint,
            &mut shell.atlas,
            [720., 520.],
            &Input::default(),
            Locale::Zh,
        );
        capture(
            &mut shell,
            &mut events,
            paint,
            [720., 520.],
            1,
            root.join("zh-float-time-ratio1.5.png"),
        );
        floating(&mut shell, &mut events, id, key(Key::Tab));
        floating(
            &mut shell,
            &mut events,
            id,
            Input {
                text: "200".into(),
                ..Default::default()
            },
        );
        floating(&mut shell, &mut events, id, key(Key::Enter));
        assert!(shell.dialog.is_some());
        floating(&mut shell, &mut events, id, key(Key::Enter));
        apply(&mut shell, &mut events);
        let actual: Project = serde_json::from_value(shell.state["project"].clone()).unwrap();
        assert_eq!(
            actual.layers[0]
                .time_mapping
                .as_ref()
                .unwrap()
                .clock
                .stretch(),
            TimeStretch::new(2, 1).unwrap()
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], baseline["project"]);
        let source = crate::layer_clipboard::KeyframeSource::of(&shell.state);
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&Command::LayerTime {
                    object: 7,
                    action: LayerTimeAction::Reverse {},
                })
                .unwrap(),
            )
            .unwrap();
        shell
            .owned_dialog_event(dialogs::Event::LayerTime {
                source,
                commands: vec![Command::LayerTime {
                    object: 7,
                    action: LayerTimeAction::Reset {},
                }],
            })
            .unwrap();
        assert!(shell.actions.is_empty());
        assert!(shell.panels.message.as_ref().unwrap().contains("未应用"));
        shell.engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LAYER_TIME_EDITOR_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    for reopen in [false, true] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG");
        if reopen {
            command.env("MOTION_LAYER_TIME_EDITOR_REOPEN", "1");
        }
        let output = command.output().unwrap();
        let phase = if reopen { "reopen" } else { "capture" };
        std::fs::write(root.join(format!("{phase}-stdout.log")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{phase}-stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let read = |name| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
    };
    assert_ne!(
        read("capture-process.json")["pid"],
        read("reopen-process.json")["pid"]
    );
}
