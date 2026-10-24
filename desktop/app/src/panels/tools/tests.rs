use super::*;
use motion_ui::{
    dock::Panel,
    input::{Key, Modifiers},
    text::TextAtlas,
};
use std::path::PathBuf;
fn state() -> Value {
    serde_json::json!({"root":"tools","revision":1,"frame":0,"project":{"width":1920,"height":1080,"composition_id":"main","layers":[]}})
}
fn key(k: Key) -> Input {
    Input {
        keys: vec![(k, Modifiers::default())],
        ..Default::default()
    }
}
fn route(p: &mut Panels, s: &Value, i: Input) -> Vec<Action> {
    let mut out = vec![];
    super::super::keyboard::route(&i, p, s, false, None, &mut out);
    out
}
#[test]
fn activation_and_shape_cycles_never_create_or_edit_a_layer() {
    for locale in [Locale::Zh, Locale::En] {
        let s = state();
        let before = s.clone();
        let mut p = Panels {
            locale,
            focused: Some(Panel::Composition),
            ..Default::default()
        };
        for (k, t) in [
            (Key::V, Active::Select),
            (Key::G, Active::Pen),
            (Key::Y, Active::Anchor),
            (Key::W, Active::Rotate),
            (Key::Q, Active::Rectangle),
            (Key::Q, Active::Ellipse),
            (Key::T, Active::Text),
            (Key::H, Active::Hand),
            (Key::Z, Active::Zoom),
        ] {
            assert!(route(&mut p, &s, key(k)).is_empty());
            assert_eq!(active(&p), t);
        }
        assert!(route(&mut p, &s, key(Key::C)).is_empty());
        assert_eq!(active(&p), Active::Zoom);
        assert!(p
            .message
            .as_deref()
            .unwrap()
            .contains(if locale == Locale::Zh {
                "观察视图"
            } else {
                "observation"
            }));
        assert!(route(&mut p, &s, key(Key::Escape)).is_empty());
        assert_eq!(active(&p), Active::Select);
        assert_eq!(s, before);
    }
}
#[test]
fn text_ime_focus_menus_and_repeat_keep_tool_activation_owned() {
    let s = state();
    for mode in 0..7 {
        let mut p = Panels {
            focused: Some(Panel::Composition),
            ..Default::default()
        };
        let mut i = key(Key::Q);
        match mode {
            0 => i.composing = true,
            1 => i.ime_consumed = true,
            2 => i.focus_lost = true,
            3 => i.key_repeats = vec![true],
            4 => p.project.search = true,
            5 => p.menu = Some(0),
            _ => p.focused = Some(Panel::Timeline),
        }
        assert!(route(&mut p, &s, i).is_empty());
        assert_eq!(active(&p), Active::Select, "mode {mode}");
    }
}
#[test]
fn shape_commit_uses_dragged_source_geometry_and_is_cancelled_before_release() {
    let s = state();
    let mut p = Panels::default();
    let mut out = vec![];
    let mut list = PaintList::default();
    let v = Rect::new(0., 0., 960., 540.);
    let m = p.composition.navigation.mapping(v, [1920., 1080.]).unwrap();
    choose(&mut p, Active::Rectangle, &s, &mut out);
    assert!(out.is_empty());
    viewport(
        &mut p,
        &mut list,
        v,
        m,
        &s,
        &Input {
            mouse: [100., 80.],
            pressed: Some((MouseButton::Left, [100., 80.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        &mut out,
    );
    assert!(out.is_empty());
    assert!(p.tools.editing());
    viewport(
        &mut p,
        &mut list,
        v,
        m,
        &s,
        &Input {
            mouse: [200., 130.],
            released: Some((MouseButton::Left, [200., 130.])),
            ..Default::default()
        },
        &mut out,
    );
    assert!(matches!(
        out.as_slice(),
        [Action::PlaceShape {
            shape: ShapeKind::Rectangle,
            size: [200., 100.],
            position: [300., 210., 0.]
        }]
    ));
    assert!(!p.tools.editing());
    out.clear();
    viewport(
        &mut p,
        &mut list,
        v,
        m,
        &s,
        &Input {
            mouse: [100., 80.],
            pressed: Some((MouseButton::Left, [100., 80.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        &mut out,
    );
    viewport(&mut p, &mut list, v, m, &s, &key(Key::Escape), &mut out);
    viewport(
        &mut p,
        &mut list,
        v,
        m,
        &s,
        &Input {
            mouse: [500., 400.],
            released: Some((MouseButton::Left, [500., 400.])),
            ..Default::default()
        },
        &mut out,
    );
    assert!(out.is_empty());
}
#[test]
fn text_tool_opens_existing_dialog_only_on_canvas_press() {
    let s = state();
    let mut p = Panels::default();
    let mut out = vec![];
    let mut list = PaintList::default();
    let v = Rect::new(0., 0., 960., 540.);
    let m = p.composition.navigation.mapping(v, [1920., 1080.]).unwrap();
    choose(&mut p, Active::Text, &s, &mut out);
    assert!(out.is_empty());
    viewport(&mut p, &mut list, v, m, &s, &Input::default(), &mut out);
    assert!(out.is_empty());
    viewport(
        &mut p,
        &mut list,
        v,
        m,
        &s,
        &Input {
            mouse: [100., 80.],
            pressed: Some((MouseButton::Left, [100., 80.])),
            ..Default::default()
        },
        &mut out,
    );
    assert!(matches!(out.as_slice(), [Action::NewText]));
}
#[test]
fn existing_main_and_floating_chrome_use_the_same_tool_owner() {
    let s = state();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut p = Panels {
                locale,
                focused: Some(Panel::Composition),
                ..Default::default()
            };
            let mut list = PaintList::default();
            let out = if floating {
                super::super::floating_chrome(
                    &mut list,
                    &mut atlas,
                    Panel::Composition,
                    [900., 600.],
                    &mut p,
                    &s,
                    &key(Key::Q),
                )
            } else {
                super::super::chrome(
                    &mut list,
                    &mut atlas,
                    [900., 600.],
                    &motion_ui::dock::Node::dock(Panel::Composition.dock()),
                    &mut p,
                    &s,
                    &key(Key::Q),
                )
            };
            assert!(out.is_empty());
            assert_eq!(active(&p), Active::Rectangle);
        }
    }
}

fn host() -> (tempfile::TempDir, crate::engine::Engine, Value) {
    let root = tempfile::tempdir().unwrap();
    let e = crate::engine::Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut layers = vec![];
    for id in [7, 8] {
        let mut layer = motion_core::Layer::solid(
            id,
            "Tool fixture",
            [200., 100.],
            [960., 540., 0.],
            [0.18, 0.25, 0.3, 1.],
        );
        layer.locked = id == 8;
        layers.push(motion_core::Command::Add { layer });
    }
    let s = e.edit(serde_json::to_string(&layers).unwrap()).unwrap();
    (root, e, s)
}
#[test]
fn layer_nudge_uses_real_host_state_respects_lock_and_undoes_once() {
    let (_root, e, s) = host();
    let mut p = Panels {
        focused: Some(Panel::Composition),
        ..Default::default()
    };
    p.selected = Some(7);
    p.selected_layers = vec![7, 8];
    let mut i = key(Key::Right);
    i.keys[0].1.shift = true;
    let out = route(&mut p, &s, i);
    assert_eq!(out.len(), 1);
    let Action::EditBatch(commands) = out.into_iter().next().unwrap() else {
        panic!()
    };
    assert_eq!(commands.len(), 1);
    let after = e.edit(serde_json::to_string(&commands).unwrap()).unwrap();
    assert_eq!(after["sampledLayers"][0]["position"][0], 970.);
    assert_eq!(after["sampledLayers"][1]["position"][0], 960.);
    e.history(0).unwrap();
    assert_eq!(e.state().unwrap()["project"], s["project"]);
    p.focused = Some(Panel::Timeline);
    let out = route(&mut p, &s, key(Key::Right));
    assert!(matches!(out.as_slice(), [Action::Seek(_)]));
}
#[test]
fn hand_zoom_and_middle_override_never_modify_project_or_leave_stale_cursor() {
    let s = state();
    let mut p = Panels::default();
    let mut out = vec![];
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let area = Rect::new(0., 0., 900., 600.);
    super::super::composition::render(
        &mut list,
        &mut atlas,
        area,
        &mut p,
        &s,
        &Input::default(),
        &mut out,
    );
    choose(&mut p, Active::Hand, &s, &mut out);
    let before = p.composition.navigation;
    for i in [
        Input {
            mouse: [100., 100.],
            pressed: Some((MouseButton::Left, [100., 100.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Input {
            mouse: [200., 160.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Input {
            mouse: [200., 160.],
            released: Some((MouseButton::Left, [200., 160.])),
            ..Default::default()
        },
    ] {
        super::super::composition::render(&mut list, &mut atlas, area, &mut p, &s, &i, &mut out);
    }
    assert_ne!(p.composition.navigation, before);
    assert_eq!(active(&p), Active::Hand);
    assert_eq!(p.cursor, Some(Cursor::Grab));
    assert!(out.is_empty());
    choose(&mut p, Active::Zoom, &s, &mut out);
    let before = p.composition.navigation;
    super::super::composition::render(
        &mut list,
        &mut atlas,
        area,
        &mut p,
        &s,
        &Input {
            mouse: [300., 200.],
            pressed: Some((MouseButton::Left, [300., 200.])),
            ..Default::default()
        },
        &mut out,
    );
    assert_ne!(p.composition.navigation, before);
    assert!(out.is_empty());
    choose(&mut p, Active::Rectangle, &s, &mut out);
    for i in [
        Input {
            mouse: [300., 200.],
            pressed: Some((MouseButton::Middle, [300., 200.])),
            held: Some(MouseButton::Middle),
            ..Default::default()
        },
        Input {
            mouse: [340., 220.],
            held: Some(MouseButton::Middle),
            ..Default::default()
        },
        Input {
            mouse: [340., 220.],
            released: Some((MouseButton::Middle, [340., 220.])),
            ..Default::default()
        },
    ] {
        super::super::composition::render(&mut list, &mut atlas, area, &mut p, &s, &i, &mut out);
    }
    assert_eq!(active(&p), Active::Rectangle);
    assert!(!p.tools.editing());
    assert_eq!(p.cursor, Some(Cursor::Crosshair));
    assert!(out.is_empty());
}
#[test]
fn remapped_viewer_tools_keep_text_property_and_global_shortcuts_separate() {
    use crate::shortcuts::Scope;
    let s = state();
    let mut p = Panels {
        focused: Some(Panel::Composition),
        ..Default::default()
    };
    std::sync::Arc::make_mut(&mut p.shortcut_bindings)
        .bind(
            Scope::Viewer,
            Command::ToolShape,
            Some(motion_ui::input::KeyChord::new(Key::B, false, false, false)),
        )
        .unwrap();
    assert!(std::sync::Arc::make_mut(&mut p.shortcut_bindings)
        .bind(
            Scope::Viewer,
            Command::ToolText,
            Some(motion_ui::input::KeyChord::new(Key::S, true, false, false))
        )
        .is_err());
    assert!(route(&mut p, &s, key(Key::Q)).is_empty());
    assert_eq!(active(&p), Active::Select);
    assert!(route(&mut p, &s, key(Key::B)).is_empty());
    assert_eq!(active(&p), Active::Rectangle);
    assert!(hint(Active::Rectangle, &p).contains("    B ·"));
    assert!(route(&mut p, &s, key(Key::T)).is_empty());
    assert_eq!(active(&p), Active::Text);
    let mut new = key(Key::T);
    new.keys[0].1 = Modifiers {
        control: true,
        shift: true,
        alt: true,
    };
    assert!(matches!(
        route(&mut p, &s, new).as_slice(),
        [Action::NewText]
    ));
    p.focused = Some(Panel::Timeline);
    route(&mut p, &s, key(Key::T));
    assert_eq!(active(&p), Active::Text);
}

#[test]
fn actual_nudge_failure_reaches_default_sinks_and_repeat_observation_resets() {
    const CHILD: &str = "MOTION_TOOLS_ERROR_CHILD";
    const NAME:&str="panels::tools::tests::actual_nudge_failure_reaches_default_sinks_and_repeat_observation_resets";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let err = String::from_utf8_lossy(&result.stderr);
        let out = String::from_utf8_lossy(&result.stdout);
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        for text in [&*err, &*file] {
            assert_eq!(
                text.matches("viewer tool rejected operation=nudge_layers")
                    .count(),
                2
            );
            assert_eq!(text.matches("site=viewer.tool.nudge").count(), 1);
            assert!(text.contains("sampled position missing for layer 7"));
        }
        assert!(!out.contains("viewer tool rejected"));
        assert!(!out.contains("site=viewer.tool.nudge"));
        return;
    };
    let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
    let (_root, _engine, s) = host();
    let mut broken = s.clone();
    broken["sampledLayers"] = serde_json::json!([]);
    let mut p = Panels {
        focused: Some(Panel::Composition),
        ..Default::default()
    };
    p.select_only(Some(7));
    assert!(route(&mut p, &broken, key(Key::Right)).is_empty());
    assert!(p
        .message
        .as_deref()
        .unwrap()
        .contains("sampled position missing"));
    let mut repeated = key(Key::Right);
    repeated.key_repeats = vec![true];
    for _ in 0..59 {
        assert!(route(&mut p, &broken, repeated.clone()).is_empty());
    }
    assert!(p.tools.nudge_failed);
    assert!(route(&mut p, &broken, repeated.clone()).is_empty());
    assert!(matches!(
        route(&mut p, &s, repeated.clone()).as_slice(),
        [Action::EditBatch(_)]
    ));
    assert!(!p.tools.nudge_failed);
    assert!(route(&mut p, &broken, repeated).is_empty());
}

#[cfg(target_os = "windows")]
#[path = "tests/native.rs"]
mod native;

#[test]
fn auxiliary_gestures_cancel_at_native_owner_and_project_context_boundaries() {
    let s = state();
    let mut p = Panels::default();
    let mut out = vec![];
    let mut list = PaintList::default();
    let view = Rect::new(0., 0., 960., 540.);
    let mapping = p
        .composition
        .navigation
        .mapping(view, [1920., 1080.])
        .unwrap();
    choose(&mut p, Active::Rectangle, &s, &mut out);
    let begin = Input {
        mouse: [100., 100.],
        pressed: Some((MouseButton::Left, [100., 100.])),
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    prepare(&mut p, None, &begin);
    viewport(&mut p, &mut list, view, mapping, &s, &begin, &mut out);
    assert!(p.tools.editing());
    prepare(&mut p, Some(Panel::Composition), &Input::default());
    assert!(
        p.tools.editing(),
        "background paint cannot steal main input"
    );
    prepare(&mut p, Some(Panel::Composition), &key(Key::Q));
    assert!(!p.tools.editing());
    viewport(
        &mut p,
        &mut list,
        view,
        mapping,
        &s,
        &Input {
            mouse: [500., 400.],
            released: Some((MouseButton::Left, [500., 400.])),
            ..Default::default()
        },
        &mut out,
    );
    assert!(out.is_empty());
    viewport(&mut p, &mut list, view, mapping, &s, &begin, &mut out);
    assert!(p.tools.editing());
    p.composition.context = Some(42);
    viewport(
        &mut p,
        &mut list,
        view,
        mapping,
        &s,
        &Input {
            mouse: [500., 400.],
            released: Some((MouseButton::Left, [500., 400.])),
            ..Default::default()
        },
        &mut out,
    );
    assert!(out.is_empty());
    assert!(!p.tools.editing());
}
#[test]
fn observation_camera_tool_uses_existing_orbit_commands_and_disables_on_exit() {
    let (_root, e, _s) = host();
    let s = e.observe(true, 0., 0.).unwrap();
    let mut p = Panels {
        focused: Some(Panel::Composition),
        ..Default::default()
    };
    let mut out = route(&mut p, &s, key(Key::C));
    assert!(out.is_empty());
    assert_eq!(active(&p), Active::Camera);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let area = Rect::new(0., 0., 900., 600.);
    for i in [
        Input {
            mouse: [100., 100.],
            pressed: Some((MouseButton::Left, [100., 100.])),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Input {
            mouse: [150., 120.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Input {
            mouse: [150., 120.],
            released: Some((MouseButton::Left, [150., 120.])),
            ..Default::default()
        },
    ] {
        super::super::composition::render(&mut list, &mut atlas, area, &mut p, &s, &i, &mut out);
    }
    assert!(matches!(out.as_slice(), [Action::ViewOrbit { .. }]));
    let s = e.observe(false, 0., 0.).unwrap();
    out.clear();
    super::super::composition::render(
        &mut list,
        &mut atlas,
        area,
        &mut p,
        &s,
        &Input::default(),
        &mut out,
    );
    assert_eq!(active(&p), Active::Select);
    assert!(out.is_empty());
}
