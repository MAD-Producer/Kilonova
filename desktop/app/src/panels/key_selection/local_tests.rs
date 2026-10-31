//! The existing selection owns authored local addresses; it never manufactures
//! an integer composition key or falls back to editing an entire layer.
use super::*;
use crate::panels::{Action, Panels};
use crate::shortcuts::Command as Shortcut;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};
use motion_core::LocalKeyEdit;
use motion_ui::dock::Panel;

fn mapped(reverse: bool) -> Engine {
    let mut p = fixture().snapshot();
    let layer = &mut p.layers[0];
    layer.timeline.as_mut().unwrap().offset_frame = 2;
    layer.timeline.as_mut().unwrap().in_frame = 2;
    layer
        .transform
        .position
        .upsert(-3, [-3., 20., 0.], Ease::Linear)
        .unwrap();
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            if reverse { 40. } else { 0. },
            0.,
        )
        .unwrap(),
        remap: None,
    });
    Engine::new(p).unwrap()
}
fn local(frame: i32) -> AddressedKey {
    LocalKey {
        target: key(7, None, 0).target,
        frame,
    }
    .into()
}
fn panels(engine: &Engine, keys: impl IntoIterator<Item = AddressedKey>) -> Panels {
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .selected_keys
        .apply_addressed(&state(engine), keys, Mode::Replace)
        .unwrap();
    panels
}

#[test]
fn signed_local_selection_survives_fractional_projection_and_trim_without_integer_aliases() {
    let engine = mapped(false);
    let s = state(&engine);
    assert_eq!(
        engine.project().layers[0]
            .property_composition_frame(3.)
            .unwrap(),
        6.5
    );
    let mut selected = Set::default();
    selected
        .apply_addressed(
            &s,
            [local(-3), local(3), local(13), local(3)],
            Mode::Replace,
        )
        .unwrap();
    assert_eq!(selected.status(&s), Status::Current);
    assert!(selected.has_local() && selected.primary().is_none());
    assert_eq!(selected.addressed_primary(), Some(local(3)));
    assert_eq!(
        selected.local_selections(),
        [(key(7, None, 0).target, vec![-3, 3, 13])]
    );
    assert!(selected.selections().is_empty());
    assert!(!selected.contains(&key(7, None, 6)));
    let before = selected.clone();
    selected
        .apply_addressed(&s, [local(3), local(3)], Mode::Toggle)
        .unwrap();
    assert!(!selected.same_selection(&before));
    assert_eq!(selected.local_selections()[0].1, [-3, 13]);
    let mut trimmed = s.clone();
    trimmed["project"]["layers"][0]["timeline"]["in_frame"] = json!(80);
    trimmed["revision"] = json!(engine.revision() + 1);
    assert_eq!(
        selected.status(&trimmed),
        Status::Current,
        "visibility never changes storage identity"
    );
    selected.clear(&trimmed);
    assert!(selected.owns_keys() && selected.local_primary.is_none());
    assert_eq!(selected.status(&trimmed), Status::Empty);
}

