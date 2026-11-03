use super::*;
use motion_core::{Project, Track, Tween};

pub(in crate::panels) fn spatial_state(reverse: bool) -> Value {
    let (_folder, engine) = spatial_fixture(reverse);
    engine.state().unwrap()
}
pub(in crate::panels) fn spatial_fixture(
    reverse: bool,
) -> (tempfile::TempDir, crate::engine::Engine) {
    let (_folder, engine) = mapped_anchor_tests::fixture(reverse);
    let mut state = engine.state().unwrap();
    let mut project: Project = serde_json::from_value(state["project"].clone()).unwrap();
    let transform = &mut project.layers[0].transform;
    // The legacy fixture separates position; restore its three vector keys.
    transform.position.merge_dimensions().unwrap();
    fn fixed_spatial<T: Tween>(track: &mut Track<T>) {
        track.enable_roving(&[-3]).unwrap();
        let spatial = track.spatial_animation.as_mut().unwrap();
        let key = spatial.keys()[1].id;
        spatial.set_fixed_time(key, -3.).unwrap();
    }
    fixed_spatial(&mut transform.anchor);
    fixed_spatial(&mut transform.position);
    project.validate().unwrap();
    drop(engine);
    motion_core::storage::save(_folder.path(), &project).unwrap();
    let engine = crate::engine::Engine::start(
        _folder.path().to_owned(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine.seek(6.5).unwrap();
    (_folder, engine)
}
fn pointer(point: [f32; 2], press: bool) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
pub(in crate::panels) fn replace_middle(state: &mut Value, property: &str) {
    let spatial = &mut state["project"]["layers"][0]["transform"][property]["spatial_animation"];
    let next = spatial["next_id"].as_u64().unwrap();
    spatial["keys"][1]["id"] = next.into();
    spatial["next_id"] = (next + 1).into();
    let project: Project = serde_json::from_value(state["project"].clone()).unwrap();
    project.validate().unwrap();
}
#[test]
fn replacing_either_compensated_key_cancels_numeric_anchor_before_an_edit_is_sent() {
    for reverse in [false, true] {
        for property in ["anchor", "position"] {
            let state = spatial_state(reverse);
            let mut changed = state.clone();
            replace_middle(&mut changed, property);
            let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
            let mut control = AnchorControls::default();
            let area = Rect::new(0., 0., 400., 160.);
            let styles = PaintList::default().styles();
            let rect = inspector_layout(&styles, area, 0., 0, 2).number;
            let start = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            let mut actions = vec![];
            control.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                0.,
                Panel::Properties,
                &state["project"]["layers"][0],
                &state,
                &pointer(start, true),
                false,
                &mut actions,
            );
            assert!(control.editing());
            actions.clear();
            control.process(
                &styles,
                &changed,
                &pointer([start[0] + 20., start[1]], false),
                &mut actions,
            );
            assert!(!control.editing(), "reverse={reverse} replaced={property}");
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Edit(_))),
                "stale anchor must not target the replacement key"
            );
        }
    }
}

#[test]
fn stale_bound_worker_command_uses_default_error_outputs_and_fresh_binding_recovers() {
    const CHILD: &str = "MOTION_ANCHOR_KEYS_FAILURE_CHILD";
    const TEST:&str="panels::properties::anchor_key_tests::stale_bound_worker_command_uses_default_error_outputs_and_fresh_binding_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut state = spatial_state(false);
        replace_middle(&mut state, "position");
        let project: motion_core::Project =
            serde_json::from_value(state["project"].clone()).unwrap();
        let project_root = root.join("project");
        motion_core::storage::save(&project_root, &project).unwrap();
        let engine =
            crate::engine::Engine::start(project_root, motion_desktop_media::platform(), 8 << 30)
                .unwrap();
        engine.seek(6.5).unwrap();
        let before = engine.state().unwrap();
        let stale = Command::AnchorAt {
            object: 7,
            frame: 6.5,
            anchor: [0.7, 0.2],
            keys: Some(motion_core::AnchorKeyBinding {
                anchor: Some(2),
                position: Some(2),
            }),
        };
        let error = engine
            .edit(serde_json::to_string(&stale).unwrap())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("anchor_key_binding")
                && error.contains("key=2")
                && error.contains("no longer exists"),
            "{error}"
        );
        let rejected = engine.state().unwrap();
        assert_eq!(rejected["project"], before["project"]);
        assert_eq!(rejected["history"], before["history"]);
        let current = AnchorKeysDraft::read(&before, 7).unwrap();
        let healthy = current.command(anchor_command(&before, 7, [0.7, 0.2]));
        engine
            .edit(serde_json::to_string(&healthy).unwrap())
            .unwrap();
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let failures: Vec<_> = file
            .lines()
            .filter(|l| l.contains(" ERROR ") && l.contains("anchor_key_binding"))
            .collect();
        assert_eq!(failures.len(), 1, "{file}");
        assert!(
            failures[0].contains("edit")
                && failures[0].contains("key=2")
                && failures[0].contains("7")
        );
        assert!(file
            .lines()
            .any(|l| l.contains(" INFO ") && l.contains("start #") && l.contains("edit")));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.contains("anchor_key_binding") && l.contains("ERROR"))
            .count(),
        1,
        "{stderr}"
    );
    assert!(
        !stdout.contains("anchor_key_binding")
            && !stdout.contains(" INFO [")
            && !stdout.contains(" ERROR ")
    );
    if let Some(destination) = std::env::var_os("MOTION_ANCHOR_KEYS_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::copy(
            root.path().join("motion-studio.log"),
            destination.join("motion-studio.log"),
        )
        .unwrap();
        std::fs::write(destination.join("stdout.log"), output.stdout).unwrap();
        std::fs::write(destination.join("stderr.log"), output.stderr).unwrap();
    }
}
