//! Exercise shortcut ownership through real chrome and the model history.
use super::*;
use crate::i18n::Locale;
use motion_core::{Engine, Layer, Project};
use motion_ui::{dock::Node, input::Modifiers, paint::PaintList, text::TextAtlas};

fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    project.layers.push(Layer::solid(
        7,
        "History fixture",
        [100.; 2],
        [0.; 3],
        [1.; 4],
    ));
    let mut engine = Engine::new(project).unwrap();
    engine.apply(Edit::Delete { object: 7 }).unwrap();
    assert!(engine.undo().unwrap());
    engine
}

fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"redo-contract","composition":motion_core::composition::MAIN_COMPOSITION,
        "revision":engine.revision(),"frame":0,"project":engine.project(),
        "canUndo":engine.can_undo(),"canRedo":engine.can_redo(),
        "timeline_layers":engine.project().timeline_layers(0.).unwrap()})
}

fn chord(key: Key, shift: bool) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control: true,
                shift,
                alt: false,
            },
        )],
        ..Default::default()
    }
}

fn draw(
    p: &mut Panels,
    s: &Value,
    input: &Input,
    floating: bool,
    atlas: &mut TextAtlas,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        crate::panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::Timeline,
            [900., 600.],
            p,
            s,
            input,
        )
    } else {
        crate::panels::chrome(
            &mut paint,
            atlas,
            [900., 600.],
            &Node::dock(Panel::Timeline.dock()),
            p,
            s,
            input,
        )
    };
    (paint, actions)
}

#[test]
fn main_and_floating_chords_redo_real_history_and_keep_solid_creation_distinct() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for (key, shift) in [(Key::Y, false), (Key::Z, true)] {
                let mut engine = fixture();
                let original = engine.project().clone();
                let mut p = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                p.select_only(Some(7));
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                assert!(draw(
                    &mut p,
                    &state(&engine),
                    &Input::default(),
                    floating,
                    &mut atlas
                )
                .1
                .is_empty());
                let actions = draw(
                    &mut p,
                    &state(&engine),
                    &chord(key, shift),
                    floating,
                    &mut atlas,
                )
                .1;
                assert!(
                    matches!(actions.as_slice(), [Action::History(1)]),
                    "{locale:?} floating={floating}: {actions:?}"
                );
                assert!(engine.redo().unwrap());
                assert!(engine.project().layers.is_empty());
                assert!(engine.undo().unwrap());
                assert_eq!(engine.project(), &original);
                let actions = draw(
                    &mut p,
                    &state(&engine),
                    &chord(Key::Y, true),
                    floating,
                    &mut atlas,
                )
                .1;
                assert!(
                    matches!(actions.as_slice(), [Action::AddSolid]),
                    "{actions:?}"
                );
                assert_eq!(
                    engine.project(),
                    &original,
                    "opening the creation dialog cannot edit history"
                );
            }
        }
    }
}

#[test]
fn y_redoes_only_the_focused_node_document_in_all_node_hosts() {
    for locale in [Locale::En, Locale::Zh] {
        for panel in [Panel::Nodes, Panel::NodeProperties, Panel::NodePreview] {
            for owner in [None, Some(panel)] {
                let mut p = Panels {
                    locale,
                    focused: Some(panel),
                    ..Default::default()
                };
                p.node_editor
                    .add_node(motion_graph::NodeOp::Float { value: 1. }, [0., 0.])
                    .unwrap();
                let edited = p.node_editor.document().clone();
                let mut actions = vec![];
                route(
                    &chord(Key::Z, false),
                    &mut p,
                    &Value::Null,
                    true,
                    owner,
                    &mut actions,
                );
                assert_ne!(p.node_editor.document(), &edited);
                actions.clear();
                route(
                    &chord(Key::Y, false),
                    &mut p,
                    &Value::Null,
                    true,
                    owner,
                    &mut actions,
                );
                assert_eq!(p.node_editor.document(), &edited);
                assert!(matches!(
                    actions.as_slice(),
                    [Action::Node(crate::node_editor::Action::Changed {
                        semantic: true,
                        ..
                    })]
                ));
            }
        }
    }
}

