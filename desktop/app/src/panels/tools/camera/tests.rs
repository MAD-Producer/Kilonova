use super::super::tests::{host, key, route};
use super::*;
fn apply(e: &crate::engine::Engine, out: Vec<Action>) -> Value {
    let mut state = e.state().unwrap();
    for action in out {
        let op=match action {
            Action::CameraGesture(op)=>op,
            // The Engine-only native fixture explicitly binds the global host
            // observer to cell zero; production Shell consumes this locally.
            Action::ViewerCamera{cell:0,action,..}=>action,
            _=>panic!("unexpected document action"),
        };
        state = e.camera_gesture(op).unwrap();
    }
    state
}
fn pointer(position: [f32; 2], phase: u8) -> Input {
    Input {
        mouse: position,
        pressed: (phase == 0).then_some((MouseButton::Left, position)),
        released: (phase == 2).then_some((MouseButton::Left, position)),
        held: (phase < 2).then_some(MouseButton::Left),
        ..Default::default()
    }
}
#[test]
fn host_observation_transactions_are_absolute_cancellable_and_separate_from_project_history() {
    let (_root, e, _) = host();
    let before = e.view(1).unwrap();
    let baseline = before["observationCamera"].clone();
    for mode in [CameraMode::Orbit, CameraMode::Pan, CameraMode::Dolly] {
        e.camera_gesture(Op::Begin { id: 1 }).unwrap();
        let changed = e
            .camera_gesture(Op::Update {
                id: 1,
                mode,
                delta: [120., -80.],
                viewport: [1920, 1080],
            })
            .unwrap();
        assert_ne!(changed["observationCamera"], baseline);
        let repeated = e
            .camera_gesture(Op::Update {
                id: 1,
                mode,
                delta: [120., -80.],
                viewport: [1920, 1080],
            })
            .unwrap();
        assert_eq!(
            repeated["observationCamera"], changed["observationCamera"],
            "absolute update cannot accumulate drift"
        );
        assert_eq!(changed["project"], before["project"]);
        assert_eq!(changed["sampledCamera"], before["sampledCamera"]);
        assert_eq!(changed["revision"], before["revision"]);
        assert_eq!(changed["history"], before["history"]);
        assert_eq!(
            e.camera_gesture(Op::End {
                id: 1,
                commit: false
            })
            .unwrap()["observationCamera"],
            baseline
        );
        assert_eq!(e.state().unwrap()["canUndoObservation"], false);
        e.camera_gesture(Op::Begin { id: 2 }).unwrap();
        e.camera_gesture(Op::Update {
            id: 2,
            mode,
            delta: [120., -80.],
            viewport: [1920, 1080],
        })
        .unwrap();
        let committed = e
            .camera_gesture(Op::End {
                id: 2,
                commit: true,
            })
            .unwrap();
        assert_eq!(committed["canUndoObservation"], true);
        assert_eq!(
            e.camera_gesture(Op::Undo).unwrap()["observationCamera"],
            baseline
        );
        assert_eq!(
            e.camera_gesture(Op::Redo).unwrap()["observationCamera"],
            committed["observationCamera"]
        );
        assert_eq!(
            e.camera_gesture(Op::Undo).unwrap()["observationCamera"],
            baseline
        );
    }
    e.save().unwrap();
    assert_eq!(e.state().unwrap()["project"], before["project"]);
}
#[test]
fn camera_cycles_respect_view_capability_and_preserve_ime_and_document_shortcuts() {
    let (_root, e, _) = host();
    for (kind, tools) in [
        (
            1,
            vec![
                Active::Camera,
                Active::CameraPan,
                Active::CameraDolly,
                Active::Camera,
            ],
        ),
        (
            2,
            vec![Active::CameraPan, Active::CameraDolly, Active::CameraPan],
        ),
        (
            3,
            vec![Active::CameraPan, Active::CameraDolly, Active::CameraPan],
        ),
    ] {
        let state = e.view(kind).unwrap();
        let mut p = Panels {
            focused: Some(motion_ui::dock::Panel::Composition),
            ..Default::default()
        };
        for tool in tools {
            assert!(route(&mut p, &state, key(Key::C)).is_empty());
            assert_eq!(active(&p), tool);
            assert!(
                route(&mut p, &state, key(Key::Right)).is_empty(),
                "observation tools cannot nudge authored layers"
            );
            assert!(hint(tool, &p).contains("C"));
            let mut ime = key(Key::C);
            ime.composing = true;
            assert!(route(&mut p, &state, ime).is_empty());
            assert_eq!(active(&p), tool);
        }
        let mut undo = key(Key::Z);
        undo.keys[0].1.control = true;
        let actions = route(&mut p, &state, undo);
        assert!(matches!(
            actions.as_slice(),
            [Action::CameraGesture(Op::Undo)]
        ));
        let mut save = key(Key::S);
        save.keys[0].1.control = true;
        assert!(matches!(
            route(&mut p, &state, save).as_slice(),
            [Action::Save]
        ));
        choose(&mut p, Active::Select, &state, &mut vec![]);
        let mut undo = key(Key::Z);
        undo.keys[0].1.control = true;
        assert!(matches!(
            route(&mut p, &state, undo).as_slice(),
            [Action::History(0)]
        ));
    }
}
#[test]
fn scaled_input_release_and_cancel_restore_the_whole_observation_gesture() {
    let (_root, e, _) = host();
    let s = e.view(1).unwrap();
    let before = s["observationCamera"].clone();
    for tool in [Active::Camera, Active::CameraPan, Active::CameraDolly] {
        let mut expected = None;
        for zoom in [0.5, 1., 2.] {
            let mut p = Panels {
                focused: Some(motion_ui::dock::Panel::Composition),
                ..Default::default()
            };
            choose(&mut p, tool, &s, &mut vec![]);
            let view = Rect::new(0., 0., 1920., 1080.);
            p.composition
                .navigation
                .zoom_at(view, [1920., 1080.], [960., 540.], zoom);
            let mapping = p
                .composition
                .navigation
                .mapping(view, [1920., 1080.])
                .unwrap();
            let mut out = vec![];
            for i in [
                pointer([800., 500.], 0),
                pointer([800. + 120. * zoom as f32, 500. - 80. * zoom as f32], 1),
            ] {
                viewport(&mut p, view, mapping, &s, &i, &mut out);
            }
            let changed = apply(&e, out);
            assert_ne!(changed["observationCamera"], before);
            if let Some(expected) = &expected {
                assert_eq!(&changed["observationCamera"], expected);
            } else {
                expected = Some(changed["observationCamera"].clone());
            }
            let mut out = vec![];
            viewport(
                &mut p,
                view,
                mapping,
                &s,
                &Input {
                    focus_lost: true,
                    ..Default::default()
                },
                &mut out,
            );
            assert_eq!(apply(&e, out)["observationCamera"], before);
            assert!(!p.tools.camera.editing());
        }
    }
}
#[test]
fn invalid_and_stale_gestures_do_not_change_published_view_or_project() {
    let (_root, e, _) = host();
    let before = e.view(1).unwrap();
    e.camera_gesture(Op::Begin { id: 9 }).unwrap();
    for op in [
        Op::Begin { id: 10 },
        Op::Update {
            id: 10,
            mode: CameraMode::Pan,
            delta: [1., 2.],
            viewport: [1920, 1080],
        },
        Op::Update {
            id: 9,
            mode: CameraMode::Pan,
            delta: [f32::NAN, 2.],
            viewport: [1920, 1080],
        },
        Op::Update {
            id: 9,
            mode: CameraMode::Pan,
            delta: [1., 2.],
            viewport: [0, 1080],
        },
    ] {
        assert!(e.camera_gesture(op).is_err());
        let state = e.state().unwrap();
        assert_eq!(state["observationCamera"], before["observationCamera"]);
        assert_eq!(state["project"], before["project"]);
    }
    e.camera_gesture(Op::End {
        id: 9,
        commit: false,
    })
    .unwrap();
    let top = e.view(2).unwrap();
    e.camera_gesture(Op::Begin { id: 11 }).unwrap();
    assert!(e
        .camera_gesture(Op::Update {
            id: 11,
            mode: CameraMode::Orbit,
            delta: [1., 2.],
            viewport: [1920, 1080]
        })
        .is_err());
    assert_eq!(
        e.state().unwrap()["observationCamera"],
        top["observationCamera"]
    );
    e.view(3).unwrap();
    assert!(e
        .camera_gesture(Op::End {
            id: 11,
            commit: false
        })
        .is_ok());
    assert!(e
        .camera_gesture(Op::Update {
            id: 11,
            mode: CameraMode::Pan,
            delta: [1., 2.],
            viewport: [1920, 1080]
        })
        .is_err());
}

