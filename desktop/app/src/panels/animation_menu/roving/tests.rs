use super::super::tests::{commands, context, snapshot};
use super::*;
use motion_core::{Axis, Engine};

fn target(object: u64) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property: Property::Position,
        axis: None,
    }
}
pub(super) fn fixture() -> Engine {
    let (engine, _) = super::super::tests::fixture();
    let mut project = engine.snapshot();
    for layer in &mut project.layers {
        for (key, x) in layer.transform.position.keys.iter_mut().zip([0., 1., 8.]) {
            key.value = [x, 0., 0.];
        }
    }
    Engine::new(project).unwrap()
}

#[test]
fn roving_shortcut_matches_menu_in_main_and_floating_owners_and_rejects_text_ime_and_repeat() {
    use motion_ui::input::{Key, Modifiers};
    let modifiers = Modifiers {
        control: true,
        alt: true,
        shift: false,
    };
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Timeline), Some(Panel::Curves)] {
            let mut engine = fixture();
            let original = engine.snapshot();
            let mut p = context(&snapshot(&engine), locale);
            p.focused = Some(owner.unwrap_or(Panel::Timeline));
            for desired in [true, false] {
                let s = snapshot(&engine);
                let mut menu = vec![];
                super::super::apply(&mut p, &s, Operation::Roving(desired), &mut menu);
                let mut keys = vec![];
                super::super::super::keyboard::route(
                    &Input {
                        keys: vec![(Key::F8, modifiers)],
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
                    "locale={locale:?} owner={owner:?} desired={desired}"
                );
                assert_eq!(
                    serde_json::to_value(&menu).unwrap(),
                    serde_json::to_value(&keys).unwrap()
                );
                engine.apply_batch(keys).unwrap();
                p.normalize_selection(&snapshot(&engine));
            }
            engine.undo().unwrap();
            engine.undo().unwrap();
            assert_eq!(engine.project(), &original);
            assert!(!engine.can_undo());
        }
    }
    for fault in [
        "text", "ime", "consumed", "repeat", "focus", "readonly", "stale",
    ] {
        for owner in [None, Some(Panel::Timeline)] {
            let engine = fixture();
            let mut s = snapshot(&engine);
            let mut p = context(&s, Locale::Zh);
            let mut input = Input {
                keys: vec![(Key::F8, modifiers)],
                ..Default::default()
            };
            match fault {
                "text" => p.timeline.timeline_search = true,
                "ime" => input.composing = true,
                "consumed" => input.ime_consumed = true,
                "repeat" => input.key_repeats = vec![true],
                "focus" => input.focus_lost = true,
                "readonly" => s["observing"] = true.into(),
                _ => s["document_generation"] = 9.into(),
            }
            let mut out = vec![];
            super::super::super::keyboard::route(&input, &mut p, &s, false, owner, &mut out);
            assert!(out.is_empty(), "{fault} {owner:?}");
            assert!(!engine.can_undo());
        }
    }
}

#[test]
fn roving_binding_conflicts_remapping_and_saved_custom_chords_keep_their_owner() {
    use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
    use motion_ui::input::{Key, KeyChord, Modifiers};
    let chord = KeyChord::new(Key::F8, true, false, true);
    let mut bindings = Bindings::default();
    assert_eq!(
        bindings.resolve(Scope::Animation, chord.key, chord.modifiers, false),
        Some(Shortcut::ToggleRoving)
    );
    assert_eq!(
        bindings.resolve(Scope::Animation, chord.key, chord.modifiers, true),
        None
    );
    assert!(bindings
        .bind(
            Scope::Animation,
            Shortcut::ToggleAutomaticBezier,
            Some(chord)
        )
        .is_err());
    bindings
        .bind(
            Scope::Animation,
            Shortcut::ToggleRoving,
            Some(KeyChord::new(Key::F7, true, false, true)),
        )
        .unwrap();
    assert_eq!(
        bindings.resolve(Scope::Animation, Key::F8, chord.modifiers, false),
        None
    );
    assert_eq!(
        bindings.resolve(Scope::Animation, Key::F7, chord.modifiers, false),
        Some(Shortcut::ToggleRoving)
    );
    let loaded: Bindings = serde_json::from_value(serde_json::json!({"overrides":[{
        "scope":"animation","command":"toggle_automatic_bezier","chord":chord
    }]}))
    .unwrap();
    assert_eq!(
        loaded.resolve(Scope::Animation, Key::F8, chord.modifiers, false),
        Some(Shortcut::ToggleAutomaticBezier)
    );
    assert!(loaded
        .hint(Scope::Animation, Shortcut::ToggleRoving)
        .is_empty());
    assert_eq!(
        bindings.resolve(Scope::Animation, Key::F7, Modifiers::default(), false),
        None
    );
}
fn select(p: &mut Panels, s: &Value, keys: impl IntoIterator<Item = key_selection::AddressedKey>) {
    p.selected_keys
        .apply_addressed(s, keys, key_selection::Mode::Replace)
        .unwrap();
}
fn frame(object: u64, frame: u32) -> key_selection::AddressedKey {
    key_selection::Key {
        target: target(object),
        frame,
    }
    .into()
}
fn id(object: u64, id: u64) -> key_selection::AddressedKey {
    SpatialKey {
        target: target(object),
        id,
    }
    .into()
}
fn run(p: &mut Panels, engine: &mut Engine, enabled: bool) {
    let s = snapshot(engine);
    let mut out = vec![];
    super::super::apply(p, &s, Operation::Roving(enabled), &mut out);
    assert!(matches!(out.as_slice(),[Action::EditorAction {source,..}]
        if source==&crate::layer_clipboard::KeyframeSource::of(&s)));
    engine.apply_batch(commands(out)).unwrap();
    p.normalize_selection(&snapshot(engine));
}

