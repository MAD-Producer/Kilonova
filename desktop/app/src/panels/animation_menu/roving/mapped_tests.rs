//! Initial conversion uses authored identities, including fractional projections.
use super::super::tests::{commands, context, snapshot};
use super::*;
use motion_core::{layer_time::*, Ease, Engine};

fn target(object: u64) -> KeyframeTarget {
    KeyframeTarget::Property {
        object,
        property: Property::Position,
        axis: None,
    }
}
fn local(frame: i32) -> key_selection::AddressedKey {
    key_selection::LocalKey {
        target: target(7),
        frame,
    }
    .into()
}
fn fixture(reverse: bool) -> Engine {
    let mut project = super::tests::fixture().snapshot();
    let layer = &mut project.layers[0];
    layer.timeline.as_mut().unwrap().offset_frame = 2;
    layer.timeline.as_mut().unwrap().in_frame = 0;
    layer.timeline.as_mut().unwrap().out_frame = 120;
    layer.transform.position.keys.clear();
    for (frame, x) in [(-12, 0.), (-9, 1.), (-6, 7.), (0, 10.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -12. },
        )
        .unwrap(),
        remap: None,
    });
    Engine::new(project).unwrap()
}
fn panels(state: &Value, locale: Locale, owner: Panel, frames: &[i32]) -> Panels {
    let mut p = context(state, locale);
    p.focused = Some(owner);
    p.selected_keys
        .apply_addressed(
            state,
            frames.iter().copied().map(local),
            key_selection::Mode::Replace,
        )
        .unwrap();
    p
}

#[test]
fn fractional_local_conversion_menu_and_shortcut_agree_and_rebind_stable_ids() {
    use motion_ui::input::{Key, Modifiers};
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let mut engine = fixture(reverse);
                let before = engine.snapshot();
                assert_eq!(
                    before.layers[0]
                        .property_composition_frame(-9.)
                        .unwrap()
                        .fract(),
                    0.5
                );
                let s = snapshot(&engine);
                let mut p = panels(&s, locale, Panel::Timeline, &[-9, -6]);
                let mut shortcut = panels(&s, locale, Panel::Timeline, &[-9, -6]);
                let mut menu = vec![];
                super::super::apply(&mut p, &s, Operation::Roving(true), &mut menu);
                let mut keys = vec![];
                super::super::super::keyboard::route(
                    &Input {
                        keys: vec![(
                            Key::F8,
                            Modifiers {
                                control: true,
                                alt: true,
                                shift: false,
                            },
                        )],
                        ..Default::default()
                    },
                    &mut shortcut,
                    &s,
                    false,
                    floating.then_some(Panel::Timeline),
                    &mut keys,
                );
                let menu = commands(menu);
                let keys = commands(keys);
                assert_eq!(
                    serde_json::to_value(&menu).unwrap(),
                    serde_json::to_value(&keys).unwrap()
                );
                assert!(
                    matches!(menu.as_slice(), [Command::SpatialKeyframes { edit: SpatialKeyEdit::EnableLocal { frames }, .. }] if frames == &[-9, -6])
                );
                engine.apply_batch(menu).unwrap();
                let mut expected = before.clone();
                expected.layers[0]
                    .transform
                    .position
                    .enable_roving(&[-9, -6])
                    .unwrap();
                assert_eq!(engine.project(), &expected);
                p.normalize_selection(&snapshot(&engine));
                assert!(p.selected_keys.local_selections().is_empty());
                assert!(p.selected_keys.selections().is_empty());
                assert_eq!(
                    p.selected_keys.spatial_selections(),
                    [(target(7), vec![2, 3])]
                );
                assert!(super::super::checked(
                    &p,
                    &snapshot(&engine),
                    Operation::Roving(true)
                ));
                let samples = (-24..=24)
                    .map(|tick| {
                        expected.layers[0]
                            .transform
                            .position
                            .sample(f64::from(tick) / 2.)
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                let mut freeze = vec![];
                super::super::apply(
                    &mut p,
                    &snapshot(&engine),
                    Operation::Roving(false),
                    &mut freeze,
                );
                engine.apply_batch(commands(freeze)).unwrap();
                assert_eq!(
                    (-24..=24)
                        .map(|tick| engine.project().layers[0]
                            .transform
                            .position
                            .sample(f64::from(tick) / 2.)
                            .unwrap())
                        .collect::<Vec<_>>(),
                    samples
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &expected);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                engine.redo().unwrap();
                assert_eq!(engine.project(), &expected);
            }
        }
    }
}

