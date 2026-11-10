//! Native event routing before action generation, then real scheduling recovery.
use super::*;
use crate::{dock, Locale, Shell};
use winit::{
    application::ApplicationHandler,
    event::{DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    platform::pump_events::EventLoopExtPumpEvents,
    window::WindowId,
};

const SIZE: [f32; 2] = [1280., 800.];

struct Input<'a> {
    shell: &'a mut Shell,
    event: Option<WindowEvent>,
}
impl Input<'_> {
    fn send(&mut self, active: &ActiveEventLoop) {
        if let Some(event) = self.event.take() {
            let id = self.shell.window.as_ref().unwrap().id();
            self.shell.window_event(active, id, event);
        }
    }
}
impl ApplicationHandler for Input<'_> {
    fn resumed(&mut self, active: &ActiveEventLoop) {
        self.send(active);
    }
    fn about_to_wait(&mut self, active: &ActiveEventLoop) {
        self.send(active);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
fn feed(events: &mut EventLoop<()>, shell: &mut Shell, event: WindowEvent) {
    let mut driver = Input {
        shell,
        event: Some(event),
    };
    let began = Instant::now();
    while driver.event.is_some() {
        events.pump_app_events(Some(Duration::ZERO), &mut driver);
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "input routing deadline"
        );
    }
}
fn button(pressed: bool) -> WindowEvent {
    WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        },
        button: MouseButton::Left,
    }
}
fn observer_requests_redraw(events: &mut EventLoop<()>, shell: &Shell) {
    struct Redraws {
        id: WindowId,
        count: usize,
    }
    impl ApplicationHandler for Redraws {
        fn resumed(&mut self, _: &ActiveEventLoop) {}
        fn window_event(&mut self, _: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            if id == self.id && matches!(event, WindowEvent::RedrawRequested) {
                self.count += 1;
            }
        }
    }
    let mut redraws = Redraws {
        id: shell.window.as_ref().unwrap().id(),
        count: 0,
    };
    // Drain redraws from the preceding real presentation. No Shell event
    // translation or action dispatch is run in this observer-only control.
    events.pump_app_events(Some(Duration::from_millis(10)), &mut redraws);
    let before = redraws.count;
    shell.observe_prerender_input(&WindowEvent::Ime(winit::event::Ime::Commit(String::new())));
    assert!(shell.engine.prerender_signals.input_pending().unwrap());
    let began = Instant::now();
    while redraws.count == before {
        events.pump_app_events(Some(Duration::from_millis(5)), &mut redraws);
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "input observer must request its acknowledgment redraw without UI translation"
        );
    }
}
fn show(events: &mut EventLoop<()>, shell: &mut Shell) {
    let began = Instant::now();
    loop {
        let (paint, actions) = shell.draw_commands(SIZE);
        assert!(
            actions.is_empty(),
            "control must not discard editor actions"
        );
        let ui = shell.ui_frame(paint, SIZE, false);
        assert!(ui.composition.is_some());
        if receive(events, &shell.present_ui_async(ui).unwrap()) {
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "native Present deadline"
        );
    }
}
fn wait_for(engine: &Engine, phase: &str, ready: impl Fn(&Value) -> bool) -> Value {
    let began = Instant::now();
    loop {
        let state = engine.state().unwrap();
        let status = &state["desktop"]["prerender"];
        assert!(status["fault"].is_null(), "{state}");
        if ready(status) {
            return status.clone();
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "{phase}: {state}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "explicit native event routing, blocked-worker preemption and genuine warmup recovery"]
fn native_input_preempts_before_action_generation_and_waits_for_its_present() {
    const NAME: &str = "engine::prerender::tests::input_priority::native_input_preempts_before_action_generation_and_waits_for_its_present";
    const CHILD: &str = "MOTION_PRERENDER_PRIORITY_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRERENDER_PRIORITY_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "child status={}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let directory = root.join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Input priority control",
        [16.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    // This fixture verifies actual OS redraw delivery as well
                    // as retained rendering; keep its native window visible.
                    .with_visible(true)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
            )
            .unwrap(),
    );
    let mut shell = Shell::new(directory, None, Some(Locale::En)).unwrap();
    shell.home = None;
    shell.window = Some(window.clone());
    shell.scale = 1.;
    shell.attached = true;
    shell.workspace.root = dock::Node::dock(dock::Panel::Composition.dock());
    receive(
        &mut events,
        &shell.engine.attach_async(window, 1280, 800).unwrap(),
    );
    receive(
        &mut events,
        &shell
            .engine
            .request(|reply| Command::TestStudy {
                operation: |id| {
                    motion_host::session::with_session(id, |s| {
                        s.preview.set_mode(motion_render::PreviewMode::High);
                        Ok(serde_json::json!({}))
                    })
                },
                reply,
            })
            .unwrap(),
    );
    show(&mut events, &mut shell);
    // A blank corner avoids producing an editor action. The owning worker is
    // deliberately blocked: routing the event cannot rely on its next handler.
    feed(
        &mut events,
        &mut shell,
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(1279., 799.),
        },
    );
    let (entered_tx, entered) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let pending = shell
        .engine
        .request(|reply| Command::TestWait {
            entered: entered_tx,
            release: release_rx,
            reply,
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let signals = shell.engine.prerender_signals.clone();
    // Publish AFTER TestWait's dispatch, so only the native event can preempt
    // this fresh production cancellation token. This phase is a controlled
    // token proof, not a claim that the blocked worker is rendering a frame.
    let active = motion_desktop_media::movie::prerender::worker::YieldControl::default();
    let ticket = signals
        .publish_yield(signals.generation().unwrap(), active.clone())
        .unwrap()
        .unwrap();
    feed(
        &mut events,
        &mut shell,
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: winit::dpi::PhysicalPosition::new(1279., 799.),
        },
    );
    assert!(active.control().check().is_ok());
    feed(&mut events, &mut shell, button(true));
    assert!(
        active.control().check().is_err(),
        "input must preempt without waiting for action generation or Engine dispatch"
    );
    assert_eq!(
        active.control().check().unwrap_err(),
        "movie export cancelled",
        "input must preempt without waiting for action generation or Engine dispatch"
    );
    assert!(signals.input_pending().unwrap());
    assert_eq!(shell.engine.queue_depth(), 0);
    assert_eq!(shell.engine.in_flight().unwrap().0, "test_wait");
    signals.finish(ticket).unwrap();
    feed(&mut events, &mut shell, button(false));
    assert!(
        signals.input_pending().unwrap(),
        "release alone cannot reopen admission"
    );
    release.send(()).unwrap();
    receive(&mut events, &pending);
    show(&mut events, &mut shell);
    assert!(!signals.input_pending().unwrap());

    // Separate from the token control: a real native producer must warm,
    // stay idle during a gesture, and recover after the actual foreground frame.
    let policy = motion_host::prerender::Policy {
        enabled: true,
        window: 2,
        idle_delay: Duration::ZERO,
        ..Default::default()
    };
    shell.engine.configure_prerender(policy).unwrap();
    let warmed = wait_for(&shell.engine, "initial warmup", |s| {
        s["cache"]["resident_frames"] == 2 && s["pending"] == 0
    });
    feed(&mut events, &mut shell, button(true));
    let held = wait_for(&shell.engine, "held input", |s| {
        s["hold"] == "input" && s["pending"] == 0
    });
    let started = held["started"].as_u64().unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let still_held = shell.engine.state().unwrap()["desktop"]["prerender"].clone();
    assert_eq!(still_held["started"], started);
    assert_eq!(still_held["inputPending"], true);
    show(&mut events, &mut shell);
    assert!(
        signals.input_pending().unwrap(),
        "a presented frame cannot end a held gesture"
    );
    feed(&mut events, &mut shell, button(false));
    assert!(signals.input_pending().unwrap());
    show(&mut events, &mut shell);
    assert!(!signals.input_pending().unwrap());
    shell.engine.configure_prerender(policy).unwrap();
    let resumed = wait_for(&shell.engine, "resumed warmup", |s| {
        s["cache"]["resident_frames"] == 2
            && s["pending"] == 0
            && s["started"].as_u64().unwrap() > started
    });
    shell
        .engine
        .configure_prerender(Default::default())
        .unwrap();
    observer_requests_redraw(&mut events, &shell);
    show(&mut events, &mut shell);
    assert!(!signals.input_pending().unwrap());
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "preemptedBeforeActionGeneration":true, "blockedQueueDepth":0,
        "observerRequestedNativeRedrawWithoutTranslation":true,
        "warmed":warmed,"held":held,"stillHeld":still_held,"resumed":resumed,
        "limits":"Controlled cancellation token plus separate actual native warmup; not whole-application P95 or native crash causality."
    })).unwrap()).unwrap();
}
