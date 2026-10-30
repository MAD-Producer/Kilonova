use super::super::tests::{commands, context, fixture, snapshot};
use super::*;
use motion_core::{Curve, CurveShape, CurveSpace, Engine, SpatialTangents};

#[test]
fn automatic_shortcut_matches_menu_in_main_and_floating_owners_with_single_undo() {
    use motion_ui::input::{Key, Modifiers};
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Timeline), Some(Panel::Curves)] {
            let (mut engine, s) = fixture();
            let before = engine.snapshot();
            let mut p = context(&s, locale);
            p.focused = Some(owner.unwrap_or(Panel::Timeline));
            for desired in [true, false] {
                let s = snapshot(&engine);
                let mut menu = vec![];
                super::super::apply(&mut p, &s, Operation::Automatic(desired), &mut menu);
                let mut keys = vec![];
                super::super::super::keyboard::route(
                    &Input {
                        keys: vec![(
                            Key::F9,
                            Modifiers {
                                control: true,
                                alt: true,
                                shift: false,
                            },
                        )],
                        ..Default::default()
                    },
                    &mut p,
                    &s,
                    false,
                    owner,
                    &mut keys,
                );
                let menu = commands(menu);
                let keys = commands(keys);
                assert!(
                    !keys.is_empty(),
                    "locale={locale:?} owner={owner:?} enabled={desired}"
                );
                assert_eq!(
                    serde_json::to_value(&menu).unwrap(),
                    serde_json::to_value(&keys).unwrap()
                );
                engine.apply_batch(keys).unwrap();
            }
            engine.undo().unwrap();
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn automatic_shortcut_respects_text_ime_repeat_focus_loss_and_readonly_selection() {
    use motion_ui::input::{Key, Modifiers};
    let (engine, s) = fixture();
    for owner in [None, Some(Panel::Timeline)] {
        for fault in [
            "text", "ime", "consumed", "repeat", "focus", "readonly", "stale",
        ] {
            let mut state = s.clone();
            let mut p = context(&state, Locale::Zh);
            let mut input = Input {
                keys: vec![(
                    Key::F9,
                    Modifiers {
                        control: true,
                        alt: true,
                        shift: false,
                    },
                )],
                ..Default::default()
            };
            match fault {
                "text" => p.timeline.timeline_search = true,
                "ime" => input.composing = true,
                "consumed" => input.ime_consumed = true,
                "repeat" => input.key_repeats = vec![true],
                "focus" => input.focus_lost = true,
                "readonly" => state["observing"] = true.into(),
                _ => state["document_generation"] = 5.into(),
            }
            let mut out = vec![];
            super::super::super::keyboard::route(&input, &mut p, &state, false, owner, &mut out);
            assert!(out.is_empty(), "fault={fault} owner={owner:?}");
        }
    }
    assert!(!engine.can_undo());
}

#[test]
fn automatic_shortcut_conflicts_are_rejected_and_custom_presets_keep_their_owner() {
    use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
    use motion_ui::input::{Key, KeyChord};
    let chord = KeyChord::new(Key::F9, true, false, true);
    let mut bindings = Bindings::default();
    bindings.validate().unwrap();
    assert_eq!(
        bindings.resolve(Scope::Animation, chord.key, chord.modifiers, false),
        Some(Shortcut::ToggleAutomaticBezier)
    );
    assert!(bindings
        .bind(Scope::Animation, Shortcut::ToggleGraph, Some(chord))
        .is_err());
    assert_eq!(bindings, Bindings::default());
    let custom = serde_json::json!({"overrides":[{"scope":"animation","command":"toggle_graph","chord":chord}]});
    let preserved: Bindings = serde_json::from_value(custom).unwrap();
    preserved.validate().unwrap();
    assert_eq!(
        preserved.resolve(Scope::Animation, chord.key, chord.modifiers, false),
        Some(Shortcut::ToggleGraph)
    );
    assert!(preserved
        .hint(Scope::Animation, Shortcut::ToggleAutomaticBezier)
        .is_empty());
}

#[test]
fn malformed_type_snapshots_reach_default_error_channels_and_healthy_capabilities_stay_quiet() {
    const CHILD: &str = "MOTION_TYPE_MENU_ERROR_CHILD";
    const NAME: &str = "panels::animation_menu::type_state::tests::malformed_type_snapshots_reach_default_error_channels_and_healthy_capabilities_stay_quiet";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let (engine, healthy) = fixture();
        let p = context(&healthy, Locale::Zh);
        assert!(summary(&p, &healthy).enable);
        assert!(std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .is_empty());
        let mut bad = healthy.clone();
        bad["revision"] = 1.into();
        bad["project"]["layers"][0]["transform"]["position"]["keys"][0]["frame"] = "invalid".into();
        for _ in 0..120 {
            assert_eq!(summary(&p, &bad), Summary::default());
        }
        let mut unordered = healthy.clone();
        unordered["revision"] = 2.into();
        unordered["project"]["layers"][0]["transform"]["position"]["keys"][0]["frame"] = 15.into();
        for _ in 0..120 {
            assert_eq!(summary(&p, &unordered), Summary::default());
        }
        let failure = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let records: Vec<_> = failure
            .lines()
            .filter(|line| line.contains("site=animation.types.frames "))
            .collect();
        assert_eq!(records.len(), 2, "{failure}");
        assert!(records[0].contains("count=1 ") && records[0].contains("WARN"));
        assert!(records[1].contains("count=60 ") && records[1].contains("ERROR"));
        assert!(records
            .iter()
            .all(|line| line.contains("object=7") && line.contains("phase=type_capability")));
        for _ in 0..120 {
            assert!(summary(&p, &healthy).enable);
        }
        assert_eq!(
            std::fs::read_to_string(root.join("motion-studio.log")).unwrap(),
            failure
        );
        assert!(!engine.can_undo());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TYPE_MENU_ERROR_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("site=animation.types.frames ") && stderr.contains("count=60 "));
    assert!(!stderr.contains("count=1 "));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("animation.types."));
}