#[test]
fn repeat_ime_and_text_drafts_keep_y_chords_out_of_project_history() {
    for owner in [None, Some(Panel::Timeline)] {
        for shift in [false, true] {
            for mode in 0..4 {
                let engine = fixture();
                let before = engine.project().clone();
                let mut p = Panels {
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                let mut input = chord(Key::Y, shift);
                match mode {
                    0 => input.key_repeats = vec![true],
                    1 => input.composing = true,
                    2 => input.ime_consumed = true,
                    _ => p.timeline.timeline_search = true,
                }
                let mut actions = vec![];
                route(&input, &mut p, &state(&engine), false, owner, &mut actions);
                assert!(
                    actions.is_empty(),
                    "owner={owner:?} shift={shift} mode={mode}: {actions:?}"
                );
                assert_eq!(engine.project(), &before);
                assert!(engine.can_redo());
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "manual GPU menu evidence, two native window owners"]
fn native_menus_show_redo_and_solid_chords_in_both_languages() {
    use crate::{
        engine::{AtlasUpload, UiFrame},
        native_test_support,
    };
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(
        std::env::var_os("MOTION_REDO_NATIVE_ARTIFACTS").expect("explicit evidence directory"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let mut events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let make_window = || {
        #[allow(deprecated)]
        Arc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(900, 600)),
                )
                .unwrap(),
        )
    };
    let main = make_window();
    let float = make_window();
    let engine = crate::engine::Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let reply = engine.attach_async(main.clone(), 900, 600).unwrap();
    native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    engine.float_window(1, float.clone(), 900, 600).unwrap();
    let model = fixture();
    let s = state(&model);
    let original = engine.state().unwrap();
    for (locale, language) in [(Locale::En, "en"), (Locale::Zh, "zh")] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for (name, floating, menu) in [
            ("edit", false, Some(1)),
            ("create", false, Some(3)),
            ("context-create", true, None),
        ] {
            let mut p = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            assert!(draw(&mut p, &s, &Input::default(), floating, &mut atlas)
                .1
                .is_empty());
            if let Some(menu) = menu {
                p.menu = Some(menu);
            } else {
                p.layer_actions.open(None, [100., 100.], &s);
            }
            let (paint, actions) = draw(&mut p, &s, &Input::default(), floating, &mut atlas);
            assert!(actions.is_empty());
            let config = atlas.config();
            let path = root.join(format!("{language}-{name}.png"));
            let report = native_test_support::capture_ui(
                &mut events,
                &engine,
                UiFrame { inspection: Default::default(),
                    paint,
                    atlas: Some(AtlasUpload {
                        pixels: atlas.pixel_data().to_vec(),
                        width: config.width,
                        height: config.height,
                    }),
                    logical_size: [900., 600.],
                    composition: None,
                    composition_image: None,
                    window: u64::from(floating),
                    node_preview: None,
                },
                path.clone(),
            )
            .unwrap();
            assert_eq!(report["width"], 900);
            assert_eq!(report["height"], 600);
            assert!(std::fs::metadata(path).unwrap().len() > 1000);
        }
    }
    let after = engine.state().unwrap();
    std::fs::write(
        root.join("worker-before.json"),
        serde_json::to_vec_pretty(&original).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("worker-after.json"),
        serde_json::to_vec_pretty(&after).unwrap(),
    )
    .unwrap();
    // CaptureUi legitimately advances presentation/view state. Author data and
    // history must remain identical; comparing the whole snapshot conflates them.
    for field in [
        "project",
        "revision",
        "frame",
        "composition",
        "root",
        "canUndo",
        "canRedo",
    ] {
        assert!(!original[field].is_null(), "fixture must publish {field}");
        assert_eq!(
            after[field], original[field],
            "menu captures changed {field}"
        );
    }
    engine.close_window(1).unwrap();
}
