use super::*;
const NAME:&str="panels::audio::tests::mapped::faults::audio_authoring_faults_use_default_bounded_logs_and_never_stdout";
const SITES: [&str; 6] = [
    "audio.numeric_time_begin",
    "audio.numeric_time_continue",
    "audio.rail_times",
    "audio.rail_select_navigate",
    "audio.rail_select_toggle",
    "audio.navigation_time",
];

fn render(h: &mut Harness, state: &Value, input: &Input, out: &mut Vec<Action>) {
    let mut paint = PaintList::default();
    let area = h.area();
    let width = GUTTER.min(area.width() * 0.52);
    let gutter = Rect::new(area.min[0], area.min[1], width, area.height());
    let track = Rect::new(
        area.min[0] + width,
        area.min[1],
        area.width() - width,
        area.height(),
    );
    let row = crate::panels::timeline::rows(&h.state).remove(0);
    let y = h.fields(Control::Gain).watch.min[1];
    render_rows(
        &mut paint,
        &mut h.atlas,
        gutter,
        area,
        track,
        y,
        22.,
        8.,
        0.,
        &row,
        &mut h.panels,
        state,
        input,
        out,
    );
}
#[test]
fn audio_authoring_faults_use_default_bounded_logs_and_never_stdout() {
    const CHILD: &str = "MOTION_AUDIO_AUTHORING_FAULT_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _log = crate::logging::init(root.clone()).unwrap();
        let mut h = harness(false, Locale::En, false);
        let healthy = h.state.clone();
        let before = healthy["project"].clone();
        let mut bad_identity = healthy.clone();
        bad_identity["root"] = Value::Null;
        let mut bad_time = healthy.clone();
        bad_time["frame"] = Value::Null;
        let mut bad_clock = healthy.clone();
        bad_clock["project"]["layers"][0]["time_mapping"]["clock"] = Value::Null;
        let field = h.field(Control::Left);
        let at = [field.min[0] + 5., field.min[1] + 10.];
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for _ in 0..120 {
            let mut controls = Controls::default();
            let mut actions = vec![];
            let mut number = |controls: &mut Controls, state: &Value| {
                controls.number(
                    &mut PaintList::default(),
                    &mut atlas,
                    field,
                    7,
                    Control::Left,
                    -12.,
                    NumberOptions {
                        min: -192.,
                        max: 24.,
                        speed: 0.1,
                        decimals: 2,
                    },
                    state,
                    &pointer(at, true, false),
                    true,
                    &mut actions,
                )
            };
            number(&mut controls, &bad_identity);
            assert!(controls.field.is_none());
            number(&mut controls, &healthy);
            assert!(controls.field.is_some());
            h.panels.audio_controls = controls;
            prepare(
                &mut h.panels,
                &bad_identity,
                &Input::default(),
                &mut actions,
            );
            assert!(h.panels.audio_controls.field.is_none());
            assert!(actions.is_empty());
            // Healthy calls between failures do not reset process-lifetime sites.
            render(&mut h, &healthy, &Input::default(), &mut actions);
            assert!(actions.is_empty());
            render(&mut h, &bad_time, &Input::default(), &mut actions);
            assert!(actions.is_empty());
            for slot in [0, 1] {
                let point = rail(&h, Control::Left, slot);
                render(
                    &mut h,
                    &bad_identity,
                    &pointer(point, true, false),
                    &mut actions,
                );
                assert!(actions.is_empty());
            }
            h.panels.audio_controls.active_object = Some(7);
            h.panels.audio_controls.active_control = Some(Control::Left);
            h.panels.audio_controls.context = Some((
                healthy["root"].as_str().unwrap().into(),
                healthy["composition"].as_str().unwrap().into(),
            ));
            assert!(shortcut(
                crate::shortcuts::Command::NextKey,
                &mut h.panels,
                &bad_clock,
                &mut actions
            ));
            assert!(actions.is_empty());
            assert!(shortcut(
                crate::shortcuts::Command::NextKey,
                &mut h.panels,
                &healthy,
                &mut actions
            ));
            assert!(matches!(actions.as_slice(), [Action::Seek(11.)]));
            actions.clear();
        }
        assert_eq!(h.engine.state().unwrap()["project"], before);
        crate::logging::flush();
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let rows: Vec<_> = log
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(rows.len(), 2, "{site}: {log}");
            assert!(rows[0].contains("WARN") && rows[0].contains("count=1 "));
            assert!(rows[1].contains("ERROR") && rows[1].contains("count=60 "));
            assert!(rows
                .iter()
                .all(|l| l.contains("object=7") && l.contains("reason=")));
        }
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_AUDIO_AUTHORING_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &result.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &result.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{stdout}\n{stderr}");
    for site in SITES {
        let rows: Vec<_> = stderr
            .lines()
            .filter(|l| l.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(rows.len(), 1, "{site}: {stderr}");
        assert!(rows[0].contains("ERROR") && rows[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
