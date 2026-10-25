use super::*;
use crate::{
    i18n::Locale,
    panels::{self, animation_controls::TrackSelection},
    shortcuts::Command,
};
use motion_core::{Engine, Layer, Project, Property};
use motion_ui::{
    dock::Panel,
    input::{Input, Key, Modifiers},
};

fn fixture() -> (Panels, Engine, Value) {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Keys", [32.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 110,
        offset_frame: 10,
    });
    layer.transform.position.set_animated(0, true).unwrap();
    layer.transform.position.set_at(20, [40.; 3]).unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "posterize")
        .unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [32.; 2],
    );
    let parameter = effect.params.get_mut("levels").unwrap();
    parameter.track.set_animated(0, true).unwrap();
    parameter.track.set_at(20, [8., 0., 0., 0.]).unwrap();
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let engine = Engine::new(project).unwrap();
    let state = serde_json::json!({"root":"fixture", "composition":"main", "revision":engine.revision(), "project":engine.project(), "timeline_layers":engine.project().timeline_layers(10.).unwrap(), "frame":10});
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        keyframe_clipboard_available: true,
        clipboard_available: true,
        ..Default::default()
    };
    panels.select_only(Some(7));
    (panels, engine, state)
}
fn chord(key: Key) -> Input {
    Input {
        keys: vec![(
            key,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
fn route(panels: &mut Panels, state: &Value, input: &Input, owner: Option<Panel>) -> Vec<Action> {
    let mut actions = vec![];
    panels::keyboard::route(input, panels, state, false, owner, &mut actions);
    actions
}

#[test]
fn clipboard_property_and_single_key_route_in_both_locales_and_timeline_modes() {
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Timeline)] {
            for graph in [false, true] {
                let (mut panels, engine, state) = fixture();
                panels.locale = locale;
                panels.timeline.graph_mode = graph;
                panels
                    .animation_controls
                    .choose_track_in(&state, 7, Property::Position, None);
                let actions = route(&mut panels, &state, &chord(Key::C), owner);
                let [Action::KeyframeClipboard {
                    intent: Intent::Copy,
                    selections,
                    targets,
                    ..
                }] = actions.as_slice()
                else {
                    panic!("{actions:?}")
                };
                assert_eq!(selections[0].frames, [10, 30]);
                assert_eq!(
                    targets,
                    &[KeyframeTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: None
                    }]
                );
                motion_core::KeyframeClipboard::capture(engine.project(), selections).unwrap();
                panels
                    .animation_controls
                    .select_key_in(&state, 7, Property::Position, None, 30);
                let actions = route(&mut panels, &state, &chord(Key::X), owner);
                let [Action::KeyframeClipboard {
                    intent: Intent::Cut,
                    selections,
                    ..
                }] = actions.as_slice()
                else {
                    panic!("{actions:?}")
                };
                assert_eq!(selections[0].frames, [30]);
                let actions = route(&mut panels, &state, &chord(Key::V), owner);
                assert!(matches!(
                    actions.as_slice(),
                    [Action::KeyframeClipboard {
                        intent: Intent::Paste { in_place: false },
                        ..
                    }]
                ));
            }
        }
    }
}

#[test]
fn clipboard_projection_and_stale_property_never_steal_each_others_scope() {
    let (mut panels, _, state) = fixture();
    panels
        .animation_controls
        .display_track(7, Property::Position, None);
    assert!(matches!(
        route(&mut panels, &state, &chord(Key::C), None).as_slice(),
        [Action::LayerClipboard(Intent::Copy)]
    ));
    panels
        .animation_controls
        .choose_track_in(&state, 7, Property::Position, None);
    let mut stale = state.clone();
    stale["composition"] = "different".into();
    assert_eq!(
        panels.animation_controls.track_selection(&stale, Some(7)),
        TrackSelection::Invalid
    );
    for key in [Key::C, Key::X, Key::V] {
        assert!(route(&mut panels, &stale, &chord(key), None).is_empty());
    }
    panels
        .animation_controls
        .select_key_in(&state, 7, Property::Position, None, 99);
    assert!(route(&mut panels, &state, &chord(Key::C), None).is_empty());
    assert!(matches!(
        route(&mut panels, &state, &chord(Key::V), None).as_slice(),
        [Action::KeyframeClipboard { .. }]
    ));
}

