//! Real native surfaces/GPU output; inputs are explicitly injected production
//! Input values, not physical OS foreground mouse/keyboard events.
use crate::*;
use motion_core::{Ease, Layer, Project};
use winit::platform::{pump_events::EventLoopExtPumpEvents, windows::EventLoopBuilderExtWindows};
mod curve_clipboard_native;

struct DrawFloat<'a> {
    shell: &'a mut Shell,
    id: WindowId,
    called: bool,
}
impl DrawFloat<'_> {
    fn draw(&mut self, events: &ActiveEventLoop) {
        if !self.called {
            self.called = true;
            self.shell.present_float(self.id, events).unwrap();
        }
    }
}
impl ApplicationHandler for DrawFloat<'_> {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        self.draw(e);
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
fn capture(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    paint: PaintList,
    size: [f32; 2],
    key: u64,
    path: PathBuf,
) {
    let mut frame = shell.ui_frame(paint, size, true);
    frame.window = key;
    frame.composition = None;
    frame.composition_image = None;
    frame.node_preview = None;
    crate::native_test_support::capture_ui(events, &shell.engine, frame, path.clone()).unwrap();
    assert!(image::open(path).unwrap().width() > 0);
}
#[test]
#[ignore = "manual native GPU screenshots; does not inject physical OS input"]
fn curve_comparison_native_main_and_float_pixels_preserve_authored_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_COMPARISON_GPU_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(
        7,
        "Curve study",
        [100., 100.],
        [100., 100., 0.],
        [0.25, 0.5, 0.8, 1.],
    );
    for (frame, x, y, o, ease) in [
        (0, 100., 100., 0.2, Ease::InOut),
        (30, 300., 200., 0.8, Ease::Out),
        (60, 150., 50., 0.5, Ease::In),
    ] {
        layer
            .transform
            .position
            .upsert(frame, [x, y, 0.], ease)
            .unwrap();
        layer.transform.opacity.upsert(frame, o, ease).unwrap();
    }
    project.layers.push(layer);
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    let pending = shell
        .engine
        .attach_async(window.clone(), 1600, 1000)
        .unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap();
    shell.window = Some(window.clone());
    shell.attached = true;
    let before = shell.state.clone();
    panels::seed_curve_comparison(&mut shell.panels, &shell.state);
    for (locale, lang) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        for (size, ratio) in [([1600., 1000.], 1.), ([900., 600.], 1.5)] {
            shell.scale = ratio;
            shell.atlas.set_pixel_ratio(ratio);
            let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
            let _ =
                window.request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
            let pending = shell
                .engine
                .resize_async(0, physical[0], physical[1])
                .unwrap();
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            for speed in [false, true] {
                shell.panels.timeline.speed_graph = speed;
                let (paint, actions) = shell.draw_commands(size);
                assert!(actions.is_empty());
                capture(
                    &mut shell,
                    &mut events,
                    paint,
                    size,
                    0,
                    root.join(format!(
                        "{lang}-{}-{}-ratio{ratio}.png",
                        size[0] as u32,
                        if speed { "speed" } else { "value" }
                    )),
                );
            }
            if size[0] == 900. {
                for (index, name) in [(2, "channels"), (1, "units")] {
                    let rect = panels::comparison_test_anchor(&shell.panels, index);
                    let point = [
                        (rect.min[0] + rect.max[0]) * 0.5,
                        (rect.min[1] + rect.max[1]) * 0.5,
                    ];
                    shell.input = Input {
                        mouse: point,
                        pressed: Some((motion_ui::input::MouseButton::Left, point)),
                        ..Default::default()
                    };
                    let (paint, actions) = shell.draw_commands(size);
                    assert!(actions.is_empty());
                    shell.input = Input::default();
                    capture(
                        &mut shell,
                        &mut events,
                        paint,
                        size,
                        0,
                        root.join(format!("{lang}-compact-{name}-ratio{ratio}.png")),
                    );
                    shell.input = Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    };
                    let (_, actions) = shell.draw_commands(size);
                    assert!(actions.is_empty());
                    shell.input = Input::default();
                }
            }
        }
    }
    shell.panels.locale = Locale::Zh;
    shell.workspace.locale = Locale::Zh;
    shell.scale = 1.5;
    #[allow(deprecated)]
    let float = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1080, 780)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, float.clone(), 1080, 780)
        .unwrap();
    let id = float.id();
    shell.floating.insert(
        id,
        FloatWindow {
            window: float,
            key: 1,
            panel: dock::Panel::Timeline,
            input: Default::default(),
            paint_input: Default::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    shell.present_due = false;
    let mut host = DrawFloat {
        shell: &mut shell,
        id,
        called: false,
    };
    let _ = events.pump_app_events(Some(Duration::from_millis(5)), &mut host);
    assert!(host.called);
    let mut paint = PaintList::default();
    paint.stylesheet = shell.panels.styles.clone();
    shell.atlas.set_pixel_ratio(1.5);
    assert!(panels::floating_chrome(
        &mut paint,
        &mut shell.atlas,
        dock::Panel::Timeline,
        [720., 520.],
        &mut shell.panels,
        &shell.state,
        &Input::default()
    )
    .is_empty());
    capture(
        &mut shell,
        &mut events,
        paint,
        [720., 520.],
        1,
        root.join("zh-native-float-ratio1.5.png"),
    );
    let after = shell.engine.state().unwrap();
    assert_eq!(before["project"], after["project"]);
    assert_eq!(before["history"], after["history"]);
    assert_eq!(before["revision"], after["revision"]);
    std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"actualNativeFloatPresent":true,"screenshots":13,"authoredProjectRevisionHistoryUnchanged":true,"physicalOsInput":"not tested"})).unwrap()).unwrap();
    shell.engine.request_shutdown().unwrap();
}
