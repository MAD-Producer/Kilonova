use super::*;
use motion_core::{EffectInstance, Engine, Layer, Project};
use motion_ui::input::Modifiers;

fn fixture() -> (Engine, Value) {
    let graph = motion_graph::builtin_examples()
        .iter()
        .find(|e| e.id == "soft_sphere_motion")
        .unwrap()
        .load()
        .unwrap();
    let compiled = motion_graph::compile(&graph, &Default::default()).unwrap();
    let bytes = motion_graph::compiled_package_bytes(&compiled.manifest, &compiled.source).unwrap();
    let package = motion_effects::EffectPackage::from_bytes(bytes).unwrap();
    let mut project = Project::new(640, 360, 30, 180).unwrap();
    let mut layer = Layer::solid(7, "Effect tracks", [100.; 2], [100., 100., 0.], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 10,
        out_frame: 180,
        offset_frame: 10,
    });
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [100.; 2],
    );
    let speed = effect.params.get_mut("speed").unwrap();
    speed
        .track
        .upsert(10, [0.25, 0., 0., 0.], Ease::Linear)
        .unwrap();
    speed
        .track
        .upsert(70, [1., 0., 0., 0.], Ease::Linear)
        .unwrap();
    layer.effects.push(effect);
    project.layers.push(layer);
    project.layers.push(Layer::solid(
        8,
        "Following layer",
        [100.; 2],
        [100., 100., 0.],
        [1.; 4],
    ));
    project.rebuild_plugin_dependencies();
    let catalogue =
        serde_json::json!({"packages":[{"hash":package.hash,"manifest":package.manifest}]});
    (Engine::new(project).unwrap(), catalogue)
}
fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"effect-fixture","composition":motion_core::composition::MAIN_COMPOSITION,"revision":engine.revision(),"frame":20,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(20.).unwrap()})
}
fn setup(catalogue: &Value, locale: Locale) -> Panels {
    let mut p = Panels {
        locale,
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    p.select_only(Some(7));
    p.effect_controls.catalogue = catalogue.clone().into();
    p.timeline.expanded.insert(7);
    p.timeline.collapsed_transforms.insert(7);
    p.timeline.expanded_effects.insert(7);
    p
}
fn draw(
    p: &mut Panels,
    s: &Value,
    input: Input,
    floating: bool,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut list = PaintList::default();
    if floating {
        super::super::floating_chrome(
            &mut list,
            atlas,
            Panel::Timeline,
            [1200., 850.],
            p,
            s,
            &input,
        )
    } else {
        super::super::chrome(
            &mut list,
            atlas,
            [1200., 850.],
            &dock::Node::dock(Panel::Timeline.dock()),
            p,
            s,
            &input,
        )
    }
}
fn point(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
fn run(e: &mut Engine, actions: Vec<Action>) {
    for a in actions {
        match a {
            Action::Edit(c) => e.apply(c).unwrap(),
            Action::EditBatch(cs) => {
                e.apply_batch(cs).unwrap();
            }
            Action::History(2) => e.begin_gesture().unwrap(),
            Action::History(3) => e.end_gesture(true).unwrap(),
            Action::History(4) => e.end_gesture(false).unwrap(),
            Action::Seek(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }
}
#[test]
fn node_effect_rows_reserve_every_parameter_and_expansion_invalidates_scroll_extent() {
    let (e, catalogue) = fixture();
    let s = state(&e);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut p = setup(&catalogue, locale);
            assert!(draw(&mut p, &s, Input::default(), floating, &mut atlas).is_empty());
            let rows = timeline::rows(&s);
            let row = rows.iter().find(|r| r.id == 7).unwrap();
            let expanded = timeline_layout::LayerRows::new(row, &p.timeline, 20., locale);
            assert_eq!(
                expanded.effect_rows.len(),
                1 + e.project().layers[0].effects[0].params.len()
            );
            let caption = p.effect_timeline.rects[&(1, "speed".into(), -1)];
            let field = p.effect_timeline.rects[&(1, "speed".into(), -10)];
            assert_eq!(caption.min[1], field.min[1]);
            assert!(caption.max[0] <= field.min[0]);
            p.timeline.collapsed_effect_items.insert((7, 1));
            let collapsed = timeline_layout::LayerRows::new(row, &p.timeline, 20., locale);
            assert_eq!(collapsed.effect_rows.len(), 1);
            assert_eq!(
                expanded.len() - collapsed.len(),
                e.project().layers[0].effects[0].params.len()
            );
            let old = p.timeline.scroll_extent_cache.unwrap().0;
            assert!(draw(&mut p, &s, Input::default(), floating, &mut atlas).is_empty());
            assert_ne!(p.timeline.scroll_extent_cache.unwrap().0, old);
        }
    }
}
#[test]
fn real_node_parameter_numeric_and_timing_drags_each_undo_once_in_both_hosts() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (mut e, catalogue) = fixture();
            let before = e.project().clone();
            let mut p = setup(&catalogue, locale);
            draw(&mut p, &state(&e), Input::default(), floating, &mut atlas);
            let start = point(p.effect_timeline.rects[&(1, "speed".into(), -10)]);
            run(
                &mut e,
                draw(
                    &mut p,
                    &state(&Engine::new(before.clone()).unwrap()),
                    press(start),
                    floating,
                    &mut atlas,
                ),
            );
            let end = [start[0] + 30., start[1]];
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: end,
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: end,
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            assert_ne!(e.project().layers[0].effects, before.layers[0].effects);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            draw(&mut p, &state(&e), Input::default(), floating, &mut atlas);
            let start = point(p.effect_timeline.rects[&(1, "speed".into(), 20)]);
            let track = p.timeline.zoom_track.unwrap();
            let ppf = track.width() / 180.;
            let s = state(&e);
            run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
            let end = [start[0] + 15. * ppf, start[1]];
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: [9999.; 2],
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            let keys = &e.project().layers[0].effects[0].params["speed"].track.keys;
            assert_eq!(
                keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                vec![25, 70]
            );
            assert_eq!(
                keys[0].value,
                before.layers[0].effects[0].params["speed"].track.keys[0].value
            );
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}
#[test]
fn effect_key_f9_and_context_menu_edit_both_sides_without_touching_transform() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (mut e, catalogue) = fixture();
            let before = e.project().clone();
            let mut p = setup(&catalogue, locale);
            draw(&mut p, &state(&e), Input::default(), floating, &mut atlas);
            let start = point(p.effect_timeline.rects[&(1, "speed".into(), 20)]);
            let s = state(&e);
            run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: start,
                        released: Some((MouseButton::Left, start)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            let s = state(&e);
            let actions = draw(
                &mut p,
                &s,
                Input {
                    keys: vec![(Key::F9, Modifiers::default())],
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(
                actions.iter().any(|a| matches!(a, Action::EditBatch(_))),
                "{actions:?}"
            );
            run(&mut e, actions);
            assert!(
                e.project().layers[0].effects[0].params["speed"].track.keys[0]
                    .curve
                    .is_some()
            );
            assert_eq!(e.project().layers[0].transform, before.layers[0].transform);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            let s = state(&e);
            assert!(draw(
                &mut p,
                &s,
                Input {
                    mouse: start,
                    pressed: Some((MouseButton::Right, start)),
                    ..Default::default()
                },
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(p.effect_timeline.menu.is_some());
            let menu = p.effect_timeline.menu.as_ref().unwrap();
            let click = [menu.point[0] + 15., menu.point[1] + 10.];
            let actions = draw(&mut p, &s, press(click), floating, &mut atlas);
            assert!(
                actions.iter().any(|a| matches!(a, Action::EditBatch(_))),
                "{actions:?}"
            );
            run(&mut e, actions);
            assert!(
                e.project().layers[0].effects[0].params["speed"].track.keys[0]
                    .curve
                    .is_some()
            );
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}
#[test]
fn effect_graph_edits_actual_values_and_handles_and_each_gesture_restores_on_undo() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let (mut e, catalogue) = fixture();
            let before = e.project().clone();
            let mut p = setup(&catalogue, locale);
            let s = state(&e);
            choose(&mut p, &s, &Target::new(&s, 7, 1, "speed"), Some(20));
            p.timeline.set_graph_mode(true);
            draw(&mut p, &s, Input::default(), floating, &mut atlas);
            let start = point(p.effect_timeline.rects[&(1, "speed".into(), 20)]);
            let ppf = p.timeline.zoom_track.unwrap().width() / 180.;
            run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
            let end = [start[0] + 15. * ppf, start[1] - 12.];
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: end,
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: [9999.; 2],
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            let key = &e.project().layers[0].effects[0].params["speed"].track.keys[0];
            assert_eq!(key.frame, 25);
            assert!(key.value[0] > 0.25);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            let s = state(&e);
            choose(&mut p, &s, &Target::new(&s, 7, 1, "speed"), Some(20));
            draw(&mut p, &s, Input::default(), floating, &mut atlas);
            let start = point(p.effect_timeline.rects[&(1, "speed".into(), -100)]);
            run(&mut e, draw(&mut p, &s, press(start), floating, &mut atlas));
            let end = [start[0] + 20., start[1] - 15.];
            let s = state(&e);
            run(
                &mut e,
                draw(
                    &mut p,
                    &s,
                    Input {
                        mouse: [9999.; 2],
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    },
                    floating,
                    &mut atlas,
                ),
            );
            assert!(
                e.project().layers[0].effects[0].params["speed"].track.keys[0]
                    .curve
                    .is_some()
            );
            assert_eq!(e.project().layers[0].transform, before.layers[0].transform);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}
#[test]
fn stale_and_readonly_effect_targets_cancel_once_and_do_not_recover_command_ownership() {
    let (mut e, catalogue) = fixture();
    let before = e.project().clone();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for case in 0..7 {
        let mut p = setup(&catalogue, Locale::Zh);
        let s = state(&e);
        let target = Target::new(&s, 7, 1, "speed");
        choose(&mut p, &s, &target, Some(20));
        let input = press([500., 400.]);
        begin_key(
            &mut p,
            &s,
            target.clone(),
            20,
            [500., 400.],
            &input,
            5.,
            0.,
            0.,
            0,
            &mut vec![],
        );
        let mut out = vec![];
        update(
            &mut p,
            &s,
            &Input {
                mouse: [550., 400.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            5.,
            0.,
            &mut out,
        );
        run(&mut e, out);
        let mut changed = state(&e);
        let mut input = Input::default();
        match case {
            0 => changed["root"] = "other".into(),
            1 => changed["composition"] = "other".into(),
            2 => changed["project"]["layers"][0]["effects"] = serde_json::json!([]),
            3 => changed["project"]["layers"][0]["locked"] = true.into(),
            4 => changed["observing"] = true.into(),
            5 => input.focus_lost = true,
            _ => input.keys = vec![(Key::Escape, Default::default())],
        }
        let mut out = vec![];
        prepare(&mut p, &changed, &input, &mut out);
        assert_eq!(
            out.iter()
                .filter(|a| matches!(a, Action::History(4)))
                .count(),
            1
        );
        run(&mut e, out);
        assert_eq!(e.project(), &before);
        let mut out = vec![];
        prepare(&mut p, &changed, &input, &mut out);
        assert!(out.is_empty());
        assert!(p.effect_timeline.drag.is_none());
    }
}
#[test]
fn malformed_effect_rows_emit_bounded_default_file_and_error_stderr_without_stdout() {
    const CHILD: &str = "MOTION_TIMELINE_EFFECT_OBSERVATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (mut engine, catalogue) = fixture();
        let mut s = state(&engine);
        let target = Target::new(&s, 7, 1, "speed");
        let healthy = s["project"]["layers"][0]["effects"][0]["params"]["speed"].clone();
        assert!(target.parameter(&s).is_some());
        assert!(std::fs::read_to_string(root.join("motion-studio.log"))
            .unwrap()
            .is_empty());
        for _ in 0..120 {
            s["project"]["layers"][0]["effects"][0]["params"]["speed"] = 17.into();
            assert!(target.parameter(&s).is_none());
            s["project"]["layers"][0]["effects"][0]["params"]["speed"] = healthy.clone();
            assert!(target.parameter(&s).is_some());
        }
        // Existing panel observations count failures per fixed site for the
        // process lifetime. Healthy siblings do not erase malformed-row evidence.
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = text
            .lines()
            .filter(|l| l.contains("site=timeline.effect_parameter "))
            .collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
        assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
        assert!(lines
            .iter()
            .all(|l| l.contains("object=7 effect=1 parameter=speed") && l.contains("reason=")));
        s["project"]["layers"][0]["effects"] = serde_json::json!([]);
        assert!(target.parameter(&s).is_none());
        assert_eq!(
            std::fs::read_to_string(root.join("motion-studio.log")).unwrap(),
            text
        );
        // Drive the actual handle validation with an invalid input coordinate.
        // It must retain the cause and never send an invalid edit to the engine.
        let s = state(&engine);
        let before = engine.project().clone();
        let mut panels = setup(&catalogue, Locale::Zh);
        choose(&mut panels, &s, &target, Some(20));
        panels.effect_timeline.handle = Some(HandleDrag {
            target: target.clone(),
            frame: 20,
            index: 0,
            easing: Easing {
                ease: Ease::Linear,
                curve: Some(motion_core::Curve {
                    space: motion_core::CurveSpace::Progress,
                    shape: motion_core::CurveShape::Cubic {
                        control1: [1. / 3., 1. / 3.],
                        control2: [2. / 3., 2. / 3.],
                        start: 0.,
                        end: 1.,
                    },
                }),
            },
            start: 20.,
            duration: 60.,
            a: 0.25,
            b: 1.,
            point: [100., 100.],
            plot: Rect::new(0., 0., 600., 400.),
            bounds: [0., 2.],
            ppf: 5.,
            offset: 0.,
            begun: false,
        });
        for _ in 0..120 {
            let mut out = vec![];
            graph::update_handle(
                &mut panels,
                &s,
                &Input {
                    mouse: [200., f32::NAN],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                5.,
                0.,
                false,
                30.,
                &mut out,
            );
            assert!(!out
                .iter()
                .any(|a| matches!(a, Action::Edit(_) | Action::EditBatch(_))));
            run(&mut engine, out);
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = text
            .lines()
            .filter(|l| l.contains("site=timeline.effect_curve "))
            .collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
        assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
        assert!(lines
            .iter()
            .all(|l| l.contains("object=7 effect=1 parameter=speed frame=20")
                && l.contains("reason=")));
        let mut out = vec![];
        prepare(
            &mut panels,
            &s,
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            &mut out,
        );
        assert_eq!(out.len(), 1);
        run(&mut engine, out);
        assert_eq!(engine.project(), &before);
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let result=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::effect_timeline::tests::malformed_effect_rows_emit_bounded_default_file_and_error_stderr_without_stdout"])
        .env_remove("MOTION_LOG").env(CHILD,folder.path()).output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("persistent panel degradation site=timeline.effect_parameter"));
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("persistent panel degradation site=timeline.effect_curve"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("panel degradation"));
}

#[test]
fn effect_speed_uses_the_same_clamp_as_the_value_sampler() {
    let (engine, _) = fixture();
    let mut parameter = engine.project().layers[0].effects[0].params["speed"].clone();
    parameter.min = 0.;
    parameter.max = 1.;
    parameter.track.keys[0].value[0] = 0.;
    parameter.track.keys[1].value[0] = 1.;
    assert!((graph::speed_value(&parameter, 40., 30., 0) - 0.5).abs() < 0.0001);
    parameter.track.keys[0].curve = Some(motion_core::Curve {
        space: motion_core::CurveSpace::Progress,
        shape: motion_core::CurveShape::Cubic {
            control1: [1. / 3., 4.],
            control2: [2. / 3., 4.],
            start: 0.,
            end: 1.,
        },
    });
    assert!(parameter.track.sample(40.)[0] > parameter.max);
    assert_eq!(parameter.sample(40.)[0], 1.);
    assert!(graph_sampling::velocity(&parameter.track, 40., 30., |v| v[0] as f64).abs() > 0.1);
    assert_eq!(graph::speed_value(&parameter, 40., 30., 0), 0.);
    assert_eq!(graph::speed_value(&parameter, 5., 30., 0), 0.);
    parameter.kind = motion_effects::ParamKind::Bool;
    assert_eq!(graph::speed_value(&parameter, 20., 30., 0), 0.);
}

#[test]
fn effect_move_and_copy_preserve_occupied_keys_release_position_and_roundtrip_history() {
    for copy in [false, true] {
        let (mut engine, catalogue) = fixture();
        let original = engine.project().clone();
        let mut panels = setup(&catalogue, Locale::Zh);
        let source = state(&engine);
        let target = Target::new(&source, 7, 1, "speed");
        let mut input = press([100., 100.]);
        input.modifiers.control = copy;
        let mut out = vec![];
        begin_key(
            &mut panels,
            &source,
            target,
            20,
            [100., 100.],
            &input,
            5.,
            0.,
            0.,
            0,
            &mut out,
        );
        run(&mut engine, out);
        let mut out = vec![];
        // Composition frame 80 is local frame 70, which is already occupied.
        update(
            &mut panels,
            &state(&engine),
            &Input {
                mouse: [400., 100.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            5.,
            0.,
            &mut out,
        );
        assert!(!out
            .iter()
            .any(|a| matches!(a, Action::Edit(_) | Action::EditBatch(_))));
        run(&mut engine, out);
        assert_eq!(engine.project(), &original);
        let mut out = vec![];
        update(
            &mut panels,
            &state(&engine),
            &Input {
                mouse: [175., 100.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            5.,
            0.,
            &mut out,
        );
        run(&mut engine, out);
        let mut out = vec![];
        update(
            &mut panels,
            &state(&engine),
            &Input {
                mouse: [9999.; 2],
                released: Some((MouseButton::Left, [200., 100.])),
                ..Default::default()
            },
            5.,
            0.,
            &mut out,
        );
        run(&mut engine, out);
        let moved = engine.project().clone();
        let frames: Vec<_> = moved.layers[0].effects[0].params["speed"]
            .track
            .keys
            .iter()
            .map(|k| k.frame)
            .collect();
        assert_eq!(frames, if copy { vec![10, 30, 70] } else { vec![30, 70] });
        let reopened: Project =
            serde_json::from_str(&serde_json::to_string(&moved).unwrap()).unwrap();
        assert_eq!(Engine::new(reopened).unwrap().project(), &moved);
        let mut out = vec![];
        assert!(delete(&mut panels, &state(&engine), &mut out));
        run(&mut engine, out);
        assert!(!engine.project().layers[0].effects[0].params["speed"]
            .track
            .keys
            .iter()
            .any(|k| k.frame == 30));
        engine.undo().unwrap();
        assert_eq!(engine.project(), &moved);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &original);
    }
}
