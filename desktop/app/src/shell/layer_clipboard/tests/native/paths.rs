//! Real hidden HWND/wgpu surfaces and production component input, not OS injection.
use super::*;
use crate::panels::{
    self,
    path_controls::{layer_projection, Tool},
};
use motion_core::{
    vector::{FillRule, PathNode, VectorContent, VectorPath, VectorSource},
    Track,
};

fn fixture(root: &std::path::Path) {
    let mut p = motion_core::Project::new(64, 64, 30, 120).unwrap();
    for id in [7, 9] {
        let mut l = motion_core::Layer::solid(
            id,
            if id == 7 {
                "Source path"
            } else {
                "Destination"
            },
            [32.; 2],
            [32., 32., 0.],
            [1.; 4],
        );
        l.content = motion_core::Content::Vector {
            vector: VectorContent {
                source: VectorSource::Paths {
                    paths: if id == 9 {
                        vec![]
                    } else {
                        vec![VectorPath {
                            id: 3,
                            closed: true,
                            nodes: [[-12., -10.], [12., -10.], [12., 10.], [-12., 10.]]
                                .into_iter()
                                .enumerate()
                                .map(|(i, p)| PathNode {
                                    id: i as u64 + 10,
                                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                                })
                                .collect(),
                        }]
                    },
                },
                fill: Some(Track::constant([0.2, 0.6, 0.9, 1.])),
                fill_rule: FillRule::NonZero,
                stroke: None,
                trim: None,
            },
        };
        p.layers.push(l);
    }
    motion_core::storage::save(root, &p).unwrap();
}
fn execute_paths(shell: &mut Shell, actions: Vec<Action>, intent: Intent) {
    assert!(
        matches!(actions.as_slice(), [Action::PathClipboard { intent: actual, .. }] if *actual == intent),
        "intent={intent:?} actions={actions:?} focused={:?} path_focus={} editing={} cache={} menu={:?} message={:?}", shell.panels.focused, shell.panels.path_controls.wants_keyboard(), shell.panels.path_controls.editing(), shell.layer_clipboard.paths_available(), shell.panels.menu, shell.panels.message
    );
    for action in actions {
        shell.action(action, None).unwrap();
    }
    wait(shell);
}
fn draw_paths(shell: &mut Shell, size: [f32; 2], window: u64, input: Input) -> Vec<Action> {
    draw_panel(shell, size, window, input, Panel::Composition).1
}

#[test]
fn native_path_clipboard_selects_authored_nodes_and_shares_menu_keyboard_across_windows() {
    const NAME: &str = "shell::layer_clipboard::tests::native::paths::native_path_clipboard_selects_authored_nodes_and_shares_menu_keyboard_across_windows";
    const CHILD: &str = "MOTION_PATH_CLIPBOARD_UI_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_PATH_CLIPBOARD_UI_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let directory = root.join("project");
    fixture(&directory);
    let mut shell = shell(&directory, Locale::En);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 1280, 800).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.window = Some(main.clone());
    shell
        .engine
        .float_window(1, float.clone(), 1280, 800)
        .unwrap();
    let size = [1280., 800.];
    for (window, locale, scale, name) in [
        (0, Locale::En, 1., "main-en"),
        (0, Locale::Zh, 1.5, "main-zh"),
        (1, Locale::En, 1., "float-en"),
        (1, Locale::Zh, 1.5, "float-zh"),
    ] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.panels.menu = None;
        shell.panels.clipboard_menu_allowed = None;
        shell.panels.clipboard_menu_paths = None;
        shell.panels.focused = Some(Panel::Composition);
        shell.panels.select_only(Some(7));
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
        let window_handle = if window == 0 { &main } else { &float };
        let _ = window_handle.request_inner_size(PhysicalSize::new(physical[0], physical[1]));
        let resize = shell
            .engine
            .resize_async(window, physical[0], physical[1])
            .unwrap();
        shell.state = native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        shell.state = shell.engine.seek(0.).unwrap();
        let mut actions = vec![];
        shell.panels.path_controls.choose(Tool::Pen, &mut actions);
        assert!(actions.is_empty());
        assert!(draw_paths(&mut shell, size, window, Input::default()).is_empty());
        let styles = shell
            .panels
            .styles
            .clone()
            .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
        let view = if window == 0 {
            panels::composition_view_styled(shell.workspace.display_root(), size, &styles)
        } else {
            panels::floating_view_styled(Panel::Composition, size, &styles).unwrap()
        };
        let mapping = shell
            .panels
            .composition
            .navigation
            .mapping(view, [64., 64.])
            .unwrap();
        let projection = layer_projection(&shell.state, 7, mapping.image).unwrap();
        for (index, local) in [[-12., -10.], [12., -10.], [12., 10.], [-12., 10.]]
            .into_iter()
            .enumerate()
        {
            let point = projection.screen(local).unwrap();
            let modifiers = Modifiers {
                shift: index > 0,
                ..Default::default()
            };
            let mut pressed = click(point);
            pressed.held = Some(MouseButton::Left);
            pressed.modifiers = modifiers;
            assert!(draw_paths(&mut shell, size, window, pressed).is_empty());
            assert!(draw_paths(
                &mut shell,
                size,
                window,
                Input {
                    mouse: point,
                    released: Some((MouseButton::Left, point)),
                    modifiers,
                    ..Default::default()
                }
            )
            .is_empty());
        }
        if window == 0 {
            assert!(crate::shell::layer_clipboard::tests::text_menu::open(&mut shell).is_empty());
            assert_eq!(shell.panels.menu, Some(1));
            capture_panel(
                &mut events,
                &mut shell,
                size,
                scale,
                window,
                root.join(format!("{name}-paths-menu.png")),
                Panel::Composition,
            );
            let actions = crate::shell::layer_clipboard::tests::text_menu::choose(&mut shell, 2);
            execute_paths(&mut shell, actions, Intent::Copy);
        } else {
            let actions = draw_paths(&mut shell, size, window, chord(Key::C, false));
            execute_paths(&mut shell, actions, Intent::Copy);
        }
        shell.panels.select_only(Some(9));
        shell.state = shell.engine.seek(30.).unwrap();
        assert!(draw_paths(&mut shell, size, window, Input::default()).is_empty());
        let before = shell.state["project"].clone();
        let other_window = 1 - window;
        // The same Shell cache feeds both component surfaces.
        let actions = draw_paths(&mut shell, size, other_window, chord(Key::V, true));
        execute_paths(&mut shell, actions, Intent::Paste { in_place: true });
        let path = &shell.state["project"]["layers"][1]["content"]["vector"]["source"]["paths"][0];
        assert!(path["closed"].as_bool().unwrap());
        assert_eq!(path["nodes"].as_array().unwrap().len(), 4);
        capture_panel(
            &mut events,
            &mut shell,
            size,
            scale,
            window,
            root.join(format!("{name}-pasted-path.png")),
            Panel::Composition,
        );
        shell.engine.save().unwrap();
        assert_eq!(
            serde_json::to_value(motion_core::storage::load(&directory).unwrap()).unwrap(),
            shell.state["project"]
        );
        shell.state = shell.engine.history(0).unwrap();
        assert_eq!(shell.state["project"], before);
    }
}