#[test]
fn real_menu_conversion_rebinds_ids_and_freezing_preserves_samples_reopen_and_one_undo() {
    for locale in [Locale::Zh, Locale::En] {
        for owner in [Panel::Timeline, Panel::Curves] {
            let mut engine = fixture();
            let original = engine.snapshot();
            let s = snapshot(&engine);
            let mut p = context(&s, locale);
            p.focused = Some(owner);
            assert!(super::super::enabled(&p, &s, Operation::Roving(true)));
            assert!(!super::super::enabled(&p, &s, Operation::Roving(false)));
            let rows = super::super::entries(&p, &s);
            assert!(rows
                .iter()
                .any(|(_, op, on)| matches!(op, Operation::Roving(true)) && *on));
            assert_eq!(
                engine.project(),
                &original,
                "capability inspection must not edit"
            );
            run(&mut p, &mut engine, true);
            let roving = engine.snapshot();
            let s = snapshot(&engine);
            assert_eq!(p.selected_keys.status(&s), key_selection::Status::Current);
            assert_eq!(p.selected_keys.spatial_selections(), [(target(7), vec![2])]);
            assert!(p.selected_keys.selections().is_empty());
            assert_eq!(
                roving.layers[0].transform.position.key_frames().nth(1),
                Some(2.375)
            );
            assert!(super::super::checked(&p, &s, Operation::Roving(true)));
            assert!(!super::super::enabled(&p, &s, Operation::Roving(true)));
            assert!(super::super::enabled(&p, &s, Operation::Roving(false)));
            let samples = (0..200)
                .map(|tick| {
                    roving.layers[0]
                        .transform
                        .position
                        .sample(f64::from(tick) / 10.)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            run(&mut p, &mut engine, false);
            let fixed = engine.snapshot();
            let s = snapshot(&engine);
            assert!(plans(&p, &s)
                .title(locale)
                .unwrap()
                .contains(if locale == Locale::Zh {
                    "固定"
                } else {
                    "fixed"
                }));
            assert!(!super::super::checked(&p, &s, Operation::Roving(true)));
            assert!(super::super::enabled(&p, &s, Operation::Roving(true)));
            assert!(!super::super::enabled(&p, &s, Operation::Roving(false)));
            assert_eq!(
                fixed.layers[0]
                    .transform
                    .position
                    .spatial_animation
                    .as_ref()
                    .unwrap()
                    .keys()[1]
                    .fixed,
                Some(2.375)
            );
            for (tick, expected) in samples.into_iter().enumerate() {
                assert_eq!(
                    fixed.layers[0]
                        .transform
                        .position
                        .sample(tick as f64 / 10.)
                        .unwrap(),
                    expected
                );
            }
            let dir = tempfile::tempdir().unwrap();
            motion_core::storage::save(dir.path(), &fixed).unwrap();
            assert_eq!(motion_core::storage::load(dir.path()).unwrap(), fixed);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &roving);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &original);
            assert!(!engine.can_undo());
            engine.redo().unwrap();
            assert_eq!(engine.project(), &roving);
        }
    }
}

