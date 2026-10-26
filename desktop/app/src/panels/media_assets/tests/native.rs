//! Real native surfaces and production input routing, with controlled Input.
//! This does not inject foreground OS keyboard events or open a native picker.
use super::*;
use crate::{engine, native_test_support, FloatWindow, Shell};
use std::{path::PathBuf, sync::Arc, time::Duration};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows},
    window::{Window, WindowId},
};

fn draw_main(shell: &mut Shell, size: [f32; 2], input: Input) -> (PaintList, Vec<Action>) {
    shell.input = input;
    let result = shell.draw_commands(size);
    shell.input = Input::default();
    result
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    window: u64,
    path: PathBuf,
    scale: f32,
) {
    let mut ui = shell.ui_frame(paint, size, true);
    ui.window = window;
    ui.composition = None;
    ui.composition_image = None;
    ui.node_preview = None;
    native_test_support::capture_ui(events, &shell.engine, ui, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        ((size[0] * scale) as u32, (size[1] * scale) as u32)
    );
}
struct FloatDraw<'a> {
    shell: &'a mut Shell,
    id: WindowId,
    called: bool,
}
impl ApplicationHandler for FloatDraw<'_> {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: winit::event::WindowEvent) {}
}
impl FloatDraw<'_> {
    fn draw(&mut self, e: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, e).unwrap();
        }
    }
}
fn float_draw(shell: &mut Shell, events: &mut EventLoop<()>, id: WindowId, input: Input) {
    shell.floating.get_mut(&id).unwrap().input = input;
    shell.present_due = false;
    let mut host = FloatDraw {
        shell,
        id,
        called: false,
    };
    events.pump_app_events(Some(Duration::ZERO), &mut host);
    assert!(host.called);
    if let Some(pending) = shell.rendering.remove(&1) {
        native_test_support::receive(events, &pending, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
}

#[test]
#[ignore = "manual native GPU capture and transaction evidence"]
fn asset_actions_native_menus_focus_replacement_removal_and_reopen() {
    const NAME:&str="panels::media_assets::tests::native::asset_actions_native_menus_focus_replacement_removal_and_reopen";
    const CHILD: &str = "MOTION_MEDIA_ASSET_UI_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_MEDIA_ASSET_UI_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
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
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    let project_root = root.join("project");
    std::fs::create_dir_all(project_root.join("assets")).unwrap();
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.name = "素材操作 · Asset study".into();
    for (id, file, color) in [
        (1, "reference.png", [30, 85, 145, 255]),
        (2, "unused-texture.png", [90, 70, 45, 255]),
        (3, "spare-plate.png", [45, 110, 70, 255]),
    ] {
        let path = format!("assets/{file}");
        image::RgbaImage::from_pixel(32, 32, image::Rgba(color))
            .save(project_root.join(&path))
            .unwrap();
        project.assets.push(Asset {
            id,
            path,
            width: 32,
            height: 32,
        });
    }
    for id in [7, 8] {
        let mut layer = Layer::solid(
            id,
            if id == 7 {
                "参考图层 Reference"
            } else {
                "背景输入 Background"
            },
            [32.; 2],
            [160., 100., 0.],
            [1.; 4],
        );
        layer.content = Content::Image { asset: 1 };
        project.layers.push(layer);
    }
    motion_core::storage::save(&project_root, &project).unwrap();
    let mut shell = Shell::new(project_root.clone(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    shell.workspace.root = Node::Split {
        edge: dock::Edge::Left,
        split: 0.36,
        first: Box::new(Node::dock(Panel::Project.dock())),
        second: Box::new(Node::dock(Panel::Composition.dock())),
    };
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
            )
            .unwrap(),
    );
    let pending = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window.clone());
    shell.attached = true;
    let baseline = shell.state.clone();
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        for size in [[900., 600.], [1600., 1000.]] {
            for scale in [1., 1.5, 2.] {
                shell.scale = scale;
                let physical = [(size[0] * scale) as u32, (size[1] * scale) as u32];
                let _ = window
                    .request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
                let pending = shell
                    .engine
                    .resize_async(0, physical[0], physical[1])
                    .unwrap();
                native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                assert!(draw_main(&mut shell, size, Input::default()).1.is_empty());
                let rect = shell
                    .panels
                    .media_assets
                    .rows
                    .iter()
                    .find(|(target, _)| target.id == 1)
                    .unwrap()
                    .1;
                let (paint, actions) = draw_main(&mut shell, size, press(rect, MouseButton::Right));
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    0,
                    root.join(format!(
                        "{lang}-{}-ratio{scale}-referenced-menu.png",
                        size[0] as u32
                    )),
                    scale,
                );
                // Space/Delete are consumed by the open menu, preserving authored state.
                for key in [Key::Space, Key::Delete] {
                    assert!(draw_main(
                        &mut shell,
                        size,
                        Input {
                            keys: vec![(key, Default::default())],
                            ..Default::default()
                        }
                    )
                    .1
                    .is_empty());
                }
                assert_eq!(shell.state["project"], baseline["project"]);
                assert!(draw_main(
                    &mut shell,
                    size,
                    Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    }
                )
                .1
                .is_empty());
            }
        }
    }
    shell.scale = 1.;
    shell.workspace.locale = Locale::Zh;
    shell.panels.locale = Locale::Zh;
    let size = [900., 600.];
    let pending = shell.engine.resize_async(0, 900, 600).unwrap();
    native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    draw_main(&mut shell, size, Input::default());
    let row = shell
        .panels
        .media_assets
        .rows
        .iter()
        .find(|(t, _)| t.id == 1)
        .unwrap()
        .1;
    draw_main(&mut shell, size, press(row, MouseButton::Right));
    let usages = shell.panels.media_assets.menu_rows[2];
    draw_main(&mut shell, size, press(usages, MouseButton::Left));
    let (paint, actions) = draw_main(&mut shell, size, Input::default());
    assert!(actions.is_empty());
    capture(
        &mut shell,
        &mut events,
        paint,
        size,
        0,
        root.join("zh-usages-menu.png"),
        1.,
    );
    let use_rect = shell.panels.media_assets.menu_rows[2];
    let (_, mut actions) = draw_main(&mut shell, size, press(use_rect, MouseButton::Left));
    assert_eq!(actions.len(), 1);
    shell.action(actions.remove(0), None).unwrap();
    assert_eq!(shell.panels.selected, Some(8));
    assert_eq!(shell.state["revision"], baseline["revision"]);

    // Render and route the very same component through a real native Media float.
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(900, 630)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, float.clone(), 900, 630)
        .unwrap();
    let id = float.id();
    shell.floating.insert(
        id,
        FloatWindow {
            window: float,
            key: 1,
            panel: Panel::Media,
            input: Default::default(),
            paint_input: Default::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    float_draw(&mut shell, &mut events, id, Input::default());
    let row = shell
        .panels
        .media_assets
        .rows
        .iter()
        .find(|(t, _)| t.id == 2)
        .unwrap()
        .1;
    float_draw(&mut shell, &mut events, id, press(row, MouseButton::Right));
    assert!(shell.panels.media_assets.open());
    assert!(shell.actions.is_empty());
    float_draw(
        &mut shell,
        &mut events,
        id,
        Input {
            keys: vec![(Key::Space, Default::default())],
            ..Default::default()
        },
    );
    assert!(shell.actions.is_empty());
    assert!(shell.playback.is_none());
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    shell.atlas.set_pixel_ratio(1.5);
    assert!(floating_chrome(
        &mut paint,
        &mut shell.atlas,
        Panel::Media,
        [600., 420.],
        &mut shell.panels,
        &shell.state,
        &Input::default()
    )
    .is_empty());
    capture(
        &mut shell,
        &mut events,
        paint,
        [600., 420.],
        1,
        root.join("zh-float-media-orphan-menu.png"),
        1.5,
    );
    // Repainting the main window does not paint or consume a foreign popup.
    let (paint, actions) = draw_main(&mut shell, size, Input::default());
    assert!(actions.is_empty());
    assert!(shell.panels.media_assets.open());
    capture(
        &mut shell,
        &mut events,
        paint,
        size,
        0,
        root.join("zh-main-while-float-menu.png"),
        1.,
    );
    let remove = shell.panels.media_assets.menu_rows[3];
    float_draw(
        &mut shell,
        &mut events,
        id,
        press(remove, MouseButton::Left),
    );
    assert_eq!(shell.actions.len(), 1);
    let action = shell.actions.pop_front().unwrap();
    shell.action(action, None).unwrap();
    assert_eq!(
        shell.state["project"]["assets"].as_array().unwrap().len(),
        2
    );
    assert!(project_root.join("assets/unused-texture.png").is_file());
    shell.state = shell.engine.history(0).unwrap();
    assert_eq!(shell.state["project"], baseline["project"]);
    shell.state = shell.engine.history(1).unwrap();
    shell.engine.save().unwrap();

    let source = Source::read(
        &shell.state,
        AssetRef {
            kind: AssetKind::Image,
            id: 1,
        },
    )
    .unwrap();
    let replacement = root.join("new-reference.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([20, 170, 80, 255]))
        .save(&replacement)
        .unwrap();
    shell
        .replace_asset_file(source.clone(), replacement)
        .unwrap();
    let (_, reply) = shell
        .importing_image
        .as_ref()
        .map(|reply| ((), reply))
        .unwrap();
    // Pump while the real decoder/renderer owns the request; poll_import consumes it.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let _ = reply;
    while shell.importing_image.is_some() {
        shell.poll_import().unwrap();
        assert!(std::time::Instant::now() < deadline);
        events.pump_app_events(Some(Duration::from_millis(1)), &mut Empty);
    }
    assert!(shell.replacing_asset.is_none());
    assert!(shell
        .panels
        .message
        .as_ref()
        .unwrap()
        .contains("素材已替换"));
    assert_eq!(
        shell.state["project"]["layers"],
        baseline["project"]["layers"]
    );
    shell.engine.save().unwrap();
    let saved = shell.state["project"].clone();
    shell
        .replace_asset_file(source, root.join("missing.png"))
        .unwrap();
    assert!(shell.importing_image.is_none());
    assert_eq!(shell.state["project"], saved);
    shell.engine.request_shutdown().unwrap();
    drop(shell);
    let reopened =
        engine::Engine::start(project_root, motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], saved);
    reopened.request_shutdown().unwrap();
    std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"actualNativeFloat":true,"bilingualMenus":true,"ratios":[1,1.5,2],"referenceRemovalBlocked":true,"orphanRemoveUndoRedo":true,"replacement":true,"staleCancelled":true,"saveReopen":true,"input":"controlled Input; no OS foreground injection or native picker automation"})).unwrap()).unwrap();
}
struct Empty;
impl ApplicationHandler for Empty {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: winit::event::WindowEvent) {}
}
