use super::*;
use crate::{workspace, Shell};
use motion_ui::dock::{Edge, Node, Panel};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn capture(shell: &mut Shell, path: PathBuf) {
    // capture_ui reads the existing composition scratch texture; it does not
    // render the scene itself. Complete a genuine Present before reading it.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (paint, actions) = shell.draw_commands([1000., 760.]);
        assert!(actions.is_empty(), "{actions:?}");
        let ui = shell.ui_frame(paint, [1000., 760.], false);
        let ready = shell
            .engine
            .present_async(ui)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        shell.input = Input::default();
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "camera scene did not become ready"
        );
    }
    let (paint, actions) = shell.draw_commands([1000., 760.]);
    assert!(actions.is_empty(), "{actions:?}");
    let ui = shell.ui_frame(paint, [1000., 760.], true);
    shell.engine.capture_ui(ui, path).unwrap();
    shell.input = Input::default();
}

#[test]
fn native_camera_lens_presets_render_in_both_languages_and_commit_to_the_fov_track() {
    const CHILD: &str = "MOTION_CAMERA_LENS_NATIVE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let mut project = motion_core::Project::new(1920, 1080, 30, 300).unwrap();
        let mut shape = motion_core::Layer::solid(
            1,
            "Lens study",
            [320., 320.],
            [960., 540., 0.],
            [0.4, 0.65, 0.9, 1.],
        );
        shape.three_d = true;
        project.layers.push(shape);
        motion_core::storage::save(&root.join("project"), &project).unwrap();
        let mut shell = Shell::new(root.join("project"), None, Some(Locale::En)).unwrap();
        shell.home = None;
        shell.workspace = workspace::Workspace::default();
        shell.workspace.root = Node::Split {
            edge: Edge::Left,
            split: 0.36,
            first: Box::new(Node::dock(Panel::Properties.dock())),
            second: Box::new(Node::dock(Panel::Composition.dock())),
        };
        let object = create(&shell.engine);
        shell.state = shell.engine.state().unwrap();
        shell.panels.select_only(Some(object));
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
                        .with_inner_size(winit::dpi::PhysicalSize::new(1000, 760)),
                )
                .unwrap(),
        );
        shell.state = shell
            .engine
            .attach_async(window.clone(), 1000, 760)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.window = Some(window);
        shell.attached = true;
        for (locale, tag) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
            shell.workspace.locale = locale;
            shell.panels.locale = locale;
            capture(&mut shell, root.join(format!("{tag}-camera.png")));
            let rect = shell.panels.camera_controls.lens.preset_rect.unwrap();
            shell.input = input(center(rect), true, false);
            capture(&mut shell, root.join(format!("{tag}-presets.png")));
            assert!(shell.panels.camera_controls.lens.presets.open);
            shell.input = Input {
                keys: vec![(Key::Escape, Modifiers::default())],
                ..Default::default()
            };
            capture(&mut shell, root.join(format!("{tag}-closed.png")));
            assert!(!shell.panels.camera_controls.lens.presets.open);
        }
        // Preset keyboard selection is a genuine dropdown interaction, not a
        // direct edit of the fixture. Custom -> 15mm, then commit.
        let before = shell.engine.state().unwrap();
        let rect = shell.panels.camera_controls.lens.preset_rect.unwrap();
        shell.input = input(center(rect), true, false);
        capture(&mut shell, root.join("before-preset.png"));
        shell.input = Input {
            keys: vec![(Key::Down, Modifiers::default())],
            ..Default::default()
        };
        capture(&mut shell, root.join("preset-highlight.png"));
        shell.input = Input {
            keys: vec![(Key::Enter, Modifiers::default())],
            ..Default::default()
        };
        let (_, actions) = shell.draw_commands([1000., 760.]);
        assert_eq!(actions.len(), 1, "{actions:?}");
        run(&shell.engine, actions);
        shell.state = shell.engine.state().unwrap();
        let lens = snapshot(&shell.state, object).unwrap().0;
        assert!((lens.focal_length_mm - 15.).abs() < 1e-4, "{lens:?}");
        assert_eq!(
            shell.state["history"]["undoEntries"].as_u64().unwrap(),
            before["history"]["undoEntries"].as_u64().unwrap() + 1
        );
        capture(&mut shell, root.join("zh-preset-applied.png"));
        assert_eq!(
            shell.engine.history(0).unwrap()["project"],
            before["project"]
        );
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CAMERA_LENS_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::camera_controls::lens::tests::native::native_camera_lens_presets_render_in_both_languages_and_commit_to_the_fov_track"])
        .env(CHILD,&root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for name in [
        "en-camera.png",
        "zh-camera.png",
        "en-presets.png",
        "zh-presets.png",
        "zh-preset-applied.png",
    ] {
        assert!(root.join(name).is_file(), "{name}");
    }
}