#[test]
fn unsupported_endpoints_separated_axes_and_mixed_properties_never_dispatch_a_subset() {
    for case in [
        "first",
        "last",
        "endpoint_and_middle",
        "scalar",
        "separated",
        "hold",
        "readonly",
        "locked",
        "stale",
        "mixed",
    ] {
        let base = fixture();
        let mut project = base.snapshot();
        if case == "separated" {
            project.layers[0].transform.position.separate().unwrap();
        }
        if case == "hold" {
            project.layers[0].transform.position.keys[0].ease = Ease::Hold;
        }
        if case == "scalar" || case == "mixed" {
            project.layers[0]
                .transform
                .opacity
                .upsert(10, 0.5, Ease::Linear)
                .unwrap();
        }
        let engine = Engine::new(project).unwrap();
        let original = engine.snapshot();
        let mut s = snapshot(&engine);
        let mut p = context(&s, Locale::Zh);
        let keys = match case {
            "first" => vec![frame(7, 12)],
            "last" => vec![frame(7, 31)],
            "endpoint_and_middle" => vec![frame(7, 12), frame(7, 22)],
            "scalar" => vec![key_selection::Key {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Opacity,
                    axis: None,
                },
                frame: 22,
            }
            .into()],
            "mixed" => vec![
                frame(7, 22),
                key_selection::Key {
                    target: KeyframeTarget::Property {
                        object: 7,
                        property: Property::Opacity,
                        axis: None,
                    },
                    frame: 22,
                }
                .into(),
            ],
            "separated" => vec![key_selection::Key {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: Some(Axis::X),
                },
                frame: 22,
            }
            .into()],
            _ => vec![frame(7, 22)],
        };
        select(&mut p, &s, keys);
        match case {
            "readonly" => s["observing"] = true.into(),
            "locked" => s["project"]["layers"][0]["locked"] = true.into(),
            "stale" => s["document_generation"] = 10.into(),
            _ => {}
        }
        for enabled in [true, false] {
            assert!(
                !super::super::enabled(&p, &s, Operation::Roving(enabled)),
                "{case}"
            );
            let mut out = vec![];
            super::super::apply(&mut p, &s, Operation::Roving(enabled), &mut out);
            assert!(out.is_empty(), "{case}");
        }
        assert_eq!(engine.project(), &original);
        assert!(!engine.can_undo());
    }
}

