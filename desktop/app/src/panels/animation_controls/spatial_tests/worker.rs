use super::*;
use crate::panels::{self, Panel, Panels};

pub(super) fn render(
    p: &mut Panels,
    s: &Value,
    atlas: &mut TextAtlas,
    floating: bool,
    input: Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            atlas,
            Panel::Curves,
            [1000., 800.],
            p,
            s,
            &input,
        )
    } else {
        panels::chrome(
            &mut paint,
            atlas,
            [1000., 800.],
            &motion_ui::dock::Node::dock(Panel::Curves.dock()),
            p,
            s,
            &input,
        )
    };
    (paint, actions)
}
pub(super) fn send(e: &crate::engine::Engine, actions: Vec<Action>) {
    for a in actions {
        match a {
            Action::Edit(command) => {
                e.edit(serde_json::to_string(&command).unwrap()).unwrap();
            }
            Action::History(op) => {
                e.history(op).unwrap();
            }
            _ => panic!("unexpected action {a:?}"),
        }
    }
}
pub(super) fn point(c: &Controls, t: f64, v: f64) -> [f32; 2] {
    let (_, graph, bounds) = c.mini_test_geometry().unwrap();
    [
        graph.min[0] + bounds.x.fraction(t) as f32 * graph.width(),
        graph.max[1] - bounds.y.fraction(v) as f32 * graph.height(),
    ]
}
#[test]
fn actual_worker_mini_spatial_handles_keep_one_history_cancel_and_saved_project_in_both_chrome_owners(
) {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mode in ["plain", "forward", "reverse"] {
        for locale in [Locale::En, Locale::Zh] {
            for floating in [false, true] {
                let mut core = fixture(mode, false);
                core.apply(Command::SpatialKeyframes {
                    target: target(),
                    edit: SpatialKeyEdit::Curve {
                        key: 2,
                        easing: quadratic(),
                    },
                })
                .unwrap();
                let before = core.snapshot();
                let at = projected(&core, 4.125);
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), &before).unwrap();
                let engine = crate::engine::Engine::start(
                    folder.path().to_owned(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                engine.seek(at).unwrap();
                for cancel in [false, true] {
                    let mut p = Panels {
                        locale,
                        focused: Some(Panel::Curves),
                        ..Default::default()
                    };
                    p.select_only(Some(7));
                    let s = engine.state().unwrap();
                    p.animation_controls = ready(&s);
                    assert!(render(&mut p, &s, &mut atlas, floating, Input::default())
                        .1
                        .is_empty());
                    let press = point(&p.animation_controls, 0.5, 0.25);
                    let actions = render(
                        &mut p,
                        &s,
                        &mut atlas,
                        floating,
                        pointer(press, true, false),
                    )
                    .1;
                    assert!(
                        matches!(actions.first(), Some(Action::History(2))),
                        "{mode} {floating} {actions:?}"
                    );
                    send(&engine, actions);
                    for offset in [15., 30., 45.] {
                        let s = engine.state().unwrap();
                        let actions = render(
                            &mut p,
                            &s,
                            &mut atlas,
                            floating,
                            pointer([press[0] + offset, press[1] - 20.], false, false),
                        )
                        .1;
                        assert!(
                            actions.iter().any(|a| matches!(
                                a,
                                Action::Edit(Command::SpatialKeyframes {
                                    edit: SpatialKeyEdit::Curve { key: 2, .. },
                                    ..
                                })
                            )),
                            "{actions:?}"
                        );
                        send(&engine, actions);
                    }
                    let mut s = engine.state().unwrap();
                    if cancel {
                        s["timeline_layers"][0]["properties"]["position"]["spatial_keys"]
                            .as_array_mut()
                            .unwrap()
                            .remove(1);
                    }
                    let actions = render(
                        &mut p,
                        &s,
                        &mut atlas,
                        floating,
                        pointer([press[0] + 45., press[1] - 20.], false, true),
                    )
                    .1;
                    assert!(
                        matches!(actions.last(),Some(Action::History(op)) if *op==if cancel {4}else{3}),
                        "{actions:?}"
                    );
                    send(&engine, actions);
                    assert!(!p.animation_controls.editing());
                    let after = engine.state().unwrap()["project"].clone();
                    if cancel {
                        assert_eq!(after, json!(before));
                    } else {
                        let mut expected = before.clone();
                        let actual: Project = serde_json::from_value(after.clone()).unwrap();
                        let easing = actual.layers[0]
                            .transform
                            .position
                            .spatial_animation
                            .as_ref()
                            .unwrap()
                            .keys()[1]
                            .easing;
                        assert_ne!(easing, quadratic());
                        expected.layers[0]
                            .transform
                            .position
                            .spatial_animation
                            .as_mut()
                            .unwrap()
                            .set_easing(2, easing)
                            .unwrap();
                        assert_eq!(actual, expected);
                        engine.save().unwrap();
                        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
                        engine.history(0).unwrap();
                        assert_eq!(engine.state().unwrap()["project"], json!(before));
                        engine.history(1).unwrap();
                        assert_eq!(engine.state().unwrap()["project"], after);
                        engine.history(0).unwrap();
                        assert_eq!(engine.state().unwrap()["project"], json!(before));
                    }
                    assert!(
                        render(
                            &mut p,
                            &s,
                            &mut atlas,
                            floating,
                            pointer(press, false, true)
                        )
                        .1
                        .is_empty(),
                        "late release"
                    );
                }
                engine.request_shutdown().unwrap();
            }
        }
    }
}

#[test]
fn malformed_mini_spatial_keys_and_easing_use_bounded_default_error_channels_and_recover() {
    crate::panels::property_authoring::spatial_tests::faults::verify(
        "panels::animation_controls::spatial_tests::worker::malformed_mini_spatial_keys_and_easing_use_bounded_default_error_channels_and_recover",
        "spatial-mini", &["curve.controls.keys","curve.controls.spatial_easing"],|| {
            let e=fixture("plain",false);let healthy=state(&e,4.125);let mut atlas=TextAtlas::from_system(12.).unwrap();
            for _ in 0..180 {
                let mut bad=healthy.clone();bad["timeline_layers"][0]["properties"]["position"]["spatial_keys"][1]["id"]=1.into();
                assert!(draw(&mut ready(&bad),&mut atlas,&bad,&Input::default(),Locale::En).is_empty());
                let mut bad=healthy.clone();bad["timeline_layers"][0]["properties"]["position"]["spatial_keys"][1]["easing"]=json!({"ease":"invalid"});
                assert!(draw(&mut ready(&bad),&mut atlas,&bad,&Input::default(),Locale::En).is_empty());
                assert!(draw(&mut ready(&healthy),&mut atlas,&healthy,&Input::default(),Locale::En).is_empty());
            }
            let mut c=ready(&healthy);let actions=draw(&mut c,&mut atlas,&healthy,&click([230.,182.]),Locale::En);
            assert!(matches!(actions.as_slice(),[Action::Edit(Command::SpatialKeyframes {edit:SpatialKeyEdit::Curve {key:2,..},..})]));
        });
}
