//! Explicit WindowEvents use real independent owners and production input
//! routing. This verifies focus/capture, not physical mouse injection.
use super::*;
use winit::{
    application::ApplicationHandler,
    event::{DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

#[test]
#[ignore = "manual independent Windows owners and actual mask gesture history"]
fn independent_native_owners_release_once_and_cancel_complete_tracks_on_focus_loss() {
    const CHILD: &str = "MOTION_MASK_VIEWER_OWNER_CHILD";
    const NAME: &str = "panels::path_controls::mask_boundaries::viewer::tests::native::owner::independent_native_owners_release_once_and_cancel_complete_tracks_on_focus_loss";
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_MASK_VIEWER_OWNER_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("evidence"));
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logging = crate::logging::init(root.join("logs")).unwrap();
        let mut host = Host { root, cases: 0 };
        EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap()
            .run_app(&mut host)
            .unwrap();
        assert_eq!(host.cases, 24);
        return;
    }
    std::fs::create_dir_all(&output).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            NAME,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &output)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(output.join("stdout.log"), &result.stdout).unwrap();
    std::fs::write(output.join("stderr.log"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
struct Host {
    root: PathBuf,
    cases: usize,
}
impl ApplicationHandler for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let size = [1100., 700.];
        let mut results = vec![];
        for locale in [Locale::Zh, Locale::En] {
            for scale in [1., 1.5, 2.] {
                for owner in [0, 1] {
                    for cancel in [false, true] {
                        let directory = self.root.join(format!("case-{}", self.cases));
                        motion_core::storage::save(&directory, &project()).unwrap();
                        let mut shell = Shell::new(directory, None, Some(locale)).unwrap();
                        shell.home = None;
                        shell.scale = scale;
                        shell.state = shell.engine.seek(33.).unwrap();
                        let before = shell.state["project"].clone();
                        let window = || {
                            Arc::new(
                                event_loop
                                    .create_window(
                                        Window::default_attributes()
                                            .with_visible(false)
                                            .with_inner_size(PhysicalSize::new(
                                                (size[0] * scale) as u32,
                                                (size[1] * scale) as u32,
                                            )),
                                    )
                                    .unwrap(),
                            )
                        };
                        let main = window();
                        let float = window();
                        let main_id = main.id();
                        let float_id = float.id();
                        shell.window = Some(main);
                        shell.floating.insert(
                            float_id,
                            crate::FloatWindow {
                                window: float,
                                key: 1,
                                panel: Panel::Composition,
                                input: Input::default(),
                                paint_input: Input::default(),
                                pointer_position: Default::default(),
                                scale,
                            },
                        );
                        shell.workspace.root = Node::dock(Panel::Composition.dock());
                        shell.panels.select_only(Some(7));
                        shell.panels.focused = Some(Panel::Composition);
                        shell.panels.path_controls.tool = Tool::Mask;
                        shell.panels.path_controls.mask = Some(1);
                        let active = if owner == 0 { main_id } else { float_id };
                        let other = if owner == 0 { float_id } else { main_id };
                        let (paint, actions) = consume(&mut shell, size, owner, active);
                        assert!(actions.is_empty());
                        let viewport = view(&shell, size, owner, &paint);
                        let mapping = shell
                            .panels
                            .composition
                            .navigation
                            .mapping(viewport, [320., 180.])
                            .unwrap();
                        let projection = layer_projection(&shell.state, 7, mapping.image).unwrap();
                        let grip = shell
                            .panels
                            .path_controls
                            .boundaries
                            .grips
                            .iter()
                            .find(|g| {
                                matches!(g.property, MaskProperty::Feather)
                                    && g.axis == 0
                                    && g.sign > 0.
                            })
                            .unwrap()
                            .clone();
                        pointer(&mut shell, event_loop, active, grip.screen, scale);
                        button(&mut shell, event_loop, active, ElementState::Pressed);
                        assert!(!input(&shell, other).pending, "input leaked to other owner");
                        let (_, actions) = consume(&mut shell, size, owner, active);
                        assert!(matches!(actions.as_slice(),[Action::Seek(f)] if *f==33.));
                        dispatch(&mut shell, actions);
                        pointer(
                            &mut shell,
                            event_loop,
                            active,
                            moved(&projection, &grip, 9.),
                            scale,
                        );
                        let (_, actions) = consume(&mut shell, size, owner, active);
                        assert!(!actions.is_empty());
                        dispatch(&mut shell, actions);
                        close(current_mask(&shell).feather.sample(13.).unwrap()[0], 20.);
                        if cancel {
                            shell.window_event(event_loop, active, WindowEvent::Focused(false));
                            let actions = shell.actions.drain(..).collect();
                            dispatch(&mut shell, actions);
                            let (_, actions) = consume(&mut shell, size, owner, active);
                            dispatch(&mut shell, actions);
                            assert_eq!(
                                shell.state["project"], before,
                                "focus loss must restore the whole track"
                            );
                        } else {
                            pointer(
                                &mut shell,
                                event_loop,
                                active,
                                moved(&projection, &grip, 15.),
                                scale,
                            );
                            button(&mut shell, event_loop, active, ElementState::Released);
                            let (_, actions) = consume(&mut shell, size, owner, active);
                            dispatch(&mut shell, actions);
                            close(current_mask(&shell).feather.sample(13.).unwrap()[0], 28.);
                            let after = shell.state["project"].clone();
                            button(&mut shell, event_loop, active, ElementState::Released);
                            pointer(
                                &mut shell,
                                event_loop,
                                active,
                                moved(&projection, &grip, 25.),
                                scale,
                            );
                            assert!(consume(&mut shell, size, owner, active).1.is_empty());
                            assert_eq!(shell.engine.state().unwrap()["project"], after);
                            shell.state = shell.engine.history(0).unwrap();
                            assert_eq!(
                                shell.state["project"], before,
                                "one undo restores whole tracks"
                            );
                        }
                        pointer(&mut shell, event_loop, other, [20., 20.], scale);
                        assert!(input(&shell, other).pending);
                        assert!(!input(&shell, active).pending);
                        assert!(shell.panels.path_controls.boundaries.capture.is_none());
                        results.push(serde_json::json!({"case":self.cases,"language":if locale==Locale::Zh{"zh"}else{"en"},
                    "scale":scale,"owner":owner,"finish":if cancel{"focus_lost"}else{"release"},"result":"passed"}));
                        self.cases += 1;
                    }
                }
            }
        }
        std::fs::write(
            self.root.join("matrix.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
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
fn consume(
    shell: &mut Shell,
    size: [f32; 2],
    owner: u64,
    id: WindowId,
) -> (PaintList, Vec<Action>) {
    let input = input(shell, id).clone();
    let result = draw(shell, size, owner, &input);
    if owner == 0 {
        shell.input.begin_frame();
    } else {
        shell.floating.get_mut(&id).unwrap().input.begin_frame();
    }
    result
}
fn pointer(shell: &mut Shell, events: &ActiveEventLoop, id: WindowId, point: [f32; 2], scale: f32) {
    shell.window_event(
        events,
        id,
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(
                f64::from(point[0] * scale),
                f64::from(point[1] * scale),
            ),
        },
    );
}
fn button(shell: &mut Shell, events: &ActiveEventLoop, id: WindowId, state: ElementState) {
    shell.window_event(
        events,
        id,
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: winit::event::MouseButton::Left,
        },
    );
}
fn dispatch(shell: &mut Shell, actions: Vec<Action>) {
    for action in actions {
        shell.action(action, None).unwrap();
        if let Some((_, reply)) = shell.editing.take() {
            shell.accept_edit_state(
                reply
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap()
                    .unwrap(),
            );
        }
    }
}
