use super::*;

#[test]
fn malformed_tracks_report_once_per_revision_at_default_level_then_recover_without_stdout_diagnostics(
) {
    const CHILD: &str = "MOTION_CURVE_COMPARISON_ERROR_CHILD";
    const NAME:&str="panels::curve_comparison::tests::error_tests::malformed_tracks_report_once_per_revision_at_default_level_then_recover_without_stdout_diagnostics";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let engine = fixture();
        let good = state(&engine);
        let before = engine.project().clone();
        let mut c = Controls::default();
        c.sync(&good, Locale::En);
        c.add(&good, Locale::En, &[property(Property::Position)])
            .unwrap();
        let mut bad = good.clone();
        bad["revision"] = serde_json::json!(1);
        bad["project"]["layers"][0]["transform"]["position"]["keys"][1]["frame"] =
            serde_json::json!("not-a-frame");
        for _ in 0..90 {
            c.sync(&bad, Locale::En);
        }
        assert!(c.entries.iter().all(|e| e.data.is_err()));
        let mut restored = good.clone();
        restored["revision"] = serde_json::json!(2);
        c.sync(&restored, Locale::En);
        assert!(c.entries.iter().all(|e| e.data.is_ok()));
        assert!(matches!(
            c.add(
                &restored,
                Locale::En,
                &[KeyframeTarget::Property {
                    object: 999,
                    property: Property::Opacity,
                    axis: None
                }]
            ),
            Err(Unavailable::Missing)
        ));
        assert_eq!(engine.project(), &before);
        assert_eq!(good["project"], restored["project"]);
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"cachedFailureFrames":90,"decodeFailures":1,"recovered":true,"authoredProjectUnchanged":true})).unwrap()).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CURVE_COMPARISON_ERROR_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &result.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let errors = file
        .lines()
        .filter(|l| l.contains("ERROR") && l.contains("curve comparison failed"))
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("object=7"));
    assert!(errors[0].contains("revision=Some(1)"));
    assert!(errors[0].contains("not-a-frame"));
    assert!(file.contains("INFO") && file.contains("operation=add"));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("curve comparison failed"));
    assert!(!stderr.contains("curve comparison start"));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(!stdout.contains("curve comparison start"));
    assert!(!stdout.contains("ERROR"));
}

#[test]
fn comparison_menu_and_keyboard_controls_work_in_read_only_views_and_do_not_start_playback() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut s = state(&fixture());
            s["observing"] = serde_json::json!(true);
            let mut p = setup(&s, locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            assert!(super::super::super::animation_menu::enabled(
                &p,
                &s,
                super::super::super::animation_menu::Operation::CompareCurves
            ));
            let mut actions = vec![];
            super::super::super::animation_menu::apply(
                &mut p,
                &s,
                super::super::super::animation_menu::Operation::CompareCurves,
                &mut actions,
            );
            assert!(actions.is_empty());
            p.timeline.comparison.bar_focus = Some(0);
            let keys = Input {
                keys: vec![(Key::Tab, Modifiers::default())],
                ..Default::default()
            };
            assert!(draw(&mut p, &s, &keys, &mut atlas, floating).is_empty());
            eprintln!("comparison keyboard zh={} float={floating} speed={} unit={:?} compact={} focus={:?} mode_open={}",locale==Locale::Zh,p.timeline.speed_graph,p.timeline.comparison.unit,p.timeline.comparison.compact_groups,p.timeline.comparison.bar_focus,p.timeline.comparison.mode.open);
            assert_eq!(p.timeline.comparison.bar_focus, Some(1));
            let keys = Input {
                keys: vec![(Key::Space, Modifiers::default())],
                ..Default::default()
            };
            assert!(draw(&mut p, &s, &keys, &mut atlas, floating).is_empty());
            assert!(p.timeline.comparison.mode.open);
            let keys = Input {
                keys: vec![
                    (Key::Down, Modifiers::default()),
                    (Key::Enter, Modifiers::default()),
                ],
                ..Default::default()
            };
            assert!(draw(&mut p, &s, &keys, &mut atlas, floating).is_empty());
            // All values, px values, % values, then all speeds.
            for _ in 0..2 {
                p.timeline.comparison.bar_focus = Some(1);
                assert!(draw(
                    &mut p,
                    &s,
                    &Input {
                        keys: vec![(Key::Space, Modifiers::default())],
                        ..Default::default()
                    },
                    &mut atlas,
                    floating
                )
                .is_empty());
                assert!(draw(&mut p, &s, &keys, &mut atlas, floating).is_empty());
                eprintln!("comparison keyboard step zh={} float={floating} speed={} unit={:?} compact={} focus={:?} mode_open={}",locale==Locale::Zh,p.timeline.speed_graph,p.timeline.comparison.unit,p.timeline.comparison.compact_groups,p.timeline.comparison.bar_focus,p.timeline.comparison.mode.open);
            }
            assert!(p.timeline.speed_graph);
            let ime = Input {
                composing: true,
                ime_consumed: true,
                keys: vec![(Key::Space, Modifiers::default())],
                ..Default::default()
            };
            p.timeline.comparison.bar_focus = Some(1);
            assert!(draw(&mut p, &s, &ime, &mut atlas, floating).is_empty());
            assert!(p.timeline.speed_graph);
            draw(
                &mut p,
                &s,
                &Input {
                    focus_lost: true,
                    ..Default::default()
                },
                &mut atlas,
                floating,
            );
            assert_eq!(p.timeline.comparison.bar_focus, None);
        }
    }
}
