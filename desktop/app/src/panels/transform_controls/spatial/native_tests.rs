//! Explicit native surfaces and default-level failure evidence; never a timing gate.
use super::*;
use crate::{
    engine::{AtlasUpload, Engine, UiFrame},
    i18n::Locale,
    panels::{self, Panels},
};
use motion_ui::{
    dock::{self, Panel},
    text::TextAtlas,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

const SIZE: [f32; 2] = [1100., 760.];
fn draw(
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    window: u64,
    input: &Input,
) -> (PaintList, Vec<Action>, Rect) {
    let state = engine.state().unwrap();
    let mut paint = PaintList::default();
    let actions = if window == 0 {
        panels::chrome(
            &mut paint,
            atlas,
            SIZE,
            &dock::Node::dock(Panel::Composition.dock()),
            panels,
            &state,
            input,
        )
    } else {
        panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::Composition,
            SIZE,
            panels,
            &state,
            input,
        )
    };
    let styles = paint.styles();
    let view = if window == 0 {
        panels::composition_view_styled(&dock::Node::dock(Panel::Composition.dock()), SIZE, &styles)
    } else {
        panels::floating_view_styled(Panel::Composition, SIZE, &styles).unwrap()
    };
    let image = panels
        .composition
        .navigation
        .mapping(view, [1920., 1080.])
        .unwrap()
        .image;
    (paint, actions, image)
}
fn dispatch(events: &mut EventLoop<()>, engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        let reply = match action {
            Action::History(op) => engine.history_async(op).unwrap(),
            Action::EditBatch(commands) => engine
                .edit_async(serde_json::to_string(&commands).unwrap())
                .unwrap(),
            Action::Edit(command) => engine
                .edit_async(serde_json::to_string(&command).unwrap())
                .unwrap(),
            Action::ViewerDrag {key,cell,object,delta,size} => {
                assert_eq!(cell,0,"single-view fixture must keep its view identity");
                engine.viewer_drag_async(crate::engine::viewer_input::Drag {
                    window:key.window,source:key.source,frame:f64::from_bits(key.frame),
                    camera:crate::engine::viewer_frame::Camera::Composition,object,delta,size}).unwrap()
            },
            Action::ViewerPick {key,cell,point} => {
                assert_eq!(cell,0,"single-view fixture must keep its view identity");
                let reply=engine.viewer_input_async(crate::engine::viewer_input::Request {
                    window:key.window,source:key.source,frame:f64::from_bits(key.frame),
                    query:crate::engine::viewer_input::Query::Pick {
                        camera:crate::engine::viewer_frame::Camera::Composition,point}}).unwrap();
                let value=crate::native_test_support::receive(events,&reply,Duration::from_secs(30)).unwrap().unwrap();
                assert_eq!(value["cancelled"],false);
                assert_eq!(value["pick"]["candidates"][0]["id"],7);
                continue;
            },
            Action::PreviewDrag {
                object,
                delta,
                size,
            } => engine.drag_async(object as i64, delta, size).unwrap(),
            Action::SelectAt(point) => {
                let reply = engine
                    .request(|reply| crate::engine::Command::HitCandidates {
                        x: point[0],
                        y: point[1],
                        reply,
                    })
                    .unwrap();
                let value =
                    crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
                        .unwrap()
                        .unwrap();
                assert_eq!(value["candidates"][0]["id"], 7);
                continue;
            }
            other => panic!("unexpected native spatial action: {other:?}"),
        };
        crate::native_test_support::receive(events, &reply, Duration::from_secs(30))
            .unwrap()
            .unwrap();
    }
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    window: u64,
    path: &Path,
) -> Rect {
    let (paint, actions, image) = draw(engine, panels, atlas, window, &Input::default());
    assert!(actions.is_empty());
    let styles = paint.styles();
    let viewport = if window == 0 {
        panels::composition_view_styled(&dock::Node::dock(Panel::Composition.dock()), SIZE, &styles)
    } else {
        panels::floating_view_styled(Panel::Composition, SIZE, &styles).unwrap()
    };
    let rect = |r: Rect| [r.min[0], r.min[1], r.width(), r.height()];
    let config = atlas.config();
    let frame = UiFrame { views: None,
        paint,
        atlas: Some(AtlasUpload {
            pixels: atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: SIZE,
        composition: Some(rect(viewport)),
        composition_image: Some(rect(image)),
        inspection: Default::default(),
        window,
        node_preview: None,
    };
    let presented = engine
        .present_async(UiFrame { views: None,
            paint: PaintList::default(),
            atlas: None,
            logical_size: SIZE,
            composition: frame.composition,
            composition_image: frame.composition_image,
            inspection: Default::default(),
            window,
            node_preview: None,
        })
        .unwrap();
    assert!(
        crate::native_test_support::receive(events, &presented, Duration::from_secs(30))
            .unwrap()
            .unwrap()
    );
    crate::native_test_support::capture_ui(events, engine, frame, path.into()).unwrap();
    assert_eq!(
        image::open(path).unwrap().to_rgba8().dimensions(),
        (SIZE[0] as u32, SIZE[1] as u32)
    );
    image
}
fn pointer(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        pending: true,
        ..Default::default()
    }
}

mod snapping;

#[test]
#[ignore = "explicit native GPU, main/floating routes, screenshots and default error-channel acceptance"]
fn native_spatial_handles_preserve_owner_locale_history_and_report_projection_failures() {
    const CHILD: &str = "MOTION_SPATIAL_GIZMO_NATIVE_CHILD";
    const NAME: &str = "panels::transform_controls::spatial::native_tests::native_spatial_handles_preserve_owner_locale_history_and_report_projection_failures";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_SPATIAL_GIZMO_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("MOTION_LOG")
            .env(CHILD, &root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
                std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
                panic!("native spatial acceptance exceeded 240 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let logs = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        for text in [
            "site=transform.spatial_drag count=60",
            "object=7 phase=constraint",
            "transform projection is not invertible",
        ] {
            assert!(logs.contains(text), "file missing: {text}");
            assert!(stderr.contains(text), "stderr missing: {text}");
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains(text),
                "stdout leaked: {text}"
            );
        }
        assert_eq!(
            logs.matches("site=transform.spatial_drag count=60").count(),
            1
        );
        assert_eq!(
            stderr
                .matches("site=transform.spatial_drag count=60")
                .count(),
            1
        );
        assert!(!stderr.contains("site=transform.spatial_drag count=1 "));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let (_folder, engine, before, _) = super::worker_tests::fixture(false);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let main = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(SIZE[0] as u32, SIZE[1] as u32)),
            )
            .unwrap(),
    );
    #[allow(deprecated)]
    let floating = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(SIZE[0] as u32, SIZE[1] as u32)),
            )
            .unwrap(),
    );
    let attached = engine
        .attach_async(main, SIZE[0] as u32, SIZE[1] as u32)
        .unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    engine
        .float_window(1, floating, SIZE[0] as u32, SIZE[1] as u32)
        .unwrap();
    let mut report = vec![];
    for window in [0, 1] {
        for tool in [Tool::Move, Tool::Scale] {
            let mut locale_result = None;
            for locale in [Locale::Zh, Locale::En] {
                let mut panels = Panels {
                    locale,
                    selected: Some(7),
                    ..Default::default()
                };
                panels.transform_controls.tool = tool;
                let mut atlas =
                    TextAtlas::from_family(PaintList::default().styles().font_family(), 12., true)
                        .unwrap();
                let name = format!("{window}-{tool:?}-{locale:?}");
                let view = capture(
                    &mut events,
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &root.join(format!("{name}-before.png")),
                );
                let pose = Pose::new(&before, 7, view).unwrap();
                let frame = Frame::new(&pose, tool, &PaintList::default().styles()).unwrap();
                // Axis endpoint is outside the source grips and plane handles.
                let start = axis(0) * frame.lengths[0].unwrap();
                let delta = if tool == Tool::Move {
                    40.
                } else {
                    frame.lengths[0].unwrap() * 0.2
                };
                let from = frame.projection.screen3(start).unwrap();
                let to = frame.projection.screen3(start + DVec3::X * delta).unwrap();
                assert_eq!(
                    pose.handle(from, tool, &PaintList::default().styles()),
                    Some(Handle::Spatial(Constraint::Axis(0), tool == Tool::Scale))
                );
                for input in [
                    Input {
                        pressed: Some((MouseButton::Left, from)),
                        ..pointer(from)
                    },
                    pointer(to),
                ] {
                    let (_, actions, _) = draw(&engine, &mut panels, &mut atlas, window, &input);
                    dispatch(&mut events, &engine, actions);
                }
                let after = engine.state().unwrap();
                assert_ne!(after["project"], before["project"]);
                capture(
                    &mut events,
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &root.join(format!("{name}-drag.png")),
                );
                let (_, actions, _) = draw(
                    &engine,
                    &mut panels,
                    &mut atlas,
                    window,
                    &Input {
                        mouse: to,
                        released: Some((MouseButton::Left, to)),
                        ..Default::default()
                    },
                );
                dispatch(&mut events, &engine, actions);
                assert!(!panels.transform_controls.gesture_active());
                if let Some(expected) = &locale_result {
                    assert_eq!(&after["project"], expected);
                } else {
                    locale_result = Some(after["project"].clone());
                }
                let undo = engine.history_async(0).unwrap();
                let restored = crate::native_test_support::receive(
                    &mut events,
                    &undo,
                    Duration::from_secs(30),
                )
                .unwrap()
                .unwrap();
                assert_eq!(restored["project"], before["project"]);
                // The same route rolls back an active gesture on focus loss.
                for input in [
                    Input {
                        pressed: Some((MouseButton::Left, from)),
                        ..pointer(from)
                    },
                    pointer(to),
                    Input {
                        focus_lost: true,
                        ..Default::default()
                    },
                ] {
                    let (_, actions, _) = draw(&engine, &mut panels, &mut atlas, window, &input);
                    dispatch(&mut events, &engine, actions);
                }
                assert!(!panels.transform_controls.gesture_active());
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
                report.push(serde_json::json!({"owner":window,"tool":format!("{tool:?}"),"locale":format!("{locale:?}"),"release_and_undo":true,"focus_cancel":true}));
            }
        }
    }
    crate::logging::flush();
    assert!(!std::fs::read_to_string(root.join("motion-studio.log"))
        .unwrap()
        .contains("site=transform.spatial_drag"));
    // Fault only the already-constructed ray viewport, leaving the primary
    // cause intact. This exercises the real spatial command degradation path.
    let pose = Pose::new(&before, 7, Rect::new(0., 0., 1920., 1080.)).unwrap();
    let frame = Frame::new(&pose, Tool::Move, &PaintList::default().styles()).unwrap();
    let point = frame.tips[0].unwrap();
    let mut drag = Drag::new(
        Seed::new(point, &before, pose.projection.image),
        pose,
        Handle::Spatial(Constraint::Axis(0), false),
    )
    .unwrap();
    let start = frame.point(point, Constraint::Axis(0)).unwrap();
    drag.spatial = Some(Box::new(Gesture { frame, start }));
    drag.spatial.as_mut().unwrap().frame.projection.image = Rect::new(0., 0., 0., 0.);
    for _ in 0..61 {
        assert!(drag
            .spatial_commands(&before, &pointer(point), &PaintList::default().styles())
            .is_empty());
        assert!(matches!(drag.notice.take(), Some(Problem::Transform)));
    }
    assert!(!drag.begun);
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    crate::logging::flush();
    engine.close_window(1).unwrap();
}
