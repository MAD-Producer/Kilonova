//! Actual viewport gestures must retain author identity after Roving retiming.
use super::*;
use crate::engine::Engine;
use motion_core::{layer_time::*, Ease, LayerTimeline, Project, Track};
use serde_json::json;

pub(super) fn base() -> (Project, Rect) {
    let (_folder, engine, state, view) = super::worker_tests::fixture(false);
    let project = serde_json::from_value(state["project"].clone()).unwrap();
    engine.request_shutdown().unwrap();
    (project, view)
}
pub(super) fn mapped(mut p: Project, mode: &str, roving: bool) -> (Project, f64) {
    let layer = &mut p.layers[0];
    let start = layer.transform.position.sample(0.).unwrap();
    let mut track = Track::constant(start);
    for (time, x) in [(0, 0.), (4, 10.), (10, 40.), (20, 80.)] {
        track
            .upsert(time, [start[0] + x, start[1], start[2]], Ease::Linear)
            .unwrap();
    }
    track.enable_roving(&[4]).unwrap();
    if !roving {
        track
            .spatial_animation
            .as_mut()
            .unwrap()
            .set_fixed_time(2, 4.125)
            .unwrap();
    }
    let local = track
        .spatial_animation
        .as_ref()
        .unwrap()
        .prepared()
        .keys()
        .nth(1)
        .unwrap()
        .1;
    layer.transform.position = track;
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: p.frames.try_into().unwrap(),
        offset_frame: 8,
    });
    layer.time_mapping = (mode != "plain").then(|| LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if mode == "reverse" {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if mode == "reverse" { 24. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    let at = layer.property_composition_frame(local).unwrap();
    (p, at)
}
fn start(p: &Project, at: f64) -> (tempfile::TempDir, Engine) {
    let folder = tempfile::tempdir().unwrap();
    motion_core::storage::save(folder.path(), p).unwrap();
    let engine = Engine::start(
        folder.path().to_owned(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine.seek(at).unwrap();
    (folder, engine)
}
fn pointer(at: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: at,
        held: (!release).then_some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, at)),
        released: release.then_some((MouseButton::Left, at)),
        ..Default::default()
    }
}
fn render(c: &mut Controls, s: &Value, view: Rect, input: Input) -> Vec<Action> {
    let mut out = vec![];
    c.draw(
        &mut PaintList::default(),
        view,
        s,
        Some(7),
        &input,
        &mut out,
    );
    out
}
fn trajectory(frame: &Frame, constraint: Constraint) -> (DVec3, DVec3) {
    match constraint {
        Constraint::Axis(i) => (
            axis(i) * frame.lengths[i].unwrap() * 0.6,
            axis(i) * frame.lengths[i].unwrap() * 0.15,
        ),
        Constraint::Plane(n) => {
            let a = (n + 1) % 3;
            let b = (n + 2) % 3;
            (
                axis(a) * frame.lengths[a].unwrap() * 0.3
                    + axis(b) * frame.lengths[b].unwrap() * 0.3,
                axis(a) * frame.lengths[a].unwrap() * 0.15
                    - axis(b) * frame.lengths[b].unwrap() * 0.1,
            )
        }
    }
}

#[test]
fn continuous_viewport_position_gestures_keep_the_key_through_three_worker_previews_save_undo_and_cancel(
) {
    let (base, view) = base();
    for mode in ["plain", "forward", "reverse"] {
        for roving in [false, true] {
            let (before, at) = mapped(base.clone(), mode, roving);
            let (folder, engine) = start(&before, at);
            for constraint in [Constraint::Axis(0), Constraint::Plane(2)] {
                for cancel in [false, true] {
                    let initial = engine.state().unwrap();
                    assert_eq!(initial["project"], json!(before));
                    let pose = Pose::new(&initial, 7, view).unwrap();
                    let styles = PaintList::default().styles();
                    let frame = Frame::new(&pose, Tool::Move, &styles).unwrap();
                    let (origin, delta) = trajectory(&frame, constraint);
                    let from = frame.projection.screen3(origin).unwrap();
                    assert_eq!(
                        pose.handle(from, Tool::Move, &styles),
                        Some(Handle::Spatial(constraint, false)),
                        "{mode} {constraint:?}"
                    );
                    let mut c = Controls::default();
                    assert!(render(&mut c, &initial, view, pointer(from, true, false)).is_empty());
                    assert!(c.drag.as_ref().unwrap().position_key.is_some());
                    let mut expected = before.clone();
                    for step in [1., 2., 3.] {
                        let to = frame.projection.screen3(origin + delta * step).unwrap();
                        assert!(distance(from, to) > styles.metric("motion.drag_threshold", 3.));
                        let actions = render(
                            &mut c,
                            &engine.state().unwrap(),
                            view,
                            pointer(to, false, false),
                        );
                        if step == 1. {
                            assert!(matches!(actions.first(), Some(Action::History(2))));
                        }
                        let values: Vec<_> = actions
                            .iter()
                            .filter_map(|a| match a {
                                Action::EditBatch(commands) => Some(commands),
                                _ => None,
                            })
                            .flatten()
                            .collect();
                        assert_eq!(values.len(), 1, "{actions:?}");
                        let Command::TrackAt {
                            target:
                                motion_core::KeyframeTarget::Property {
                                    object: 7,
                                    property: Property::Position,
                                    axis: None,
                                },
                            frame: actual_frame,
                            edit:
                                motion_core::TrackEdit::SetSpatialKey {
                                    key: 2,
                                    value: motion_core::TrackValue::Vec3(value),
                                },
                        } = values[0]
                        else {
                            panic!("stable ID not used: {values:?}")
                        };
                        assert_eq!(*actual_frame, at);
                        for i in 0..3 {
                            let expected_value = pose.position[i] + (delta[i] * step) as f32;
                            assert!(
                                (value[i] - expected_value).abs() < 0.03,
                                "{mode} {constraint:?} {step}: {value:?} != {expected_value}"
                            );
                        }
                        expected.layers[0]
                            .transform
                            .position
                            .spatial_animation
                            .as_mut()
                            .unwrap()
                            .set_value(2, *value)
                            .unwrap();
                        super::super::tests::apply(&engine, actions);
                        let preview = engine.state().unwrap();
                        assert_eq!(
                            preview["project"],
                            json!(expected),
                            "unrelated authored state changed"
                        );
                        assert_eq!(
                            expected.layers[0]
                                .transform
                                .position
                                .spatial_animation
                                .as_ref()
                                .unwrap()
                                .keys()
                                .len(),
                            4
                        );
                        if roving {
                            assert_ne!(
                                expected.layers[0]
                                    .transform
                                    .position
                                    .spatial_animation
                                    .as_ref()
                                    .unwrap()
                                    .prepared()
                                    .keys()
                                    .nth(1)
                                    .unwrap()
                                    .1,
                                before.layers[0]
                                    .transform
                                    .position
                                    .spatial_animation
                                    .as_ref()
                                    .unwrap()
                                    .prepared()
                                    .keys()
                                    .nth(1)
                                    .unwrap()
                                    .1
                            );
                        }
                    }
                    let end = frame.projection.screen3(origin + delta * 3.).unwrap();
                    let mut s = engine.state().unwrap();
                    if cancel {
                        s["project"]["layers"][0]["transform"]["position"]["spatial_animation"]
                            ["keys"]
                            .as_array_mut()
                            .unwrap()
                            .remove(1);
                    }
                    let actions = render(&mut c, &s, view, pointer(end, false, true));
                    assert!(
                        matches!(actions.as_slice(),[Action::History(op)] if *op==if cancel {4}else{3}),
                        "{actions:?}"
                    );
                    super::super::tests::apply(&engine, actions);
                    assert!(!c.gesture_active());
                    assert!(render(
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
                        assert_eq!(engine.history(0).unwrap()["project"], json!(before));
                    }
                }
            }
            engine.request_shutdown().unwrap();
        }
    }
}

#[test]
fn continuous_viewport_identity_failures_reach_bounded_default_logs_and_recover() {
    crate::panels::property_authoring::spatial_tests::faults::verify(
        "panels::transform_controls::spatial::continuous_position_tests::continuous_viewport_identity_failures_reach_bounded_default_logs_and_recover",
        "spatial-viewport", &["transform.spatial_key_begin","transform.spatial_key_continue","transform.spatial_key_command"],|| {
            let (base,view)=base();let (p,at)=mapped(base,"forward",true);let (_folder,engine)=start(&p,at);
            let s=engine.state().unwrap();let pose=Pose::new(&s,7,view).unwrap();let styles=PaintList::default().styles();
            let frame=Frame::new(&pose,Tool::Move,&styles).unwrap();let (origin,delta)=trajectory(&frame,Constraint::Axis(0));
            let from=frame.projection.screen3(origin).unwrap();let to=frame.projection.screen3(origin+delta).unwrap();
            let mut duplicate=s.clone();duplicate["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"].as_array_mut().unwrap().push(
                s["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"][1].clone());
            let mut deleted=s.clone();deleted["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"].as_array_mut().unwrap().remove(1);
            for _ in 0..180 {
                let mut c=Controls::default();assert!(render(&mut c,&duplicate,view,pointer(from,true,false)).is_empty());assert!(!c.gesture_active());
                let mut c=Controls::default();assert!(render(&mut c,&s,view,pointer(from,true,false)).is_empty());
                assert!(matches!(render(&mut c,&s,view,pointer(to,false,false)).first(),Some(Action::History(2))));
                assert!(matches!(render(&mut c,&duplicate,view,pointer(to,false,false)).as_slice(),[Action::History(4)]));assert!(!c.gesture_active());
                // Independent command assembly boundary: normal renderer checks
                // the key before calling this, so no malformed view is installed.
                let mut d=Drag::at_time(Seed::new(from,&s,view),pose.clone(),Handle::Spatial(Constraint::Axis(0),false),&s).unwrap();
                let old=d.last_value;let mut value=old;value[0]+=1.;
                assert!(d.vector(Property::Position,value,&deleted).is_empty());assert_eq!(d.last_value,old);
                let actions=d.vector(Property::Position,value,&s);assert!(matches!(actions.as_slice(),[Action::EditBatch(commands)] if matches!(commands.as_slice(),[Command::TrackAt {edit:motion_core::TrackEdit::SetSpatialKey {key:2,..},..}])));
                assert_eq!(engine.state().unwrap()["project"],s["project"]);
            }
            engine.request_shutdown().unwrap();
        });
}
