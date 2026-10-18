use super::*;

#[test]
fn real_panel_fallbacks_are_visible_bounded_and_preserve_normal_optional_fields() {
    const CHILD: &str = "MOTION_PANEL_DATA_OBSERVATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7,
            "Observation",
            [100.; 2],
            [100., 100., 0.],
            [1.; 4],
        ));
        let healthy = serde_json::json!({"frame":0,"project":project,"timeline_layers":project.timeline_layers(0.)});
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        let log_path = root.join("motion-studio.log");
        let track = serde_json::json!({"value":1.,"keys":[{"frame":0,"value":1.}]});
        for _ in 0..120 {
            let rows = timeline::rows(&healthy);
            assert_eq!(rows.len(), 1);
            assert!(rows[0].masks.is_empty() && rows[0].vector.is_none());
            assert!(!timeline_columns::matte_choices(&healthy, 7, Locale::En)
                .0
                .is_empty());
            assert_eq!(
                animation_data::track::<f32>(&track, 0).unwrap().sample(0.),
                1.
            );
            assert_eq!(
                graph_handles::observed_easing(&serde_json::json!({}), 7, Property::Opacity, 0.),
                motion_core::Easing::default()
            );
        }
        assert!(std::fs::read_to_string(&log_path).unwrap().is_empty());
        let mut broken = healthy.clone();
        broken["project"]["layers"][0]["masks"] = serde_json::json!(17);
        broken["project"]["layers"][0]["content"] =
            serde_json::json!({"kind":"vector","vector":17});
        broken["project"]["layers"][0]["size"] = serde_json::json!(17);
        broken["project"]["layers"][0]["track_matte"] = serde_json::json!(17);
        let mut bad_track = track.clone();
        bad_track["keys"][0]["value"] = serde_json::json!({"bad":true});
        for _ in 0..120 {
            let rows = timeline::rows(&broken);
            assert!(rows[0].masks.is_empty() && rows[0].vector.is_none());
            assert_eq!(rows[0].vector_size, [100.; 2]);
            timeline_columns::matte_choices(&broken, 7, Locale::En);
            assert!(animation_data::track::<f32>(&bad_track, 0).is_none());
            // Interleave healthy rows: a bad item must not warn every frame.
            timeline::rows(&healthy);
            timeline_columns::matte_choices(&healthy, 7, Locale::En);
            assert!(animation_data::track::<f32>(&track, 0).is_some());
        }
        let mut panels = Panels {
            selected: Some(7),
            ..Default::default()
        };
        let area = Rect::new(0., 0., 640., 480.);
        for _ in 0..120 {
            let mut actions = Vec::new();
            curves::render(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &mut panels,
                &healthy,
                &Input::default(),
                &mut actions,
            );
            assert!(actions.is_empty()); // Missing sample keeps the original early return.
        }
        let text = std::fs::read_to_string(&log_path).unwrap();
        for site in [
            "timeline.masks",
            "timeline.vector",
            "timeline.vector_size",
            "timeline.matte_choice",
            "timeline.matte_layers",
            "animation.key_decode",
            "curves.snapshot",
        ] {
            let records = text
                .lines()
                .filter(|line| line.contains(&format!("site={site} ")))
                .collect::<Vec<_>>();
            assert_eq!(records.len(), 2, "{site}: {text}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
        }
        assert!(text.contains("object=7") && text.contains("frame=0 key_index=0"));
        assert_eq!(healthy["project"], serde_json::to_value(project).unwrap());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "panels::observation_tests::real_panel_fallbacks_are_visible_bounded_and_preserve_normal_optional_fields"])
        .env_remove("MOTION_LOG").env(CHILD, temp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("persistent panel degradation site=animation.key_decode"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}

#[test]
fn retained_clip_gestures_log_lost_release_and_routing_without_closing_history() {
    const CHILD: &str = "MOTION_PANEL_GESTURE_OBSERVATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7,
            "Gesture",
            [100.; 2],
            [100., 100., 0.],
            [1.; 4],
        ));
        let state = serde_json::json!({"frame":0,"project":project,"timeline_layers":project.timeline_layers(0.)});
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut panels = Panels::default();
        panels.timeline.trimming = Some((7, 0., 120., true, 0.));
        panels.timeline.moving = Some(types::MovingLayers {
            start_mouse: 0.,
            initial: 0,
            last: 0,
            layers: vec![(7, 0, 120)],
        });
        for _ in 0..120 {
            let mut actions = Vec::new();
            timeline::render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 1200., 400.),
                &mut panels,
                &state,
                &Input::default(),
                &mut actions,
            );
            assert!(actions.is_empty());
            assert!(panels.timeline.trimming.is_some() && panels.timeline.moving.is_some());
        }
        observe_interaction_boundary(&panels, "close_panel");
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in ["timeline.trim_release", "timeline.move_release"] {
            assert_eq!(
                text.lines()
                    .filter(|line| line.contains(&format!("site={site} ")))
                    .count(),
                2
            );
        }
        assert!(text.contains("operation=close_panel") && text.contains("phase=trimming"));
        let mut actions = Vec::new();
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 1200., 400.),
            &mut panels,
            &state,
            &Input {
                released: Some((MouseButton::Left, [0.; 2])),
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::History(3)))
                .count(),
            2
        );
        assert!(panels.timeline.trimming.is_none() && panels.timeline.moving.is_none());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "panels::observation_tests::retained_clip_gestures_log_lost_release_and_routing_without_closing_history"])
        .env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("persistent panel degradation site=timeline.trim_release"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}

#[test]
fn malformed_blend_mode_reports_but_optional_mode_remains_quiet() {
    const CHILD: &str = "MOTION_BLEND_FALLBACK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let path = root.join("motion-studio.log");
        let normal = serde_json::json!({"id":7});
        for _ in 0..120 { assert_eq!(timeline_columns::blend(&normal),motion_core::compositing::BlendMode::default()); }
        assert!(std::fs::read_to_string(&path).unwrap().is_empty());
        let bad = serde_json::json!({"id":7,"blend":{"mode":"unknown_mode"}});
        for _ in 0..120 { assert_eq!(timeline_columns::blend(&bad),motion_core::compositing::BlendMode::default()); timeline_columns::blend(&normal); }
        let text = std::fs::read_to_string(path).unwrap();
        let records = text.lines().filter(|l| l.contains("site=timeline.blend ")).collect::<Vec<_>>();
        assert_eq!(records.len(),2);
        assert!(records[1].contains("ERROR") && records[1].contains("object=7"));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::observation_tests::malformed_blend_mode_reports_but_optional_mode_remains_quiet"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("site=timeline.blend"));
}
