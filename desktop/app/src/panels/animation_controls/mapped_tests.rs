use super::*;
use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Engine, Layer, LayerTimeline, LocalKeyEdit, Project,
};
use serde_json::json;
#[cfg(target_os = "windows")]
mod native;
mod clipboard;

pub(super) fn pointer(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        held: (!released).then_some(MouseButton::Left),
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
pub(super) fn quadratic() -> Easing {
    Easing {
        ease: Ease::Linear,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Quadratic {
                control: [0.5, 0.25],
                start: 0.,
                end: 1.,
            },
        }),
    }
}
fn seed_curve(engine: &mut Engine) {
    engine
        .apply(Command::LocalKeyframes {
            target: motion_core::KeyframeTarget::Property {
                object: 7,
                property: Property::Opacity,
                axis: None,
            },
            edit: LocalKeyEdit::Curve {
                frames: vec![3],
                easing: quadratic(),
            },
        })
        .unwrap();
}

fn fixture(reverse: bool) -> Engine {
    let mut project = Project::new(64, 64, 30, 90).unwrap();
    let mut layer = Layer::solid(7, "Mapped mini", [32.; 2], [32., 32., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 90,
        offset_frame: 8,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 12. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    for (frame, value) in [(-3, 0.2), (3, 0.5), (9, 0.8)] {
        layer
            .transform
            .opacity
            .upsert(frame, value, Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn projected(engine: &Engine, local: f64) -> f64 {
    engine.project().layers[0]
        .property_composition_frame(local)
        .unwrap()
}
fn state(engine: &Engine, frame: f64) -> Value {
    json!({"root":"mapped-mini", "composition":motion_core::MAIN_COMPOSITION,"document_generation":1,
        "revision":engine.revision(),"frame":frame,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(frame).unwrap(),
        "sampledLayers":[{"id":7,"opacity":engine.project().layers[0].transform.opacity.sample(
            engine.project().layers[0].local_frame(frame)).unwrap()}]})
}
fn controls() -> Controls {
    Controls {
        expanded: true,
        property: Some(Property::Opacity),
        ..Default::default()
    }
}
pub(super) fn draw(
    c: &mut Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    input: &Input,
    locale: Locale,
) -> Vec<Action> {
    let mut out = vec![];
    c.render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 600., 800.),
        &mut 0.,
        &state["project"]["layers"][0],
        &state["sampledLayers"][0],
        &state["timeline_layers"][0],
        state,
        input,
        locale,
        &mut out,
        None,
    );
    out
}
pub(super) fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
pub(super) fn apply(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => {
                engine.apply(command).unwrap();
            }
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::History(2) => {
                engine.begin_gesture().unwrap();
            }
            Action::History(3) => {
                engine.end_gesture(true).unwrap();
            }
            Action::History(4) => {
                engine.end_gesture(false).unwrap();
            }
            other => panic!("unexpected action {other:?}"),
        }
    }
}
#[test]
fn mini_presets_add_move_delete_use_exact_signed_local_identity_in_both_directions() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let mut engine = fixture(reverse);
            let mut c = controls();
            let s = state(&engine, projected(&engine, 3.));
            let actions = draw(&mut c, &mut atlas, &s, &click([230., 182.]), locale);
            assert!(
                matches!(actions.as_slice(),[Action::Edit(Command::LocalKeyframes {
            edit:LocalKeyEdit::Curve {frames,..},..
        })] if frames==&[3])
            );
            apply(&mut engine, actions);
            assert!(engine.project().layers[0].transform.opacity.keys[1]
                .curve
                .is_some());
            assert!(engine.project().layers[0].transform.opacity.keys[0]
                .curve
                .is_none());
            engine.undo().unwrap();
            assert!(
                c.user_key().is_none(),
                "a local address must never escape as a legacy composition key"
            );

            let s = state(&engine, projected(&engine, 6.));
            let actions = draw(&mut c, &mut atlas, &s, &click([450., 98.]), locale);
            assert!(matches!(actions.as_slice(),[Action::EditBatch(commands)]
            if commands.len()==2 && commands.iter().all(|c| matches!(c,Command::TrackAt {frame,..} if *frame==s["frame"].as_f64().unwrap()))));
            apply(&mut engine, actions);
            assert_eq!(
                engine.project().layers[0]
                    .transform
                    .opacity
                    .keys
                    .iter()
                    .map(|k| k.frame)
                    .collect::<Vec<_>>(),
                [-3, 3, 6, 9]
            );
            assert!(
                (engine.project().layers[0].transform.opacity.keys[2].value - 0.65).abs() < 1e-6
            );
            engine.undo().unwrap();

            let mut c = controls();
            c.select_display_key(&s, 7, Property::Opacity, None, -3, true);
            let s = state(&engine, projected(&engine, 0.));
            let actions = draw(&mut c, &mut atlas, &s, &click([100., 154.]), locale);
            assert!(
                matches!(actions.as_slice(),[Action::Edit(Command::LocalKeyframes {
            edit:LocalKeyEdit::Translate {frames,delta_frame:3,copy:false,value_offset:None},..
        })] if frames==&[-3])
            );
            apply(&mut engine, actions);
            assert_eq!(
                engine.project().layers[0].transform.opacity.keys[0].frame,
                0
            );
            assert_eq!(
                engine.project().layers[0].transform.opacity.keys[0].value,
                0.2
            );
            engine.undo().unwrap();
            let mut c = controls();
            c.select_display_key(&s, 7, Property::Opacity, None, -3, true);
            let actions = draw(&mut c, &mut atlas, &s, &click([450., 154.]), locale);
            assert!(
                matches!(actions.as_slice(),[Action::Edit(Command::LocalKeyframes {edit:LocalKeyEdit::Delete {frames},..})] if frames==&[-3])
            );
            apply(&mut engine, actions);
            assert_eq!(engine.project().layers[0].transform.opacity.keys.len(), 2);
            assert_eq!(c.take_local_selection(), Some(None));
        }
    }
}
#[test]
fn navigation_orders_composition_time_and_fractional_local_edits_stay_disabled() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let engine = fixture(reverse);
            let s = state(&engine, projected(&engine, 3.));
            for (x, right) in [(28., false), (60., true)] {
                let mut c = controls();
                let actions = draw(&mut c, &mut atlas, &s, &click([x, 126.]), locale);
                let local = if right ^ reverse { 9 } else { -3 };
                assert!(
                    matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==projected(&engine,local as f64))
                );
                assert_eq!(c.take_local_selection().unwrap().unwrap().frame, local);
                assert!(c.user_key().is_none());
            }
            let s = state(&engine, projected(&engine, 3.) + 0.1);
            for point in [[450., 98.], [100., 154.]] {
                let mut c = controls();
                assert!(draw(&mut c, &mut atlas, &s, &click(point), locale).is_empty());
            }
            let mut c = controls();
            assert!(draw(&mut c, &mut atlas, &s, &click([100., 98.]), locale)
                .iter()
                .any(|a| matches!(
                    a,
                    Action::Edit(Command::TrackAt {
                        edit: motion_core::TrackEdit::Animate { enabled: false },
                        ..
                    })
                )));
        }
    }
}

