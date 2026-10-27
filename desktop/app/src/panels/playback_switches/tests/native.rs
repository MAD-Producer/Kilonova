//! Real native surfaces, production Input routing, independent PNG output and
//! default-level failure channels. No physical OS input injection is claimed.
use super::*;
use crate::{workspace, Shell};
use motion_core::{Content, EffectInstance, Layer, Project};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows};

struct DrawFloat<'a> {
    shell: &'a mut Shell,
    id: winit::window::WindowId,
    called: bool,
}
impl winit::application::ApplicationHandler for DrawFloat<'_> {
    fn resumed(&mut self, events: &winit::event_loop::ActiveEventLoop) {
        self.draw(events);
    }
    fn about_to_wait(&mut self, events: &winit::event_loop::ActiveEventLoop) {
        self.draw(events);
    }
    fn window_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }
}
impl DrawFloat<'_> {
    fn draw(&mut self, events: &winit::event_loop::ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, events).unwrap();
        }
    }
}

fn fixture() -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    p.background = [0., 0., 0., 1.];
    p.layers = vec![
        Layer::solid(
            1,
            "Red - visibility independent of audio",
            [24., 48.],
            [16., 32., 0.],
            [1., 0., 0., 1.],
        ),
        Layer::solid(
            2,
            "Blue - effect bypass and Solo",
            [24., 48.],
            [48., 32., 0.],
            [0., 0., 1., 1.],
        ),
        Layer::camera(3, "Camera - output switches unavailable", 64, 64),
    ];
    let mut adjustment = Layer::solid(4, "Adjustment", [64.; 2], [32., 32., 0.], [1.; 4]);
    adjustment.content = Content::Adjustment;
    p.layers.push(adjustment);
    let mut locked = Layer::solid(5, "Locked", [4.; 2], [2., 2., 0.], [1.; 4]);
    locked.locked = true;
    p.layers.push(locked);
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "invert")
        .unwrap();
    p.layers[1].effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    ));
    let mut disabled = p.layers[1].effects[0].clone();
    disabled.id = 2;
    disabled.enabled = false;
    p.layers[1].effects.push(disabled);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}

fn apply(shell: &mut Shell, switch: LayerSwitch, enabled: bool) {
    shell.state = shell
        .engine
        .edit(
            serde_json::to_string(&Command::SetLayerSwitch {
                object: 2,
                switch,
                enabled,
            })
            .unwrap(),
        )
        .unwrap();
}
fn draw(
    shell: &mut Shell,
    floating: bool,
    size: [f32; 2],
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Timeline,
            size,
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        crate::panels::chrome(
            &mut paint,
            &mut shell.atlas,
            size,
            &shell.workspace.root,
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (paint, actions)
}
fn click(shell: &mut Shell, floating: bool, size: [f32; 2], column: usize, object: u64) {
    let (paint, actions) = draw(shell, floating, size, &Input::default());
    assert!(actions.is_empty());
    let bounds = if floating {
        Rect::new(0., 30., size[0], size[1] - 52.)
    } else {
        crate::panels::frames(&shell.workspace.root, size)
            .into_iter()
            .find(|frame| frame.panel == Panel::Timeline)
            .unwrap()
            .content
    };
    let plan = ColumnRects::with_output_switches(
        &paint.styles(),
        bounds.min[0],
        640f32.min(bounds.width() * 0.52),
        shell.panels.timeline.column_visibility,
        shell.panels.timeline.sampling_visibility,
        true,
        true,
    );
    let slot = plan.playback[column].expect("complete measured icon slot");
    let row = crate::panels::timeline::rows(&shell.state)
        .iter()
        .position(|row| row.id == object)
        .unwrap();
    let point = [
        slot.x + slot.width * 0.5,
        bounds.min[1]
            + crate::panels::timeline::header_height(&paint.styles())
            + paint.styles().metric("row.height", 22.) * (row as f32 + 0.5),
    ];
    let input = Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    };
    let (_, actions) = draw(shell, floating, size, &input);
    assert_eq!(actions.len(), 1, "one checked edit per click: {actions:?}");
    assert!(matches!(actions[0], Action::EditChecked { .. }));
    for action in actions {
        shell.action(action, None).unwrap();
    }
    let (_, reply) = shell.editing.take().unwrap();
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
    );
}
fn pixels(shell: &mut Shell, root: &std::path::Path, name: &str, expected: [[u8; 3]; 2]) {
    let path = root.join(format!("{name}.png"));
    let reply = shell
        .engine
        .capture_async(path.clone(), Default::default())
        .unwrap();
    let result = reply
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(result["width"], 64);
    let image = image::open(&path).unwrap().to_rgba8();
    for ((x, color), index) in [16, 48].into_iter().zip(expected).zip(0..) {
        let actual = image.get_pixel(x, 32).0;
        assert!(
            actual[..3]
                .iter()
                .zip(color)
                .all(|(a, b)| a.abs_diff(b) < 8),
            "{name} sample={index} actual={actual:?} expected={color:?}"
        );
    }
}

