//! Free translation crosses the UI, worker and host rather than a local mock.
use super::continuous_position_tests::{base, mapped};
use super::*;
use crate::engine::Engine;
use motion_core::{Project, Track};
use serde_json::json;

fn pointer(point: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: point,
        pressed: press.then_some((MouseButton::Left, point)),
        held: (!release).then_some(MouseButton::Left),
        released: release.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(c: &mut Controls, state: &Value, view: Rect, input: Input) -> Vec<Action> {
    let mut out = vec![];
    c.draw(
        &mut PaintList::default(),
        view,
        state,
        Some(7),
        &input,
        &mut out,
    );
    out
}
pub(super) fn key_screen(p: &Project, at: f64) -> [f32; 2] {
    let mut p = p.clone();
    let value = p.layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .keys()[1]
        .value;
    p.layers[0].transform.position = Track::constant(value);
    let mut scene = motion_render::Scene::new(&p).unwrap();
    scene
        .sample(&p, at, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let point = scene.project_node(7).unwrap();
    [point[0], point[1]]
}

#[test]
fn free_position_keeps_one_key_and_screen_distance_across_clock_dimension_and_cancel() {
    let (base, view) = base();
    for three_d in [false, true] {
        for mode in ["plain", "forward", "reverse"] {
            for roving in [false, true] {
                let (mut before, at) = mapped(base.clone(), mode, roving);
                before.layers[0].three_d = three_d;
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), &before).unwrap();
                let engine = Engine::start(
                    folder.path().into(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                engine.seek(at).unwrap();
                for cancel in [false, true] {
                    let initial = engine.state().unwrap();
                    assert_eq!(initial["project"], json!(before));
                    let pose = Pose::new(&initial, 7, view).unwrap();
                    let from = pose.pivot;
                    assert_eq!(
                        pose.handle(from, Tool::Move, &PaintList::default().styles()),
                        None
                    );
                    let mut c = Controls::default();
                    assert!(draw(&mut c, &initial, view, pointer(from, true, false)).is_empty());
                    let origin = key_screen(&before, at);
                    let mut expected = before.clone();
                    for step in [1., 2., 3.] {
                        let point = [from[0] + 24. * step, from[1] - 12. * step];
                        let actions = draw(
                            &mut c,
                            &engine.state().unwrap(),
                            view,
                            pointer(point, false, false),
                        );
                        assert!(
                            actions.iter().any(|a| matches!(
                                a,
                                Action::PreviewDrag {
                                    object: 7,
                                    spatial_key: Some(2),
                                    ..
                                }
                            )),
                            "{actions:?}"
                        );
                        super::super::tests::apply(&engine, actions);
                        let actual: Project =
                            serde_json::from_value(engine.state().unwrap()["project"].clone())
                                .unwrap();
                        let value = actual.layers[0]
                            .transform
                            .position
                            .spatial_animation
                            .as_ref()
                            .unwrap()
                            .keys()[1]
                            .value;
                        expected.layers[0]
                            .transform
                            .position
                            .spatial_animation
                            .as_mut()
                            .unwrap()
                            .set_value(2, value)
                            .unwrap();
                        assert_eq!(actual, expected, "unrelated authored state changed");
                        let screen = key_screen(&actual, at);
                        for axis in 0..2 {
                            let delta = [24., -12.][axis] * step;
                            assert!((screen[axis]-origin[axis]-delta).abs()<0.03,"3d={three_d} {mode} roving={roving} step={step}: {screen:?} origin={origin:?}");
                        }
                        assert_eq!(
                            actual.layers[0]
                                .transform
                                .position
                                .spatial_animation
                                .as_ref()
                                .unwrap()
                                .keys()
                                .iter()
                                .map(|k| k.id)
                                .collect::<Vec<_>>(),
                            vec![1, 2, 3, 4]
                        );
                    }
                    let end = [from[0] + 72., from[1] - 36.];
                    let mut state = engine.state().unwrap();
                    if cancel {
                        state["project"]["layers"][0]["transform"]["position"]["spatial_animation"]
                            ["keys"]
                            .as_array_mut()
                            .unwrap()
                            .remove(1);
                    }
                    let actions = draw(&mut c, &state, view, pointer(end, false, true));
                    assert!(
                        matches!(actions.as_slice(),[Action::History(op)] if *op==if cancel {4}else{3}),
                        "{actions:?}"
                    );
                    super::super::tests::apply(&engine, actions);
                    assert!(!c.gesture_active());
                    assert!(draw(
                        &mut c,
                        &engine.state().unwrap(),
                        view,
                        pointer(end, false, true)
                    )
                    .is_empty());
                    if cancel {
                        assert_eq!(engine.state().unwrap()["project"], json!(before));
                    } else {
                        engine.save().unwrap();
                        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
                        assert_eq!(engine.history(0).unwrap()["project"], json!(before));
                        assert_eq!(engine.history(1).unwrap()["project"], json!(expected));
                        engine.history(0).unwrap();
                    }
                }
                engine.request_shutdown().unwrap();
            }
        }
    }
}

#[test]
fn free_mapped_discrete_position_writes_author_time_instead_of_composition_frame() {
    let (base, _) = base();
    for mode in ["forward", "reverse"] {
        for separated in [false, true] {
            let (mut p, _) = mapped(base.clone(), mode, false);
            p.layers[0].three_d = false;
            p.layers[0].parent = None;
            let mut track = Track::constant([10., 20., 0.]);
            track
                .upsert(0, [10., 20., 0.], motion_core::Ease::Linear)
                .unwrap();
            track
                .upsert(20, [50., 60., 0.], motion_core::Ease::Linear)
                .unwrap();
            if separated {
                track.separate().unwrap();
            }
            p.layers[0].transform.position = track;
            let at = p.layers[0].property_composition_frame(4.).unwrap();
            let folder = tempfile::tempdir().unwrap();
            motion_core::storage::save(folder.path(), &p).unwrap();
            let engine = Engine::start(
                folder.path().into(),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            engine.seek(at).unwrap();
            let before = engine.state().unwrap();
            engine.history(2).unwrap();
            let after = engine
                .drag_at_key_async(7, [24., -12.], [1920, 1080], None)
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
                .unwrap();
            let actual: Project = serde_json::from_value(after["project"].clone()).unwrap();
            let mut expected = p.clone();
            expected.layers[0].transform.position = actual.layers[0].transform.position.clone();
            assert_eq!(actual, expected);
            let position = &actual.layers[0].transform.position;
            if separated {
                for axis in [
                    &position.axes.as_ref().unwrap().x,
                    &position.axes.as_ref().unwrap().y,
                ] {
                    assert!(axis.keys.iter().any(|k| k.frame == 4));
                    assert!(!axis.keys.iter().any(|k| k.frame == at as i32));
                }
            } else {
                assert!(position.keys.iter().any(|k| k.frame == 4));
                assert!(!position.keys.iter().any(|k| k.frame == at as i32));
            }
            assert_eq!(position.sample(4.).unwrap(), [42., 16., 0.]);
            engine.history(3).unwrap();
            assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
            engine.request_shutdown().unwrap();
        }
    }
}
