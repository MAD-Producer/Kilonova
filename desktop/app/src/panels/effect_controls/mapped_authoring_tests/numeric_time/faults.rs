use super::*;
const NAME: &str = "panels::effect_controls::mapped_authoring_tests::numeric_time::faults::numeric_time_faults_keep_original_causes_in_default_file_and_error_stderr";
const SITES: [&str; 4] = [
    "properties.numeric_time_begin",
    "properties.numeric_time_continue",
    "effects.numeric_time_begin",
    "effects.numeric_time_continue",
];

#[test]
fn numeric_time_faults_keep_original_causes_in_default_file_and_error_stderr() {
    const CHILD: &str = "MOTION_NUMERIC_TIME_FAULT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (mut engine, catalogue) = fixture(false);
        let before = engine.snapshot();
        let healthy = property_state(&engine);
        let mut bad = healthy.clone();
        bad["root"] = Value::Null;
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for _ in 0..120 {
            let mut panels = Panels {
                selected: Some(7),
                ..Default::default()
            };
            let mut paint = PaintList::default();
            let mut actions = vec![];
            panels::properties::draw_value(
                &mut paint,
                &mut atlas,
                Rect::new(90., 20., 120., 30.),
                &mut panels,
                &bad,
                &pointer([100., 35.], true, false),
                7,
                Property::Position,
                0,
                20.,
                false,
                false,
                &mut actions,
            );
            assert!(actions.is_empty() && panels.properties.field.is_none());
            start_property(&mut panels, &mut atlas, &mut engine, false);
            panels::properties::process_field(&mut panels, &bad, &Input::default(), &mut actions);
            assert!(actions.is_empty() && panels.properties.field.is_none());

            let mut controls = controls(&catalogue);
            draw(
                &mut controls,
                &mut atlas,
                &healthy,
                &Input::default(),
                Locale::En,
            );
            let point = midpoint(controls.test_rects[&(7, 1, "p0001".into(), 0)]);
            assert!(draw(
                &mut controls,
                &mut atlas,
                &bad,
                &pointer(point, true, false),
                Locale::En
            )
            .1
            .is_empty());
            assert!(controls.field.is_none());
            start_effect(&mut controls, &mut atlas, &mut engine, Locale::En, false);
            assert!(!controls.numeric_outside(
                &bad,
                &pointer([800., 740.], true, false),
                None,
                &mut actions
            ));
            assert!(actions.is_empty() && controls.field.is_none());
            assert_eq!(engine.snapshot(), before);
            // Healthy construction between faults must not reset these existing
            // process-lifetime site counters or spam mixed-row diagnostics.
        }
        crate::logging::flush();
        let logs = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let records: Vec<_> = logs
                .lines()
                .filter(|line| line.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(records.len(), 2, "{site}: {logs}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
            assert!(records.iter().all(|line| line.contains("object=7")
                && line.contains("invalid numeric authoring document identity")));
        }
        assert!(!engine.can_undo() && !engine.gesture_active());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_NUMERIC_TIME_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    for site in SITES {
        let records: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(records.len(), 1, "{stderr}");
        assert!(records[0].contains("ERROR") && records[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
