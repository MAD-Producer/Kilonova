use super::*;

fn draw_snapshot(panels: &mut Panels, atlas: &mut TextAtlas, snapshot: &Value, input: Input) -> Vec<Action> {
    let mut out = Vec::new();
    timeline_graph::render(&mut PaintList::default(), atlas, Rect::new(0., 0., 1200., 600.),
        panels, snapshot, &input, 0., 10., &mut out);
    out
}

fn malformed(valid: &Value, frame: Value) -> Value {
    let mut broken = valid.clone();
    let key = &mut broken["timeline_layers"][0]["properties"]["position"]["keys"][1];
    assert_eq!(key["local_frame"], 20);
    key["frame"] = frame;
    broken
}

fn reject(panels: &mut Panels, atlas: &mut TextAtlas, valid: &Value, broken: &Value) {
    assert!(graph_box::select_all(panels, valid));
    assert!(panels.timeline.graph_box.has_selection());
    let unchanged = broken.clone();
    let actions = draw_snapshot(panels, atlas, broken, curve_pointer([220., 384.75], true, false));
    assert!(actions.is_empty(), "{actions:?}");
    assert!(!panels.timeline.graph_box.editing());
    assert!(!panels.timeline.graph_box.has_selection());
    assert!(panels.timeline.key_drag.is_none() && panels.timeline.graph_drag.is_none());
    assert!(panels.message.is_some());
    assert_eq!(broken, &unchanged);
}

#[test]
fn curve_box_snapshot_rejects_noninteger_composition_frames_before_history_and_recovers() {
    for locale in [Locale::En, Locale::Zh] {
        for host in [Panel::Timeline, Panel::Curves] {
            for frame in [serde_json::json!(32.0), serde_json::json!(32.25)] {
                let mut engine = engine();
                let before = engine.snapshot();
                let valid = state(&engine, 32.);
                let broken = malformed(&valid, frame);
                let mut panels = panels();
                panels.locale = locale;
                panels.focused = Some(host);
                panels.timeline.graph_mode = true;
                let mut atlas = TextAtlas::from_system(12.).unwrap();
                assert!(draw_snapshot(&mut panels, &mut atlas, &valid, Input::default()).is_empty());
                reject(&mut panels, &mut atlas, &valid, &broken);
                assert_eq!(engine.project(), &before);

                assert!(graph_box::select_all(&mut panels, &valid));
                assert!(draw_snapshot(&mut panels, &mut atlas, &valid,
                    curve_pointer([220., 384.75], true, false)).is_empty());
                let actions = draw_snapshot(&mut panels, &mut atlas, &valid,
                    curve_pointer([320., 364.75], false, true));
                assert!(matches!(actions.as_slice(),
                    [Action::History(2), Action::EditBatch(_), Action::History(3)]), "{actions:?}");
                apply_graph_actions(&mut engine, actions);
                assert_ne!(engine.project(), &before);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
            }
        }
    }
}

#[test]
fn curve_box_snapshot_faults_use_bounded_default_file_and_error_stderr_without_stdout() {
    const CHILD: &str = "MOTION_GRAPH_BOX_SNAPSHOT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let engine = engine();
        let valid = state(&engine, 32.);
        let broken = malformed(&valid, serde_json::json!(32.25));
        let mut panels = panels();
        panels.timeline.graph_mode = true;
        let _logger = crate::logging::init(root.clone()).unwrap();
        let path = root.join("motion-studio.log");
        assert!(draw_snapshot(&mut panels, &mut atlas, &valid, Input::default()).is_empty());
        crate::logging::flush();
        let startup = std::fs::read_to_string(&path).unwrap();
        assert!(!startup.contains("graph_box.seed_frames"));
        for _ in 0..59 { reject(&mut panels, &mut atlas, &valid, &broken); }
        crate::logging::flush();
        let at_59 = std::fs::read_to_string(&path).unwrap();
        let records: Vec<_> = at_59.lines().filter(|l| l.contains("site=graph_box.seed_frames")).collect();
        assert!(records.is_empty());

        // A neutral repaint does not attempt to begin a drag.
        assert!(draw_snapshot(&mut panels, &mut atlas, &valid, Input::default()).is_empty());
        crate::logging::flush();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), at_59);
        for _ in 0..121 { reject(&mut panels, &mut atlas, &valid, &broken); }
        crate::logging::flush();
        let text = std::fs::read_to_string(&path).unwrap();
        let records: Vec<_> = text.lines().filter(|l| l.contains("site=graph_box.seed_frames")).collect();
        assert_eq!(records.len(), 1);
        assert!(records[0].contains("ERROR") && records[0].contains("count=60"));
        assert!(records.iter().all(|l| l.contains("object=7") && l.contains("phase=begin")
            && l.contains("key_index=1") && l.contains("missing_or_noninteger_frame")));
        // A real successful start/empty release and a normal empty curve each reset the streak.
        for empty in [false, true] {
            if empty {
                let mut empty = valid.clone();
                empty["timeline_layers"][0]["properties"]["position"]["keys"] = serde_json::json!([]);
                assert!(draw_snapshot(&mut panels, &mut atlas, &empty, Input::default()).is_empty());
            } else {
                assert!(graph_box::select_all(&mut panels, &valid));
                assert!(draw_snapshot(&mut panels, &mut atlas, &valid,
                    curve_pointer([220., 384.75], true, false)).is_empty());
                assert!(draw_snapshot(&mut panels, &mut atlas, &valid,
                    curve_pointer([220., 384.75], false, true)).is_empty());
            }
            let before = std::fs::read_to_string(&path).unwrap();
            for _ in 0..59 { reject(&mut panels, &mut atlas, &valid, &broken); }
            crate::logging::flush();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
            reject(&mut panels, &mut atlas, &valid, &broken);
            crate::logging::flush();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains("site=graph_box.seed_frames")).count(), 3);
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_GRAPH_BOX_SNAPSHOT_ARTIFACTS")
        .map(std::path::PathBuf::from).unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "panels::timeline_shortcuts_tests::graph_box_snapshot_tests::curve_box_snapshot_faults_use_bounded_default_file_and_error_stderr_without_stdout",
            "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG").env(CHILD, &root).output().unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines()
        .filter(|l| l.contains("site=graph_box.seed_frames")).count(), 3);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("graph_box.seed_frames"));
}
