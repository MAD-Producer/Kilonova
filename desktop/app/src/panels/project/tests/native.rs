//! Actual Windows GPU surfaces, production panel input and real worker commits.
//! Input is controlled, not injected through the operating system.
use super::*;
use crate::{engine, native_test_support, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};
mod drag;
mod thumbnails;

fn frame(
    shell: &mut Shell,
    owner: Option<Panel>,
    size: [f32; 2],
    input: Input,
) -> (PaintList, Vec<Action>) {
    if let Some(panel) = owner {
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
    } else {
        let previous=std::mem::replace(&mut shell.input,input);
        let result = shell.draw_commands(size);
        // Passive fixture paint must retain native capture coordinates between
        // CursorMoved and MouseInput::Released, like production end_frame.
        shell.input = previous;
        result
    }
}
fn run(shell: &mut Shell, events: &mut EventLoop<()>, actions: Vec<Action>) {
    for action in actions {
        shell.action(action, None).unwrap();
        if let Some((_, reply)) = shell.editing.take() {
            shell.accept_edit_state(
                native_test_support::receive(events, &reply, Duration::from_secs(30))
                    .unwrap()
                    .unwrap(),
            );
        }
    }
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    key: u64,
    path: PathBuf,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = key;
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(&path).unwrap().to_rgba8().dimensions(),
        (
            (size[0] * shell.scale) as u32,
            (size[1] * shell.scale) as u32
        )
    );
}
#[test]
#[ignore = "manual native window screenshots and project organization transactions"]
fn project_browser_native_surfaces_menus_and_folder_edits_work_in_both_languages_and_owners() {
    const CHILD: &str = "MOTION_PROJECT_BROWSER_NATIVE_CHILD";
    const NAME:&str="panels::project::tests::native::project_browser_native_surfaces_menus_and_folder_edits_work_in_both_languages_and_owners";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_PROJECT_BROWSER_NATIVE_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
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
    let project_root = root.join("project");
    std::fs::create_dir_all(project_root.join("assets")).unwrap();
    for id in 1..=3 {
        image::RgbaImage::from_pixel(4, 4, image::Rgba([80, 120, 180, 255]))
            .save(project_root.join(format!("assets/source-{id}.png")))
            .unwrap();
    }
    let project: Project = serde_json::from_value(fixture()["project"].clone()).unwrap();
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut shell = Shell::new(project_root.clone(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.workspace.root = Node::Split {
        edge: dock::Edge::Left,
        split: 0.36,
        first: Box::new(Node::dock(Panel::Project.dock())),
        second: Box::new(Node::dock(Panel::Composition.dock())),
    };
    shell.panels.focused = Some(Panel::Project);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(900, 600))
                    .with_title("Motion Studio · Project browser"),
            )
            .unwrap(),
    );
    let reply = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window.clone());
    shell.attached = true;
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(600, 420))
                    .with_title("Motion Studio · Project"),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, floating.clone(), 600, 420)
        .unwrap();
    let mut cases = Vec::new();
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        for owner in [None, Some(Panel::Project)] {
            shell.panels.locale = locale;
            shell.workspace.locale = locale;
            for (size, scale) in [
                ([900., 600.], 1.),
                ([1600., 1000.], 1.5),
                ([900., 600.], 2.),
            ] {
                let size = if owner.is_some() { [600., 420.] } else { size };
                shell.scale = scale;
                let physical =
                    PhysicalSize::new((size[0] * scale) as u32, (size[1] * scale) as u32);
                let key = u64::from(owner.is_some());
                let target = if owner.is_some() { &floating } else { &window };
                let _ = target.request_inner_size(physical);
                let resize = shell
                    .engine
                    .resize_async(key, physical.width, physical.height)
                    .unwrap();
                native_test_support::receive(&mut events, &resize, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                shell.panels.project.browser = Controls::default();
                shell.panels.project.query.clear();
                shell.panels.project.context_menu = None;
                let suffix = format!(
                    "{lang}-{}-{}-ratio{scale}",
                    if owner.is_some() { "floating" } else { "main" },
                    size[0] as u32
                );
                let (paint, actions) = frame(&mut shell, owner, size, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-inventory.png")),
                );
                let row = rect(&shell.panels, &RowKey::Folder(7));
                let (paint, actions) =
                    frame(&mut shell, owner, size, click(row, MouseButton::Right));
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-folder-menu.png")),
                );
                let rename = shell.panels.project.browser.buttons[1];
                let (_, actions) = frame(&mut shell, owner, size, click(rename, MouseButton::Left));
                assert!(actions.is_empty());
                let (paint, actions) = frame(&mut shell, owner, size, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-rename.png")),
                );
                let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Escape));
                assert!(actions.is_empty());
                let (_, actions) = frame(&mut shell, owner, size, click(row, MouseButton::Left));
                assert!(actions.is_empty());
                let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Enter));
                run(&mut shell, &mut events, actions);
                let (paint, actions) = frame(&mut shell, owner, size, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-folder-contents.png")),
                );
                let asset = rect(&shell.panels, &RowKey::Item(ProjectItem::Image { id: 2 }));
                let (paint, actions) =
                    frame(&mut shell, owner, size, click(asset, MouseButton::Right));
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-asset-menu.png")),
                );
                let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Escape));
                assert!(actions.is_empty());
                let (_, actions) = frame(&mut shell, owner, size, click(asset, MouseButton::Left));
                assert!(actions.is_empty());
                let mv = shell.panels.project.browser.buttons[2];
                let (_, actions) = frame(&mut shell, owner, size, click(mv, MouseButton::Left));
                assert!(actions.is_empty());
                let (paint, actions) = frame(&mut shell, owner, size, Input::default());
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    key,
                    root.join(format!("{suffix}-move.png")),
                );
                let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Home));
                assert!(actions.is_empty());
                let (_, actions) = frame(&mut shell, owner, size, key_input(Key::Up));
                assert!(actions.is_empty());
                let apply = shell.panels.project.browser.buttons[0];
                let (_, actions) = frame(&mut shell, owner, size, click(apply, MouseButton::Left));
                assert!(matches!(actions.as_slice(), [Action::EditorAction { .. }]));
                run(&mut shell, &mut events, actions);
                let library: motion_core::ProjectLibrary =
                    serde_json::from_value(shell.state["project"]["library"].clone()).unwrap();
                assert_eq!(library.folder_of(&ProjectItem::Image { id: 2 }), None);
                shell.state = shell.engine.history(0).unwrap();
                assert_eq!(
                    shell.state["project"]["library"]["placements"],
                    serde_json::to_value(&project.library.placements).unwrap()
                );
                cases.push(serde_json::json!({"case":suffix,"move":true,"undo":true,"captures":6}));
            }
        }
    }
    shell.engine.save().unwrap();
    let saved_project = shell.state["project"].clone();
    drop(shell);
    let reopened =
        engine::Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], saved_project);
    std::fs::write(
        root.join("cases.json"),
        serde_json::to_vec_pretty(&cases).unwrap(),
    )
    .unwrap();
}
fn key_input(key: Key) -> Input {
    super::key(key)
}