#[test]
fn local_and_legacy_channels_convert_as_one_transaction_and_keep_all_identities() {
    for reverse in [false, true] {
        let mut engine = fixture(reverse);
        let before = engine.snapshot();
        let s = snapshot(&engine);
        let mut p = panels(&s, Locale::Zh, Panel::Curves, &[-9]);
        p.selected_keys
            .apply_addressed(
                &s,
                [key_selection::Key {
                    target: target(8),
                    frame: 15,
                }
                .into()],
                key_selection::Mode::Add,
            )
            .unwrap();
        let mut actions = vec![];
        super::super::apply(&mut p, &s, Operation::Roving(true), &mut actions);
        let edits = commands(actions);
        assert_eq!(edits.len(), 2);
        engine.apply_batch(edits).unwrap();
        p.normalize_selection(&snapshot(&engine));
        assert_eq!(
            p.selected_keys.spatial_selections(),
            [(target(7), vec![2]), (target(8), vec![2])]
        );
        let mut expected = before.clone();
        expected.layers[0]
            .transform
            .position
            .enable_roving(&[-9])
            .unwrap();
        expected.layers[1]
            .transform
            .position
            .enable_roving(&[10])
            .unwrap();
        assert_eq!(engine.project(), &expected);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn mapped_property_selection_is_claimed_as_local_before_the_async_conversion() {
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let mut engine = fixture(reverse);
            let before = engine.snapshot();
            let state = snapshot(&engine);
            let mut p = context(&state, locale);
            p.focused = Some(Panel::Properties);
            p.animation_controls
                .select_key_in(&state, 7, Property::Position, None, 11);
            assert!(!p.selected_keys.owns_keys());
            let mut actions = vec![];
            super::super::apply(&mut p, &state, Operation::Roving(true), &mut actions);
            assert_eq!(p.selected_keys.local_selections(), [(target(7), vec![-6])]);
            let edits = commands(actions);
            assert!(
                matches!(edits.as_slice(), [Command::SpatialKeyframes { edit: SpatialKeyEdit::EnableLocal { frames }, .. }] if frames == &[-6])
            );
            engine.apply_batch(edits).unwrap();
            p.normalize_selection(&snapshot(&engine));
            assert_eq!(p.selected_keys.spatial_selections(), [(target(7), vec![3])]);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}

#[test]
fn local_capability_cache_tracks_selection_and_preserves_endpoint_lock_and_stale_guards() {
    for reverse in [false, true] {
        let engine = fixture(reverse);
        let original = engine.snapshot();
        let s = snapshot(&engine);
        let mut p = panels(&s, Locale::Zh, Panel::Timeline, &[-9]);
        let first = plans(&p, &s);
        for _ in 0..120 {
            assert!(Arc::ptr_eq(&first, &plans(&p, &s)));
        }
        p.selected_keys
            .apply_addressed(&s, [local(-6)], key_selection::Mode::Replace)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &plans(&p, &s)));
        assert_eq!(p.animation_type_cache.borrow().roving.solves, 2);
        for case in ["first", "last", "locked", "readonly", "stale", "cancelled"] {
            let mut s = s.clone();
            let mut p = panels(&s, Locale::Zh, Panel::Timeline, &[-9]);
            match case {
                "first" => p
                    .selected_keys
                    .apply_addressed(&s, [local(-12)], key_selection::Mode::Replace)
                    .unwrap(),
                "last" => p
                    .selected_keys
                    .apply_addressed(&s, [local(0)], key_selection::Mode::Replace)
                    .unwrap(),
                "locked" => s["project"]["layers"][0]["locked"] = true.into(),
                "readonly" => s["observing"] = true.into(),
                "stale" => s["document_generation"] = 99.into(),
                _ => p.selected_keys.clear(&s),
            }
            assert!(
                !super::super::enabled(&p, &s, Operation::Roving(true)),
                "{case}"
            );
            let mut out = vec![];
            super::super::apply(&mut p, &s, Operation::Roving(true), &mut out);
            assert!(out.is_empty(), "{case}");
        }
        assert_eq!(engine.project(), &original);
        assert!(!engine.can_undo());
    }
}