#[test]
fn mapped_separated_curve_edits_preserve_other_axes_and_vector_values() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        let mut project = fixture(reverse).snapshot();
        for frame in [-3, 3, 9] {
            project.layers[0]
                .transform
                .position
                .upsert(frame, [frame as f32, 20., 30.], Ease::Linear)
                .unwrap();
        }
        project.layers[0].transform.position.separate().unwrap();
        let mut engine = Engine::new(project).unwrap();
        let before = engine.snapshot();
        let s = state(&engine, projected(&engine, 3.));
        let mut c = Controls::default();
        c.choose_track_in(&s, 7, Property::Position, Some(Axis::Y));
        let actions = draw(&mut c, &mut atlas, &s, &click([230., 182.]), Locale::En);
        assert!(
            matches!(actions.as_slice(),[Action::Edit(Command::LocalKeyframes {
            target:motion_core::KeyframeTarget::Property {property:Property::Position,axis:Some(Axis::Y),..},
            edit:LocalKeyEdit::Curve {frames,..}
        })] if frames==&[3])
        );
        apply(&mut engine, actions);
        let old = before.layers[0].transform.position.axes.as_ref().unwrap();
        let next = engine.project().layers[0]
            .transform
            .position
            .axes
            .as_ref()
            .unwrap();
        assert_eq!(old.x, next.x);
        assert_eq!(old.z, next.z);
        assert_eq!(old.y.keys[0], next.y.keys[0]);
        assert_eq!(old.y.keys[2], next.y.keys[2]);
        assert_eq!(old.y.keys[1].value, next.y.keys[1].value);
        assert_ne!(old.y.keys[1].curve, next.y.keys[1].curve);
        engine.undo().unwrap();
        assert_eq!(engine.snapshot(), before);
    }
}

