//! Mapped comparison curves use the same authored identities as typed lanes.
use super::*;
use crate::panels::curve_comparison::{data, tests::collect_targets};
#[cfg(target_os = "windows")]
#[path = "local_graph_native_tests.rs"]
mod native;

fn varied(case: typed::Case, reverse: bool) -> (Engine, Value, Vec<KeyframeTarget>) {
    let (mut engine, catalogue, targets) = typed::typed(case, reverse);
    let s = view(&engine);
    let numeric: Vec<_> = targets
        .into_iter()
        .filter(|t| data::load(&s, t, Locale::En).is_ok())
        .collect();
    assert!(!numeric.is_empty());
    for target in &numeric {
        let delta = match target {
            KeyframeTarget::Mask {
                property: motion_core::masks::MaskProperty::Opacity,
                ..
            }
            | KeyframeTarget::Effect { .. }
            | KeyframeTarget::Audio { .. } => -0.1,
            KeyframeTarget::Vector { parameter, .. } if parameter == "fill" => -0.1,
            _ => 0.1,
        };
        engine
            .apply_batch(vec![Command::LocalKeyframes {
                target: target.clone(),
                edit: LocalKeyEdit::Translate {
                    frames: vec![13],
                    delta_frame: 0,
                    copy: false,
                    value_offset: Some(LocalValueOffset {
                        component: 0,
                        delta,
                    }),
                },
            }])
            .unwrap();
    }
    (engine, catalogue, numeric)
}