#[test]
fn mixed_storage_and_partial_constraints_enable_atomically_and_keep_both_channels_selected() {
    let mut engine = fixture();
    engine
        .apply(Command::SpatialKeyframes {
            target: target(7),
            edit: SpatialKeyEdit::Enable { frames: vec![22] },
        })
        .unwrap();
    let before = engine.snapshot();
    let s = snapshot(&engine);
    let mut p = context(&s, Locale::En);
    select(&mut p, &s, [id(7, 2), frame(8, 15)]);
    let caps = plans(&p, &s);
    assert!(caps.partial());
    assert!(super::super::enabled(&p, &s, Operation::Roving(true)));
    assert!(!super::super::enabled(&p, &s, Operation::Roving(false)));
    run(&mut p, &mut engine, true);
    let s = snapshot(&engine);
    assert_eq!(
        p.selected_keys.spatial_selections(),
        [(target(7), vec![2]), (target(8), vec![2])]
    );
    assert!(plans(&p, &s).all_roving());
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    // Two spatial channels can freeze together, including an already fixed key.
    engine.redo().unwrap();
    engine
        .apply(Command::SpatialKeyframes {
            target: target(8),
            edit: SpatialKeyEdit::Roving {
                keys: vec![2],
                enabled: false,
            },
        })
        .unwrap();
    let before = engine.snapshot();
    let s = snapshot(&engine);
    assert!(plans(&p, &s).partial());
    assert!(super::super::enabled(&p, &s, Operation::Roving(false)));
    run(&mut p, &mut engine, false);
    assert_eq!(plans(&p, &snapshot(&engine)).roving, 0);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn cached_capabilities_are_readonly_revision_scoped_and_missing_identity_never_reuses_them() {
    let engine = fixture();
    let original = engine.snapshot();
    let s = snapshot(&engine);
    let p = context(&s, Locale::Zh);
    let first = plans(&p, &s);
    for _ in 0..120 {
        assert!(Arc::ptr_eq(&first, &plans(&p, &s)));
        let _ = super::super::entries(&p, &s);
    }
    assert_eq!(p.animation_type_cache.borrow().roving.solves, 1);
    assert_eq!(engine.project(), &original);
    assert!(!engine.can_undo());
    for field in ["root", "composition", "document_generation", "revision"] {
        let mut changed = s.clone();
        changed[field] = if field == "root" || field == "composition" {
            "other".into()
        } else {
            99.into()
        };
        assert!(!Arc::ptr_eq(&first, &plans(&p, &changed)));
    }
    let mut no_revision = s.clone();
    no_revision.as_object_mut().unwrap().remove("revision");
    assert!(!Arc::ptr_eq(
        &plans(&p, &no_revision),
        &plans(&p, &no_revision)
    ));
    assert!(!super::super::enabled(
        &p,
        &no_revision,
        Operation::Roving(true)
    ));
}

#[test]
fn conversion_completion_never_rebinds_changed_selection_documents_or_unrelated_revisions() {
    for case in [
        "selection",
        "root",
        "composition",
        "generation",
        "revision_gap",
        "track",
        "cancelled",
    ] {
        let mut engine = fixture();
        let s = snapshot(&engine);
        let mut p = context(&s, Locale::Zh);
        let mut out = vec![];
        super::super::apply(&mut p, &s, Operation::Roving(true), &mut out);
        let before = p.selected_keys.clone();
        engine.apply_batch(commands(out)).unwrap();
        let mut after = snapshot(&engine);
        match case {
            "selection" => select(&mut p, &after, [frame(8, 15)]),
            "root" => after["root"] = "another".into(),
            "composition" => after["composition"] = "another".into(),
            "generation" => after["document_generation"] = 99.into(),
            "revision_gap" => after["revision"] = (engine.revision() + 1).into(),
            "track" => {
                after["project"]["layers"][0]["transform"]["position"]["value"] =
                    serde_json::json!([1., 2., 3.])
            }
            _ => after["editorAction"] = serde_json::json!({"cancelled":true}),
        }
        reconcile(&mut p, &after);
        assert!(
            p.animation_type_cache.borrow().roving.pending.is_none(),
            "{case}"
        );
        if case != "selection" {
            assert!(p.selected_keys.same_selection(&before), "{case}");
        }
        assert!(!p.selected_keys.has_spatial(), "{case}");
        reconcile(&mut p, &snapshot(&engine));
        assert!(!p.selected_keys.has_spatial(), "late {case}");
    }
}

#[test]
fn malformed_candidates_report_default_error_channels_without_poisoning_recovery() {
    const CHILD: &str = "MOTION_ROVING_MENU_ERROR_CHILD";
    const NAME:&str="panels::animation_menu::roving::tests::malformed_candidates_report_default_error_channels_without_poisoning_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root).unwrap();
        let engine = fixture();
        let good = snapshot(&engine);
        let p = context(&good, Locale::Zh);
        let mut bad = good.clone();
        bad["project"]["layers"][0]["transform"]["position"]["keys"][1]["value"] =
            "malformed".into();
        for _ in 0..90 {
            assert!(plans(&p, &bad).commands(true).is_none());
        }
        assert!(plans(&p, &good).commands(true).is_some());
        assert!(!engine.can_undo());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_ROVING_MENU_ERROR_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
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
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let logs = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    assert_eq!(
        logs.lines()
            .filter(|l| l.contains("ERROR") && l.contains("animation.roving.track"))
            .count(),
        1
    );
    assert!(
        logs.contains("count=60")
            && logs.contains("phase=roving_capability")
            && logs.contains("object=7")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("ERROR")
            && stderr.contains("animation.roving.track")
            && stderr.contains("invalid type")
    );
    assert!(!stderr.contains("WARN") && !stdout.contains("panel degradation"));
}

#[test]
fn anchor_and_global_or_layer_camera_channels_keep_offsets_identities_and_atomic_history() {
    for (object, property) in [
        (7, Property::Anchor),
        (0, Property::Position),
        (0, Property::Target),
        (10, Property::Position),
        (10, Property::Target),
    ] {
        let mut project = fixture().snapshot();
        let mut position = project.layers[0].transform.position.clone();
        for key in &mut position.keys {
            key.value[2] = -100.;
        }
        let mut camera = motion_core::Camera::new(project.width, project.height);
        camera.created = true;
        camera.position = position;
        camera.target = project.layers[0].transform.position.clone();
        if object == 0 {
            project.camera = camera;
        } else if object == 10 {
            let mut layer = motion_core::Layer::camera(10, "Camera", project.width, project.height);
            layer.timeline = project.layers[0].timeline;
            layer.content = motion_core::Content::Camera {
                camera: Box::new(camera),
            };
            project.layers.push(layer);
        } else {
            let mut anchor = Track::constant([0.; 2]);
            for (frame, value) in [(0, 0.), (10, 0.1), (19, 0.8)] {
                anchor.upsert(frame, [value, 0.], Ease::Linear).unwrap();
            }
            project.layers[0].transform.anchor = anchor;
        }
        let mut engine = Engine::new(project).unwrap();
        let original = engine.snapshot();
        let s = snapshot(&engine);
        let mut p = context(&s, Locale::En);
        let target = KeyframeTarget::Property {
            object,
            property,
            axis: None,
        };
        select(
            &mut p,
            &s,
            [key_selection::Key {
                target: target.clone(),
                frame: if object == 0 { 10 } else { 22 },
            }
            .into()],
        );
        assert!(
            super::super::enabled(&p, &s, Operation::Roving(true)),
            "{target:?}"
        );
        run(&mut p, &mut engine, true);
        let roving = engine.snapshot();
        assert_eq!(
            p.selected_keys.spatial_selections(),
            [(target.clone(), vec![2])]
        );
        let state = snapshot(&engine);
        let (track, offset) = key_selection::authored_track(&state, &target).unwrap();
        assert_eq!(offset, if object == 0 { 0 } else { 12 });
        let samples = |track: &Value| {
            (0..200)
                .map(|tick| {
                    if property == Property::Anchor {
                        serde_json::to_value(
                            serde_json::from_value::<Track<[f32; 2]>>(track.clone())
                                .unwrap()
                                .sample(tick as f64 / 10.)
                                .unwrap(),
                        )
                        .unwrap()
                    } else {
                        serde_json::to_value(
                            serde_json::from_value::<Track<[f32; 3]>>(track.clone())
                                .unwrap()
                                .sample(tick as f64 / 10.)
                                .unwrap(),
                        )
                        .unwrap()
                    }
                })
                .collect::<Vec<_>>()
        };
        let before_freeze = samples(track);
        run(&mut p, &mut engine, false);
        let state = snapshot(&engine);
        let (track, _) = key_selection::authored_track(&state, &target).unwrap();
        assert_eq!(samples(track), before_freeze, "{target:?}");
        assert_eq!(
            p.selected_keys.status(&state),
            key_selection::Status::Current
        );
        engine.undo().unwrap();
        assert_eq!(engine.project(), &roving);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &original);
        assert!(!engine.can_undo());
    }
}

