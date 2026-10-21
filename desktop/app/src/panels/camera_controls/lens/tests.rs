use super::*;
use crate::engine::Engine;
use motion_ui::input::{Key, Modifiers};
#[cfg(target_os = "windows")]
mod native;

fn run(engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::History(op) => {
                engine.history(op).unwrap();
            }
            Action::Edit(command) => {
                engine
                    .edit(serde_json::to_string(&command).unwrap())
                    .unwrap();
            }
            other => panic!("unexpected action {other:?}"),
        }
    }
}
fn draw(
    controls: &mut super::super::Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    object: u64,
    input: &Input,
) -> Vec<Action> {
    let mut out = vec![];
    controls.render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 360., 760.),
        &mut 0.,
        state,
        object,
        input,
        Locale::En,
        &mut Default::default(),
        &mut 0,
        &mut out,
    );
    out
}
fn input(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        held: (!released).then_some(MouseButton::Left),
        ..Default::default()
    }
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn create(engine: &Engine) -> u64 {
    let state = engine
        .edit(serde_json::to_string(&Command::CreateCamera).unwrap())
        .unwrap();
    state["project"]["layers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn derived_numeric_gestures_share_fov_and_cancel_or_commit_one_history_entry() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let object = create(&engine);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for parameter in [Parameter::Focal, Parameter::Film, Parameter::Zoom] {
        for cancel in [false, true] {
            let before = engine.state().unwrap();
            let mut controls = super::super::Controls::default();
            assert!(draw(
                &mut controls,
                &mut atlas,
                &before,
                object,
                &Input::default()
            )
            .is_empty());
            let rect = controls
                .lens
                .rects
                .iter()
                .find(|(p, _)| *p == parameter)
                .unwrap()
                .1;
            let [x, y] = center(rect);
            run(
                &engine,
                draw(
                    &mut controls,
                    &mut atlas,
                    &before,
                    object,
                    &input([x, y], true, false),
                ),
            );
            run(
                &engine,
                draw(
                    &mut controls,
                    &mut atlas,
                    &engine.state().unwrap(),
                    object,
                    &input([x + 20., y], false, false),
                ),
            );
            let changed = engine.state().unwrap();
            assert_ne!(
                camera(&changed, object)["fov"],
                camera(&before, object)["fov"]
            );
            assert!(controls.drag_readout.is_some());
            let finish = if cancel {
                Input {
                    keys: vec![(Key::Escape, Modifiers::default())],
                    ..Default::default()
                }
            } else {
                input([x + 30., y], false, true)
            };
            run(
                &engine,
                draw(&mut controls, &mut atlas, &changed, object, &finish),
            );
            assert!(!controls.editing());
            let after = engine.state().unwrap();
            assert_eq!(
                camera(&after, object)["position"],
                camera(&before, object)["position"]
            );
            assert_eq!(
                camera(&after, object)["target"],
                camera(&before, object)["target"]
            );
            if cancel {
                assert_eq!(after["project"], before["project"]);
            } else {
                assert_eq!(
                    after["history"]["undoEntries"].as_u64().unwrap(),
                    before["history"]["undoEntries"].as_u64().unwrap() + 1
                );
                assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            }
        }
    }
}