#[test]
fn mini_actual_readout_projects_local_interval_and_signed_speed_without_rounding() {
    let target = motion_core::KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let engine = fixture(reverse);
            let s = state(&engine, projected(&engine, 3.));
            let mut view = mini::View::default();
            for (speed, expected) in [(false, 35.), (true, if reverse { -100. } else { 100. })] {
                let (title, lines) = view.actual_readout(
                    &s,
                    locale,
                    &target,
                    -3,
                    Some(3),
                    0.5,
                    speed,
                    "t=0.5".into(),
                    vec![],
                );
                assert!(
                    title.ends_with(&format!("{:.2} f", projected(&engine, 0.))),
                    "{title}"
                );
                assert_eq!(lines.len(), 1);
                assert!(lines[0].contains(&format!("{expected:.4}")), "{:?}", lines);
            }
        }
    }
}

#[test]
fn fit_selection_uses_signed_local_endpoints_and_never_a_legacy_projection_alias() {
    use crate::panels::{
        graph_ranges::{Bounds, Controls as Ranges},
        graph_view::Range,
        key_selection::{AddressedKey, LocalKey, Mode, Set},
    };
    let target = motion_core::KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    for reverse in [false, true] {
        let engine = fixture(reverse);
        let state = state(&engine, projected(&engine, 3.));
        let all = Bounds {
            x: Range::new(0., 1.).unwrap(),
            y: Range::new(0., 1.).unwrap(),
        };
        let mut selected = Set::default();
        let mut ranges = Ranges::default();
        let choose = |state: &Value, selected: &Set, ranges: &mut Ranges| {
            mini::selection(
                state,
                &target,
                -3,
                Some(3),
                &mini::Context {
                    ranges,
                    selection: selected,
                    effect_key: None,
                    focused: Some(crate::panels::Panel::Curves),
                },
                None,
                Easing::default(),
                all,
            )
        };
        assert!(choose(&state, &selected, &mut ranges).is_none());
        for frames in [vec![-3], vec![3], vec![-3, 3]] {
            selected
                .apply_addressed(
                    &state,
                    frames.iter().map(|frame| {
                        AddressedKey::Local(LocalKey {
                            target: target.clone(),
                            frame: *frame,
                        })
                    }),
                    Mode::Replace,
                )
                .unwrap();
            let actual = choose(&state, &selected, &mut ranges).unwrap();
            if frames.len() == 2 {
                assert_eq!(actual, all);
            } else {
                let t = if frames[0] == -3 { 0. } else { 1. };
                assert_eq!(actual.x, Range::fitted(t, t, 0.05, 0.1).unwrap());
            }
        }
        selected.clear(&state);
        assert!(choose(&state, &selected, &mut ranges).is_none());
        selected
            .apply_addressed(
                &state,
                [AddressedKey::Local(LocalKey {
                    target: target.clone(),
                    frame: -3,
                })],
                Mode::Replace,
            )
            .unwrap();
        let mut stale = state.clone();
        stale["document_generation"] = 2.into();
        assert!(choose(&stale, &selected, &mut ranges).is_none());
    }
}