#[test]
fn discrete_effects_are_disabled_even_though_their_storage_is_a_numeric_vector() {
    use motion_effects::ParamKind;
    let package = motion_effects::builtin::package().unwrap();
    for kind in [ParamKind::Bool, ParamKind::Enum] {
        let (definition, parameter) = package
            .manifest
            .effects
            .iter()
            .find_map(|effect| {
                effect
                    .params
                    .iter()
                    .find(|p| p.kind == kind && p.animatable && p.implemented)
                    .map(|p| (effect, p))
            })
            .unwrap();
        let mut effect = motion_core::EffectInstance::new(
            1,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [64.; 2],
        );
        let param = effect.params.get_mut(&parameter.id).unwrap();
        for frame in [0, 10] {
            param
                .track
                .upsert(frame, param.default, Ease::Hold)
                .unwrap();
        }
        let (engine, _) = fixture();
        let mut project = engine.snapshot();
        project.layers[0].effects.push(effect);
        project.rebuild_plugin_dependencies();
        let engine = Engine::new(project).unwrap();
        let s = snapshot(&engine);
        let mut p = context(&s, Locale::Zh);
        p.selected_keys
            .apply(
                &s,
                [super::super::super::key_selection::Key {
                    target: KeyframeTarget::Effect {
                        object: 7,
                        effect: 1,
                        param: parameter.id.clone(),
                    },
                    frame: 22,
                }],
                super::super::super::key_selection::Mode::Replace,
            )
            .unwrap();
        let status = summary(&p, &s);
        assert_eq!(status.kind, Some(Kind::Hold));
        assert!(!status.available(true) && !status.available(false));
        assert!(!super::super::enabled(&p, &s, Operation::Automatic(true)));
        assert!(!engine.can_undo());
    }
}

fn select(p: &mut Panels, s: &Value, frames: &[u32]) {
    p.selected_keys
        .apply(
            s,
            frames
                .iter()
                .map(|frame| super::super::super::key_selection::Key {
                    target: KeyframeTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                    },
                    frame: *frame,
                }),
            super::super::super::key_selection::Mode::Replace,
        )
        .unwrap();
}