#[test]
fn local_presence_rechecks_authored_keys_and_document_identity_but_lock_only_disables_writes() {
    let engine = mapped(false);
    let s = state(&engine);
    let panels = panels(&engine, [local(3)]);
    assert!(panels.selected_keys.writable(&s));
    for field in ["root", "composition", "document_generation"] {
        let mut other = s.clone();
        other[field] = if field == "document_generation" {
            json!(8)
        } else {
            json!("other")
        };
        assert_eq!(
            panels.selected_keys.status(&other),
            Status::Stale,
            "{field}"
        );
        assert!(!panels.selected_keys.writable(&other));
    }
    let mut locked = s.clone();
    locked["project"]["layers"][0]["locked"] = json!(true);
    assert_eq!(panels.selected_keys.status(&locked), Status::Current);
    assert!(!panels.selected_keys.writable(&locked));
    locked["project"]["layers"][0]["locked"] = json!(false);
    locked["observing"] = json!(true);
    assert!(!panels.selected_keys.writable(&locked));
    let mut missing = s.clone();
    missing["project"]["layers"][0]["transform"]["position"]["keys"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    missing["revision"] = json!(engine.revision() + 1);
    assert_eq!(panels.selected_keys.status(&missing), Status::Stale);
    assert!(panels.selected_keys.owns_keys());
}

#[test]
fn mixed_coordinate_envelopes_are_rejected_atomically_and_budget_counts_signed_addresses() {
    let engine = mapped(false);
    let s = state(&engine);
    let mut selected = panels(&engine, [local(3)]).selected_keys;
    let before = selected.clone();
    for mode in [Mode::Replace, Mode::Add, Mode::Toggle] {
        let keys = if mode == Mode::Replace {
            vec![local(3), key(7, None, 6).into()]
        } else {
            vec![key(7, None, 6).into()]
        };
        assert!(selected.apply_addressed(&s, keys, mode).is_err());
        assert!(selected.same_selection(&before));
    }
    let mut channel = Channel::default();
    channel.insert(&local(i32::MIN), false);
    assert_eq!(
        channel.retained_bytes("target"),
        Channel::bytes("target", 0) + 12
    );
    assert_eq!(
        channel.keys().collect::<Vec<_>>().len(),
        0,
        "missing typed target yields no fabricated address"
    );
    channel.target = Some(key(7, None, 0).target);
    assert_eq!(channel.keys().collect::<Vec<_>>(), [local(i32::MIN)]);
}

#[test]
fn visible_incoming_ease_swaps_local_sides_once_for_reverse_and_keeps_unmapped_channels() {
    for reverse in [false, true] {
        let mut engine = mapped(reverse);
        let mut panels = panels(&engine, [local(3), key(9, Some(Axis::Y), 43).into()]);
        let before = engine.snapshot();
        let edits = ease_commands(&panels, &state(&engine), Shortcut::EaseIncoming).unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|edit| matches!(
            edit,
            Command::EaseKeyframes {
                incoming: true,
                outgoing: false,
                ..
            }
        )));
        assert!(edits
            .iter()
            .any(|edit| matches!(edit, Command::LocalKeyframes {
            edit: LocalKeyEdit::Ease { frames, incoming, outgoing }, ..
        } if frames == &[3] && *incoming == !reverse && *outgoing == reverse)));
        let menu = crate::panels::timeline_shortcuts::ease_plan(
            &panels,
            &state(&engine),
            Shortcut::EaseIncoming,
        )
        .unwrap_or_else(|_| panic!("selected local keys must expose the same ease menu plan"));
        assert_eq!(
            serde_json::to_value(&edits).unwrap(),
            serde_json::to_value(&menu).unwrap()
        );
        let mut actions = Vec::new();
        assert!(shortcut(
            &mut panels,
            &state(&engine),
            Shortcut::EaseIncoming,
            &mut actions
        ));
        let [Action::EditorAction { commands, .. }] = actions.as_slice() else {
            panic!("one guarded atomic action");
        };
        engine.apply_batch(commands.clone()).unwrap();
        let keys = &engine.project().layers[0].transform.position.keys;
        assert_eq!(keys[0].curve.is_some(), !reverse);
        assert_eq!(keys[1].curve.is_some(), reverse);
        assert!(keys[2].curve.is_none());
        assert_eq!(
            keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
            [-3, 3, 13]
        );
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project(), &before);
        assert!(engine.redo().unwrap());
        assert_ne!(engine.project(), &before);
    }
}

#[test]
fn mixed_delete_runs_as_one_real_transaction_and_saves_without_touching_mapping_or_other_axes() {
    let mut engine = mapped(false);
    let mut panels = panels(
        &engine,
        [local(-3), local(3), key(9, Some(Axis::Y), 43).into()],
    );
    let before = engine.snapshot();
    let mut actions = Vec::new();
    assert!(shortcut(
        &mut panels,
        &state(&engine),
        Shortcut::Delete,
        &mut actions
    ));
    let [Action::EditorAction { commands, .. }] = actions.as_slice() else {
        panic!("one guarded atomic action");
    };
    assert_eq!(commands.len(), 2);
    engine.apply_batch(commands.clone()).unwrap();
    assert_eq!(
        engine.project().layers[0]
            .transform
            .position
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        [13]
    );
    assert_eq!(
        engine.project().layers[0].time_mapping,
        before.layers[0].time_mapping
    );
    let axes = engine.project().layers[1]
        .transform
        .position
        .axes
        .as_ref()
        .unwrap();
    assert_eq!(axes.x.keys.len(), 2);
    assert_eq!(axes.y.keys.len(), 1);
    assert_eq!(axes.z.keys.len(), 2);
    let reopened: Project =
        serde_json::from_slice(&serde_json::to_vec(engine.project()).unwrap()).unwrap();
    reopened.validate().unwrap();
    assert_eq!(&reopened, engine.project());
    assert_eq!(panels.selected_keys.status(&state(&engine)), Status::Stale);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo(), "all channels form one history entry");
}