#[test]
fn typed_comparison_values_speeds_and_addresses_follow_signed_clock_without_rounding() {
    for case in typed::CASES {
        for reverse in [false, true] {
            let (engine, _, targets) = varied(case, reverse);
            let s = view(&engine);
            let layer = &engine.project().layers[0];
            let clock = layer.time_mapping.as_ref().unwrap().clock;
            let offset = layer.timeline.as_ref().unwrap().offset_frame;
            let mut original = engine.snapshot();
            original.layers[0].time_mapping = None;
            let original = view(&Engine::new(original).unwrap());
            for target in targets {
                let data = data::load(&s, &target, Locale::En).unwrap();
                let oracle = data::load(&original, &target, Locale::En).unwrap();
                assert!(data.frames.is_empty());
                assert_eq!(
                    data.local_frames,
                    vec![
                        (3, layer.property_composition_frame(3.).unwrap()),
                        (13, layer.property_composition_frame(13.).unwrap())
                    ]
                );
                assert_eq!(
                    data.addressed_frames(&target)
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>(),
                    [3, 13].map(|frame| AddressedKey::Local(LocalKey {
                        target: target.clone(),
                        frame
                    }))
                );
                for component in &data.dimensions {
                    for n in 0..=80 {
                        let local = 3. + f64::from(n) / 8.;
                        let time = layer.property_composition_frame(local).unwrap();
                        for speed in [false, true] {
                            let expected = f64::from(
                                oracle
                                    .sample(local + f64::from(offset), *component, 30., speed)
                                    .unwrap(),
                            ) * if speed { clock.rate() } else { 1. };
                            let actual =
                                f64::from(data.sample(time, *component, 30., speed).unwrap());
                            assert!((actual-expected).abs()<0.00001,"{case:?} {target:?} reverse={reverse} local={local} component={component} speed={speed}: {actual} != {expected}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn continuous_spatial_keys_project_prepared_times_and_keep_neighboring_knots() {
    for reverse in [false, true] {
        let mut project = mapped(reverse).snapshot();
        project.layers.retain(|l| l.id == 7);
        let track = &mut project.layers[0].transform.position;
        track.keys.clear();
        for (frame, x) in [(0, 0.), (3, 1.), (13, 4.), (20, 8.)] {
            track.upsert(frame, [x, 20., 0.], Ease::Linear).unwrap();
        }
        track.enable_roving(&[3]).unwrap();
        let e = Engine::new(project).unwrap();
        let layer = &e.project().layers[0];
        let track = &layer.transform.position;
        let clock = layer.time_mapping.as_ref().unwrap().clock;
        let target = key(7, None, 0).target;
        let data = data::load(&view(&e), &target, Locale::En).unwrap();
        let prepared = track.spatial_animation.as_ref().unwrap().prepared();
        let expected = prepared
            .keys()
            .map(|(id, t)| (id, layer.property_composition_frame(t).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(data.spatial_frames, expected);
        assert!(data.frames.is_empty() && data.local_frames.is_empty());
        let seeds = crate::panels::graph_sampling::mapped_knots(track, 0., 60., 2., clock)
            .unwrap()
            .collect::<Vec<_>>();
        for (_, time) in &expected {
            for t in [time.next_down(), *time, time.next_up()] {
                assert!(seeds.contains(&t));
            }
        }
        for n in 0..=640 {
            let local = f64::from(n) / 32.;
            let time = layer.property_composition_frame(local).unwrap();
            let value = track.sample(local).unwrap()[0];
            assert!((data.sample(time, 0, 30., false).unwrap() - value).abs() < 0.00001);
            let speed =
                crate::panels::graph_sampling::velocity(track, local, 30., |v| f64::from(v[0]))
                    .unwrap()
                    * clock.rate();
            assert!((f64::from(data.sample(time, 0, 30., true).unwrap()) - speed).abs() < 0.00001);
        }
        let lazy = crate::panels::graph_sampling::knots(track, 0., 60., 2.).collect::<Vec<_>>();
        let mapped =
            crate::panels::graph_sampling::mapped_knots(track, 0., 60., 2., LayerClock::default())
                .unwrap()
                .collect::<Vec<_>>();
        assert_eq!(mapped, lazy, "the identity seed stream is unchanged");
    }
}

#[test]
fn audio_property_graph_ignores_source_remap_while_obeying_its_layer_clock() {
    use motion_core::layer_time::{TimeRemap, TimeRemapKey};
    let (engine, _, targets) = varied(typed::Case::Audio, true);
    let original = view(&engine);
    let mut project = engine.snapshot();
    project.layers[0].time_mapping.as_mut().unwrap().remap = Some(
        TimeRemap::new(vec![TimeRemapKey {
            frame: 0.,
            source_frame: 25.,
            easing: motion_core::Easing::default(),
        }])
        .unwrap(),
    );
    let remapped = view(&Engine::new(project).unwrap());
    for target in targets {
        let a = data::load(&original, &target, Locale::En).unwrap();
        let b = data::load(&remapped, &target, Locale::En).unwrap();
        assert_eq!(a.local_frames, b.local_frames);
        for frame in 0..60 {
            for speed in [false, true] {
                assert_eq!(
                    a.sample(f64::from(frame), 0, 30., speed).unwrap(),
                    b.sample(f64::from(frame), 0, 30., speed).unwrap()
                );
            }
        }
    }
}

#[test]
fn comparison_fractional_key_drag_edits_local_key_in_main_and_floating_owners() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for case in typed::CASES {
        for reverse in [false, true] {
            for floating in [false, true] {
                for locale in [Locale::Zh, Locale::En] {
                    let (engine, catalogue, targets) = varied(case, reverse);
                    // One channel per view makes coincident components an explicit choice.
                    for target in targets {
                        let mut engine = Engine::new(engine.snapshot()).unwrap();
                        let before = engine.snapshot();
                        let s = view(&engine);
                        let mut panels = typed::ready(case, &catalogue, &s, locale, false);
                        panels.timeline.set_graph_mode(true);
                        collect_targets(&mut panels, &s, &[target.clone()]);
                        let (paint, actions) =
                            draw(&mut panels, &s, Input::default(), floating, &mut atlas);
                        selection_only(&actions);
                        let time = engine.project().layers[0]
                            .property_composition_frame(3.)
                            .unwrap();
                        let start = typed::points(&paint, &panels, time)[0];
                        let modifiers = Modifiers {
                            alt: true,
                            ..Default::default()
                        };
                        let (_, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(start, true, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        selection_only(&actions);
                        assert_eq!(
                            panels.selected_keys.local_selections(),
                            vec![(target.clone(), vec![3])]
                        );
                        let ppf = panels.timeline.zoom_track.unwrap().width() / 120.;
                        let next = [start[0] + 6. * ppf, start[1]];
                        let (_, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(next, false, false, modifiers),
                            floating,
                            &mut atlas,
                        );
                        selection_only(&actions);
                        let (_, actions) = draw(
                            &mut panels,
                            &s,
                            pointer(next, false, true, modifiers),
                            floating,
                            &mut atlas,
                        );
                        let commands = atomic(actions);
                        assert!(
                            matches!(commands.as_slice(),[Command::LocalKeyframes {target:actual,edit:LocalKeyEdit::Translate {frames,delta_frame,value_offset:None,copy:false}}]
                if actual==&target && frames==&[3] && *delta_frame==if reverse {-4}else{4}),
                            "{commands:?}"
                        );
                        engine.apply_batch(commands).unwrap();
                        assert_ne!(engine.project(), &before);
                        assert!(engine.undo().unwrap());
                        assert_eq!(engine.project(), &before);
                        assert!(!engine.can_undo());
                    }
                }
            }
        }
    }
}

#[test]
fn comparison_local_graph_keeps_vertical_value_drag_instead_of_only_moving_time() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for floating in [false, true] {
            let (mut e, catalogue, targets) = varied(typed::Case::Masks, reverse);
            let target = targets[1].clone(); // scalar feather, with a nonzero end value
            let before = e.snapshot();
            let s = view(&e);
            let mut p = typed::ready(typed::Case::Masks, &catalogue, &s, Locale::En, false);
            p.timeline.set_graph_mode(true);
            collect_targets(&mut p, &s, &[target.clone()]);
            let (paint, _) = draw(&mut p, &s, Input::default(), floating, &mut atlas);
            let time = e.project().layers[0]
                .property_composition_frame(13.)
                .unwrap();
            let start = typed::points(&paint, &p, time)[0];
            draw(
                &mut p,
                &s,
                pointer(start, true, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            let end = [start[0], start[1] + 12.];
            draw(
                &mut p,
                &s,
                pointer(end, false, false, Modifiers::default()),
                floating,
                &mut atlas,
            );
            let (_, actions) = draw(
                &mut p,
                &s,
                pointer(end, false, true, Modifiers::default()),
                floating,
                &mut atlas,
            );
            let commands = atomic(actions);
            assert!(
                matches!(commands.as_slice(),[Command::LocalKeyframes {target:actual,edit:LocalKeyEdit::Translate {frames,delta_frame:0,value_offset:Some(LocalValueOffset {component:0,delta}),copy:false}}] if actual==&target && frames==&[13] && *delta<0.),
                "{commands:?}"
            );
            e.apply_batch(commands).unwrap();
            assert!(e.undo().unwrap());
            assert_eq!(e.project(), &before);
        }
    }
}

#[test]
fn malformed_graph_clock_is_bounded_visible_at_default_level_and_recovers_without_stdout_logs() {
    const NAME:&str="panels::key_selection::tests::ui::local_timing::graphs::malformed_graph_clock_is_bounded_visible_at_default_level_and_recovers_without_stdout_logs";
    const CHILD: &str = "MOTION_GRAPH_CLOCK_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let (e, catalogue, targets) = varied(typed::Case::GroupVector, false);
        let s = view(&e);
        let mut p = typed::ready(typed::Case::GroupVector, &catalogue, &s, Locale::En, false);
        p.timeline.set_graph_mode(true);
        collect_targets(&mut p, &s, &targets[..1]);
        let mut bad = s.clone();
        bad["revision"] = json!(e.revision() + 1);
        bad["project"]["layers"][0]["time_mapping"]["clock"]["stretch"]["denominator"] = json!(0);
        for _ in 0..120 {
            draw(&mut p, &bad, Input::default(), false, &mut atlas);
        }
        let mut good = s.clone();
        good["revision"] = json!(e.revision() + 2);
        draw(&mut p, &good, Input::default(), false, &mut atlas);
        assert!(data::load(&good, &targets[0], Locale::En).is_ok());
        let (e, catalogue) = crate::panels::effect_timeline::tests::fixture();
        let mut s = crate::panels::effect_timeline::tests::state(&e);
        let before = s["project"].clone();
        let mut p = crate::panels::effect_timeline::tests::setup(&catalogue, Locale::En);
        p.effect_controls.choose_parameter(&s, 7, 1, "speed");
        p.timeline.set_graph_mode(true);
        s["project"]["layers"][0]["time_mapping"] = json!(LayerTimeMapping {
            clock: LayerClock::new(
                TimeStretch::new(3, 2).unwrap(),
                TimeDirection::Forward,
                0.,
                0.
            )
            .unwrap(),
            remap: None
        });
        s["project"]["layers"][0]["time_mapping"]["clock"]["stretch"]["denominator"] = json!(0);
        for _ in 0..120 {
            draw(&mut p, &s, Input::default(), false, &mut atlas);
        }
        assert!(matches!(
            data::load(
                &s,
                &KeyframeTarget::Effect {
                    object: 7,
                    effect: 1,
                    param: "speed".into()
                },
                Locale::En
            ),
            Err(data::Unavailable::Fault(_))
        ));
        s["project"] = before.clone();
        s["revision"] = json!(2);
        draw(&mut p, &s, Input::default(), false, &mut atlas);
        assert_eq!(s["project"], before);
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_GRAPH_CLOCK_ERROR_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &result.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let lines = file
        .lines()
        .filter(|l| l.contains("site=effect.graph_clock "))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{file}");
    assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
    assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
    assert!(lines.iter().all(|l| l.contains("object=7")
        && l.contains("effect=1")
        && l.contains("param=speed")
        && l.contains("reason=")));
    let comparison = file
        .lines()
        .filter(|l| l.contains("ERROR") && l.contains("curve comparison failed"))
        .collect::<Vec<_>>();
    assert_eq!(
        comparison.len(),
        1,
        "revision cache suppresses repeated decoding: {file}"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("site=effect.graph_clock ") && stderr.contains("curve comparison failed")
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains("motion_studio::"));
}
