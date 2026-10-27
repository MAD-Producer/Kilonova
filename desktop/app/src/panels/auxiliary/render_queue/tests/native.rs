//! Actual native-window/GPU evidence; controlled chrome Input, not foreground OS IME injection.
use super::*;
use crate::{native_test_support, Shell};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows, window::Window};

fn poll(shell: &mut Shell, condition: impl Fn(&Shell) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        shell.poll_render_queue();
        if condition(shell) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "queue did not complete: {:?}",
            shell.panels.render_queue.error
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn draw(shell: &mut Shell, input: Input) -> PaintList {
    shell.input = input;
    let (paint, actions) = shell.draw_commands([900., 600.]);
    shell.input = Input::default();
    for action in actions {
        shell.action(action, None).unwrap();
    }
    paint
}
fn capture(shell: &mut Shell, events: &mut EventLoop<()>, path: PathBuf) {
    let paint = draw(shell, Input::default());
    let frame = shell.ui_frame(paint, [900., 600.], true);
    native_test_support::capture_ui(events, &shell.engine, frame, path.clone()).unwrap();
    let pixels = image::open(path).unwrap().to_rgba8();
    assert_eq!(pixels.dimensions(), (900, 600));
    assert!(pixels.pixels().any(|p| p[0] > 150));
}
fn capture_scaled(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    path: PathBuf,
    size: [f32; 2],
    scale: f32,
) {
    let physical =
        winit::dpi::PhysicalSize::new((size[0] * scale) as u32, (size[1] * scale) as u32);
    let _ = shell.window.as_ref().unwrap().request_inner_size(physical);
    let reply = shell
        .engine
        .resize_async(0, physical.width, physical.height)
        .unwrap();
    shell.state = native_test_support::receive(events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.scale = scale;
    shell.atlas.set_pixel_ratio(scale);
    shell.input = Input::default();
    let (paint, actions) = shell.draw_commands(size);
    assert!(actions.is_empty(), "{actions:?}");
    let frame = shell.ui_frame(paint, size, true);
    native_test_support::capture_ui(events, &shell.engine, frame, path.clone()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (physical.width, physical.height)
    );
}
#[test]
#[ignore = "explicit native render queue UI, frozen exports and shared-window input evidence"]
fn render_queue_native_tasks_templates_and_actual_output() {
    const CHILD: &str = "MOTION_RENDER_QUEUE_NATIVE_CHILD";
    const NAME:&str="panels::auxiliary::render_queue::tests::native::render_queue_native_tasks_templates_and_actual_output";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_RENDER_QUEUE_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
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
    let mut project = motion_core::Project::new(32, 24, 30, 3).unwrap();
    project.name = "Queue study".into();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Frozen source",
        [32., 24.],
        [16., 12., 0.],
        [0.15, 0.45, 0.8, 1.],
    ));
    motion_core::storage::save(&root.join("project"), &project).unwrap();
    let mut shell = Shell::new(root.join("project"), None, Some(Locale::Zh)).unwrap();
    shell.render_queue.open();
    poll(&mut shell, |s| {
        s.panels.render_queue.view.as_ref().is_some_and(|v| v.ready)
    });
    for index in 0..2 {
        let template = shell
            .panels
            .render_queue
            .view
            .as_ref()
            .unwrap()
            .document
            .templates[0]
            .clone();
        shell
            .render_queue
            .freeze(
                &shell.engine,
                &shell.state,
                template,
                root.join(format!("frozen-{index}.mp4")),
            )
            .unwrap();
        poll(&mut shell, |s| {
            s.panels
                .render_queue
                .view
                .as_ref()
                .unwrap()
                .document
                .jobs
                .len()
                == index + 1
                && !s.panels.render_queue.pending
        });
        shell.state = shell
            .engine
            .edit(
                serde_json::to_string(&motion_core::Command::Rename {
                    object: 1,
                    name: format!("Later name {index}"),
                })
                .unwrap(),
            )
            .unwrap();
    }
    let baseline = shell.state["project"].clone();
    let revisions: Vec<_> = shell
        .panels
        .render_queue
        .view
        .as_ref()
        .unwrap()
        .document
        .jobs
        .iter()
        .map(|j| j.source.revision)
        .collect();
    assert!(revisions[0] < revisions[1]);
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
    let reply = shell.engine.attach_async(window.clone(), 900, 600).unwrap();
    shell.state = native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(450, 690)),
            )
            .unwrap(),
    );
    shell
        .engine
        .float_window(1, floating.clone(), 450, 690)
        .unwrap();
    shell.floating.insert(
        floating.id(),
        crate::FloatWindow {
            window: floating,
            key: 1,
            panel: Panel::RenderQueue,
            input: Input::default(),
            paint_input: Input::default(),
            pointer_position: Default::default(),
            scale: 1.5,
        },
    );
    // A valid one-panel maximized dock exercises the production application chrome.
    shell.workspace.root = dock::Node::dock(Panel::RenderQueue.dock());
    shell.panels.focused = Some(Panel::RenderQueue);
    for (locale, language) in [(Locale::Zh, "zh"), (Locale::En, "en")] {
        shell.workspace.locale = locale;
        shell.panels.locale = locale;
        capture(
            &mut shell,
            &mut events,
            root.join(format!("{language}-queue.png")),
        );
        let rect = shell
            .panels
            .auxiliary
            .render_queue
            .actions
            .iter()
            .find(|(name, _)| *name == "edit_template")
            .unwrap()
            .1;
        draw(&mut shell, input(rect));
        draw(&mut shell, Input::default());
        assert!(shell.panels.auxiliary.render_queue.draft.is_some());
        capture(
            &mut shell,
            &mut events,
            root.join(format!("{language}-template.png")),
        );
        let field = shell.panels.auxiliary.render_queue.fields[0];
        draw(&mut shell, input(field));
        draw(
            &mut shell,
            Input {
                text: format!("{language} export preset"),
                ..Default::default()
            },
        );
        draw(
            &mut shell,
            Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        );
        let save = shell
            .panels
            .auxiliary
            .render_queue
            .actions
            .iter()
            .find(|(name, _)| *name == "save_template")
            .unwrap()
            .1;
        draw(&mut shell, input(save));
        poll(&mut shell, |s| !s.panels.render_queue.pending);
        assert_eq!(
            shell
                .panels
                .render_queue
                .view
                .as_ref()
                .unwrap()
                .document
                .templates[0]
                .name,
            format!("{language} export preset")
        );
        shell.panels.auxiliary.render_queue.draft = None;
        shell.atlas.set_pixel_ratio(1.5);
        let mut paint = PaintList::default();
        let actions = crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::RenderQueue,
            [300., 460.],
            &mut shell.panels,
            &shell.state,
            &Input::default(),
        );
        assert!(actions.is_empty(), "{actions:?}");
        let mut frame = shell.ui_frame(paint, [300., 460.], true);
        frame.window = 1;
        frame.composition = None;
        frame.composition_image = None;
        frame.node_preview = None;
        let path = root.join(format!("{language}-floating-dpi15.png"));
        native_test_support::capture_ui(&mut events, &shell.engine, frame, path.clone()).unwrap();
        let baseline = image::open(path).unwrap().to_rgba8();
        assert_eq!(baseline.dimensions(), (450, 690));
        let detail = shell.panels.auxiliary.render_queue.details[2];
        let hover = Input {
            mouse: [
                (detail.min[0] + detail.max[0]) * 0.5,
                (detail.min[1] + detail.max[1]) * 0.5,
            ],
            ..Default::default()
        };
        let mut paint = PaintList::default();
        assert!(crate::panels::floating_chrome(
            &mut paint,
            &mut shell.atlas,
            Panel::RenderQueue,
            [300., 460.],
            &mut shell.panels,
            &shell.state,
            &hover,
        )
        .is_empty());
        let mut frame = shell.ui_frame(paint, [300., 460.], true);
        frame.window = 1;
        frame.composition = None;
        frame.composition_image = None;
        frame.node_preview = None;
        let path = root.join(format!("{language}-floating-tooltip.png"));
        native_test_support::capture_ui(&mut events, &shell.engine, frame, path.clone()).unwrap();
        let tooltip = image::open(path).unwrap().to_rgba8();
        assert!(
            baseline
                .pixels()
                .zip(tooltip.pixels())
                .filter(|(a, b)| a != b)
                .count()
                > 1000,
            "the actual long-path tooltip must be visible"
        );
        shell.atlas.set_pixel_ratio(1.);
    }
    draw(&mut shell, Input::default());
    let templates_before = shell
        .panels
        .render_queue
        .view
        .as_ref()
        .unwrap()
        .document
        .templates
        .len();
    let new_template = shell
        .panels
        .auxiliary
        .render_queue
        .actions
        .iter()
        .find(|(name, _)| *name == "new_template")
        .unwrap()
        .1;
    draw(&mut shell, input(new_template));
    draw(&mut shell, Input::default());
    let save_template = shell
        .panels
        .auxiliary
        .render_queue
        .actions
        .iter()
        .find(|(name, _)| *name == "save_template")
        .unwrap()
        .1;
    draw(&mut shell, input(save_template));
    poll(&mut shell, |s| !s.panels.render_queue.pending);
    draw(&mut shell, Input::default());
    let saved_document = &shell.panels.render_queue.view.as_ref().unwrap().document;
    assert_eq!(saved_document.templates.len(), templates_before + 1);
    let saved_id = shell.panels.render_queue.last_template.unwrap().1;
    assert_eq!(shell.panels.auxiliary.render_queue.template, Some(saved_id));
    assert!(shell.panels.auxiliary.render_queue.draft.is_none());
    assert!(!shell
        .panels
        .auxiliary
        .render_queue
        .actions
        .iter()
        .any(|(name, _)| *name == "save_template"));
    capture(
        &mut shell,
        &mut events,
        root.join("en-template-created.png"),
    );
    shell
        .action(Action::RenderQueue(Intent::Start), None)
        .unwrap();
    poll(&mut shell, |s| {
        s.panels
            .render_queue
            .view
            .as_ref()
            .unwrap()
            .document
            .jobs
            .iter()
            .all(|j| matches!(j.state, JobState::Succeeded { .. }))
    });
    capture(&mut shell, &mut events, root.join("en-completed.png"));
    for index in 0..2 {
        let output =
            motion_desktop_media::verify_video_runtime(&root.join(format!("frozen-{index}.mp4")))
                .unwrap();
        assert_eq!(output["sourceFrames"], 3);
    }
    assert_eq!(shell.engine.state().unwrap()["project"], baseline);
    let saved = std::fs::read(root.join("project/.motion-render-queue/queue.json")).unwrap();
    std::fs::write(root.join("saved-queue.json"), &saved).unwrap();
    let saved: Document = serde_json::from_slice(&saved).unwrap();
    assert!(saved
        .jobs
        .iter()
        .all(|j| matches!(j.state, JobState::Succeeded { .. })));
    assert_ne!(saved.jobs[0].template.name, saved.templates[0].name);

    // A real competing output creates a failed task with its original cause;
    // the pre-existing file must remain intact and the editor stays unchanged.
    let destination = root.join("racing-output.mp4");
    let template = saved.templates[0].clone();
    shell
        .render_queue
        .freeze(&shell.engine, &shell.state, template, destination.clone())
        .unwrap();
    poll(&mut shell, |s| {
        s.panels
            .render_queue
            .view
            .as_ref()
            .unwrap()
            .document
            .jobs
            .len()
            == 3
            && !s.panels.render_queue.pending
    });
    std::fs::write(&destination, b"foreign output").unwrap();
    shell
        .action(Action::RenderQueue(Intent::Start), None)
        .unwrap();
    poll(&mut shell, |s| {
        matches!(
            s.panels.render_queue.view.as_ref().unwrap().document.jobs[2].state,
            JobState::Failed { .. }
        )
    });
    assert_eq!(std::fs::read(destination).unwrap(), b"foreign output");
    shell.workspace.locale = Locale::Zh;
    shell.panels.locale = Locale::Zh;
    capture(&mut shell, &mut events, root.join("zh-failed.png"));
    assert_eq!(shell.engine.state().unwrap()["project"], baseline);
    capture_scaled(
        &mut shell,
        &mut events,
        root.join("zh-default.png"),
        [1600., 1000.],
        1.,
    );
    capture_scaled(
        &mut shell,
        &mut events,
        root.join("zh-main-dpi2.png"),
        [900., 600.],
        2.,
    );
    let queue = std::fs::read(root.join("project/.motion-render-queue/queue.json")).unwrap();
    std::fs::write(root.join("saved-queue-final.json"), &queue).unwrap();
    shell.state = shell.engine.history(0).unwrap();
    assert_ne!(shell.state["project"], baseline);
    assert_eq!(
        std::fs::read(root.join("project/.motion-render-queue/queue.json")).unwrap(),
        queue
    );
    shell.state = shell.engine.history(1).unwrap();
    assert_eq!(shell.state["project"], baseline);
    assert_eq!(
        std::fs::read(root.join("project/.motion-render-queue/queue.json")).unwrap(),
        queue
    );
}
