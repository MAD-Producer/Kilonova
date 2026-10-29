use super::*;
use crate::engine::{AtlasUpload, Command as WorkerCommand, Engine, UiFrame};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn receive<T>(
    events: &mut EventLoop<()>,
    pending: std::sync::mpsc::Receiver<Result<T, String>>,
) -> T {
    crate::native_test_support::receive(events, &pending, Duration::from_secs(60))
        .unwrap()
        .unwrap()
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
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
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = super::super::super::chrome(
        &mut paint,
        atlas,
        [960., 640.],
        &layout(),
        panels,
        &engine.state().unwrap(),
        input,
    );
    (paint, actions)
}
fn frame(paint: PaintList, atlas: &TextAtlas, panels: &Panels, composition: bool) -> UiFrame {
    let styles = paint.styles();
    let viewport = super::super::super::composition_view_styled(&layout(), [960., 640.], &styles);
    let mapping = panels
        .composition
        .navigation
        .mapping(viewport, [64.; 2])
        .unwrap();
    let rect = |r: Rect| [r.min[0], r.min[1], r.width(), r.height()];
    let config = atlas.config();
    UiFrame {
        paint,
        atlas: Some(AtlasUpload {
            pixels: atlas.pixel_data().to_vec(),
            width: config.width,
            height: config.height,
        }),
        logical_size: [960., 640.],
        composition: composition.then(|| rect(viewport)),
        composition_image: composition.then(|| rect(mapping.image)),
        window: 0,
        node_preview: None,
    }
}
fn capture(
    events: &mut EventLoop<()>,
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    path: &Path,
) {
    let (paint, actions) = draw(engine, panels, atlas, &Input::default());
    assert!(actions.is_empty(), "{actions:?}");
    let ui = frame(paint, atlas, panels, true);
    crate::native_test_support::capture_ui(events, engine, ui, path.into()).unwrap();
}
fn present(
    events: &mut EventLoop<()>,
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
) {
    let (paint, actions) = draw(engine, panels, atlas, &Input::default());
    assert!(actions.is_empty());
    assert!(receive(
        events,
        engine
            .present_async(frame(paint, atlas, panels, true))
            .unwrap()
    ));
}
fn arm(
    engine: &Engine,
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    effect: u64,
    parameter: &str,
    kind: Kind,
) {
    draw(engine, panels, atlas, &Input::default());
    let point = center(panels.effect_controls.test_rects[&(7, effect, parameter.into(), -6)]);
    let (_, actions) = draw(engine, panels, atlas, &press(point));
    let [Action::BeginEffectPick(ticket)] = actions.as_slice() else {
        panic!("{actions:?}")
    };
    assert_eq!(ticket.kind, kind);
    panels.effect_pick.ticket = Some(ticket.clone());
}
fn apply(events: &mut EventLoop<()>, engine: &Engine, actions: Vec<Action>) -> Value {
    let mut actions = actions.into_iter();
    let Some(Action::EffectPick { ticket, point }) = actions.next() else {
        panic!("missing pick")
    };
    assert!(actions.next().is_none(), "pick must own the click");
    receive(
        events,
        engine
            .request(|reply| WorkerCommand::EffectPick {
                ticket,
                point,
                reply,
            })
            .unwrap(),
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires native GPU")]
fn native_effect_picks_preserve_source_coordinates_precision_history_and_failure_channels() {
    const CHILD: &str = "MOTION_EFFECT_PICK_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::effect_pick::tests::native::native_effect_picks_preserve_source_coordinates_precision_history_and_failure_channels","--nocapture","--test-threads=1"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
        if let Some(destination) = std::env::var_os("MOTION_EFFECT_PICK_OUTPUT") {
            std::fs::create_dir_all(&destination).unwrap();
            for entry in std::fs::read_dir(root.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(
                        entry.path(),
                        Path::new(&destination).join(entry.file_name()),
                    )
                    .unwrap();
                }
            }
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(log
            .lines()
            .any(|l| l.contains("INFO") && l.contains("start #") && l.ends_with(" effect_pick")));
        assert!(log
            .lines()
            .any(|l| l.contains("ERROR") && l.contains("position pick requires a Vec2")));
        assert!(String::from_utf8_lossy(&output.stderr).contains("position pick requires a Vec2"));
        assert!(log.lines().any(|l| l.contains("ERROR")
            && l.contains("object=7 effect=")
            && l.contains("phase=unproject")));
        assert!(String::from_utf8_lossy(&output.stderr).contains("phase=unproject"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("start #"));
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.clone()).unwrap();
    let mut project = motion_core::Project::new(64, 64, 30, 24).unwrap();
    project.background = [0.; 4];
    project.layers.push(motion_core::Layer::solid(
        7,
        "拾取示例",
        [64.; 2],
        [32., 32., 0.],
        [0.25, 0.5, 0.75, 0.5],
    ));
    let directory = root.join("project");
    motion_core::storage::save(&directory, &project).unwrap();
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    let catalogue = engine.plugin(r#"{"op":"catalogue"}"#.into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["manifest"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == "gradient_ramp")
        })
        .unwrap();
    engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"gradient_ramp"}).to_string()).unwrap();
    let state = engine.state().unwrap();
    let effect = state["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    engine
        .edit(
            serde_json::to_string(&Command::Effect {
                object: 7,
                action: motion_core::EffectAction::Enable {
                    effect,
                    enabled: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
    engine
        .edit(
            serde_json::to_string(&Command::Effect {
                object: 7,
                action: motion_core::EffectAction::Set {
                    effect,
                    param: "start".into(),
                    frame: 0,
                    value: [12.5, 34.567, 0., 0.],
                },
            })
            .unwrap(),
        )
        .unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(960, 640)),
            )
            .unwrap(),
    );
    receive(
        &mut events,
        engine.attach_async(window.clone(), 960, 640).unwrap(),
    );
    let mut panels = Panels {
        selected: Some(7),
        locale: Locale::Zh,
        ..Default::default()
    };
    panels.effect_controls.catalogue = catalogue.into();
    let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
    present(&mut events, &engine, &mut panels, &mut atlas);
    capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        &root.join("effect-pickers-main.png"),
    );
    let key = |name: &str, part: i64| (7, effect, name.to_owned(), part);
    let original_field = panels.effect_controls.test_rects[&key("start", 0)];
    let label = panels.effect_controls.test_rects[&key("start", -7)];
    assert!(draw(&engine, &mut panels, &mut atlas, &press(center(label))).1.is_empty());
    capture(&mut events, &engine, &mut panels, &mut atlas,
        &root.join("effect-selected-row.png"));
    assert_eq!(panels.effect_controls.test_rects[&key("start", 0)], original_field,
        "selection must not move value fields");
    let rail = panels.effect_controls.test_rects[&key("start", -9)];
    assert!(rail.width() > 0.);
    assert_eq!(panels.effect_controls.test_rects[&key("end", -9)].width(), 0.);
    let pixels = image::open(root.join("effect-selected-row.png")).unwrap().to_rgba8();
    let position = center(panels.effect_controls.test_rects[&key("start", -6)]);
    let background = pixels.get_pixel(position[0] as u32, (position[1] - 10.) as u32)[0];
    let visible = |dx: std::ops::RangeInclusive<i32>, dy: std::ops::RangeInclusive<i32>| {
        dx.into_iter().any(|x| dy.clone().any(|y|
            pixels.get_pixel((position[0] as i32 + x) as u32,
                (position[1] as i32 + y) as u32)[0] > background.saturating_add(60)))
    };
    assert!(visible(-2..=2, -7..=-4) && visible(-7..=-4, -2..=2),
        "the position icon must retain its cardinal arms at native scale");
    let before_key = engine.state().unwrap();
    let (_, actions) = draw(&engine, &mut panels, &mut atlas, &press(center(rail)));
    let [Action::EditBatch(commands)] = actions.as_slice() else { panic!("{actions:?}") };
    let keyed = engine.edit(serde_json::to_string(commands).unwrap()).unwrap();
    assert_eq!(keyed["project"]["layers"][0]["effects"][0]["params"]["start"]["track"]["keys"].as_array().unwrap().len(), 1);
    assert_eq!(engine.history(0).unwrap()["project"], before_key["project"]);
    let end_row = panels.effect_controls.test_rects[&key("end", -3)];
    assert!(draw(&engine, &mut panels, &mut atlas,
        &press([center(rail)[0], center(end_row)[1]])).1.is_empty(),
        "a hidden navigation lane selects its row without adding a key");
    draw(&engine, &mut panels, &mut atlas, &Input::default());
    assert_eq!(panels.effect_controls.test_rects[&key("start", -9)].width(), 0.);
    assert!(panels.effect_controls.test_rects[&key("end", -9)].width() > 0.);
    panels.effect_controls.choose_parameter(&engine.state().unwrap(), 7, effect, "start");
    let mut wide_field = None;
    for width in [220., 360., 520.] {
        let size = [width, 560.];
        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(width as u32, 560));
        receive(
            &mut events,
            engine
                .request(|reply| WorkerCommand::Resize {
                    window: 0,
                    width: width as u32,
                    height: 560,
                    reply,
                })
                .unwrap(),
        );
        let mut paint = PaintList::default();
        let actions = super::super::super::floating_chrome(
            &mut paint,
            &mut atlas,
            Panel::EffectControls,
            size,
            &mut panels,
            &engine.state().unwrap(),
            &Input::default(),
        );
        assert!(actions.is_empty());
        let key = |name: &str, part: i64| (7, effect, name.to_owned(), part);
        let pick = panels.effect_controls.test_rects[&key("start", -6)];
        let first = panels.effect_controls.test_rects[&key("start", 0)];
        let second = panels.effect_controls.test_rects[&key("start", 1)];
        let rail = panels.effect_controls.test_rects[&key("start", -9)];
        assert_eq!(rail.width() > 0., width >= 320.);
        assert_eq!(panels.effect_controls.test_rects[&key("end", -9)].width(), 0.);
        assert!(
            first.max[0] <= second.min[0] && second.max[0] <= pick.min[0] && pick.max[0] <= width
        );
        let number = paint
            .styles()
            .resolve("field.number", Default::default())
            .unwrap();
        let text_width = |value: &str, atlas: &mut TextAtlas| {
            atlas.measure(value) * number.font_size / atlas.font_size()
        };
        assert!(
            first.shrink(number.padding).width() >= text_width("12.50", &mut atlas)
                && second.shrink(number.padding).width() >= text_width("34.57", &mut atlas)
        );
        let select = panels.effect_controls.test_rects[&key("shape", -8)];
        assert_eq!(first.min[0], select.min[0]);
        assert_eq!(first.min[0], panels.effect_controls.test_rects[&key("scatter", 0)].min[0]);
        assert_eq!(first.min[0], panels.effect_controls.test_rects[&key("effect_opacity", 0)].min[0]);
        if width >= 360. {
            if let Some(previous) = wide_field { assert_eq!(first, previous, "extra width must not spread the coordinate cluster"); }
            wide_field = Some(first);
        }
        let color_pick = panels.effect_controls.test_rects[&key("start_color", -6)];
        assert_eq!(color_pick.min[0], pick.min[0], "position and color actions share a column");
        assert!(select.width() <= paint.styles().metric("effect.enum.max_width", 112.));
        assert!(select.height() >= 26.);
        let config = atlas.config();
        let ui = UiFrame {
            paint,
            atlas: Some(AtlasUpload {
                pixels: atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            }),
            logical_size: size,
            composition: None,
            composition_image: None,
            window: 0,
            node_preview: None,
        };
        crate::native_test_support::capture_ui(
            &mut events,
            &engine,
            ui,
            root.join(format!("effect-pickers-{width:.0}.png")),
        )
        .unwrap();
        let pixels = image::open(root.join(format!("effect-pickers-{width:.0}.png"))).unwrap().to_rgba8();
        let swatch = panels.effect_controls.test_rects[&key("start_color", -1)];
        assert!(pixels.get_pixel(center(swatch)[0] as u32, swatch.min[1] as u32)[0] > 80,
            "black color well needs a visible neutral border");
        for mode in ["hover", "editing"] {
            let point = center(first);
            if mode == "editing" {
                for input in [press(point), Input { mouse:point, released:Some((MouseButton::Left,point)), ..Default::default() }] {
                    let mut paint = PaintList::default();
                    assert!(super::super::super::floating_chrome(&mut paint, &mut atlas,
                        Panel::EffectControls, size, &mut panels, &engine.state().unwrap(), &input).is_empty());
                }
            }
            let mut paint = PaintList::default();
            assert!(super::super::super::floating_chrome(&mut paint, &mut atlas,
                Panel::EffectControls, size, &mut panels, &engine.state().unwrap(),
                &Input {mouse:point,..Default::default()}).is_empty());
            let config = atlas.config();
            let ui = UiFrame { paint, atlas:Some(AtlasUpload {pixels:atlas.pixel_data().to_vec(),width:config.width,height:config.height}),
                logical_size:size,composition:None,composition_image:None,window:0,node_preview:None };
            crate::native_test_support::capture_ui(&mut events,&engine,ui,
                root.join(format!("effect-{mode}-{width:.0}.png"))).unwrap();
            if mode == "hover" {
                let image = image::open(root.join(format!("effect-{mode}-{width:.0}.png"))).unwrap().to_rgba8();
                assert!(image.get_pixel(first.min[0] as u32, center(first)[1] as u32)[0] > 80,
                    "cached numeric fields must retain their visible hover outline");
            }
        }
        let mut paint = PaintList::default();
        assert!(super::super::super::floating_chrome(&mut paint,&mut atlas,Panel::EffectControls,size,
            &mut panels,&engine.state().unwrap(),&Input {keys:vec![(Key::Escape,Default::default())],..Default::default()}).is_empty());
    }
    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(960, 640));
    receive(
        &mut events,
        engine
            .request(|reply| WorkerCommand::Resize {
                window: 0,
                width: 960,
                height: 640,
                reply,
            })
            .unwrap(),
    );
    draw(&engine, &mut panels, &mut atlas, &Input::default());
    let select = panels.effect_controls.test_rects[&(7, effect, "shape".into(), -8)];
    assert!(
        draw(&engine, &mut panels, &mut atlas, &press(center(select)))
            .1
            .is_empty()
    );
    capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        &root.join("effect-dropdown-open.png"),
    );
    assert!(draw(
        &engine,
        &mut panels,
        &mut atlas,
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        }
    )
    .1
    .is_empty());
    let baseline = engine.state().unwrap();
    arm(
        &engine,
        &mut panels,
        &mut atlas,
        effect,
        "start",
        Kind::Position,
    );
    capture(
        &mut events,
        &engine,
        &mut panels,
        &mut atlas,
        &root.join("position-pick-armed.png"),
    );
    let viewport = super::super::super::composition_view_styled(
        &layout(),
        [960., 640.],
        &PaintList::default().styles(),
    );
    let mapping = panels
        .composition
        .navigation
        .mapping(viewport, [64.; 2])
        .unwrap();
    let point = mapping.screen([48., 16.]);
    let (_, actions) = draw(&engine, &mut panels, &mut atlas, &press(point));
    let changed = apply(&mut events, &engine, actions);
    let value = &changed["project"]["layers"][0]["effects"][0]["params"]["start"]["track"]["value"];
    assert!((value[0].as_f64().unwrap() - 48.).abs() < 0.001, "{value}");
    assert!((value[1].as_f64().unwrap() - 16.).abs() < 0.001, "{value}");
    assert!(draw(
        &engine,
        &mut panels,
        &mut atlas,
        &Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        }
    )
    .1
    .is_empty());
    assert_eq!(engine.history(0).unwrap()["project"], baseline["project"]);
    assert_eq!(engine.history(1).unwrap()["project"], changed["project"]);
    engine.history(0).unwrap();
    arm(
        &engine,
        &mut panels,
        &mut atlas,
        effect,
        "start",
        Kind::Position,
    );
    let (_, actions) = draw(
        &engine,
        &mut panels,
        &mut atlas,
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
    );
    assert!(actions.is_empty() && panels.effect_pick.ticket.is_none());
    for precision in [
        motion_effects::image_contract::ChannelPrecision::Unorm8,
        motion_effects::image_contract::ChannelPrecision::Unorm16,
        motion_effects::image_contract::ChannelPrecision::Float16,
        motion_effects::image_contract::ChannelPrecision::Float32,
    ] {
        engine
            .edit(
                serde_json::json!({"op":"set_working_precision","precision":precision}).to_string(),
            )
            .unwrap();
        present(&mut events, &engine, &mut panels, &mut atlas);
        // A precision transition changes the host view revision while rendering.
        // The first command therefore cannot stamp a retained front; the next UI
        // frame establishes the unchanged revision before a user may sample it.
        present(&mut events, &engine, &mut panels, &mut atlas);
        let before = engine.state().unwrap();
        arm(
            &engine,
            &mut panels,
            &mut atlas,
            effect,
            "start_color",
            Kind::Color,
        );
        if precision == motion_effects::image_contract::ChannelPrecision::Unorm8 {
            capture(
                &mut events,
                &engine,
                &mut panels,
                &mut atlas,
                &root.join("color-pick-armed.png"),
            );
        }
        let (_, actions) = draw(&engine, &mut panels, &mut atlas, &press(point));
        let changed = apply(&mut events, &engine, actions);
        let value = &changed["project"]["layers"][0]["effects"][0]["params"]["start_color"]
            ["track"]["value"];
        for (axis, expected) in [0.25, 0.5, 0.75, 0.5].iter().enumerate() {
            assert!(
                (value[axis].as_f64().unwrap() - expected).abs() < 0.01,
                "{precision:?} {value}"
            );
        }
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    }
    let before = engine.state().unwrap();
    // History invalidated the retained front. A normal miss must not use old pixels.
    let ticket = Ticket::new(&before, 7, effect, "start_color", Kind::Color);
    let pending = receive(
        &mut events,
        engine
            .request(|reply| WorkerCommand::EffectPick {
                ticket,
                point: [0.5; 2],
                reply,
            })
            .unwrap(),
    );
    assert_eq!(pending["editorAction"]["reason"], "preview_pending");
    assert_eq!(pending["project"], before["project"]);
    // Picking owns the input across the docked and floating composition paths.
    arm(
        &engine,
        &mut panels,
        &mut atlas,
        effect,
        "start",
        Kind::Position,
    );
    let mut paint = PaintList::default();
    let actions = super::super::super::floating_chrome(
        &mut paint,
        &mut atlas,
        Panel::EffectControls,
        [360., 560.],
        &mut panels,
        &before,
        &Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert!(actions.is_empty() && panels.effect_pick.ticket.is_some());
    let size = [640., 560.];
    let viewport =
        super::super::super::floating_view_styled(Panel::Composition, size, &paint.styles())
            .unwrap();
    let mapping = panels
        .composition
        .navigation
        .mapping(viewport, [64.; 2])
        .unwrap();
    let point = mapping.screen([20., 40.]);
    let mut paint = PaintList::default();
    let actions = super::super::super::floating_chrome(
        &mut paint,
        &mut atlas,
        Panel::Composition,
        size,
        &mut panels,
        &before,
        &press(point),
    );
    let picked = apply(&mut events, &engine, actions);
    let value = &picked["project"]["layers"][0]["effects"][0]["params"]["start"]["track"]["value"];
    assert!(
        (value[0].as_f64().unwrap() - 20.).abs() < 0.001
            && (value[1].as_f64().unwrap() - 40.).abs() < 0.001,
        "{value}"
    );
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    let before = engine.state().unwrap();
    let mut invalid_projection = before.clone();
    invalid_projection["source_transforms"] = serde_json::json!([]);
    panels.effect_pick.ticket = Some(Ticket::new(
        &invalid_projection,
        7,
        effect,
        "start",
        Kind::Position,
    ));
    let main_view = super::super::super::composition_view_styled(
        &layout(),
        [960., 640.],
        &PaintList::default().styles(),
    );
    let main_point = panels
        .composition
        .navigation
        .mapping(main_view, [64.; 2])
        .unwrap()
        .screen([32.; 2]);
    let actions = super::super::super::chrome(
        &mut PaintList::default(),
        &mut atlas,
        [960., 640.],
        &layout(),
        &mut panels,
        &invalid_projection,
        &press(main_point),
    );
    assert!(actions.is_empty() && panels.effect_pick.ticket.is_none());
    assert!(panels.message.take().unwrap().contains("图层平面"));
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    let bad = Ticket::new(&before, 7, effect, "start_color", Kind::Position);
    let result = engine
        .request(|reply| WorkerCommand::EffectPick {
            ticket: bad,
            point: [1., 2.],
            reply,
        })
        .unwrap();
    let result =
        crate::native_test_support::receive(&mut events, &result, Duration::from_secs(30)).unwrap();
    assert!(result
        .unwrap_err()
        .contains("position pick requires a Vec2"));
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    let ticket = Ticket::new(&before, 7, effect, "start", Kind::Position);
    engine.seek(1.).unwrap();
    let cancelled = receive(
        &mut events,
        engine
            .request(|reply| WorkerCommand::EffectPick {
                ticket,
                point: [1., 2.],
                reply,
            })
            .unwrap(),
    );
    assert_eq!(cancelled["editorAction"]["reason"], "context_changed");
    assert_eq!(cancelled["project"], before["project"]);
}
