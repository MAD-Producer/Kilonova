//! Native event translation through the actual controls and worker history.
//! Hidden windows supply real IDs; injected events do not emulate OS devices.
use crate::*;
use winit::{
    dpi::PhysicalPosition, event::DeviceId, platform::windows::EventLoopBuilderExtWindows,
};

#[test]
fn native_property_gesture_commits_once_across_dpi_and_acknowledged_redraws() {
    const CHILD: &str = "MOTION_NATIVE_GESTURE_TEST_ROOT";
    const NAME: &str = "shell::presentation::curve_playback_tests::native_gesture_tests::native_property_gesture_commits_once_across_dpi_and_acknowledged_redraws";
    if let Some(root) = std::env::var_os(CHILD) {
        let mut host = GestureHost {
            root: root.into(),
            cases: 0,
        };
        EventLoop::<()>::builder()
            .with_any_thread(true)
            .build()
            .unwrap()
            .run_app(&mut host)
            .unwrap();
        assert_eq!(host.cases, 6, "every locale/DPI case must execute");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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

struct GestureHost {
    root: PathBuf,
    cases: usize,
}

impl ApplicationHandler for GestureHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Native gesture verification")
                        .with_visible(false),
                )
                .unwrap(),
        );
        for locale in [Locale::Zh, Locale::En] {
            for scale in [1., 1.5, 2.] {
                let mut shell = Shell::new(
                    self.root.join(format!("case-{}", self.cases)),
                    None,
                    Some(locale),
                )
                .unwrap();
                shell.home = None;
                shell.window = Some(window.clone());
                shell.scale = scale;
                let empty = shell.state["project"].clone();
                shell.state = shell
                    .engine
                    .edit(
                        serde_json::to_string(&Command::Add {
                            layer: Layer::solid(
                                7,
                                "Native gesture",
                                [100.; 2],
                                [100., 200., 0.],
                                [1.; 4],
                            ),
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let before = shell.state["project"].clone();
                shell.panels.select_only(Some(7));
                shell.panels.focused = Some(dock::Panel::Properties);
                shell.workspace.root = dock::Node::dock(dock::Panel::Properties.dock());
                let size = [400., 1000.];
                let (paint, actions) = shell.draw_commands(size);
                assert!(actions.is_empty());
                let area = panels::frames(&shell.workspace.root, size)[0].content;
                let styles = paint.styles();
                // Rendered Position X: the navigation rail precedes the value.
                let point = [
                    area.min[0]
                        + styles.metric("padding.panel", 8.) * 2.
                        + styles.metric("tree.indent", 14.)
                        + styles.metric("icon.slot", 20.) * 4.
                        + styles.metric("label.column", 72.)
                        + 10.,
                    area.min[1] + 8. + 4. * styles.metric("row.height", 22.) + 10.,
                ];
                pointer(&mut shell, event_loop, point);
                button(&mut shell, event_loop, ElementState::Pressed);
                assert!(shell.draw_commands(size).1.is_empty());

                pointer(&mut shell, event_loop, [point[0] + 40., point[1]]);
                let actions = shell.draw_commands(size).1;
                assert!(
                    !actions.is_empty(),
                    "native movement did not reach the field"
                );
                dispatch(&mut shell, actions);
                assert_eq!(shell.state["sampledLayers"][0]["position"][0], 120.);

                // Release arrives while the worker acknowledgement needs paint.
                // Repeated acknowledgement redraws must leave it queued.
                pointer(&mut shell, event_loop, [point[0] + 60., point[1]]);
                button(&mut shell, event_loop, ElementState::Released);
                for _ in 0..3 {
                    assert!(shell.draw_acknowledged(size).1.is_empty());
                    assert!(shell.input.pending && shell.input.released.is_some());
                    assert_eq!(shell.state["sampledLayers"][0]["position"][0], 120.);
                }
                let actions = shell.draw_commands(size).1;
                assert!(!actions.is_empty(), "native release was lost");
                dispatch(&mut shell, actions);
                assert_eq!(shell.state["sampledLayers"][0]["position"][0], 130.);
                assert_eq!(shell.state["sampledLayers"][0]["position"][1], 200.);
                let after = shell.state["project"].clone();
                assert_ne!(after, before);

                // Empty redraws, a duplicate late release and later cursor
                // movement cannot keep editing or create another undo record.
                for _ in 0..3 {
                    assert!(shell.draw_commands(size).1.is_empty());
                }
                button(&mut shell, event_loop, ElementState::Released);
                assert!(shell.draw_commands(size).1.is_empty());
                pointer(&mut shell, event_loop, [point[0] + 160., point[1]]);
                assert!(shell.draw_commands(size).1.is_empty());
                assert_eq!(shell.engine.state().unwrap()["project"], after);
                assert_eq!(shell.engine.history(0).unwrap()["project"], before);
                assert_eq!(shell.engine.history(0).unwrap()["project"], empty);
                assert_eq!(shell.engine.history(1).unwrap()["project"], before);
                assert_eq!(shell.engine.history(1).unwrap()["project"], after);
                self.cases += 1;
            }
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn pointer(shell: &mut Shell, event_loop: &ActiveEventLoop, point: [f32; 2]) {
    shell.window_event(
        event_loop,
        shell.window.as_ref().unwrap().id(),
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(point[0] * shell.scale),
                f64::from(point[1] * shell.scale),
            ),
        },
    );
}

fn button(shell: &mut Shell, event_loop: &ActiveEventLoop, state: ElementState) {
    shell.window_event(
        event_loop,
        shell.window.as_ref().unwrap().id(),
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: WinitMouseButton::Left,
        },
    );
}

fn dispatch(shell: &mut Shell, actions: Vec<panels::Action>) {
    shell.actions.extend(actions);
    while let Some(action) = shell::actions::pop_action(&mut shell.actions) {
        shell.action(action, None).unwrap();
        if let Some((_, reply)) = shell.editing.take() {
            shell.accept_edit_state(reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap());
        }
    }
}