#[test]
fn exact_primary_and_clipboard_gates_never_fall_back_to_an_integer_key_or_layer() {
    let engine = mapped(false);
    let mut panels = panels(&engine, [local(3)]);
    crate::panels::key_selection::ui::project_primary(&mut panels, &state(&engine));
    assert_eq!(panels.selected, Some(7));
    assert_eq!(panels.selected_layers, [7]);
    assert!(panels.animation_controls.user_key().is_none());
    let selected = crate::panels::keyframe_clipboard::selection(&panels, &state(&engine)).unwrap();
    assert!(selected.selections.is_empty() && !selected.writable);
    assert!(panels.selected_keys.owns_keys());
    let before = crate::panels::animation_menu::MenuScope::of(&panels, &state(&engine));
    panels
        .selected_keys
        .apply_addressed(&state(&engine), [local(13)], Mode::Replace)
        .unwrap();
    let after = crate::panels::animation_menu::MenuScope::of(&panels, &state(&engine));
    assert!(
        before != after,
        "popup guard includes exact authored identities"
    );
    assert!(panels.selected_keys.translated(1).is_err());
    assert!(
        transform::Plan::capture(&panels.selected_keys, &state(&engine)).is_err(),
        "pending timing integration cannot publish an empty successful transform"
    );
    let mut actions = Vec::new();
    assert!(shortcut(
        &mut panels,
        &state(&engine),
        Shortcut::NudgeKeyLater,
        &mut actions
    ));
    assert!(
        actions.is_empty() && panels.message.is_some(),
        "unsupported integer gestures are a visible disabled capability, not a false-success edit"
    );
    assert!(!engine.can_undo());
}

#[test]
fn malformed_local_source_is_visible_at_default_level_and_normal_absence_is_quiet() {
    const NAME: &str = "panels::key_selection::tests::local::malformed_local_source_is_visible_at_default_level_and_normal_absence_is_quiet";
    const CHILD: &str = "MOTION_LOCAL_SELECTION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let s = state(&mapped(false));
        let target = key(7, None, 0).target;
        let frames = BTreeSet::from([3]);
        for _ in 0..120 {
            assert!(source::contains_local(&s, &target, &frames));
            assert!(!source::contains_local(&s, &target, &BTreeSet::from([5])));
        }
        assert!(!std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .contains("panel degradation"));
        let mut bad = s.clone();
        bad["project"]["layers"][0]["transform"]["position"]["keys"][1]["frame"] = json!(3.5);
        for _ in 0..120 {
            assert!(!source::contains_local(&bad, &target, &frames));
            assert!(source::contains_local(&s, &target, &frames));
        }
        let engine = mapped(false);
        let selected = panels(&engine, [local(3)]);
        let mut bad_clock = s.clone();
        bad_clock["project"]["layers"][0]["time_mapping"]["clock"]["stretch"]["denominator"] =
            json!(0);
        for _ in 0..120 {
            assert!(ease_commands(&selected, &bad_clock, Shortcut::EaseIncoming).is_none());
            assert!(ease_commands(&selected, &s, Shortcut::EaseIncoming).is_some());
        }
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let records: Vec<_> = file
            .lines()
            .filter(|l| l.contains("site=selection.local_authored_keys "))
            .collect();
        assert_eq!(records.len(), 2);
        assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
        assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
        assert!(records.iter().all(|l| l.contains("object=7")
            && l.contains("phase=selection")
            && l.contains("not i32")));
        let clock_records: Vec<_> = file
            .lines()
            .filter(|l| l.contains("site=selection.local_ease_clock "))
            .collect();
        assert_eq!(clock_records.len(), 2);
        assert!(clock_records[0].contains("WARN") && clock_records[0].contains("count=1 "));
        assert!(clock_records[1].contains("ERROR") && clock_records[1].contains("count=60 "));
        assert!(clock_records.iter().all(|l| l.contains("object=7")
            && l.contains("phase=ease")
            && l.contains("duration ratio")));
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_LOCAL_SELECTION_LOG_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tmp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &root)
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
    assert!(stderr.contains("persistent panel degradation site=selection.local_authored_keys "));
    assert!(stderr.contains("persistent panel degradation site=selection.local_ease_clock "));
    assert!(!stderr.contains("count=1 "));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}
