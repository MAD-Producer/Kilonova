use super::*;
use crate::panels::{properties, Action, Panels};
use motion_ui::{dock::Panel, input::Input, paint::PaintList, text::TextAtlas, MouseButton, Rect};
mod stable;

fn start(mode: &str) -> (tempfile::TempDir, crate::engine::Engine, Value, f64) {
    let (p, at) = fixture(Channel::Position, mode, false);
    let folder = tempfile::tempdir().unwrap();
    motion_core::storage::save(folder.path(), &p).unwrap();
    let engine = crate::engine::Engine::start(
        folder.path().to_owned(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = engine.seek(at).unwrap();
    (folder, engine, state, at)
}
fn pointer(x: f32, press: bool, release: bool) -> Input {
    let point = [x, 20.];
    Input {
        mouse: point,
        pressed: press.then_some((MouseButton::Left, point)),
        released: release.then_some((MouseButton::Left, point)),
        held: (!release).then_some(MouseButton::Left),
        ..Default::default()
    }
}
fn begin(panels: &mut Panels, atlas: &mut TextAtlas, state: &Value) -> Vec<Action> {
    let mut actions = vec![];
    let value = state["sampledLayers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == 7)
        .unwrap()["position"][0]
        .as_f64()
        .unwrap();
    properties::draw_value(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 100., 40.),
        panels,
        state,
        &pointer(20., true, false),
        7,
        Property::Position,
        0,
        value,
        false,
        panels.focused == Some(Panel::Timeline),
        &mut actions,
    );
    actions
}
fn apply(engine: &crate::engine::Engine, actions: Vec<Action>) {
    for a in actions {
        match a {
            Action::History(op) => {
                engine.history(op).unwrap();
            }
            Action::Edit(c) => {
                engine.edit(serde_json::to_string(&c).unwrap()).unwrap();
            }
            _ => panic!("unexpected numeric action: {a:?}"),
        }
    }
}
fn ready(host: Panel) -> Panels {
    let mut panels = Panels {
        focused: Some(host),
        ..Default::default()
    };
    panels.select_only(Some(7));
    panels
}

#[test]
fn real_numeric_scrubs_edit_the_stable_key_and_keep_one_atomic_history() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mode in ["plain", "forward", "reverse"] {
        for host in [Panel::Properties, Panel::Timeline] {
            let (_folder, engine, before, at) = start(mode);
            let mut panels = ready(host);
            assert!(begin(&mut panels, &mut atlas, &before).is_empty());
            assert!(panels.properties.field.as_ref().unwrap().time.is_some());
            let mut actions = vec![];
            properties::process_field(
                &mut panels,
                &before,
                &pointer(60., false, false),
                &mut actions,
            );
            assert!(matches!(actions.first(), Some(Action::History(2))));
            assert!(actions.iter().any(|a| matches!(a, Action::Edit(Command::TrackAt {frame, target, ..}) if *frame==at && target==&Channel::Position.target())), "{actions:?}");
            apply(&engine, actions);
            let preview = engine.state().unwrap();
            let mut expected: Project = serde_json::from_value(before["project"].clone()).unwrap();
            expected.layers[0]
                .transform
                .position
                .spatial_animation
                .as_mut()
                .unwrap()
                .set_value(2, [341., 180., 0.])
                .unwrap();
            assert_eq!(preview["project"], json!(expected));
            let mut actions = vec![];
            properties::process_field(
                &mut panels,
                &preview,
                &pointer(60., false, true),
                &mut actions,
            );
            assert!(matches!(actions.as_slice(), [Action::History(3)]));
            apply(&engine, actions);
            assert!(panels.properties.field.is_none());
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            assert_eq!(engine.history(1).unwrap()["project"], json!(expected));
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            assert_eq!(
                engine.history(0).unwrap()["project"],
                before["project"],
                "no second numeric history"
            );
            engine.request_shutdown().unwrap();
        }
    }
}

#[test]
fn numeric_context_changes_cancel_the_worker_preview_and_late_release_is_inert() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for cause in [
        "frame",
        "offset",
        "clock",
        "generation",
        "selection",
        "focus",
    ] {
        let (_folder, engine, before, _) = start("plain");
        let mut panels = ready(Panel::Properties);
        assert!(begin(&mut panels, &mut atlas, &before).is_empty());
        let mut actions = vec![];
        properties::process_field(
            &mut panels,
            &before,
            &pointer(60., false, false),
            &mut actions,
        );
        apply(&engine, actions);
        let mut state = engine.state().unwrap();
        let mut input = pointer(60., false, false);
        match cause {
            "frame" => state["frame"] = json!(state["frame"].as_f64().unwrap() + 0.25),
            "offset" => state["project"]["layers"][0]["timeline"]["offset_frame"] = json!(3),
            "clock" => {
                state["project"]["layers"][0]["time_mapping"] = json!(LayerTimeMapping {
                    clock: LayerClock::new(
                        TimeStretch::new(2, 1).unwrap(),
                        TimeDirection::Forward,
                        0.,
                        0.
                    )
                    .unwrap(),
                    remap: None
                })
            }
            "generation" => {
                state["document_generation"] =
                    json!(state["document_generation"].as_u64().unwrap() + 1)
            }
            "selection" => panels.select_only(None),
            "focus" => input.focus_lost = true,
            _ => unreachable!(),
        }
        let mut actions = vec![];
        properties::process_field(&mut panels, &state, &input, &mut actions);
        assert!(
            matches!(actions.as_slice(), [Action::History(4)]),
            "{cause}: {actions:?}"
        );
        apply(&engine, actions);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        let mut actions = vec![];
        properties::process_field(
            &mut panels,
            &before,
            &pointer(80., false, true),
            &mut actions,
        );
        assert!(actions.is_empty());
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        engine.request_shutdown().unwrap();
    }
}