#[test]
fn clipboard_menu_retains_property_owner_and_disables_revised_locked_and_busy_writes() {
    for locale in [Locale::Zh, Locale::En] {
        let (mut panels, _, mut state) = fixture();
        panels.locale = locale;
        panels
            .animation_controls
            .choose_track_in(&state, 7, Property::Position, None);
        state["project"]["layers"][0]["locked"] = true.into();
        let entries = panels::clipboard_menu::entries(&panels, &state);
        assert_eq!(
            entries.iter().map(|e| e.2).collect::<Vec<_>>(),
            [true, false, false, false, false]
        );
        state["project"]["layers"][0]["locked"] = false.into();
        panels.clipboard_menu_keys = selection(&panels, &state);
        panels.clipboard_menu_allowed = Some(true);
        panels.focused = None;
        panels.menu = Some(1);
        let entries = panels::clipboard_menu::entries(&panels, &state);
        assert!(entries[0].0.contains(if locale == Locale::Zh {
            "关键帧"
        } else {
            "Keyframes"
        }));
        assert!(entries[..4]
            .iter()
            .all(|e| e.2 && matches!(&e.1, Action::KeyframeClipboard { .. })));
        panels.clipboard_busy = true;
        assert_eq!(
            panels::clipboard_menu::entries(&panels, &state)
                .iter()
                .map(|e| e.2)
                .collect::<Vec<_>>(),
            [false, false, false, false, true]
        );
        panels.clipboard_busy = false;
        state["revision"] = 1.into();
        assert!(panels::clipboard_menu::entries(&panels, &state)
            .iter()
            .all(|e| !e.2));
    }
}

#[test]
fn clipboard_effect_owner_and_inspector_projection_copy_real_parameter_tracks() {
    for inspector in [false, true] {
        let (mut panels, engine, state) = fixture();
        panels.focused = Some(if inspector {
            Panel::Properties
        } else {
            Panel::EffectControls
        });
        if inspector {
            panels
                .properties
                .effects
                .choose_parameter(&state, 7, 1, "levels");
        } else {
            panels
                .effect_controls
                .choose_parameter(&state, 7, 1, "levels");
        }
        let actions = route(&mut panels, &state, &chord(Key::C), None);
        let [Action::KeyframeClipboard {
            selections,
            targets,
            ..
        }] = actions.as_slice()
        else {
            panic!("{actions:?}")
        };
        assert_eq!(selections[0].frames, [10, 30]);
        assert_eq!(
            targets,
            &[KeyframeTarget::Effect {
                object: 7,
                effect: 1,
                param: "levels".into()
            }]
        );
        motion_core::KeyframeClipboard::capture(engine.project(), selections).unwrap();
    }
}

#[test]
fn clipboard_shortcuts_defer_to_text_ime_menu_busy_and_custom_binding() {
    let (mut panels, _, state) = fixture();
    panels
        .animation_controls
        .choose_track_in(&state, 7, Property::Position, None);
    for input in [
        Input {
            composing: true,
            ..chord(Key::C)
        },
        Input {
            ime_consumed: true,
            ..chord(Key::C)
        },
        Input {
            focus_lost: true,
            ..chord(Key::C)
        },
        Input {
            key_repeats: vec![true],
            ..chord(Key::C)
        },
    ] {
        assert!(route(&mut panels, &state, &input, None).is_empty());
    }
    panels.timeline.timeline_search = true;
    assert!(route(&mut panels, &state, &chord(Key::C), None).is_empty());
    panels.timeline.timeline_search = false;
    panels.clipboard_busy = true;
    assert!(route(&mut panels, &state, &chord(Key::C), None).is_empty());
    panels.clipboard_busy = false;
    let mut bindings = crate::shortcuts::bindings::Bindings::default();
    bindings
        .bind(
            crate::shortcuts::Scope::Animation,
            Command::CopySelection,
            Some(motion_ui::input::KeyChord::new(Key::B, true, false, false)),
        )
        .unwrap();
    panels.shortcut_bindings = std::sync::Arc::new(bindings);
    assert!(route(&mut panels, &state, &chord(Key::C), None).is_empty());
    assert!(matches!(
        route(&mut panels, &state, &chord(Key::B), None).as_slice(),
        [Action::KeyframeClipboard { .. }]
    ));
}

#[test]
fn clipboard_malformed_key_snapshots_reach_default_error_channel_with_bounded_lifetime_evidence() {
    const NAME: &str = "panels::keyframe_clipboard::tests::clipboard_malformed_key_snapshots_reach_default_error_channel_with_bounded_lifetime_evidence";
    const CHILD: &str = "MOTION_CLIPBOARD_SELECTION_CHANNEL_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(root.clone().into()).unwrap();
        let (_, _, state) = fixture();
        crate::logging::flush();
        let path = std::path::PathBuf::from(&root).join("motion-studio.log");
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            json_frames(&state, &serde_json::json!({"keys":[]}), 0, 7),
            Some(vec![])
        );
        crate::logging::flush();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        for _ in 0..120 {
            assert!(
                json_frames(&state, &serde_json::json!({"keys":[{"frame":"bad"}]}), 0, 7).is_none()
            );
            assert_eq!(
                json_frames(&state, &serde_json::json!({"keys":[{"frame":2}]}), 0, 7),
                Some(vec![2])
            );
        }
        assert!(json_frames(&state, &Value::Null, 0, 7).is_none());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CLIPBOARD_SELECTION_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture"])
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
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let records: Vec<_> = file
        .lines()
        .filter(|line| line.contains("site=clipboard.key_frames "))
        .collect();
    assert_eq!(records.len(), 2);
    assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
    assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
    assert!(records
        .iter()
        .all(|line| line.contains("phase=select object=7")
            && line.contains("key frame is not an integer")));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("persistent panel degradation site=clipboard.key_frames"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("count=1 "));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}
