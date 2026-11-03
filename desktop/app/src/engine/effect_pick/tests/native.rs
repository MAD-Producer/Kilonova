//! Real GPU previews, production pick buttons and composition input in both owners.
use super::*;
use crate::{
    i18n::Locale,
    panels::{self, Action, Panels},
};
use motion_ui::{
    dock::{self, Node, Panel},
    input::{Input, Key, MouseButton},
    paint::PaintList,
    text::TextAtlas,
    Rect,
};
use std::{path::PathBuf, sync::Arc};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
const SIZE: [f32; 2] = [960., 640.];
fn layout() -> Node {
    Node::Split {
        edge: dock::Edge::Left,
        split: 0.375,
        first: Box::new(Node::dock(Panel::EffectControls.dock())),
        second: Box::new(Node::dock(Panel::Composition.dock())),
    }
}
fn draw(
    engine: &Engine,
    p: &mut Panels,
    atlas: &mut TextAtlas,
    floating: bool,
    source: bool,
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let state = engine.state().unwrap();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            atlas,
            if source {
                Panel::EffectControls
            } else {
                Panel::Composition
            },
            SIZE,
            p,
            &state,
            input,
        )
    } else {
        panels::chrome(&mut paint, atlas, SIZE, &layout(), p, &state, input)
    };
    (paint, actions)
}
fn receive<T>(
    events: &mut EventLoop<()>,
    pending: std::sync::mpsc::Receiver<Result<T, String>>,
) -> T {
    crate::native_test_support::receive(events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap()
}
fn ui(
    paint: PaintList,
    atlas: &TextAtlas,
    p: &Panels,
    floating: bool,
    composition: bool,
) -> UiFrame {
    let styles = paint.styles();
    let view = if floating {
        panels::floating_view_styled(Panel::Composition, SIZE, &styles).unwrap()
    } else {
        panels::composition_view_styled(&layout(), SIZE, &styles)
    };
    let image = p
        .composition
        .navigation
        .mapping(view, [64.; 2])
        .unwrap()
        .image;
    let rect = |r: Rect| [r.min[0], r.min[1], r.width(), r.height()];
    UiFrame {
        views: None,
        inspection: Default::default(),
        paint,
        atlas: Some(AtlasUpload {
            pixels: atlas.pixel_data().to_vec(),
            width: atlas.config().width,
            height: atlas.config().height,
        }),
        logical_size: SIZE,
        composition: composition.then(|| rect(view)),
        composition_image: composition.then(|| rect(image)),
        window: u64::from(floating),
        node_preview: None,
    }
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn record(path: impl AsRef<Path>, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

#[test]
#[ignore = "manual actual GPU picks, default failure logs and separate-process saved-project reopen"]
fn mapped_native_picks_use_exact_local_keys_in_two_owners_and_languages() {
    const NAME:&str="engine::effect_pick::tests::native::mapped_native_picks_use_exact_local_keys_in_two_owners_and_languages";
    const CHILD: &str = "MOTION_MAPPED_PICK_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_MAPPED_PICK_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().into());
        std::fs::create_dir_all(&root).unwrap();
        for reopening in [false, true] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    NAME,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, &root)
                .env_remove("MOTION_LOG");
            if reopening {
                command.env("MOTION_MAPPED_PICK_REOPEN", "1");
            } else {
                command.env_remove("MOTION_MAPPED_PICK_REOPEN");
            }
            let result = command.output().unwrap();
            std::fs::write(root.join(format!("{reopening}-stdout.log")), &result.stdout).unwrap();
            std::fs::write(root.join(format!("{reopening}-stderr.log")), &result.stderr).unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let log = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let stderr = std::fs::read_to_string(root.join("false-stderr.log")).unwrap();
        let stdout = std::fs::read_to_string(root.join("false-stdout.log")).unwrap();
        assert!(log
            .lines()
            .any(|l| l.contains("INFO") && l.contains("start #") && l.ends_with(" effect_pick")));
        for text in [
            "continuous key addressing",
            "effect pick object=7 effect=1 parameter=start frame=8.75",
            "track_at",
        ] {
            assert!(log.contains(text), "{text}: {log}");
            assert!(stderr.contains(text), "{text}: {stderr}");
            assert!(!stdout.contains(text));
        }
        let first: Value =
            serde_json::from_slice(&std::fs::read(root.join("capture-process.json")).unwrap())
                .unwrap();
        let second: Value =
            serde_json::from_slice(&std::fs::read(root.join("reopen-process.json")).unwrap())
                .unwrap();
        assert_ne!(first["pid"], second["pid"]);
        return;
    };
    let root = PathBuf::from(root);
    let reopening = std::env::var_os("MOTION_MAPPED_PICK_REOPEN").is_some();
    let _log =
        crate::logging::init(root.join(if reopening { "reopen-logs" } else { "logs" })).unwrap();
    record(
        root.join(if reopening {
            "reopen-process.json"
        } else {
            "capture-process.json"
        }),
        &json!({"pid":std::process::id()}),
    );
    if reopening {
        for folder in std::fs::read_dir(&root)
            .unwrap()
            .map(Result::unwrap)
            .map(|e| e.path())
            .filter(|p| p.join("saved.json").is_file())
        {
            let engine = Engine::start(
                folder.join("project"),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let saved: Value =
                serde_json::from_slice(&std::fs::read(folder.join("saved.json")).unwrap()).unwrap();
            let actual = engine.state().unwrap();
            assert_eq!(actual["project"], saved["project"]);
            assert_eq!(actual["history"]["undoEntries"], 0);
            record(folder.join("reopened.json"), &actual);
            engine.request_shutdown().unwrap();
        }
        return;
    }
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let windows: [Arc<winit::window::Window>; 2] = std::array::from_fn(|_| {
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(960, 640)),
                )
                .unwrap(),
        )
    });
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let folder = root.join(format!("{locale:?}-{floating}-{reverse}"));
                std::fs::create_dir_all(&folder).unwrap();
                let engine = start(&folder.join("project"), &project(reverse, true), 6.5);
                receive(
                    &mut events,
                    engine.attach_async(windows[0].clone(), 960, 640).unwrap(),
                );
                engine
                    .float_window(1, windows[1].clone(), 960, 640)
                    .unwrap();
                let mut p = Panels {
                    selected: Some(7),
                    locale,
                    ..Default::default()
                };
                p.viewer_views.window = u64::from(floating);
                p.effect_controls.catalogue = engine
                    .plugin(r#"{"op":"catalogue"}"#.into())
                    .unwrap()
                    .into();
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                for name in ["start", "start_color"] {
                    let before = engine.state().unwrap();
                    draw(
                        &engine,
                        &mut p,
                        &mut atlas,
                        floating,
                        true,
                        &Input::default(),
                    );
                    let button = p.effect_controls.test_rects[&(7, 1, name.into(), -6)];
                    let at = [
                        (button.min[0] + button.max[0]) * 0.5,
                        (button.min[1] + button.max[1]) * 0.5,
                    ];
                    let (_, actions) =
                        draw(&engine, &mut p, &mut atlas, floating, true, &press(at));
                    let [Action::BeginEffectPick(armed)] = actions.as_slice() else {
                        panic!("{actions:?}")
                    };
                    assert_eq!(armed.frame, 6.5);
                    p.effect_pick.ticket = Some(armed.clone());
                    // Focus transfer is normal for an armed single-click pick.
                    assert!(draw(
                        &engine,
                        &mut p,
                        &mut atlas,
                        floating,
                        true,
                        &Input {
                            focus_lost: true,
                            ..Default::default()
                        }
                    )
                    .1
                    .is_empty());
                    assert!(p.effect_pick.ticket.is_some());
                    if name == "start" {
                        assert!(draw(
                            &engine,
                            &mut p,
                            &mut atlas,
                            floating,
                            true,
                            &Input {
                                keys: vec![(Key::Escape, Default::default())],
                                ..Default::default()
                            }
                        )
                        .1
                        .is_empty());
                        assert!(p.effect_pick.ticket.is_none());
                        assert_eq!(engine.state().unwrap()["project"], before["project"]);
                        p.effect_pick.ticket = Some(armed.clone());
                    }
                    let (paint, actions) = draw(
                        &engine,
                        &mut p,
                        &mut atlas,
                        floating,
                        false,
                        &Input::default(),
                    );
                    assert!(actions.is_empty());
                    let frame = ui(paint, &atlas, &p, floating, true);
                    assert!(receive(&mut events, engine.present_async(frame).unwrap()));
                    let styles = PaintList::default().styles();
                    let viewport = if floating {
                        panels::floating_view_styled(Panel::Composition, SIZE, &styles).unwrap()
                    } else {
                        panels::composition_view_styled(&layout(), SIZE, &styles)
                    };
                    let point = p
                        .composition
                        .navigation
                        .mapping(viewport, [64.; 2])
                        .unwrap()
                        .screen(if name == "start" {
                            [40., 15.]
                        } else {
                            [32.; 2]
                        });
                    let (paint, actions) =
                        draw(&engine, &mut p, &mut atlas, floating, false, &press(point));
                    crate::native_test_support::capture_ui(
                        &mut events,
                        &engine,
                        ui(paint, &atlas, &p, floating, true),
                        folder.join(format!("{name}-pick.png")),
                    )
                    .unwrap();
                    let [Action::EffectPick { ticket, point }] = actions.as_slice() else {
                        panic!("{actions:?}")
                    };
                    let after = receive(
                        &mut events,
                        engine
                            .request(|reply| Command::EffectPick {
                                ticket: ticket.clone(),
                                point: *point,
                                reply,
                            })
                            .unwrap(),
                    );
                    let value = if name == "start" {
                        [40., 15., 0., 0.]
                    } else {
                        [0.25, 0.5, 0.75, 1.]
                    };
                    let actual: &Value = &after["project"]["layers"][0]["effects"][0]["params"]
                        [name]["track"]["keys"][1]["value"];
                    for i in 0..4 {
                        assert!(
                            (actual[i].as_f64().unwrap() - f64::from(value[i])).abs() < 0.003,
                            "{name}: {actual}"
                        );
                    }
                    // Preserve every other key, curve handle, channel and project field.
                    let mut exact = expected(&before, name, value);
                    exact["layers"][0]["effects"][0]["params"][name]["track"]["keys"][1]["value"] =
                        actual.clone();
                    assert_eq!(after["project"], exact);
                    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
                    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
                    let (paint, actions) = draw(
                        &engine,
                        &mut p,
                        &mut atlas,
                        floating,
                        true,
                        &Input::default(),
                    );
                    assert!(actions.is_empty());
                    crate::native_test_support::capture_ui(
                        &mut events,
                        &engine,
                        ui(paint, &atlas, &p, floating, !floating),
                        folder.join(format!("{name}-committed.png")),
                    )
                    .unwrap();
                }
                engine.seek(8.75).unwrap();
                let before = engine.state().unwrap();
                let error = receive_error(&mut events, &engine, ticket(&before, "start"));
                assert!(error.contains("continuous key addressing"));
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
                engine.seek(6.5).unwrap();
                let saved = engine.save().unwrap();
                record(folder.join("saved.json"), &saved);
                engine.close_window(1).unwrap();
                engine.request_shutdown().unwrap();
            }
        }
    }
    crate::logging::flush();
}
fn receive_error(events: &mut EventLoop<()>, engine: &Engine, ticket: Ticket) -> String {
    let pending = engine
        .request(|reply| Command::EffectPick {
            ticket,
            point: [40., 15.],
            reply,
        })
        .unwrap();
    crate::native_test_support::receive(events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap_err()
}