#[test]
fn lens_editor_rejects_malformed_snapshots_and_does_not_edit_locked_or_observed_cameras() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let object = create(&engine);
    let before = engine.state().unwrap();
    assert_eq!(snapshot(&before, object).unwrap().0.film_width_mm, 36.);
    let mut broken = before.clone();
    let layer = broken["project"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|l| l["id"] == object)
        .unwrap();
    layer["content"]["camera"]["film_width_mm"] = serde_json::json!("invalid");
    assert!(snapshot(&broken, object)
        .unwrap_err()
        .contains("not numeric"));
    assert!(snapshot(&before, u64::MAX).is_err());
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for observed in [false, true] {
        let mut state = before.clone();
        if observed {
            state["observing"] = true.into();
        } else {
            state["project"]["layers"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|l| l["id"] == object)
                .unwrap()["locked"] = true.into();
        }
        let mut controls = super::super::Controls::default();
        draw(&mut controls, &mut atlas, &state, object, &Input::default());
        let point = center(controls.lens.rects[0].1);
        assert!(draw(
            &mut controls,
            &mut atlas,
            &state,
            object,
            &input(point, true, false)
        )
        .is_empty());
        assert!(!controls.editing());
    }
    // Leaving the window cancels a live gesture, before another target edits.
    let mut controls = super::super::Controls::default();
    draw(
        &mut controls,
        &mut atlas,
        &before,
        object,
        &Input::default(),
    );
    let [x, y] = center(controls.lens.rects[0].1);
    run(
        &engine,
        draw(
            &mut controls,
            &mut atlas,
            &before,
            object,
            &input([x, y], true, false),
        ),
    );
    run(
        &engine,
        draw(
            &mut controls,
            &mut atlas,
            &engine.state().unwrap(),
            object,
            &input([x + 20., y], false, false),
        ),
    );
    run(
        &engine,
        draw(
            &mut controls,
            &mut atlas,
            &engine.state().unwrap(),
            object,
            &Input {
                focus_lost: true,
                ..Default::default()
            },
        ),
    );
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
}

#[test]
fn preset_popup_consumes_outside_click_before_the_camera_mode_can_open() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let object = create(&engine);
    let state = engine.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut controls = super::super::Controls::default();
    draw(&mut controls, &mut atlas, &state, object, &Input::default());
    let preset = center(controls.lens.preset_rect.unwrap());
    assert!(draw(
        &mut controls,
        &mut atlas,
        &state,
        object,
        &input(preset, true, false)
    )
    .is_empty());
    assert!(controls.lens.menu_open());
    let styles = PaintList::default().styles();
    let mode = center(
        super::super::super::properties::inspector_layout(
            &styles,
            Rect::new(0., 0., 360., 760.),
            0.,
            0,
            1,
        )
        .number,
    );
    assert!(draw(
        &mut controls,
        &mut atlas,
        &state,
        object,
        &input(mode, true, false)
    )
    .is_empty());
    assert!(!controls.lens.menu_open() && !controls.mode.open);
    assert!(draw(
        &mut controls,
        &mut atlas,
        &state,
        object,
        &input(mode, true, false)
    )
    .is_empty());
    assert!(controls.mode.open && !controls.lens.menu_open());
}

#[test]
fn actual_lens_failures_reach_default_logs_and_stderr_with_recovery_and_stdout_isolation() {
    const CHILD: &str = "MOTION_CAMERA_LENS_FAILURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let object = create(&engine);
        let healthy = engine.state().unwrap();
        let error = engine
            .edit(
                serde_json::to_string(&Command::CameraLens {
                    object,
                    frame: 0,
                    film_width_mm: 0.,
                    focal_length_mm: 50.,
                })
                .unwrap(),
            )
            .unwrap_err();
        assert!(error.contains("film width"));
        std::fs::write(root.join("expected-error.txt"), &error).unwrap();
        let mut broken = healthy.clone();
        broken["project"]["width"] = 0.into();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut controls = super::super::Controls::default();
        for _ in 0..120 {
            assert!(draw(
                &mut controls,
                &mut atlas,
                &broken,
                object,
                &Input::default()
            )
            .is_empty());
        }
        assert!(draw(
            &mut controls,
            &mut atlas,
            &healthy,
            object,
            &Input::default()
        )
        .is_empty());
        for _ in 0..60 {
            assert!(draw(
                &mut controls,
                &mut atlas,
                &broken,
                object,
                &Input::default()
            )
            .is_empty());
        }
        log::logger().flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_CAMERA_LENS_FAILURE_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::camera_controls::lens::tests::actual_lens_failures_reach_default_logs_and_stderr_with_recovery_and_stdout_isolation"])
        .env(CHILD,&root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let error = std::fs::read_to_string(root.join("expected-error.txt")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(file.contains("logging initialized level=INFO"));
    assert!(file.contains(&error) && stderr.contains(&error));
    assert_eq!(
        file.lines()
            .filter(|line| line.contains("site=camera.lens.snapshot "))
            .count(),
        4
    );
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("site=camera.lens.snapshot "))
            .count(),
        2
    );
    assert!(!stdout.contains("degradation") && !stdout.contains(&error));
}
