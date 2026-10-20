use super::*;
use crate::{workspace, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn settle(shell: &mut Shell) {
    let (_, reply) = shell
        .editing
        .take()
        .expect("ordering must dispatch one batch");
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
    );
}

fn capture(shell: &mut Shell, file: PathBuf) {
    let size = [1000., 700.];
    let (paint, actions) = shell.draw_commands(size);
    assert!(actions.is_empty(), "capture emitted actions: {actions:?}");
    let ui = shell.ui_frame(paint, size, true);
    shell.engine.capture_ui(ui, file).unwrap();
    shell.input = Input::default();
}

#[test]
fn native_layer_ordering_is_atomic_observes_stale_revision_and_renders_bilingual_menus() {
    const CHILD: &str = "MOTION_LAYER_NAVIGATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
        for id in 1..=5 {
            project.layers.push(motion_core::Layer::solid(
                id,
                &format!("Layer {id}"),
                [32.; 2],
                [0.; 3],
                [1.; 4],
            ));
        }
        motion_core::storage::save(&root.join("project"), &project).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        shell.home = None;
        shell.workspace = workspace::Workspace::default();
        shell.workspace.root = motion_ui::dock::Node::dock(Panel::Timeline.dock());
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 700)),
                )
                .unwrap(),
        );
        shell.state = shell
            .engine
            .attach_async(window.clone(), 1000, 700)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        let before = shell.engine.state().unwrap();
        shell.panels.select_only(Some(2));
        shell.panels.add_selection(4);
        shell
            .action(
                Action::OrderLayers {
                    objects: vec![4, 2],
                    order: Order::Top,
                },
                None,
            )
            .unwrap();
        settle(&mut shell);
        assert_eq!(
            shell.state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|layer| layer["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [1, 3, 5, 2, 4]
        );
        assert_eq!(
            shell.state["history"]["undoEntries"].as_u64().unwrap(),
            before["history"]["undoEntries"].as_u64().unwrap() + 1
        );
        let ordered = shell.state["project"].clone();
        shell.accept_edit_state(shell.engine.history(0).unwrap());
        assert_eq!(shell.state["project"], before["project"]);
        shell.accept_edit_state(shell.engine.history(1).unwrap());
        assert_eq!(shell.state["project"], ordered);
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&root.join("project")).unwrap())
                .unwrap(),
            ordered
        );

        // The serial worker must reject a plan based on a superseded snapshot.
        let stale = shell.state.clone();
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&Command::Rename {
                    object: 1,
                    name: "Renamed".into(),
                })
                .unwrap(),
            )
            .unwrap();
        let current = shell.state.clone();
        shell.state = stale;
        shell
            .action(
                Action::OrderLayers {
                    objects: vec![2, 4],
                    order: Order::Bottom,
                },
                None,
            )
            .unwrap();
        let (_, reply) = shell.editing.take().unwrap();
        let error = reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap_err();
        assert!(error.to_lowercase().contains("revision"), "{error}");
        shell.state = shell.engine.state().unwrap();
        for field in ["project", "revision", "history", "canUndo", "canRedo"] {
            assert_eq!(shell.state[field], current[field], "{field}");
        }
        std::fs::write(root.join("expected-error.txt"), &error).unwrap();
        for locale in [Locale::En, Locale::Zh] {
            shell.workspace.locale = locale;
            shell.panels.locale = locale;
            shell.panels.select_only(Some(2));
            shell.panels.add_selection(4);
            shell.panels.focused = Some(Panel::Timeline);
            let tag = if locale == Locale::Zh { "zh" } else { "en" };
            // Warm up actual context and layout before opening menus.
            capture(&mut shell, root.join(format!("{tag}-selection.png")));
            shell.panels.menu = Some(3);
            capture(&mut shell, root.join(format!("{tag}-layer-menu.png")));
            shell.panels.menu = None;
            shell
                .panels
                .layer_actions
                .open(Some(2), [650., 420.], &shell.state);
            capture(&mut shell, root.join(format!("{tag}-context-menu.png")));
            for _ in 0..4 {
                shell.input = Input {
                    keys: vec![(Key::Down, Modifiers::default())],
                    ..Default::default()
                };
                let (_, actions) = shell.draw_commands([1000., 700.]);
                assert!(actions.is_empty());
            }
            shell.input = Input {
                keys: vec![(Key::Right, Modifiers::default())],
                ..Default::default()
            };
            capture(&mut shell, root.join(format!("{tag}-order-menu.png")));
            shell.panels.layer_actions.cancel();
        }
        log::logger().flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LAYER_NAVIGATION_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::layer_navigation::tests::native::native_layer_ordering_is_atomic_observes_stale_revision_and_renders_bilingual_menus"])
        .env(CHILD,&root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let cause = std::fs::read_to_string(root.join("expected-error.txt")).unwrap();
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.contains(&cause) && stderr.contains(&cause));
    assert!(!stdout.contains(&cause) && !stdout.contains("logging initialized"));
    for locale in ["en", "zh"] {
        for panel in ["selection", "layer-menu", "context-menu", "order-menu"] {
            assert!(root.join(format!("{locale}-{panel}.png")).is_file());
        }
    }
}
