//! Real GPU surfaces and production main/floating panel routes. Input is
//! controlled at the normalized boundary; this is not OS mouse injection.
use super::*;
use crate::{Shell, native_test_support};
use std::{path::PathBuf, sync::Arc};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window};

fn render(
    shell: &mut Shell,
    floating: bool,
    size: [f32; 2],
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    let actions = if floating {
        floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::Timeline,
            size,
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        chrome(
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
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (
            (size[0] * shell.scale) as u32,
            (size[1] * shell.scale) as u32
        )
    );
}
#[test]
#[ignore = "manual Windows GPU shared-menu screenshots and real worker transactions"]
fn shared_menu_native_root_child_shortcuts_checks_and_small_window_owners() {
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("MOTION_SHARED_MENU_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("evidence"));
    std::fs::create_dir_all(&output).unwrap();
    let _logging = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for floating in [false, true] {
            for scale in [1., 1.5, 2.] {
                let root = temporary
                    .path()
                    .join(format!("{language}-{floating}-{scale}"));
                motion_core::storage::save(&root, engine().project()).unwrap();
                let mut shell = Shell::new(root.clone(), None, Some(locale)).unwrap();
                shell.home = None;
                shell.scale = scale;
                shell.workspace.root = dock::Node::dock(Panel::Timeline.dock());
                shell.panels.select_only(Some(9));
                shell.panels.focused = Some(Panel::Timeline);
                let size = if floating {
                    [320., 360.]
                } else if scale == 1. {
                    [1600., 1000.]
                } else {
                    [900., 600.]
                };
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
                shell.state =
                    native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                        .unwrap()
                        .unwrap();
                shell.window = Some(window);
                shell.attached = true;
                shell.atlas.set_pixel_ratio(scale);
                let before = shell.state["project"].clone();
                // Production uses the Timeline panel owner in both windows.
                shell.panels.layer_actions.owner = Some(Panel::Timeline);
                let anchor = if floating {
                    [310., 330.]
                } else {
                    [size[0] - 20., size[1] - 40.]
                };
                shell
                    .panels
                    .layer_actions
                    .open(Some(9), anchor, &shell.state);
                let (paint, actions) = render(&mut shell, floating, size, &Input::default());
                assert!(actions.is_empty());
                let prefix = format!(
                    "shared-menu-{language}-{}-{scale}x",
                    if floating { "floating-route" } else { "main" }
                );
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    output.join(format!("{prefix}-root.png")),
                );
                let enter = Input {
                    mouse: [-1.; 2],
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                };
                let (paint, actions) = render(&mut shell, floating, size, &enter);
                assert!(actions.is_empty());
                assert!(shell.panels.layer_actions.popup.as_ref().unwrap().page == Page::Switch);
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    output.join(format!("{prefix}-child.png")),
                );
                // Opening has not changed the project or consumed a history step.
                assert_eq!(shell.engine.state().unwrap()["project"], before);
                // The next Enter invokes Visibility exactly once, even when the
                // child's topmost card overlaps its parent in a narrow float route.
                let (_, actions) = render(&mut shell, floating, size, &enter);
                assert_eq!(actions.len(), 1);
                for action in actions {
                    shell.action(action, None).unwrap();
                    if let Some((_, reply)) = shell.editing.take() {
                        shell.accept_edit_state(
                            native_test_support::receive(
                                &mut events,
                                &reply,
                                Duration::from_secs(30),
                            )
                            .unwrap()
                            .unwrap(),
                        );
                    }
                }
                assert_eq!(shell.state["project"]["layers"][1]["visible"], false);
                shell.engine.save().unwrap();
                assert!(!motion_core::storage::load(&root).unwrap().layers[1].visible);
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(shell.state["project"], before);
                if !floating {
                    shell.panels.menu = Some(7);
                    let end = Input {
                        mouse: [-1.; 2],
                        keys: vec![(Key::End, Default::default())],
                        ..Default::default()
                    };
                    assert!(render(&mut shell, false, size, &end).1.is_empty());
                    let (paint, actions) = render(&mut shell, false, size, &enter);
                    assert!(actions.is_empty());
                    assert_eq!(shell.panels.menu_navigation.cards.len(), 2);
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        size,
                        output.join(format!("{prefix}-workspaces.png")),
                    );
                }
            }
        }
    }
}
