use super::*;

fn draw(
    editor: &mut Editor,
    atlas: &mut TextAtlas,
    value: &CurveObject,
    input: &Input,
) -> Vec<Action> {
    let rect = Rect::new(0., 0., 300., height(&PaintList::default().styles(), 300.));
    let mut actions = Vec::new();
    editor.draw(
        &mut PaintList::default(),
        atlas,
        rect,
        rect,
        Some(0),
        input,
        true,
        value,
        |value| Command::Effect {
            object: 7,
            action: motion_core::EffectAction::SetCurveObject {
                effect: 1,
                param: "curve".into(),
                frame: 0,
                value,
            },
        },
        &mut actions,
        &mut None,
    );
    actions
}

#[test]
fn refused_drag_sampling_cancels_once_and_retains_last_healthy_cache() {
    let mut editor = Editor::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let healthy = CurveObject::default();
    let cached = editor.samples(&healthy).unwrap().to_vec();
    let mut bad = healthy.clone();
    bad.sampled_lut = Some(vec![[0.; 4]]);
    assert!(matches!(
        editor.samples(&bad),
        Err(motion_core::Error::Invalid(_))
    ));
    assert_eq!(editor.cache.as_ref().unwrap().1, cached);
    editor.drag = Some(Drag {
        point: 0,
        value: bad,
        moved: false,
    });
    assert!(matches!(
        draw(
            &mut editor,
            &mut atlas,
            &healthy,
            &Input {
                held: Some(MouseButton::Left),
                ..Default::default()
            }
        )
        .as_slice(),
        [Action::History(4)]
    ));
    assert!(!editor.dragging());
    assert_eq!(editor.cache.as_ref().unwrap().1, cached);
    assert!(draw(&mut editor, &mut atlas, &healthy, &Input::default()).is_empty());
    assert_eq!(editor.samples(&healthy).unwrap(), cached);
}

#[test]
fn actual_curve_draw_failures_log_at_default_level_with_bounded_lifetime_counts() {
    const CHILD: &str = "MOTION_COLOR_CURVE_SAMPLING_CHILD";
    const NAME: &str = "panels::effect_controls::curve::sampling_failure_tests::actual_curve_draw_failures_log_at_default_level_with_bounded_lifetime_counts";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut editor = Editor::default();
        let healthy = CurveObject::default();
        let mut bad = healthy.clone();
        bad.sampled_lut = Some(vec![[0.; 4]]);
        for _ in 0..59 {
            assert!(draw(&mut editor, &mut atlas, &bad, &Input::default()).is_empty());
            assert!(draw(&mut editor, &mut atlas, &healthy, &Input::default()).is_empty());
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = text
            .lines()
            .filter(|l| l.contains("site=effects.curve_samples "))
            .collect();
        assert_eq!(lines.len(), 1, "{text}");
        assert!(lines[0].contains(" WARN ") && lines[0].contains("count=1 "));
        assert!(draw(&mut editor, &mut atlas, &bad, &Input::default()).is_empty());
        for _ in 0..60 {
            assert!(draw(&mut editor, &mut atlas, &bad, &Input::default()).is_empty());
            assert!(draw(&mut editor, &mut atlas, &healthy, &Input::default()).is_empty());
        }
        assert!(!editor.dragging());
        assert_eq!(editor.samples(&healthy).unwrap().len(), 256);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = file
        .lines()
        .filter(|l| l.contains("site=effects.curve_samples "))
        .collect();
    assert_eq!(lines.len(), 2, "{file}");
    assert!(lines[0].contains(" WARN ") && lines[0].contains("count=1 "));
    assert!(lines[1].contains(" ERROR ") && lines[1].contains("count=60 "));
    assert!(lines
        .iter()
        .all(|l| l.contains("phase=graph channel=0 reason=invalid frozen curve LUT")));
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.contains("site=effects.curve_samples "))
            .count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("count=60 ") && !stderr.contains("count=1 "));
    assert!(
        !stdout.contains("site=effects.curve_samples")
            && !stdout.contains("invalid frozen curve LUT")
    );
}

