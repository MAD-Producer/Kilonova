//! Typed clipboard routing uses actual timeline pointer and keyboard input.
use super::*;
use crate::layer_clipboard::controller::Intent;
use crate::panels::keyframe_clipboard::Selection;
use motion_ui::input::Key as InputKey;
#[cfg(target_os = "windows")]
#[path = "local_clipboard_native_tests.rs"]
mod native;

fn copy_input() -> Input {
    Input {
        keys: vec![(
            InputKey::C,
            Modifiers {
                control: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}
#[test]
fn all_typed_mapped_channels_copy_exact_clicked_keys_through_both_chrome_owners() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for case in typed::CASES {
        for reverse in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for floating in [false, true] {
                    let (engine, catalogue, targets) = typed::typed(case, reverse);
                    let s = view(&engine);
                    let mut panels = typed::ready(case, &catalogue, &s, locale, false);
                    let (paint, _) = draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                    let at = engine.project().layers[0]
                        .property_composition_frame(3.)
                        .unwrap();
                    let point = typed::points(&paint, &panels, at)[0];
                    draw(
                        &mut panels,
                        &s,
                        pointer(point, true, false, Modifiers::default()),
                        floating,
                        &mut atlas,
                    );
                    draw(
                        &mut panels,
                        &s,
                        pointer(point, false, true, Modifiers::default()),
                        floating,
                        &mut atlas,
                    );
                    let (_, actions) = draw(&mut panels, &s, copy_input(), floating, &mut atlas);
                    let [Action::KeyframeClipboard {
                        intent: Intent::Copy,
                        selections,
                        local_selections,
                        targets: destinations,
                        ..
                    }] = actions.as_slice()
                    else {
                        panic!("{case:?} {reverse} {floating}: {actions:?}")
                    };
                    assert!(selections.is_empty());
                    assert_eq!(local_selections.len(), targets.len());
                    assert_eq!(destinations.len(), targets.len());
                    assert!(local_selections
                        .iter()
                        .all(|s| s.frames == [3] && targets.contains(&s.target)));
                    let clipboard = KeyframeClipboard::capture_mixed(
                        engine.project(),
                        selections,
                        local_selections,
                    )
                    .unwrap();
                    assert_eq!(clipboard.tracks.len(), targets.len());
                    assert!(!engine.can_undo());
                    let before = engine.snapshot();
                    let mut edited = Engine::new(before.clone()).unwrap();
                    edited
                        .apply(Command::RemoveKeyframes {
                            clipboard: Box::new(clipboard),
                        })
                        .unwrap();
                    assert!(edited.undo().unwrap());
                    assert_eq!(edited.project(), &before);
                }
            }
        }
    }
}
#[test]
fn property_clipboard_reads_authored_frames_and_keeps_empty_audio_a_normal_paste_target() {
    for case in typed::CASES {
        for reverse in [false, true] {
            let (engine, _, targets) = typed::typed(case, reverse);
            let s = view(&engine);
            let all = Selection::mapped(&s, targets.clone(), None, false, true).unwrap();
            assert_eq!(all.targets, targets);
            assert_eq!(all.local_selections.len(), targets.len());
            assert!(all.local_selections.iter().all(|s| s.frames == [3, 13]));
            KeyframeClipboard::capture_mixed(
                engine.project(),
                &all.selections,
                &all.local_selections,
            )
            .unwrap();
            let at = engine.project().layers[0]
                .property_composition_frame(3.)
                .unwrap();
            assert!(at.fract() != 0.);
            // Integer-controller selections must not alias a fractional author key.
            let selected =
                Selection::mapped(&s, targets, Some(at.floor() as u32), true, true).unwrap();
            assert!(selected.local_selections.is_empty());
            assert!(!selected.enabled(&s, Intent::Copy, true));
        }
    }
    let (mut project, _, targets) = typed::typed(typed::Case::Audio, false);
    let mut p = project.snapshot();
    p.layers[0].audio_levels = None;
    project = Engine::new(p).unwrap();
    let s = view(&project);
    let selected = Selection::mapped(&s, targets, None, false, true).unwrap();
    assert!(!selected.enabled(&s, Intent::Copy, true));
    assert!(selected.enabled(&s, Intent::Paste { in_place: false }, true));
}
#[test]
fn mixed_frame_and_local_sets_copy_together_but_stale_sets_never_copy_a_subset() {
    let engine = mapped(false);
    let s = view(&engine);
    let mut p = setup(Locale::En, false, false);
    p.selected_keys
        .apply_addressed(
            &s,
            [
                local(7, None, 3),
                key(9, Some(motion_core::Axis::Y), 43).into(),
            ],
            Mode::Replace,
        )
        .unwrap();
    let selected = crate::panels::keyframe_clipboard::selection(&p, &s).unwrap();
    assert_eq!(selected.selections.len(), 1);
    assert_eq!(selected.local_selections.len(), 1);
    assert_eq!(selected.targets.len(), 2);
    KeyframeClipboard::capture_mixed(
        engine.project(),
        &selected.selections,
        &selected.local_selections,
    )
    .unwrap();
    // A frozen clipboard request must reject a newer revision. The authored
    // selection itself survives revisions while its keys still exist.
    let mut revised = s.clone();
    revised["revision"] = json!(999);
    assert!(!selected.enabled(&revised, Intent::Copy, true));
    let current = crate::panels::keyframe_clipboard::selection(&p, &revised).unwrap();
    assert!(current.enabled(&revised, Intent::Copy, true));
    let mut removed = revised.clone();
    removed["revision"] = json!(1000);
    removed["project"]["layers"][0]["transform"]["position"]["keys"]
        .as_array_mut()
        .unwrap()
        .retain(|k| k["frame"] != 3);
    let missing = crate::panels::keyframe_clipboard::selection(&p, &removed).unwrap();
    assert!(!missing.enabled(&removed, Intent::Copy, true));
    assert!(!missing.enabled(&removed, Intent::Cut, true));
    for field in ["document_generation", "root"] {
        let mut changed = s.clone();
        changed[field] = if field == "root" {
            json!("other")
        } else {
            json!(999)
        };
        let selected = crate::panels::keyframe_clipboard::selection(&p, &changed).unwrap();
        assert!(!selected.enabled(&changed, Intent::Copy, true));
        assert!(!selected.enabled(&changed, Intent::Cut, true));
        assert!(p.selected_keys.owns_keys());
    }
}
#[test]
fn mapped_clipboard_degradation_is_bounded_visible_at_default_level_and_recovers_functionally() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::clipboard::mapped_clipboard_degradation_is_bounded_visible_at_default_level_and_recovers_functionally";
    const CHILD: &str = "MOTION_MAPPED_CLIPBOARD_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        let (engine, _, targets) = typed::typed(typed::Case::Audio, false);
        let healthy = view(&engine);
        let mut bad = healthy.clone();
        bad["project"]["layers"][0]["audio_levels"]["left"]["keys"][0]["frame"] = json!("invalid");
        for _ in 0..120 {
            assert!(
                Selection::mapped(&healthy, vec![targets[0].clone()], None, false, true).is_some()
            );
        }
        for _ in 0..120 {
            assert!(Selection::mapped(&bad, vec![targets[0].clone()], None, false, true).is_none());
            assert_eq!(
                Selection::mapped(&healthy, vec![targets[0].clone()], None, false, true)
                    .unwrap()
                    .local_selections[0]
                    .frames,
                [3, 13]
            );
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MAPPED_CLIPBOARD_LOG_ARTIFACTS")
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
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let records = file
        .lines()
        .filter(|l| l.contains("site=clipboard.mapped_keys"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2, "{file}");
    assert!(records[0].contains("count=1"));
    assert!(records[1].contains("count=60"));
    assert!(records.iter().all(|r| r.contains("object=7")
        && r.contains("phase=select")
        && r.contains("authored key frame invalid")));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("site=clipboard.mapped_keys")
            .count(),
        1
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
}
