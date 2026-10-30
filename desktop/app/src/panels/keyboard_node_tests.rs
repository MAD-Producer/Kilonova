use super::*;
use motion_ui::input::Modifiers;

#[test]
fn node_scope_application_reachability_matrix() {
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    let ctrl_alt = Modifiers { alt: true, ..ctrl };
    let state = serde_json::json!({"frame":20,"project":{"frames":120,"layers":[]}});
    for panel in [Panel::Nodes, Panel::NodeProperties, Panel::NodePreview] {
        for owner in [None, Some(panel)] {
            for (key, modifiers, expected) in [
                (Key::N, ctrl_alt, "new"),
                (Key::N, ctrl, "composition"),
                (Key::I, ctrl, "import"),
                (Key::K, ctrl, "settings"),
                (Key::Y, Modifiers { shift: true, ..ctrl }, "solid"),
                // Redo belongs to the empty node document, not project creation.
                (Key::Y, ctrl, "blocked"),
                (Key::End, Modifiers::default(), "seek"),
                (Key::PageUp, Modifiers::default(), "seek"),
                (Key::PageDown, Modifiers::default(), "seek"),
                (Key::Left, ctrl, "seek"),
                (Key::Right, ctrl, "seek"),
                (Key::Home, Modifiers::default(), "blocked"),
                (Key::Delete, Modifiers::default(), "blocked"),
                (Key::D, ctrl, "blocked"),
                (
                    Key::D,
                    Modifiers {
                        shift: true,
                        ..ctrl
                    },
                    "blocked",
                ),
                (Key::Left, Modifiers::default(), "blocked"),
                (
                    Key::M,
                    ctrl_alt,
                    if cfg!(feature = "ffmpeg") {
                        "export"
                    } else {
                        "blocked"
                    },
                ),
            ] {
                let mut panels = Panels {
                    focused: Some(panel),
                    ..Default::default()
                };
                let mut out = vec![];
                route(
                    &Input {
                        keys: vec![(key, modifiers)],
                        ..Default::default()
                    },
                    &mut panels,
                    &state,
                    true,
                    owner,
                    &mut out,
                );
                let actual = match out.as_slice() {
                    [] => "blocked",
                    [Action::New] => "new",
                    [Action::NewComposition] => "composition",
                    [Action::ImportMedia] => "import",
                    [Action::CompositionSettings] => "settings",
                    [Action::AddSolid] => "solid",
                    [Action::Seek(_)] => "seek",
                    [Action::ExportMovie] => "export",
                    _ => panic!("unexpected actions: {out:?}"),
                };
                assert_eq!(
                    actual, expected,
                    "panel={panel:?} owner={owner:?} chord={key:?}/{modifiers:?}"
                );
            }
            for (composing, ime_consumed, repeat) in [
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                let mut panels = Panels {
                    focused: Some(panel),
                    ..Default::default()
                };
                let playing = panels.node_preview.playing;
                let mut out = vec![];
                route(
                    &Input {
                        keys: vec![(Key::Space, Modifiers::default())],
                        key_repeats: vec![repeat],
                        composing,
                        ime_consumed,
                        ..Default::default()
                    },
                    &mut panels,
                    &state,
                    true,
                    owner,
                    &mut out,
                );
                assert_eq!(panels.node_preview.playing, playing);
                assert!(out.is_empty());
            }
        }
    }
}

#[test]
fn space_routes_once_without_a_node_canvas_in_main_and_floating_windows() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    let mut atlas = motion_ui::TextAtlas::from_system(12.).unwrap();
    for panel in [Panel::Nodes, Panel::NodeProperties, Panel::NodePreview] {
        for floating in [false, true] {
            for locale in [crate::i18n::Locale::Zh, crate::i18n::Locale::En] {
                let mut panels = Panels {
                    focused: Some(panel),
                    locale,
                    ..Default::default()
                };
                let initial = panels.node_preview.playing;
                let mut render = |panels: &mut Panels, input: &Input| {
                    let mut list = motion_ui::PaintList::default();
                    if floating {
                        crate::panels::floating_chrome(
                            &mut list,
                            &mut atlas,
                            panel,
                            [1280., 700.],
                            panels,
                            &state,
                            input,
                        )
                    } else {
                        crate::panels::chrome(
                            &mut list,
                            &mut atlas,
                            [1280., 700.],
                            &motion_ui::dock::Node::dock(panel.dock()),
                            panels,
                            &state,
                            input,
                        )
                    }
                };
                let actions = render(
                    &mut panels,
                    &Input {
                        keys: vec![(Key::Space, Modifiers::default())],
                        ..Default::default()
                    },
                );
                assert_eq!(
                    panels.node_preview.playing, !initial,
                    "{panel:?} floating={floating}"
                );
                assert!(!actions.iter().any(|a| matches!(a, Action::TogglePlay)));
                render(&mut panels, &Input::default());
                assert_eq!(panels.node_preview.playing, !initial);
                render(
                    &mut panels,
                    &Input {
                        keys: vec![(Key::Space, Modifiers::default())],
                        ..Default::default()
                    },
                );
                assert_eq!(panels.node_preview.playing, initial);
            }
        }
    }
}

#[test]
fn stale_canvas_focus_does_not_steal_timeline_space() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels.node_editor.focus_panel();
    let playing = panels.node_preview.playing;
    let mut layout = motion_ui::dock::Node::dock(Panel::Nodes.dock());
    layout.split(motion_ui::dock::Edge::Bottom, Panel::Timeline.dock());
    let mut atlas = motion_ui::TextAtlas::from_system(12.).unwrap();
    let actions = crate::panels::chrome(
        &mut motion_ui::PaintList::default(),
        &mut atlas,
        [1280., 800.],
        &layout,
        &mut panels,
        &state,
        &Input {
            keys: vec![(Key::Space, Modifiers::default())],
            ..Default::default()
        },
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::TogglePlay))
            .count(),
        1
    );
    assert_eq!(panels.node_preview.playing, playing);
}