#[test]
fn refused_key_toggle_emits_no_edits_and_preserves_default_failure_channels_then_recovers() {
    const CHILD: &str = "MOTION_COLOR_CURVE_KEY_SAMPLE_CHILD";
    const NAME: &str = "panels::effect_controls::curve::sampling_failure_tests::refused_key_toggle_emits_no_edits_and_preserves_default_failure_channels_then_recovers";
    const SITE: &str = "site=effect.curve_key_navigation_sample ";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let healthy = motion_core::EffectParam {
            kind: motion_effects::ParamKind::Curve,
            animatable: true,
            implemented: true,
            min: 0.,
            max: 1.,
            default: [0.; 4],
            track: motion_core::Track::constant([0.; 4]),
            curve: Some(motion_core::CurveTrack::default()),
        };
        let mut bad = healthy.clone();
        bad.curve.as_mut().unwrap().value.sampled_lut = Some(vec![[0.; 4]]);
        let before = bad.clone();
        let state = serde_json::json!({"root":"fixture","composition":"main","frame":5,"project":{"frames":60}});
        let list = PaintList::default();
        let styles = list.styles();
        let slot = styles.metric("icon.slot", 20.);
        let row = Rect::new(0., 0., slot * 8., styles.metric("effect.row.height", 26.));
        let point = [
            styles.metric("padding.panel", 8.) + slot * 1.5,
            row.height() * 0.5,
        ];
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        let mut channel = None;
        let mut toggle = |parameter: &motion_core::EffectParam| {
            let mut out = Vec::new();
            assert!(super::super::key_navigation::parameter_navigation(
                &mut channel,
                &mut PaintList::default(),
                row,
                &state,
                &input,
                7,
                1,
                "curve",
                parameter,
                5.,
                false,
                &mut out,
            ));
            out
        };
        for _ in 0..59 {
            assert!(toggle(&bad).is_empty());
            assert!(
                matches!(toggle(&healthy).as_slice(), [Action::EditBatch(commands)] if commands.len() == 2)
            );
        }
        let first = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert_eq!(
            first.lines().filter(|line| line.contains(SITE)).count(),
            1,
            "{first}"
        );
        for _ in 0..61 {
            assert!(toggle(&bad).is_empty());
            assert!(
                matches!(toggle(&healthy).as_slice(), [Action::EditBatch(commands)] if commands.len() == 2)
            );
        }
        assert_eq!(bad, before);
        let actions = toggle(&healthy);
        let [Action::EditBatch(commands)] = actions.as_slice() else {
            panic!("healthy curve key toggle did not recover: {actions:?}");
        };
        let Command::Effect {
            action:
                motion_core::EffectAction::SetCurveObject {
                    frame: 5, value, ..
                },
            ..
        } = &commands[1]
        else {
            panic!("healthy toggle did not retain the actual curve value: {commands:?}");
        };
        assert_eq!(
            value.sampled_lut.as_ref().unwrap().as_slice(),
            healthy.curve.as_ref().unwrap().sample(5.).unwrap()
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let lines: Vec<_> = file.lines().filter(|line| line.contains(SITE)).collect();
    assert_eq!(lines.len(), 2, "{file}");
    assert!(lines[0].contains(" WARN ") && lines[0].contains("count=1 "));
    assert!(lines[1].contains(" ERROR ") && lines[1].contains("count=60 "));
    assert!(lines.iter().all(
        |line| line.contains("object=7 effect=1 param=curve frame=5")
            && line.contains("invalid frozen curve LUT")
    ));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.lines().filter(|line| line.contains(SITE)).count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("count=60 ") && !stderr.contains("count=1 "));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(SITE) && !stdout.contains("invalid frozen curve LUT"));
}