#[test]
fn mapped_handle_drag_is_one_undo_and_changed_time_owner_or_focus_cancels_it() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for reverse in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            for cancel in 0..7 {
                let mut engine = fixture(reverse);
                seed_curve(&mut engine);
                let before = engine.snapshot();
                let frame = projected(&engine, 3.);
                let mut c = controls();
                let s = state(&engine, frame);
                assert!(draw(&mut c, &mut atlas, &s, &Input::default(), locale).is_empty());
                let (_, graph, bounds) = c.mini_test_geometry().unwrap();
                let point = [
                    graph.min[0] + bounds.x.fraction(0.5) as f32 * graph.width(),
                    graph.max[1] - bounds.y.fraction(0.25) as f32 * graph.height(),
                ];
                let actions = draw(&mut c, &mut atlas, &s, &pointer(point, true, false), locale);
                assert!(matches!(actions.first(), Some(Action::History(2))));
                apply(&mut engine, actions);
                assert!(engine.gesture_active());
                let moved = [point[0] + 25., point[1] - 15.];
                let s = state(&engine, frame);
                let actions = draw(
                    &mut c,
                    &mut atlas,
                    &s,
                    &pointer(moved, false, false),
                    locale,
                );
                assert!(actions
                    .iter()
                    .any(|a| matches!(a,Action::Edit(Command::LocalKeyframes {
                edit:LocalKeyEdit::Curve {frames,..},..
            }) if frames==&[3])));
                apply(&mut engine, actions);
                assert_ne!(engine.snapshot(), before);
                let mut s = state(&engine, frame);
                let mut input = pointer(moved, false, true);
                match cancel {
                    0 => {}
                    1 => s["frame"] = (frame + 0.1).into(),
                    2 => {
                        s["project"]["layers"][0]["time_mapping"]["clock"]["elapsed_pivot"] =
                            1.0.into()
                    }
                    3 => s["project"]["layers"][0]["timeline"]["offset_frame"] = 9.into(),
                    4 => input.focus_lost = true,
                    5 => c.select_display_key(&s, 7, Property::Opacity, None, -3, true),
                    6 => {
                        s["project"]["layers"][0]["time_mapping"]["clock"] = json!({"broken":true})
                    }
                    _ => unreachable!(),
                }
                let actions = draw(&mut c, &mut atlas, &s, &input, locale);
                if cancel == 0 {
                    assert!(matches!(actions.last(), Some(Action::History(3))));
                    apply(&mut engine, actions);
                    let after = engine.snapshot();
                    engine.undo().unwrap();
                    assert_eq!(engine.snapshot(), before);
                    engine.redo().unwrap();
                    assert_eq!(engine.snapshot(), after);
                } else {
                    assert!(
                        matches!(actions.as_slice(), [Action::History(4)]),
                        "case={cancel}: {actions:?}"
                    );
                    apply(&mut engine, actions);
                    assert_eq!(engine.snapshot(), before);
                    assert!(!c.editing());
                    assert!(draw(&mut c, &mut atlas, &s, &Input::default(), locale).is_empty());
                }
                assert!(!engine.gesture_active());
            }
        }
    }
}

#[test]
fn mapped_clock_and_projection_faults_use_default_file_stderr_and_bounded_sites() {
    const CHILD: &str = "MOTION_TIME_MINI_FAULT_CHILD";
    const NAME:&str="panels::animation_controls::mapped_tests::mapped_clock_and_projection_faults_use_default_file_stderr_and_bounded_sites";
    const SITES: [&str; 3] = [
        "curve.controls.clock",
        "curve.controls.keys",
        "curve.mini.readout_clock",
    ];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let engine = fixture(false);
        let healthy = state(&engine, projected(&engine, 3.));
        let mut clock = healthy.clone();
        clock["project"]["layers"][0]["time_mapping"]["clock"] = json!({"broken":true});
        let mut keys = healthy.clone();
        keys["timeline_layers"][0]["properties"]["opacity"]["keys"][1]["frame"] = 0.into();
        let mut c = controls();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut mini = mini::View::default();
        let target = motion_core::KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        };
        for _ in 0..120 {
            for s in [&clock, &keys, &healthy] {
                assert!(draw(&mut c, &mut atlas, s, &Input::default(), Locale::En).is_empty());
            }
            let (_, lines) = mini.actual_readout(
                &clock,
                Locale::En,
                &target,
                -3,
                Some(3),
                0.5,
                false,
                "t=0.5".into(),
                vec![],
            );
            assert_eq!(lines, ["Actual readout unavailable"]);
        }
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let lines: Vec<_> = file.lines().filter(|line| line.contains(site)).collect();
            assert_eq!(lines.len(), 2, "{site}: {lines:?}");
            assert!(lines[0].contains("WARN") && lines[0].contains("count=1 "));
            assert!(lines[1].contains("ERROR") && lines[1].contains("count=60 "));
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, temp.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    for site in SITES {
        assert_eq!(
            stderr.lines().filter(|line| line.contains(site)).count(),
            1,
            "{stderr}"
        );
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("motion_studio::"));
    if let Some(path) = std::env::var_os("MOTION_TIME_MINI_FAULT_LOG_COPY") {
        std::fs::copy(temp.path().join("motion-studio.log"), path).unwrap();
    }
}