#[test]
fn menu_types_convert_only_selected_keys_freeze_exact_samples_and_restore_on_undo() {
    for locale in [Locale::Zh, Locale::En] {
        let (mut engine, s) = fixture();
        let before = engine.snapshot();
        let mut p = context(&s, locale);
        assert!(summary(&p, &s).checked(Operation::Interpolation(Ease::Linear)));
        assert!(!super::super::enabled(&p, &s, Operation::Automatic(false)));
        let mut actions = vec![];
        super::super::apply(&mut p, &s, Operation::Automatic(true), &mut actions);
        let [Action::EditorAction { source, .. }] = actions.as_slice() else {
            panic!("automatic type must retain its source");
        };
        assert_eq!(source, &crate::layer_clipboard::KeyframeSource::of(&s));
        engine.apply_batch(commands(actions)).unwrap();
        let after = engine.snapshot();
        let keys = &after.layers[0].transform.position.keys;
        assert_eq!(
            keys.iter().map(|key| key.auto_bezier).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(after.layers[1..], before.layers[1..]);
        let s = snapshot(&engine);
        assert!(summary(&p, &s).checked(Operation::Automatic(true)));
        assert!(
            !super::super::enabled(&p, &s, Operation::Automatic(true)),
            "no redundant undo entry"
        );
        let values: Vec<_> = (0..190)
            .map(|i| {
                after.layers[0]
                    .transform
                    .position
                    .sample(i as f64 / 10.)
                    .unwrap()
            })
            .collect();
        let mut actions = vec![];
        super::super::apply(&mut p, &s, Operation::Automatic(false), &mut actions);
        engine.apply_batch(commands(actions)).unwrap();
        for (i, value) in values.into_iter().enumerate() {
            assert_eq!(
                value,
                engine.project().layers[0]
                    .transform
                    .position
                    .sample(i as f64 / 10.)
                    .unwrap()
            );
        }
        assert_eq!(summary(&p, &snapshot(&engine)).kind, Some(Kind::Curve));
        engine.undo().unwrap();
        assert_eq!(engine.project(), &after);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn selection_state_and_checkmarks_distinguish_mixed_manual_hold_and_automatic() {
    let (mut engine, s) = fixture();
    let mut p = context(&s, Locale::Zh);
    select(&mut p, &s, &[12, 22]);
    assert_eq!(summary(&p, &s).kind, Some(Kind::Linear));
    engine
        .apply(Command::KeyframeAutomatic {
            selections: vec![KeyframeSelection {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                frames: vec![22],
            }],
            enabled: true,
        })
        .unwrap();
    let s = snapshot(&engine);
    let mixed = summary(&p, &s);
    assert!(mixed.mixed_automatic());
    assert_eq!(mixed.kind, Some(Kind::Mixed));
    assert!(!mixed.checked(Operation::Automatic(true)));
    assert!(!mixed.checked(Operation::Interpolation(Ease::Linear)));
    assert!(mixed.available(true) && mixed.available(false));
    for locale in [Locale::Zh, Locale::En] {
        p.locale = locale;
        assert!(super::super::entries(&p, &s)
            .iter()
            .any(|(label, op, _)| matches!(op, Operation::Automatic(true))
                && label.contains(if locale == Locale::Zh {
                    "部分启用"
                } else {
                    "partially enabled"
                })));
    }
    let mut actions = vec![];
    super::super::apply(
        &mut p,
        &s,
        Operation::Interpolation(Ease::Hold),
        &mut actions,
    );
    engine.apply_batch(commands(actions)).unwrap();
    assert!(summary(&p, &snapshot(&engine)).checked(Operation::Interpolation(Ease::Hold)));
    assert!(!summary(&p, &snapshot(&engine)).checked(Operation::Automatic(true)));
}

#[test]
fn unsupported_neighbours_and_single_key_disable_automatic_without_mutating_history() {
    let (original, _) = fixture();
    for fault in [
        "spatial",
        "velocity",
        "elastic",
        "one_key",
        "separated_without_axis",
    ] {
        let mut project = original.snapshot();
        let track = &mut project.layers[0].transform.position;
        match fault {
            "spatial" => track
                .set_spatial(
                    0,
                    Some(SpatialTangents {
                        incoming: None,
                        outgoing: Some([1., 0., 0.]),
                    }),
                )
                .unwrap(),
            "velocity" => track
                .set_curve(
                    0,
                    motion_core::Easing {
                        ease: Ease::Linear,
                        curve: Some(Curve {
                            space: CurveSpace::Velocity,
                            shape: CurveShape::Cubic {
                                control1: [0.2, 0.1],
                                control2: [0.8, 0.9],
                                start: 0.,
                                end: 1.,
                            },
                        }),
                    },
                )
                .unwrap(),
            "elastic" => track
                .set_curve(
                    0,
                    motion_core::Easing {
                        ease: Ease::Linear,
                        curve: Some(Curve {
                            space: CurveSpace::Progress,
                            shape: CurveShape::Elastic {
                                oscillations: 2.,
                                damping: 5.,
                            },
                        }),
                    },
                )
                .unwrap(),
            "one_key" => track.keys.retain(|key| key.frame == 10),
            _ => track.separate().unwrap(),
        }
        let engine = Engine::new(project).unwrap();
        let s = snapshot(&engine);
        let mut p = context(&s, Locale::Zh);
        let before = engine.snapshot();
        for enabled in [false, true] {
            assert!(
                !super::super::enabled(&p, &s, Operation::Automatic(enabled)),
                "{fault}"
            );
            let mut actions = vec![];
            super::super::apply(&mut p, &s, Operation::Automatic(enabled), &mut actions);
            assert!(actions.is_empty(), "{fault}");
        }
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn type_cache_reuses_revision_and_selection_but_invalidates_authored_changes() {
    let (mut engine, s) = fixture();
    let mut p = context(&s, Locale::Zh);
    let initial = summary(&p, &s);
    for _ in 0..120 {
        assert_eq!(summary(&p, &s), initial);
        let _ = super::super::entries(&p, &s);
    }
    assert_eq!(p.animation_type_cache.borrow().solves, 1);
    engine
        .apply(Command::KeyframeAutomatic {
            selections: vec![KeyframeSelection {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                frames: vec![22],
            }],
            enabled: true,
        })
        .unwrap();
    let mut changed = snapshot(&engine);
    assert_ne!(summary(&p, &changed), initial);
    assert_eq!(p.animation_type_cache.borrow().solves, 2);
    select(&mut p, &changed, &[12]);
    assert_eq!(summary(&p, &changed).kind, Some(Kind::Linear));
    assert_eq!(p.animation_type_cache.borrow().solves, 3);
    changed["revision"] = Value::Null;
    for _ in 0..2 {
        let _ = summary(&p, &changed);
    }
    assert_eq!(p.animation_type_cache.borrow().solves, 5);
}
