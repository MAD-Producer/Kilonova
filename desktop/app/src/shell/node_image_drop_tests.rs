//! Interactive acceptance uses a real external OLE source and native keyboard
//! input. It is opt-in because CI sessions need not have a foreground desktop.
use crate::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    fs,
    os::windows::process::CommandExt,
    process::{Child, Stdio},
    time::{Duration, Instant},
};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows},
};

struct Host {
    shell: Shell,
    drops: Vec<WindowId>,
    keys: usize,
    fronts: Vec<(u64, std::sync::mpsc::Receiver<Result<bool, String>>)>,
}
impl ApplicationHandler for Host {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.fronts.retain(|(_, reply)| match reply.try_recv() {
            Ok(result) => {
                result.unwrap();
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(error) => panic!("native fixture present reply: {error}"),
        });
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            let floating = self.shell.floating.contains_key(&id).then_some(id);
            let (window, scale, key) = if let Some(id) = floating {
                let f = &self.shell.floating[&id];
                (f.window.clone(), f.scale, f.key)
            } else {
                (
                    self.shell.window.as_ref().unwrap().clone(),
                    self.shell.scale,
                    0,
                )
            };
            if !self.fronts.iter().any(|(pending, _)| *pending == key) {
                let size = window.inner_size();
                let ui = frame(
                    self,
                    floating,
                    [size.width as f32 / scale, size.height as f32 / scale],
                );
                self.fronts
                    .push((key, self.shell.engine.present_async(ui).unwrap()));
            }
            return;
        }
        if matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::CloseRequested
        ) {
            return;
        }
        match &event {
            WindowEvent::HoveredFile(_)
            | WindowEvent::DroppedFile(_)
            | WindowEvent::Focused(_)
            | WindowEvent::MouseInput { .. } => {
                log::info!("native node fixture event window={id:?} event={event:?}")
            }
            _ => {}
        }
        if matches!(event, WindowEvent::DroppedFile(_)) {
            let editor = &self.shell.panels.node_editor;
            log::info!(
                "native node fixture phase=drop_geometry size={:?} canvas={:?} view={:?} scale={}",
                self.shell.size(),
                editor.canvas_bounds(),
                editor.view,
                self.shell.scale
            );
            self.drops.push(id);
        }
        if matches!(event, WindowEvent::KeyboardInput { .. }) {
            self.keys += 1;
        }
        self.shell.window_event(event_loop, id, event);
    }
}
fn handle(window: &Window) -> isize {
    let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
        panic!()
    };
    handle.hwnd.get()
}
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn ClientToScreen(window: *mut std::ffi::c_void, point: *mut Point) -> i32;
}
fn helper(
    script: &std::path::Path,
    mode: &str,
    value: &str,
    position: [i32; 2],
    window: &Window,
) -> Child {
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .args([
            "-Mode",
            mode,
            "-File",
            value,
            "-X",
            &position[0].to_string(),
            "-Y",
            &position[1].to_string(),
            "-Handle",
            &handle(window).to_string(),
        ])
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
fn native_keys(
    events: &mut EventLoop<()>,
    host: &mut Host,
    script: &std::path::Path,
    keys: &str,
    window: &Window,
) {
    let editor = &host.shell.panels.node_editor;
    let snapshot = editor.snapshot();
    let image = snapshot
        .document
        .nodes
        .iter()
        .find(|node| matches!(node.operation, motion_graph::NodeOp::ImageAsset { .. }))
        .unwrap();
    let point = editor.view.world_to_screen(
        // Activate the card body: clicking the header would start a node drag.
        [image.position[0] + 40., image.position[1] + 40.],
        editor.canvas_bounds(),
    );
    let mut screen = Point {
        x: point[0] as i32,
        y: point[1] as i32,
    };
    assert_ne!(
        unsafe { ClientToScreen(handle(window) as *mut _, &mut screen) },
        0
    );
    let child = helper(script, "keys", keys, [screen.x, screen.y], window);
    wait_child(events, host, child);
}
fn wait_child(events: &mut EventLoop<()>, host: &mut Host, mut child: Child) {
    let began = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if began.elapsed() >= Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("native input source timed out");
        }
        events.pump_app_events(Some(Duration::from_millis(5)), host);
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Drain the final OS input queued just before the helper returned.
    events.pump_app_events(Some(Duration::from_millis(20)), host);
}
fn draw(host: &mut Host, floating: Option<WindowId>, size: [f32; 2]) -> motion_ui::PaintList {
    let shell = &mut host.shell;
    let (paint, actions) = if let Some(id) = floating {
        let f = shell.floating.get_mut(&id).unwrap();
        let mut paint = PaintList::default();
        paint.stylesheet = shell.panels.styles.clone();
        let actions = panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            dock::Panel::Nodes,
            size,
            &mut shell.panels,
            &shell.state,
            &f.input,
        );
        f.input.begin_frame();
        (paint, actions)
    } else {
        let result = shell.draw_commands(size);
        shell.input.begin_frame();
        result
    };
    for action in actions {
        match action {
            panels::Action::Node(action) => shell.node_action(action).unwrap(),
            _ => panic!("native node command escaped its input owner"),
        }
    }
    paint
}
fn frame(host: &mut Host, floating: Option<WindowId>, size: [f32; 2]) -> engine::UiFrame {
    let paint = draw(host, floating, size);
    let shell = &mut host.shell;
    let config = shell.atlas.config();
    let area = if floating.is_some() {
        panels::floating_node_view_bound(dock::Panel::Nodes, size, false)
    } else {
        panels::node_view_bound(&shell.workspace.root, size, false)
    };
    engine::UiFrame { views: None,
        paint,
        atlas: Some(engine::AtlasUpload {
            pixels: shell.atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: size,
        composition: None,
        inspection: Default::default(),
        composition_image: None,
        window: if floating.is_some() { 1 } else { 0 },
        node_preview: shell.node_viewport(area, 1.),
    }
}
fn present_initial(
    events: &mut EventLoop<()>,
    host: &mut Host,
    floating: Option<WindowId>,
    size: [f32; 2],
) {
    let ui = frame(host, floating, size);
    let key = ui.window;
    log::info!("native node fixture phase=first_frame_submit window={key}");
    let reply = host.shell.engine.present_async(ui).unwrap();
    assert!(
        crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    log::info!("native node fixture phase=first_frame_presented window={key}");
}
fn capture(
    events: &mut EventLoop<()>,
    host: &mut Host,
    floating: Option<WindowId>,
    size: [f32; 2],
    path: PathBuf,
) {
    let ui = frame(host, floating, size);
    crate::native_test_support::capture_ui(events, &host.shell.engine, ui, path).unwrap();
}
fn install(events: &mut EventLoop<()>, host: &mut Host) {
    let shell = &mut host.shell;
    let snapshot = shell.panels.node_editor.snapshot();
    shell.engine.node_expect(snapshot.preview_stamp()).unwrap();
    shell
        .node_tasks
        .submit(
            node_editor::Action::Compile(snapshot),
            shell.workspace.locale,
            false,
        )
        .unwrap();
    let began = Instant::now();
    let ready = loop {
        assert!(began.elapsed() < Duration::from_secs(20));
        let (_, mut ready) = shell
            .node_tasks
            .poll(&mut shell.panels.node_editor, shell.workspace.locale);
        if let Some(ready) = ready.pop() {
            break ready;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let reply = shell
        .engine
        .node_install_bound(ready.snapshot, ready.compiled, ready.canonical)
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    shell.panels.node_preview.active = Some(shell.panels.node_editor.stamp());
}
#[test]
#[ignore = "requires an interactive Windows desktop for real external OLE file dragging and OS key input"]
fn native_image_drop_and_mapping_shortcut_work_in_both_window_owners_and_languages() {
    const CHILD: &str = "MOTION_NODE_NATIVE_DROP_CHILD";
    const NAME: &str = "shell::node_image_drop_tests::native_image_drop_and_mapping_shortcut_work_in_both_window_owners_and_languages";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_NODE_NATIVE_DROP_EVIDENCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        fs::create_dir_all(&root).unwrap();
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
        fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
        fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = logging::init(root.join("logs")).unwrap();
    let script = root.join("native-input.ps1");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/shell/node_image_drop_tests/native_input.ps1"),
        &script,
    )
    .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut rows = Vec::new();
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let suffix = format!(
                "{}-{}",
                if floating { "floating" } else { "main" },
                if locale == Locale::Zh { "zh" } else { "en" }
            );
            let size = [1280., 800.];
            #[allow(deprecated)]
            let window = Arc::new(
                events
                    .create_window(
                        Window::default_attributes()
                            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                            .with_title(format!("Motion Studio · {suffix}"))
                            .with_inner_size(PhysicalSize::new(1280, 800))
                            .with_position(PhysicalPosition::new(60, 60))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            let mut shell =
                Shell::new(root.join(format!("project-{suffix}")), None, Some(locale)).unwrap();
            shell.home = None;
            shell.scale = 1.;
            shell.window = Some(window.clone());
            shell.workspace.root = dock::Node::dock(dock::Panel::Nodes.dock());
            shell.panels.focused = Some(dock::Panel::Nodes);
            shell.panels.node_editor = node_editor::NodeEditor::new(crate::node_layer::identity());
            let reply = shell
                .engine
                .attach_async(window.clone(), 1280, 800)
                .unwrap();
            crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            let float_id = if floating {
                // The main window has a different handle/position; routing must use
                // the receiving floating client, never the main pointer cache.
                #[allow(deprecated)]
                let float = Arc::new(
                    events
                        .create_window(
                            Window::default_attributes()
                                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                                .with_title(format!("Motion Studio Nodes · {suffix}"))
                                .with_inner_size(PhysicalSize::new(1280, 800))
                                .with_position(PhysicalPosition::new(80, 80))
                                .with_visible(false),
                        )
                        .unwrap(),
                );
                shell
                    .engine
                    .float_window(1, float.clone(), 1280, 800)
                    .unwrap();
                let id = float.id();
                shell.floating.insert(
                    id,
                    FloatWindow {
                        window: float,
                        key: 1,
                        panel: dock::Panel::Nodes,
                        input: Default::default(),
                        paint_input: Default::default(),
                        pointer_position: Default::default(),
                        scale: 1.,
                    },
                );
                shell.workspace.root = dock::Node::dock(dock::Panel::Project.dock());
                Some(id)
            } else {
                None
            };
            let mut host = Host {
                shell,
                drops: vec![],
                keys: 0,
                fronts: Vec::new(),
            };
            events.pump_app_events(Some(Duration::from_millis(20)), &mut host);
            present_initial(&mut events, &mut host, float_id, size);
            install(&mut events, &mut host);
            present_initial(&mut events, &mut host, float_id, size);
            capture(
                &mut events,
                &mut host,
                float_id,
                size,
                root.join(format!("first-frame-{suffix}.png")),
            );
            let before = host.shell.panels.node_editor.snapshot();
            let canvas = host.shell.panels.node_editor.canvas_bounds();
            log::info!(
                "native node fixture phase=expected_geometry size={:?} canvas={canvas:?} view={:?}",
                host.shell.size(),
                host.shell.panels.node_editor.view
            );
            let logical = [
                ((canvas.min[0] + canvas.max[0]) * 0.5).floor(),
                ((canvas.min[1] + canvas.max[1]) * 0.5).floor(),
            ];
            let expected = host
                .shell
                .panels
                .node_editor
                .view
                .screen_to_world(logical, canvas);
            let target = float_id
                .map(|id| host.shell.floating[&id].window.clone())
                .unwrap_or(window.clone());
            target.set_visible(true);
            target.focus_window();
            events.pump_app_events(Some(Duration::from_millis(20)), &mut host);
            log::info!("native node fixture phase=window_visible owner={suffix}");
            let mut screen = Point {
                x: logical[0] as i32,
                y: logical[1] as i32,
            };
            assert_ne!(
                unsafe { ClientToScreen(handle(&target) as *mut _, &mut screen) },
                0
            );
            let source = root.join(format!("image-{suffix}.png"));
            fs::write(&source, crate::node_image_fixture::image().png()).unwrap();
            let child = helper(
                &script,
                "drop",
                source.to_str().unwrap(),
                [screen.x, screen.y],
                &target,
            );
            wait_child(&mut events, &mut host, child);
            assert_eq!(host.drops, vec![target.id()]);
            let began = Instant::now();
            while host.shell.node_tasks.busy() {
                assert!(began.elapsed() < Duration::from_secs(10));
                host.shell
                    .node_tasks
                    .poll(&mut host.shell.panels.node_editor, locale);
                events.pump_app_events(Some(Duration::from_millis(2)), &mut host);
            }
            assert_eq!(
                host.shell.actions.len(),
                0,
                "image must not also import as a project asset"
            );
            let imported = host.shell.panels.node_editor.snapshot();
            assert_eq!(
                imported.document.nodes.len(),
                before.document.nodes.len() + 1
            );
            assert_eq!(imported.document.images.len(), 1);
            let image = imported
                .document
                .nodes
                .iter()
                .find(|node| matches!(node.operation, motion_graph::NodeOp::ImageAsset { .. }))
                .unwrap();
            assert_eq!(
                image.position, expected,
                "drop must use the receiving client coordinate, not the cached cursor"
            );
            assert!(host.shell.panels.node_editor.focused());
            install(&mut events, &mut host);
            capture(
                &mut events,
                &mut host,
                float_id,
                size,
                root.join(format!("image-drop-{suffix}.png")),
            );
            native_keys(&mut events, &mut host, &script, "^{t}", &target);
            assert!(host.keys >= 4);
            draw(&mut host, float_id, size);
            let mapped = host.shell.panels.node_editor.snapshot();
            assert_eq!(
                mapped.document.nodes.len(),
                imported.document.nodes.len() + 3
            );
            assert!(
                mapped
                    .document
                    .nodes
                    .iter()
                    .any(|n| matches!(n.operation, motion_graph::NodeOp::UvMapping))
            );
            assert_eq!(mapped.revision, imported.revision + 1);
            install(&mut events, &mut host);
            let bounds = host.shell.panels.node_editor.canvas_bounds();
            host.shell.panels.node_editor.fit_all(bounds);
            capture(
                &mut events,
                &mut host,
                float_id,
                size,
                root.join(format!("image-mapping-{suffix}.png")),
            );
            native_keys(&mut events, &mut host, &script, "^z", &target);
            draw(&mut host, float_id, size);
            assert_eq!(
                host.shell.panels.node_editor.snapshot().document,
                imported.document
            );
            native_keys(&mut events, &mut host, &script, "^z", &target);
            draw(&mut host, float_id, size);
            assert_eq!(
                host.shell.panels.node_editor.snapshot().document,
                before.document
            );
            assert!(host.shell.panels.node_editor.redo());
            assert!(host.shell.panels.node_editor.redo());
            let saved = root.join(format!("{suffix}.msgraph"));
            host.shell
                .panels
                .node_editor
                .snapshot()
                .save(&saved)
                .unwrap();
            fs::remove_file(source).unwrap();
            assert_eq!(
                motion_graph::GraphDocument::load(&saved).unwrap(),
                mapped.document
            );
            rows.push(serde_json::json!({"case":suffix,"drops":host.drops.len(),"nativeKeys":host.keys,"undo":2,"images":mapped.document.images.len()}));
            target.set_visible(false);
            window.set_visible(false);
            drop(host);
            drop(target);
            drop(window);
            events.pump_app_events(Some(Duration::from_millis(10)), &mut Empty);
        }
    }
    fs::write(
        root.join("native-cases.json"),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
struct Empty;
impl ApplicationHandler for Empty {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
