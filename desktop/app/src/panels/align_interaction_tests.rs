use super::*;
use crate::engine::Engine;
use motion_host::ops::alignment::{Edge, Target};
use std::time::Duration;

fn fixture(root: &std::path::Path) -> Engine {
    let engine = Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Wide",
        [100., 40.],
        [180., 150., 31.],
        [1.; 4],
    ));
    project.layers.push(motion_core::Layer::solid(
        2,
        "Tall",
        [30., 90.],
        [420., 220., 19.],
        [1.; 4],
    ));
    engine
        .new_project(serde_json::to_string(&project).unwrap())
        .unwrap();
    engine
}
fn draw(
    panels: &mut Panels,
    atlas: &mut TextAtlas,
    state: &Value,
    input: &Input,
    floating: bool,
) -> Vec<Action> {
    let mut list = PaintList::default();
    if floating {
        super::super::floating_chrome(
            &mut list,
            atlas,
            Panel::Align,
            [420., 350.],
            panels,
            state,
            input,
        )
    } else {
        super::super::chrome(
            &mut list,
            atlas,
            [420., 350.],
            &dock::Node::dock(Panel::Align.dock()),
            panels,
            state,
            input,
        )
    }
}
fn press(p: [f32; 2]) -> Input {
    Input {
        mouse: p,
        pressed: Some((MouseButton::Left, p)),
        ..Default::default()
    }
}

#[test]
fn alignment_controls_in_main_and_floating_windows_dispatch_one_typed_atomic_request_and_modal_cancel_is_safe(
) {
    let root = tempfile::tempdir().unwrap();
    let engine = fixture(root.path());
    let state = engine.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let styles = PaintList::default().styles();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("row.height", 22.);
    let gap = styles.metric("auxiliary.gap", 6.);
    let control = styles.metric("auxiliary.control.height", 26.);
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Align),
                ..Default::default()
            };
            draw(&mut panels, &mut atlas, &state, &Input::default(), floating);
            panels.selected = Some(2);
            panels.selected_layers = vec![1, 2];
            let area = if floating {
                Rect::new(0., 30., 420., 298.)
            } else {
                super::super::frames(&dock::Node::dock(Panel::Align.dock()), [420., 350.])[0]
                    .content
            };
            let base = area.min[1] + pad;
            let p = [
                area.min[0] + pad + (area.width() - pad * 2. - gap * 2.) / 6.,
                base + (row + gap) * 2. + control * 0.5,
            ];
            let actions = draw(&mut panels, &mut atlas, &state, &press(p), floating);
            assert!(
                matches!(actions.as_slice(),[Action::AlignLayers{objects,edge:Edge::Left,target:Target::Composition}] if objects==&vec![2,1]),
                "{locale:?} floating={floating}: {actions:?}"
            );
            panels.auxiliary.align.value = Target::Selection;
            let actions = draw(&mut panels, &mut atlas, &state, &press(p), floating);
            assert!(matches!(
                actions.as_slice(),
                [Action::AlignLayers {
                    target: Target::Selection,
                    ..
                }]
            ));
            panels.select_only(Some(1));
            assert!(draw(&mut panels, &mut atlas, &state, &press(p), floating).is_empty());
            panels.auxiliary.align.value = Target::Composition;
            let dropdown = [area.max[0] - pad - gap, base + row * 0.5];
            assert!(draw(&mut panels, &mut atlas, &state, &press(dropdown), floating).is_empty());
            assert!(panels.auxiliary.align.target.open);
            assert!(draw(&mut panels, &mut atlas, &state, &press(p), floating).is_empty());
            assert!(!panels.auxiliary.align.target.open);
            let mut locked = state.clone();
            locked["project"]["layers"][0]["locked"] = true.into();
            assert!(draw(&mut panels, &mut atlas, &locked, &press(p), floating).is_empty());
        }
    }
}

#[test]
fn alignment_worker_errors_reach_default_file_and_stderr_and_valid_requests_recover_without_project_loss(
) {
    const CHILD: &str = "MOTION_ALIGNMENT_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let engine = fixture(&root);
        engine
            .seek_async(17.75)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let before = engine.state().unwrap();
        let error = engine
            .align_layers_async(vec![1, 999], Edge::Left, Target::Composition)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap_err();
        assert!(error.contains("align layer 999"));
        let rejected = engine.state().unwrap();
        for field in ["project", "revision", "canUndo", "canRedo", "frame"] {
            assert_eq!(before[field], rejected[field]);
        }
        let after = engine
            .align_layers_async(vec![1, 2], Edge::Left, Target::Composition)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(after["frame"], serde_json::json!(17.0));
        for layer in after["projectedLayers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| [Some(1), Some(2)].contains(&l["id"].as_u64()))
        {
            let left = layer["corners"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p[0].as_f64().unwrap())
                .fold(f64::INFINITY, f64::min);
            assert!(left.abs() < 0.002);
        }
        let undo = engine.history(0).unwrap();
        assert_eq!(before["project"], undo["project"]);
        let redo = engine.history(1).unwrap();
        assert_eq!(after["project"], redo["project"]);
        engine.save().unwrap();
        drop(engine);
        let reopened = Engine::start(
            std::path::PathBuf::from(after["root"].as_str().unwrap()),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        assert_eq!(after["project"], reopened.state().unwrap()["project"]);
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::align::interaction_tests::alignment_worker_errors_reach_default_file_and_stderr_and_valid_requests_recover_without_project_loss"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for channel in [&log, &stderr] {
        assert!(channel.contains("align_layers") && channel.contains("align layer 999"));
    }
    assert!(
        log.lines()
            .filter(|l| l.contains("start #") && l.contains(" align_layers"))
            .count()
            == 2
    );
    assert!(
        !stderr.contains("start #")
            && !stdout.contains("align layer 999")
            && !stdout.contains("start #")
    );
}