#[test]
fn signed_local_completion_never_overwrites_selection_changes_or_cancelled_commits() {
    for case in ["selection", "generation", "revision", "cancelled", "track"] {
        let mut engine = fixture(false);
        let s = snapshot(&engine);
        let mut p = panels(&s, Locale::En, Panel::Timeline, &[-9]);
        let mut out = vec![];
        super::super::apply(&mut p, &s, Operation::Roving(true), &mut out);
        let before = p.selected_keys.clone();
        engine.apply_batch(commands(out)).unwrap();
        let mut next = snapshot(&engine);
        match case {
            "selection" => p.selected_keys.clear(&next),
            "generation" => next["document_generation"] = 99.into(),
            "revision" => next["revision"] = (engine.revision() + 1).into(),
            "cancelled" => next["editorAction"]["cancelled"] = true.into(),
            _ => {
                next["project"]["layers"][0]["transform"]["position"]["value"] =
                    serde_json::json!([99., 0., 0.])
            }
        }
        reconcile(&mut p, &next);
        assert!(p.selected_keys.spatial_selections().is_empty(), "{case}");
        if case != "selection" {
            assert!(p.selected_keys.same_selection(&before), "{case}");
        }
    }
}

#[cfg(all(windows, feature = "gpu-tests"))]
mod native;

#[test]
fn malformed_local_clock_reports_bounded_default_errors_and_allows_healthy_recovery() {
    const CHILD: &str = "MOTION_LOCAL_ROVING_CLOCK_CHILD";
    const NAME: &str = "panels::animation_menu::roving::mapped_tests::malformed_local_clock_reports_bounded_default_errors_and_allows_healthy_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let _log = crate::logging::init(root.into()).unwrap();
        let engine = fixture(false);
        let before = engine.snapshot();
        let good = snapshot(&engine);
        let p = panels(&good, Locale::Zh, Panel::Timeline, &[-9]);
        let mut bad = good.clone();
        bad["revision"] = (engine.revision() + 1).into();
        bad["project"]["layers"][0]["time_mapping"]["clock"] = "malformed".into();
        for _ in 0..90 {
            assert!(plans(&p, &bad).commands(true).is_none());
        }
        assert!(plans(&p, &good).commands(true).is_some());
        // The shared observer is process-lifetime per site, not consecutive.
        for _ in 0..90 {
            assert!(plans(&p, &bad).commands(true).is_none());
        }
        assert!(plans(&p, &good).commands(true).is_some());
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_ROVING_CLOCK_ARTIFACTS")
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
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for text in [&*file, &*stderr] {
        assert_eq!(
            text.lines()
                .filter(|l| l.contains("ERROR") && l.contains("animation.roving.clock"))
                .count(),
            1,
            "{text}"
        );
        assert!(
            text.contains("count=60")
                && text.contains("phase=roving_capability")
                && text.contains("object=7")
                && text.contains("invalid type")
        );
    }
    assert!(
        !stderr.contains("WARN")
            && !stdout.contains("panel degradation")
            && !stdout.contains("motion_studio::")
    );
}