#[test]
fn actual_menu_click_dispatches_one_action_and_stale_or_cancelled_menu_dispatches_none() {
    use motion_ui::{paint::PaintList, text::TextAtlas, widgets::menu::Metrics};
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for fault in ["none", "revision", "selection", "document", "cancel"] {
            let mut engine = fixture();
            let original = engine.snapshot();
            let mut state = snapshot(&engine);
            let mut p = context(&state, locale);
            p.menu = Some(5);
            let render = |p: &mut Panels, s: &Value, atlas: &mut TextAtlas, input: &Input| {
                let mut out = vec![];
                super::super::super::menus::render(
                    &mut PaintList::default(),
                    atlas,
                    [1280., 960.],
                    p,
                    input,
                    s,
                    &mut out,
                );
                out
            };
            assert!(render(&mut p, &state, &mut atlas, &Input::default()).is_empty());
            let index = super::super::entries(&p, &state)
                .iter()
                .position(|(_, op, _)| matches!(op, Operation::Roving(true)))
                .unwrap();
            let paint = PaintList::default();
            let rect = Metrics::application(&paint.styles())
                .row_rect(p.menu_navigation.cards[0], index - p.menu_scroll);
            let point = [
                (rect.min[0] + rect.max[0]) / 2.,
                (rect.min[1] + rect.max[1]) / 2.,
            ];
            let mut input = Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            };
            match fault {
                "revision" => state["revision"] = 5.into(),
                "selection" => {
                    select(&mut p, &state, [frame(8, 15)]);
                }
                "document" => state["root"] = "other-project".into(),
                "cancel" => {
                    input = Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    }
                }
                _ => {}
            }
            let out = render(&mut p, &state, &mut atlas, &input);
            assert_eq!(p.menu, None);
            if fault == "none" {
                assert!(
                    matches!(out.as_slice(), [Action::EditorAction { source, .. }]
                    if source == &crate::layer_clipboard::KeyframeSource::of(&state))
                );
                engine.apply_batch(commands(out)).unwrap();
                p.normalize_selection(&snapshot(&engine));
                assert_eq!(p.selected_keys.spatial_selections(), [(target(7), vec![2])]);
                engine.undo().unwrap();
            } else {
                assert!(out.is_empty(), "{fault}");
            }
            assert_eq!(engine.project(), &original);
            assert!(!engine.can_undo());
        }
    }
}
