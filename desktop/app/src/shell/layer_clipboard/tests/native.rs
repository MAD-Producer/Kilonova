//! Production chrome on real hidden Windows surfaces, with explicit Input.
//! This tests component routing, not OS keyboard injection or text services.
use super::*;
use crate::{
    engine::{AtlasUpload, UiFrame},
    i18n::Text,
    native_test_support,
};
use motion_ui::{
    dock::Panel,
    input::{Input, Key, Modifiers, MouseButton},
    PaintList,
};
use std::{path::PathBuf, sync::Arc};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
mod keyframes;
mod text;
mod paths;

fn chord(key: Key, shift: bool) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control: true,
                shift,
                alt: false,
            },
        )],
        ..Default::default()
    }
}
fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(shell: &mut Shell, size: [f32; 2], window: u64, input: Input) -> (PaintList, Vec<Action>) {
    draw_panel(shell, size, window, input, Panel::Timeline)
}
fn draw_panel(shell: &mut Shell, size: [f32; 2], window: u64, mut input: Input, panel: Panel) -> (PaintList, Vec<Action>) {
    if window == 0 {
        shell.input = input;
        shell.draw_commands(size)
    } else {
        input.text_clipboard = Some(shell.text_clipboard.clone());
        shell.panels.clipboard_available = shell.layer_clipboard.available();
        shell.panels.keyframe_clipboard_available = shell.layer_clipboard.keyframes_available();
        shell.panels.path_clipboard_available = shell.layer_clipboard.paths_available();
        shell.panels.clipboard_busy = shell.layer_clipboard.busy();
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        let actions = crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            panel,
            size,
            &mut shell.panels,
            &shell.state,
            &input,
        );
        (paint, actions)
    }
}
fn execute(shell: &mut Shell, actions: Vec<Action>, intent: Intent) {
    assert_eq!(actions.len(), 1, "unexpected actions: {actions:?}");
    assert!(matches!(&actions[0],Action::LayerClipboard(actual) if *actual==intent));
    for action in actions {
        shell.action(action, None).unwrap();
    }
    wait(shell);
}
fn capture(
    events: &mut EventLoop<()>,
    shell: &mut Shell,
    size: [f32; 2],
    scale: f32,
    window: u64,
    path: PathBuf,
) {
    capture_panel(events, shell, size, scale, window, path, Panel::Timeline);
}
fn capture_panel(events: &mut EventLoop<()>, shell: &mut Shell, size: [f32; 2], scale: f32, window: u64, path: PathBuf, panel: Panel) {
    let (paint, actions) = draw_panel(shell, size, window, Input::default(), panel);
    assert!(actions.is_empty(), "capture emitted actions: {actions:?}");
    let view = (panel == Panel::Composition).then(|| if window == 0 {
        crate::panels::composition_view_styled(shell.workspace.display_root(), size, &paint.styles())
    } else { crate::panels::floating_view_styled(panel, size, &paint.styles()).unwrap() });
    let config = shell.atlas.config();
    let ui = UiFrame {
        paint,
        atlas: Some(AtlasUpload {
            pixels: shell.atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: view.map(|v| [v.min[0] * scale, v.min[1] * scale, v.width() * scale, v.height() * scale]),
        composition_image: view.and_then(|v| shell.viewer_image(v, scale)),
        window,
        node_preview: None,
    };
    let front = shell
        .engine
        .present_async(UiFrame {
            paint: PaintList::default(),
            atlas: None,
            logical_size: size,
            composition: None,
            composition_image: None,
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        native_test_support::receive(events, &front, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
#[test]
fn native_layer_clipboard_keys_and_edit_menu_use_actual_bilingual_surfaces() {
    const CHILD: &str = "MOTION_LAYER_CLIPBOARD_UI_CHILD";
    const NAME:&str="shell::layer_clipboard::tests::native::native_layer_clipboard_keys_and_edit_menu_use_actual_bilingual_surfaces";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_LAYER_CLIPBOARD_UI_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
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
    project(&root.join("project"), false);
    let mut shell = shell(&root.join("project"), Locale::En);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1350, 900)),
            )
            .unwrap(),
    );
    let attach = shell.engine.attach_async(main.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    shell.window = Some(main.clone());
    shell
        .engine
        .float_window(1, float.clone(), 1350, 900)
        .unwrap();
    for (window, size, scale, locale, name) in [
        (0, [900., 600.], 1., Locale::En, "main-en-minimum"),
        (0, [1600., 1000.], 1.5, Locale::Zh, "main-zh-default"),
        (1, [900., 600.], 1.5, Locale::En, "float-en-minimum"),
        (1, [1600., 1000.], 1., Locale::Zh, "float-zh-default"),
    ] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        shell.panels.menu = None;
        shell.panels.clipboard_menu_allowed = None;
        shell.panels.focused = Some(Panel::Timeline);
        shell.panels.select_only(Some(7));
        shell.panels.timeline.graph_mode = false;
        shell.scale = scale;
        shell.atlas.set_pixel_ratio(scale);
        let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
        let native = if window == 0 { &main } else { &float };
        let _ = native.request_inner_size(PhysicalSize::new(physical[0], physical[1]));
        let resize = shell
            .engine
            .resize_async(window, physical[0], physical[1])
            .unwrap();
        shell.state = native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let (_, actions) = draw(&mut shell, size, window, chord(Key::C, false));
        execute(&mut shell, actions, Intent::Copy);
        if window == 0 {
            let point = [
                8. + shell.atlas.measure(locale.t(Text::File)) + 22. + 12.,
                motion_ui::theme::metrics::TITLE_BAR + motion_ui::theme::metrics::MENU_BAR * 0.5,
            ];
            let (_, actions) = draw(&mut shell, size, window, click(point));
            assert!(actions.is_empty());
            assert_eq!(shell.panels.menu, Some(1));
            assert_eq!(shell.panels.focused, Some(Panel::Timeline));
            capture(
                &mut events,
                &mut shell,
                size,
                scale,
                window,
                root.join(format!("{name}-edit-menu.png")),
            );
            let point = [
                point[0] + 20.,
                motion_ui::theme::metrics::TITLE_BAR
                    + motion_ui::theme::metrics::MENU_BAR
                    + 4.
                    + 32. * 4.
                    + 15.,
            ];
            shell.state = shell.engine.seek(50.).unwrap();
            let (_, actions) = draw(&mut shell, size, window, click(point));
            execute(&mut shell, actions, Intent::Paste { in_place: false });
            assert_eq!(shell.panels.focused, Some(Panel::Timeline));
            let pasted = shell.panels.selected.unwrap();
            let layer = shell.state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == pasted)
                .unwrap();
            assert_eq!(layer["timeline"]["in_frame"], 50);
        } else {
            let (_, actions) = draw(&mut shell, size, window, chord(Key::V, true));
            execute(&mut shell, actions, Intent::Paste { in_place: true });
            let pasted = shell.panels.selected.unwrap();
            let layer = shell.state["project"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == pasted)
                .unwrap();
            assert_eq!(layer["timeline"]["in_frame"], 10);
        }
        capture(
            &mut events,
            &mut shell,
            size,
            scale,
            window,
            root.join(format!("{name}-pasted.png")),
        );
        if window == 0 {
            shell.actions.push_back(Action::RefreshState);
            shell
                .action(
                    Action::LayerClipboard(Intent::Paste { in_place: true }),
                    None,
                )
                .unwrap();
            until_phase(&mut shell, "ready");
            shell.panels.menu = Some(1);
            shell.panels.clipboard_menu_allowed = Some(true);
            capture(
                &mut events,
                &mut shell,
                size,
                scale,
                window,
                root.join(format!("{name}-cancel-menu.png")),
            );
            let point = [
                60.,
                motion_ui::theme::metrics::TITLE_BAR
                    + motion_ui::theme::metrics::MENU_BAR
                    + 4.
                    + 32. * 6.
                    + 15.,
            ];
            let (_, actions) = draw(&mut shell, size, window, click(point));
            assert!(matches!(actions.as_slice(), [Action::CancelLayerClipboard]));
            shell.actions.extend(actions);
            shell.poll_layer_clipboard();
            shell.actions.clear();
            wait(&mut shell);
            assert!(!shell.layer_clipboard.busy());
        }
    }
    shell.engine.save().unwrap();
    assert_eq!(
        motion_core::storage::load(&root.join("project"))
            .unwrap()
            .layers
            .len(),
        5
    );
}