#[test]
fn continuous_time_faults_reach_default_file_and_stderr_with_bounded_counts() {
    const NAME: &str = "panels::property_authoring::spatial_tests::worker::continuous_time_faults_reach_default_file_and_stderr_with_bounded_counts";
    const CHILD: &str = "MOTION_CONTINUOUS_TIME_FAULT_CHILD";
    const SITES: [&str; 2] = [
        "properties.numeric_time_begin",
        "properties.numeric_time_continue",
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (_folder, engine, healthy, _) = start("plain");
        let mut bad = healthy.clone();
        bad["project"]["layers"][0]["timeline"]["offset_frame"] = json!("broken");
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for _ in 0..180 {
            let mut panels = ready(Panel::Properties);
            assert!(begin(&mut panels, &mut atlas, &bad).is_empty());
            assert!(panels.properties.field.is_none());
            assert!(begin(&mut panels, &mut atlas, &healthy).is_empty());
            assert!(panels.properties.field.is_some());
            let mut actions = vec![];
            properties::process_field(&mut panels, &bad, &Input::default(), &mut actions);
            assert!(actions.is_empty() && panels.properties.field.is_none());
        }
        // Successful use after degradation must still work; cumulative observation does not reset.
        let mut panels = ready(Panel::Properties);
        assert!(begin(&mut panels, &mut atlas, &healthy).is_empty());
        let mut actions = vec![];
        properties::process_field(
            &mut panels,
            &healthy,
            &pointer(60., false, false),
            &mut actions,
        );
        apply(&engine, actions);
        apply(&engine, vec![Action::History(4)]);
        assert_eq!(engine.state().unwrap()["project"], healthy["project"]);
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let rows: Vec<_> = file
                .lines()
                .filter(|l| l.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(rows.len(), 2, "{site}: {file}");
            assert!(rows[0].contains("WARN") && rows[0].contains("count=1 "));
            assert!(rows[1].contains("ERROR") && rows[1].contains("count=60 "));
            assert!(rows
                .iter()
                .all(|l| l.contains("object=7") && l.contains("invalid property timeline offset")));
        }
        engine.request_shutdown().unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CONTINUOUS_TIME_FAULT_ARTIFACTS")
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for site in SITES {
        let rows: Vec<_> = stderr
            .lines()
            .filter(|l| l.contains(&format!("site={site} ")))
            .collect();
        assert_eq!(rows.len(), 1, "{stderr}");
        assert!(rows[0].contains("ERROR") && rows[0].contains("count=60 "));
        assert!(!stdout.contains(site));
    }
    assert!(!stdout.contains(" INFO [") && !stdout.contains(" ERROR "));
}
