use super::*;
use std::process::Command;
use winit::{
    dpi::PhysicalPosition,
    event::DeviceId,
    platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows},
};
const NATIVE:&str="plugin_editor::window::host::tests::hosted_author_routes_real_windows_and_preserves_drafts_on_owner_close";
#[test]
#[ignore = "explicit native author/host lifecycle acceptance; Windows GPU required"]
fn hosted_author_routes_real_windows_and_preserves_drafts_on_owner_close() {
    const CHILD: &str = "MOTION_PLUGIN_HOST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _log = crate::logging::init(root.join("logs")).unwrap();
        let file = root.join("plugin.msplugin");
        std::fs::write(
            &file,
            include_str!("../../../../crates/motion-graph/examples/parameter-workbench.msplugin"),
        )
        .unwrap();
        let mut events = EventLoop::<()>::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        let mut host = Harness {
            shell: crate::Shell::new(root.join("host-project"), None, Some(crate::Locale::Zh))
                .unwrap(),
            created: false,
            resumed_close: 0,
            queued: vec![],
        };
        pump(&mut events, &mut host, |h| h.created);
        let main = host.shell.window.as_ref().unwrap().id();
        let before = host.shell.engine.state().unwrap();
        let input = host.shell.input.clone();
        host.shell
            .plugin_author
            .open(Some(file.clone()), true)
            .unwrap();
        pump(&mut events, &mut host, |h| {
            h.shell
                .plugin_author
                .active
                .as_ref()
                .is_some_and(|o| o.app.frame_visible && o.app.author.is_some())
        });
        let id = host
            .shell
            .plugin_author
            .active
            .as_ref()
            .unwrap()
            .app
            .window
            .as_ref()
            .unwrap()
            .id();
        assert_ne!(main, id);
        assert_eq!(
            host.shell
                .plugin_author
                .active
                .as_ref()
                .unwrap()
                .app
                .embedded,
            true
        );
        capture(&mut events, &mut host, &root, "hosted-editor.png");
        // Production Shell::window_event must route this ID before its dock paths.
        click(&mut events, &mut host, id, [25., 52.]);
        click(&mut events, &mut host, id, [360., 125.]);
        host.send(
            id,
            WindowEvent::Ime(winit::event::Ime::Commit("// hosted edit\n".into())),
        );
        for _ in 0..8 {
            events.pump_app_events(Some(Duration::from_millis(2)), &mut host);
        }
        pump(&mut events, &mut host, |h| {
            h.shell
                .plugin_author
                .active
                .as_ref()
                .unwrap()
                .app
                .author
                .as_ref()
                .unwrap()
                .unsaved()
        });
        assert_eq!(host.shell.input.mouse, input.mouse);
        assert_eq!(host.shell.input.text, input.text);
        assert_eq!(host.shell.engine.state().unwrap(), before);
        assert!(host.shell.playback.is_none());
        host.shell.request_close();
        assert!(!host.shell.closing);
        assert!(host.shell.plugin_author.owner_close);
        pump(&mut events, &mut host, |h| {
            h.shell
                .plugin_author
                .active
                .as_ref()
                .unwrap()
                .app
                .author
                .as_ref()
                .unwrap()
                .close_prompt
        });
        capture(&mut events, &mut host, &root, "owner-close-prompt.png");
        let p = close_button(&host, 2);
        click(&mut events, &mut host, id, p);
        pump(&mut events, &mut host, |h| {
            !h.shell.plugin_author.owner_close
        });
        assert!(host.shell.plugin_author.active.is_some());
        assert!(!host.shell.closing);
        assert_eq!(host.resumed_close, 0);
        host.shell.request_close();
        // The real modal disables Save while compilation/file IO owns the source.
        pump(&mut events, &mut host, |h| {
            !h.shell
                .plugin_author
                .active
                .as_ref()
                .unwrap()
                .app
                .author
                .as_ref()
                .unwrap()
                .pending()
        });
        let p = close_button(&host, 0);
        click(&mut events, &mut host, id, p);
        pump(&mut events, &mut host, |h| h.resumed_close == 1);
        assert!(host.shell.plugin_author.active.is_none());
        assert!(!host.shell.plugin_author.busy);
        let saved = motion_graph::plugin_project::PluginProject::load(&file).unwrap();
        assert!(
            matches!(saved.logic,motion_graph::plugin_project::PluginLogic::Wgsl{document} if document.source.contains("// hosted edit"))
        );
        assert_eq!(host.shell.engine.state().unwrap(), before);
        // Child close must not request global event-loop exit or main close.
        host.shell
            .plugin_author
            .open(Some(file.clone()), true)
            .unwrap();
        pump(&mut events, &mut host, |h| {
            h.shell
                .plugin_author
                .active
                .as_ref()
                .is_some_and(|o| o.app.frame_visible && o.app.author.is_some())
        });
        let id = host
            .shell
            .plugin_author
            .active
            .as_ref()
            .unwrap()
            .app
            .window
            .as_ref()
            .unwrap()
            .id();
        host.send(id, WindowEvent::CloseRequested);
        pump(&mut events, &mut host, |h| {
            h.shell.plugin_author.active.is_none() && !h.shell.plugin_author.busy
        });
        assert_eq!(host.resumed_close, 1);
        assert!(!host.shell.closing);
        assert_eq!(host.shell.engine.state().unwrap(), before);
        // A real missing source stays local to the tool, and closes cleanly.
        host.shell
            .plugin_author
            .open(Some(root.join("missing.msplugin")), true)
            .unwrap();
        pump(&mut events, &mut host, |h| {
            h.shell
                .plugin_author
                .active
                .as_ref()
                .is_some_and(|o| o.app.error.is_some())
        });
        capture(&mut events, &mut host, &root, "hosted-read-error.png");
        let id = host
            .shell
            .plugin_author
            .active
            .as_ref()
            .unwrap()
            .app
            .window
            .as_ref()
            .unwrap()
            .id();
        host.send(id, WindowEvent::CloseRequested);
        pump(&mut events, &mut host, |h| {
            h.shell.plugin_author.active.is_none() && !h.shell.plugin_author.busy
        });
        assert_eq!(host.shell.engine.state().unwrap(), before);
        assert!(!host.shell.closing);
        std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"same_process":true,"different_window_ids":true,"production_shell_routing":true,"main_state_unchanged":true,"owner_close_cancel_retained_draft":true,"save_before_owner_close":true,"child_close_does_not_exit":true,"missing_source_isolated":true})).unwrap()).unwrap();
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_HOST_ACCEPTANCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NATIVE, "--ignored", "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_LOG", "info")
        .env_remove("MOTION_WGSL_ANALYZER")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("phase=read"));
    let logs = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    assert!(logs.contains("phase=read") && logs.contains("plugin.host start"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
}
struct Harness {
    shell: crate::Shell,
    created: bool,
    resumed_close: usize,
    queued: Vec<(WindowId, WindowEvent)>,
}
impl ApplicationHandler for Harness {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.created {
            return;
        }
        let window = events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_title("host fixture"),
            )
            .unwrap();
        self.shell.window = Some(Arc::new(window));
        self.created = true;
    }
    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // Pointer/IME assertions below inject explicit events through the actual
        // Shell router. Unrelated desktop pointer motion must not alter the fixture.
        if matches!(
            event,
            WindowEvent::RedrawRequested
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
        ) {
            self.shell.window_event(events, id, event);
        }
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        for (id, event) in std::mem::take(&mut self.queued) {
            self.shell.window_event(events, id, event);
        }
        if self.shell.plugin_author.poll(events) {
            self.resumed_close += 1;
        }
        events.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(4),
        ));
    }
}
impl Harness {
    fn send(&mut self, id: WindowId, event: WindowEvent) {
        self.queued.push((id, event));
    }
}
fn pump(events: &mut EventLoop<()>, host: &mut Harness, ready: impl Fn(&Harness) -> bool) {
    let start = Instant::now();
    while !ready(host) {
        events.pump_app_events(Some(Duration::from_millis(2)), host);
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "host fixture timeout; message={:?}",
            host.shell.plugin_author.message
        );
    }
}
fn click(events: &mut EventLoop<()>, host: &mut Harness, id: WindowId, p: [f32; 2]) {
    host.send(
        id,
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(p[0] as f64, p[1] as f64),
        },
    );
    host.send(
        id,
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Pressed,
            button: WinitButton::Left,
        },
    );
    for _ in 0..8 {
        events.pump_app_events(Some(Duration::from_millis(2)), host);
    }
    host.send(
        id,
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Released,
            button: WinitButton::Left,
        },
    );
    for _ in 0..8 {
        events.pump_app_events(Some(Duration::from_millis(2)), host);
    }
}
fn close_button(host: &Harness, index: usize) -> [f32; 2] {
    let app = &host.shell.plugin_author.active.as_ref().unwrap().app;
    let sheet = &app.styles;
    let size = app.window.as_ref().unwrap().inner_size();
    let width = size.width as f32 / app.scale;
    let height = size.height as f32 / app.scale;
    let pad = sheet.metric("padding.panel", 8.);
    let row = sheet.metric("panel.header.height", 24.);
    let action = sheet.metric("plugin.author.action.width", 80.);
    let w = (action + pad) * 3. + pad;
    [
        pad + (width - pad * 2. - w) * 0.5 + pad + index as f32 * (action + pad) + action * 0.5,
        pad + (height - pad * 2.) * 0.5 + row * 1.5 + pad,
    ]
}
fn capture(events: &mut EventLoop<()>, host: &mut Harness, root: &Path, name: &str) {
    let app = &mut host.shell.plugin_author.active.as_mut().unwrap().app;
    let mut frame = app.draw().unwrap();
    let cfg = app.atlas.config();
    frame.atlas = Some(AtlasUpload {
        pixels: app.atlas.pixel_data().to_vec(),
        width: cfg.width,
        height: cfg.height,
    });
    let receive = app
        .engine
        .request(|reply| crate::engine::Command::CaptureUi {
            ui: frame,
            path: root.join(name),
            reply,
        })
        .unwrap();
    let start = Instant::now();
    loop {
        match receive.try_recv() {
            Ok(r) => {
                r.unwrap();
                break;
            }
            Err(TryRecvError::Empty) => {
                events.pump_app_events(Some(Duration::from_millis(2)), host);
            }
            Err(e) => panic!("capture: {e}"),
        };
        assert!(start.elapsed() < Duration::from_secs(60));
    }
}
#[test]
#[ignore = "explicit lifecycle failure-channel acceptance"]
fn host_worker_publishes_blocked_state_and_reports_panics_at_default_level() {
    const CHILD: &str = "MOTION_PLUGIN_HOST_FAILURE_CHILD";
    const TEST:&str="plugin_editor::window::host::tests::host_worker_publishes_blocked_state_and_reports_panics_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _log = crate::logging::init(root.join("logs")).unwrap();
        let mut tool = Hosted::default();
        tool.worker = Some(Worker::new().unwrap());
        let (entered, entry) = mpsc::sync_channel(1);
        let (release, gate) = mpsc::sync_channel(1);
        assert!(tool
            .submit(
                Job::Block {
                    entered,
                    release: gate
                },
                "controlled_block"
            )
            .is_ok());
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(tool.busy);
        let worker = tool.worker.as_ref().unwrap();
        let guard = worker.flight.lock().unwrap();
        let state = guard.as_ref().unwrap();
        assert_eq!(state.phase, "controlled_block");
        assert_eq!(state.request, 1);
        let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(file.contains("plugin.host start request=1 phase=controlled_block"));
        assert!(matches!(worker.reply.try_recv(), Err(TryRecvError::Empty)));
        drop(guard);
        release.send(()).unwrap();
        assert!(worker
            .reply
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_ok());
        tool.busy = false;
        assert!(tool.submit(Job::Panic, "controlled_panic").is_ok());
        let result = tool
            .worker
            .as_ref()
            .unwrap()
            .reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(matches!(result,Err(e) if e.contains("original cause")));
        assert!(tool
            .worker
            .as_ref()
            .unwrap()
            .flight
            .lock()
            .unwrap()
            .is_none());
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(
            file.contains("controlled author window panic")
                && file.contains("plugin.host error request=2 phase=controlled_panic")
        );
        // A poisoned observation store must reject dispatch with a visible cause.
        // This is a distinct fault from the handler panic above; do not restart it.
        let flight = tool.worker.as_ref().unwrap().flight.clone();
        let poison = std::panic::catch_unwind(move || {
            let _guard = flight.lock().unwrap();
            panic!("controlled author in-flight poison");
        });
        assert!(poison.is_err());
        tool.busy = false;
        assert!(tool.submit(Job::Panic, "rejected_poison").is_err());
        assert!(tool.failed);
        assert!(tool.message.as_ref().unwrap().contains("state poisoned"));
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(file.contains("state poisoned; job not dispatched"));
        assert!(!file.contains("phase=rejected_poison in_flight=true"));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_HOST_FAILURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--ignored", "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_LOG", "info")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("controlled author window panic") && stderr.contains("plugin.host error")
    );
    assert!(stderr.contains("state poisoned; job not dispatched"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
}