#[cfg(target_os = "windows")]
mod native;

#[test]
fn observer_history_is_bounded_and_cleared_by_view_or_project_context_changes() {
    let (_root, e, _) = host();
    e.view(1).unwrap();
    for id in 1..=70 {
        e.camera_gesture(Op::Begin { id }).unwrap();
        e.camera_gesture(Op::Update {
            id,
            mode: CameraMode::Orbit,
            delta: [1., 0.],
            viewport: [1920, 1080],
        })
        .unwrap();
        e.camera_gesture(Op::End { id, commit: true }).unwrap();
    }
    for _ in 0..64 {
        assert_eq!(e.state().unwrap()["canUndoObservation"], true);
        e.camera_gesture(Op::Undo).unwrap();
    }
    let before = e.state().unwrap();
    assert_eq!(before["canUndoObservation"], false);
    assert_eq!(
        e.camera_gesture(Op::Undo).unwrap()["observationCamera"],
        before["observationCamera"]
    );
    assert_eq!(
        e.camera_gesture(Op::Redo).unwrap()["canUndoObservation"],
        true
    );
    e.seek(1.).unwrap();
    assert_eq!(e.state().unwrap()["canUndoObservation"], false);
    assert_eq!(
        e.camera_gesture(Op::Undo).unwrap()["canRedoObservation"],
        false
    );
    e.view(2).unwrap();
    assert_eq!(e.state().unwrap()["canUndoObservation"], false);
}
#[test]
fn rejected_camera_command_preserves_default_error_sinks_and_original_context() {
    const CHILD: &str = "MOTION_CAMERA_ERROR_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let result=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::tools::camera::tests::rejected_camera_command_preserves_default_error_sinks_and_original_context","--nocapture"]).env(CHILD,temp.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let file = std::fs::read_to_string(temp.path().join("motion-studio.log")).unwrap();
        let err = String::from_utf8_lossy(&result.stderr);
        let out = String::from_utf8_lossy(&result.stdout);
        for text in [&file, &*err] {
            assert!(text.contains("phase=update token=77 mode=Pan"));
            assert!(text.contains("invalid observation gesture coordinates"));
            assert!(text.contains("camera_gesture"));
            assert!(text.contains("error #"));
        }
        assert!(file.contains("start #"));
        assert!(!out.contains("camera_gesture"));
        assert!(!out.contains("invalid observation"));
        return;
    };
    let _logger = crate::logging::init(std::path::PathBuf::from(root)).unwrap();
    let (_root, e, _) = host();
    e.view(1).unwrap();
    e.camera_gesture(Op::Begin { id: 77 }).unwrap();
    assert!(e
        .camera_gesture(Op::Update {
            id: 77,
            mode: CameraMode::Pan,
            delta: [f32::NAN, 0.],
            viewport: [1920, 1080]
        })
        .is_err());
    e.camera_gesture(Op::End {
        id: 77,
        commit: false,
    })
    .unwrap();
}