#[test]
#[ignore = "manual native surfaces, output PNGs and default-level error evidence"]
fn native_switches_change_pixels_and_persist_in_bilingual_main_and_float() {
    const CHILD: &str = "MOTION_OUTPUT_SWITCH_NATIVE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_OUTPUT_SWITCH_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().join("native"));
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::playback_switches::tests::native::native_switches_change_pixels_and_persist_in_bilingual_main_and_float","--include-ignored"])
            .env(CHILD,&root).env_remove("MOTION_LOG").output().unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(file.contains("logging initialized level=INFO"));
        for cause in ["999999", "locked", "unavailable", "revision changed"] {
            assert!(
                file.contains(cause) && stderr.contains(cause),
                "default file/stderr: {cause}"
            );
            assert!(!stdout.contains(cause), "stdout isolation: {cause}");
        }
        assert!(file.contains("start #") && file.contains(" edit"));
        assert_eq!(
            file.lines()
                .filter(|line| line
                    .contains("persistent desktop degradation site=layer_switch.tooltip"))
                .count(),
            2
        );
        assert_eq!(
            stderr
                .lines()
                .filter(|line| line
                    .contains("persistent desktop degradation site=layer_switch.tooltip"))
                .count(),
            2
        );
        assert!(!stdout.contains("layer_switch.tooltip"));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut custom: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ui/styles/dark.json"
    )))
    .unwrap();
    custom["styles"].as_object_mut().unwrap().remove("tooltip");
    let missing =
        motion_ui::style::StyleSheet::from_json(&serde_json::to_vec(&custom).unwrap()).unwrap();
    for _ in 0..2 {
        for _ in 0..120 {
            hint_style(&missing);
        }
        hint_style(&PaintList::default().styles());
    }
    let mut events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    for (language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        let project_root = root.join(format!("project-{language}"));
        motion_core::storage::save(&project_root, &fixture()).unwrap();
        let mut shell = Shell::new(project_root.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        shell.workspace = workspace::Workspace::default();
        shell.layout_path = root.join(format!("workspace-{language}.json"));
        shell.panels.focused = Some(Panel::Timeline);
        #[allow(deprecated)]
        let window = Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
                )
                .unwrap(),
        );
        let attach = shell
            .engine
            .attach_async(window.clone(), 1600, 1000)
            .unwrap();
        shell.state =
            crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
                .unwrap()
                .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        pixels(
            &mut shell,
            &root,
            &format!("{language}-effects-on"),
            [[255, 0, 0], [255, 255, 0]],
        );
        for floating in [false, true] {
            apply(&mut shell, LayerSwitch::Solo, false);
            apply(&mut shell, LayerSwitch::Effects, true);
            let size = if floating {
                [900., 600.]
            } else {
                [1600., 1000.]
            };
            if floating {
                #[allow(deprecated)]
                let window = Arc::new(
                    events
                        .create_window(
                            winit::window::Window::default_attributes()
                                .with_visible(false)
                                .with_inner_size(winit::dpi::PhysicalSize::new(1350, 900)),
                        )
                        .unwrap(),
                );
                shell
                    .engine
                    .float_window(1, window.clone(), 1350, 900)
                    .unwrap();
                let id = window.id();
                shell.floating.insert(
                    id,
                    crate::FloatWindow {
                        window,
                        key: 1,
                        panel: Panel::Timeline,
                        input: Default::default(),
                        paint_input: Default::default(),
                        pointer_position: Default::default(),
                        scale: 1.5,
                    },
                );
                let mut host = DrawFloat {
                    shell: &mut shell,
                    id,
                    called: false,
                };
                events.pump_app_events(Some(Duration::from_millis(5)), &mut host);
                assert!(host.called);
            }
            shell.atlas.set_pixel_ratio(if floating { 1.5 } else { 1. });
            click(&mut shell, floating, size, 0, 2);
            click(&mut shell, floating, size, 1, 2);
            assert_eq!(shell.state["project"]["layers"][1]["solo"], true);
            assert_eq!(
                shell.state["project"]["layers"][1]["effects_enabled"],
                false
            );
            assert_eq!(
                shell.state["project"]["layers"][1]["effects"][0]["enabled"],
                true
            );
            assert_eq!(
                shell.state["project"]["layers"][1]["effects"][1]["enabled"],
                false
            );
            let name = format!(
                "{language}-{}",
                if floating {
                    "float-ratio1.5"
                } else {
                    "main-ratio1"
                }
            );
            pixels(
                &mut shell,
                &root,
                &format!("{name}-solo-bypass"),
                [[0, 0, 0], [0, 0, 255]],
            );
            let (paint, actions) = draw(&mut shell, floating, size, &Input::default());
            assert!(actions.is_empty());
            let mut presentation = shell.ui_frame(PaintList::default(), size, true);
            presentation.window = u64::from(floating);
            if floating {
                presentation.composition = None;
                presentation.composition_image = None;
                presentation.node_preview = None;
            }
            let reply = shell.engine.present_async(presentation).unwrap();
            assert!(crate::native_test_support::receive(
                &mut events,
                &reply,
                Duration::from_secs(30)
            )
            .unwrap()
            .unwrap());
            let mut frame = shell.ui_frame(paint, size, true);
            frame.window = u64::from(floating);
            if floating {
                frame.composition = None;
                frame.composition_image = None;
                frame.node_preview = None;
            }
            crate::native_test_support::capture_ui(
                &mut events,
                &shell.engine,
                frame,
                root.join(format!("{name}.png")),
            )
            .unwrap();
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"]["layers"][1]["effects_enabled"], true);
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(
                shell.state["project"]["layers"][1]["effects_enabled"],
                false
            );
        }
        shell.engine.save().unwrap();
        let saved = motion_core::storage::load(&project_root).unwrap();
        assert!(saved.layers[1].solo && !saved.layers[1].effects_enabled);
        for (object, switch) in [
            (999999, LayerSwitch::Solo),
            (5, LayerSwitch::Solo),
            (3, LayerSwitch::Effects),
        ] {
            let before = shell.engine.state().unwrap();
            let error = shell
                .engine
                .edit(
                    serde_json::to_string(&Command::SetLayerSwitch {
                        object,
                        switch,
                        enabled: false,
                    })
                    .unwrap(),
                )
                .unwrap_err();
            assert!(!error.is_empty());
            let after = shell.engine.state().unwrap();
            assert_eq!(before["project"], after["project"]);
            assert_eq!(before["revision"], after["revision"]);
        }
        let before = shell.engine.state().unwrap();
        let revision = before["revision"].as_u64().unwrap();
        let error = shell
            .engine
            .edit_at_revision(
                serde_json::to_string(&Command::SetLayerSwitch {
                    object: 2,
                    switch: LayerSwitch::Solo,
                    enabled: false,
                })
                .unwrap(),
                revision - 1,
            )
            .unwrap_err();
        assert!(error.contains("revision changed"));
        let after = shell.engine.state().unwrap();
        assert_eq!(before["project"], after["project"]);
        assert_eq!(before["revision"], after["revision"]);
    }
}
