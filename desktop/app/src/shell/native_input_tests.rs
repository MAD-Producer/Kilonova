//! Exercise production routing with real window IDs and an ActiveEventLoop.
//! Events are supplied explicitly; this does not emulate OS keyboard or IME services.
use crate::*;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, Ime, TouchPhase},
    keyboard::ModifiersState,
    platform::windows::EventLoopBuilderExtWindows,
};

#[test]
fn native_main_and_floating_input_remain_isolated_and_consume_each_event_once() {
    const CHILD: &str = "MOTION_NATIVE_INPUT_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let mut host = Host {
            root: root.into(),
            completed: false,
        };
        EventLoop::<()>::builder()
            .with_any_thread(true)
            .build()
            .unwrap()
            .run_app(&mut host)
            .unwrap();
        assert!(
            host.completed,
            "native event host did not execute its checks"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "shell::native_input_tests::native_main_and_floating_input_remain_isolated_and_consume_each_event_once",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "status={}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Host {
    root: PathBuf,
    completed: bool,
}

impl ApplicationHandler for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        assert!(!self.completed);
        let mut shell = Shell::new(self.root.join("project"), None, Some(Locale::En)).unwrap();
        let before = shell.engine.state().unwrap();
        let window = |name| {
            Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title(name)
                            .with_visible(false),
                    )
                    .unwrap(),
            )
        };
        let main = window("input test main");
        let floating = window("input test floating");
        let stale = window("input test stale floating");
        let main_id = main.id();
        let float_id = floating.id();
        shell.window = Some(main);
        shell.floating.insert(
            float_id,
            FloatWindow {
                window: floating,
                key: 1,
                panel: dock::Panel::Composition,
                input: Input::default(),
                paint_input: Input::default(),
                scale: 2.,
            },
        );

        // A physical coordinate belongs to the receiving window's logical space.
        // Wheel totals and committed text detect duplicate delivery, which final
        // pointer positions alone would not reveal.
        for scale in [1., 1.5, 2.] {
            shell.scale = scale;
            shell.input = Input::default();
            shell.floating.get_mut(&float_id).unwrap().input = Input::default();
            pointer(&mut shell, event_loop, main_id, [120., 60.]);
            assert_eq!(shell.input.mouse, [120. / scale, 60. / scale]);
            assert!(!shell.floating[&float_id].input.pending);
            pointer(&mut shell, event_loop, float_id, [300., 180.]);
            assert_eq!(shell.floating[&float_id].input.mouse, [150., 90.]);
            assert_eq!(shell.input.mouse, [120. / scale, 60. / scale]);

            for (id, expected) in [
                (main_id, [120. / scale, 60. / scale]),
                (float_id, [150., 90.]),
            ] {
                button(&mut shell, event_loop, id, ElementState::Pressed);
                assert_eq!(
                    input(&shell, id).pressed,
                    Some((MouseButton::Left, expected))
                );
                assert_eq!(input(&shell, id).held, Some(MouseButton::Left));
                button(&mut shell, event_loop, id, ElementState::Released);
                assert_eq!(
                    input(&shell, id).released,
                    Some((MouseButton::Left, expected))
                );
                assert_eq!(input(&shell, id).held, None);
                assert_eq!(input(&shell, id).pointer_origin, None);
            }

            wheel(
                &mut shell,
                event_loop,
                main_id,
                MouseScrollDelta::LineDelta(1., -2.),
            );
            wheel(
                &mut shell,
                event_loop,
                float_id,
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(64., -128.)),
            );
            assert_eq!(shell.input.wheel, [-32., 64.]);
            assert_eq!(shell.floating[&float_id].input.wheel, [-32., 64.]);
            wheel(
                &mut shell,
                event_loop,
                main_id,
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                    32. * f64::from(scale),
                    -64. * f64::from(scale),
                )),
            );
            assert_eq!(shell.input.wheel, [-64., 128.]);
            assert_eq!(shell.floating[&float_id].input.wheel, [-32., 64.]);
        }

        shell.window_event(
            event_loop,
            main_id,
            WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        );
        shell.window_event(
            event_loop,
            float_id,
            WindowEvent::ModifiersChanged(
                (ModifiersState::SUPER | ModifiersState::SHIFT | ModifiersState::ALT).into(),
            ),
        );
        assert_eq!(
            shell.input.modifiers,
            Modifiers {
                control: true,
                shift: false,
                alt: false
            }
        );
        assert_eq!(
            shell.floating[&float_id].input.modifiers,
            Modifiers {
                control: true,
                shift: true,
                alt: true
            }
        );

        for (id, text) in [(main_id, "主窗输入"), (float_id, "浮窗输入")] {
            shell.input.begin_frame();
            shell
                .floating
                .get_mut(&float_id)
                .unwrap()
                .input
                .begin_frame();
            shell.window_event(
                event_loop,
                id,
                WindowEvent::Ime(Ime::Preedit(text.into(), Some((0, text.len())))),
            );
            assert!(input(&shell, id).composing);
            assert!(input(&shell, id).text.is_empty());
            shell.window_event(event_loop, id, WindowEvent::Ime(Ime::Commit(text.into())));
            assert_eq!(input(&shell, id).text, text);
            assert!(!input(&shell, id).composing);
            assert!(input(&shell, id).ime_consumed);
            let other = if id == main_id { float_id } else { main_id };
            assert!(input(&shell, other).text.is_empty());
            assert!(!input(&shell, other).pending);
            shell.window_event(event_loop, id, WindowEvent::Ime(Ime::Disabled));
            assert!(!input(&shell, id).composing);
            assert_eq!(input(&shell, id).text, text);
        }

        for id in [main_id, float_id] {
            pointer(&mut shell, event_loop, id, [80., 100.]);
            button(&mut shell, event_loop, id, ElementState::Pressed);
            shell.window_event(
                event_loop,
                id,
                WindowEvent::CursorLeft {
                    device_id: DeviceId::dummy(),
                },
            );
            assert_eq!(input(&shell, id).held, None);
            assert_eq!(input(&shell, id).pressed, None);
            assert_eq!(input(&shell, id).pointer_origin, None);
            assert!(input(&shell, id)
                .mouse
                .iter()
                .all(|value| *value == f32::NEG_INFINITY));
            pointer(&mut shell, event_loop, id, [80., 100.]);
            button(&mut shell, event_loop, id, ElementState::Pressed);
            shell.window_event(
                event_loop,
                id,
                WindowEvent::Ime(Ime::Preedit("暂存".into(), None)),
            );
            shell.window_event(event_loop, id, WindowEvent::Focused(false));
            assert!(input(&shell, id).focus_lost);
            assert_eq!(input(&shell, id).held, None);
            assert_eq!(input(&shell, id).pressed, None);
            assert_eq!(input(&shell, id).modifiers, Modifiers::default());
            assert!(!input(&shell, id).composing);
            assert!(input(&shell, id).text.is_empty());
        }

        // Late events from an already redocked window must not enter the main
        // window path, resize its surface or enqueue another import.
        shell.floating.insert(
            stale.id(),
            FloatWindow {
                window: stale.clone(),
                key: 2,
                panel: dock::Panel::Composition,
                input: Input::default(),
                paint_input: Input::default(),
                scale: 1.,
            },
        );
        shell.floating.remove(&stale.id());
        shell.input = Input::default();
        pointer(&mut shell, event_loop, main_id, [20., 40.]);
        button(&mut shell, event_loop, main_id, ElementState::Pressed);
        pointer(&mut shell, event_loop, stale.id(), [999., 999.]);
        button(&mut shell, event_loop, stale.id(), ElementState::Released);
        shell.window_event(event_loop, stale.id(), WindowEvent::Focused(false));
        shell.window_event(
            event_loop,
            stale.id(),
            WindowEvent::Ime(Ime::Commit("错误目标".into())),
        );
        shell.window_event(
            event_loop,
            stale.id(),
            WindowEvent::Resized(winit::dpi::PhysicalSize::new(640, 480)),
        );
        shell.window_event(
            event_loop,
            stale.id(),
            WindowEvent::DroppedFile(self.root.join("late.mp4")),
        );
        assert_eq!(shell.input.mouse, [20. / shell.scale, 40. / shell.scale]);
        assert_eq!(shell.input.held, Some(MouseButton::Left));
        assert_eq!(shell.input.released, None);
        assert!(!shell.input.focus_lost);
        assert!(shell.input.text.is_empty());
        assert!(shell.resize_queue.is_empty());
        assert!(shell.actions.is_empty());
        let after = shell.engine.state().unwrap();
        assert_eq!(after["revision"], before["revision"]);
        assert_eq!(after["project"], before["project"]);
        self.completed = true;
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn input(shell: &Shell, id: WindowId) -> &Input {
    if shell.window.as_ref().unwrap().id() == id {
        &shell.input
    } else {
        &shell.floating[&id].input
    }
}
fn pointer(shell: &mut Shell, event_loop: &ActiveEventLoop, id: WindowId, position: [f64; 2]) {
    shell.window_event(
        event_loop,
        id,
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(position[0], position[1]),
        },
    );
}
fn button(shell: &mut Shell, event_loop: &ActiveEventLoop, id: WindowId, state: ElementState) {
    shell.window_event(
        event_loop,
        id,
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: WinitMouseButton::Left,
        },
    );
}
fn wheel(shell: &mut Shell, event_loop: &ActiveEventLoop, id: WindowId, delta: MouseScrollDelta) {
    shell.window_event(
        event_loop,
        id,
        WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase: TouchPhase::Moved,
        },
    );
}
