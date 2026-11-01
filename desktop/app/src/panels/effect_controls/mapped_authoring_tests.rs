//! Mapped property rows use exact composition time for both reads and edits.
use super::*;
use crate::panels::{self, property_authoring, Panels};
use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Ease, Engine, KeyframeTarget, Layer, LayerTimeline, Project, Property, TrackEdit, TrackValue,
};
use serde_json::json;
mod numeric_time;

fn fixture(reverse: bool) -> (Engine, Value) {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "brightness_contrast")
        .unwrap();
    let mut effect = motion_core::EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    let parameter = effect.params.get_mut("p0001").unwrap();
    for (frame, value) in [(0, 10.), (3, 20.), (6, 30.)] {
        parameter
            .track
            .upsert(frame, [value, 0., 0., 0.], Ease::Linear)
            .unwrap();
    }
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut layer = Layer::solid(
        7,
        "Exact property authoring",
        [32.; 2],
        [32., 32., 0.],
        [1.; 4],
    );
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: 2,
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
            if reverse { 6. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    for (frame, value) in [(0, 10.), (3, 20.), (6, 30.)] {
        layer
            .transform
            .position
            .upsert(frame, [value, 32., 0.], Ease::Linear)
            .unwrap();
    }
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let catalogue = json!({"packages":[{"manifest":package.manifest,"hash":package.hash}]});
    (Engine::new(project).unwrap(), catalogue)
}
fn state(engine: &Engine, frame: f64) -> Value {
    json!({"root":"exact-authoring","composition":motion_core::MAIN_COMPOSITION,"document_generation":1,
        "revision":engine.revision(),"frame":frame,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(frame).unwrap()})
}
fn controls(catalogue: &Value) -> Controls {
    Controls {
        catalogue: catalogue.clone().into(),
        ..Default::default()
    }
}
fn draw(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    input: &Input,
    locale: Locale,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let mut y = 28.;
    let mut out = vec![];
    controls.render(
        &mut paint,
        atlas,
        Rect::new(20., 28., 420., 680.),
        &mut y,
        &state["project"]["layers"][0],
        state,
        input,
        locale,
        &mut out,
    );
    (paint, out)
}
fn pointer(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        held: (!released).then_some(MouseButton::Left),
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn effect_target() -> KeyframeTarget {
    KeyframeTarget::Effect {
        object: 7,
        effect: 1,
        param: "p0001".into(),
    }
}
fn apply(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            _ => panic!("unexpected authoring action: {action:?}"),
        }
    }
}

#[test]
fn mapped_transform_sampling_uses_local_clock_in_both_directions() {
    for reverse in [false, true] {
        let (engine, _) = fixture(reverse);
        for (frame, expected) in if reverse {
            [(2., 30.), (6.5, 20.), (8., 50. / 3.)]
        } else {
            [(2., 10.), (6.5, 20.), (8., 70. / 3.)]
        } {
            let value = panels::animation_data::sample(
                &state(&engine, frame),
                7,
                Property::Position,
                None,
                frame,
            )
            .unwrap();
            assert!((value[0].as_f64().unwrap() - expected).abs() < 0.00001);
        }
    }
}

#[test]
fn mapped_edits_use_exact_commands_and_unmapped_edits_keep_the_legacy_protocol() {
    let (engine, _) = fixture(false);
    let old = Command::SetVector {
        object: 7,
        property: Property::Position,
        frame: 6,
        value: [99., 32., 0.],
    };
    for frame in [6., 6.5] {
        let mut s = state(&engine, frame);
        for mapped in [true, false] {
            if !mapped {
                s["project"]["layers"][0]["time_mapping"] = Value::Null;
            }
            let command = property_authoring::at_playhead(&s, old.clone());
            if mapped {
                assert!(matches!(command,Command::TrackAt {frame:at,..} if at==frame));
            } else {
                assert_eq!(
                    serde_json::to_value(command).unwrap(),
                    serde_json::to_value(&old).unwrap()
                );
            }
        }
    }
}

#[test]
fn parameter_readout_and_numeric_drag_edit_exact_local_key_and_undo_once() {
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let (mut engine, catalogue) = fixture(reverse);
            let before = engine.snapshot();
            let mut controls = controls(&catalogue);
            let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
            let s = state(&engine, 6.5);
            draw(&mut controls, &mut atlas, &s, &Input::default(), locale);
            let rect = controls.test_rects[&(7, 1, "p0001".into(), 0)];
            let point = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            draw(
                &mut controls,
                &mut atlas,
                &s,
                &Input {
                    mouse: point,
                    ..Default::default()
                },
                locale,
            );
            assert_eq!(
                controls
                    .drag_readout
                    .as_ref()
                    .map(|(_, text)| text.as_str()),
                Some("20")
            );
            let mut exact_updates = 0;
            for input in [
                pointer(point, true, false),
                pointer([point[0] + 20., point[1]], false, false),
                pointer([point[0] + 20., point[1]], false, false),
                pointer([point[0] + 30., point[1]], false, false),
                pointer([point[0] + 30., point[1]], false, true),
            ] {
                let (_, actions) = draw(
                    &mut controls,
                    &mut atlas,
                    &state(&engine, 6.5),
                    &input,
                    locale,
                );
                for action in &actions {
                    if let Action::Edit(command) = action {
                        assert!(
                            matches!(command,Command::TrackAt {target,frame,edit:TrackEdit::Set {..}} if target==&effect_target()&&*frame==6.5)
                        );
                        exact_updates += 1;
                    }
                }
                apply(&mut engine, actions);
            }
            assert!(exact_updates >= 2);
            let next = &engine.project().layers[0].effects[0].params["p0001"].track;
            let original = &before.layers[0].effects[0].params["p0001"].track;
            assert_ne!(next.keys[1].value, original.keys[1].value);
            assert_eq!(next.keys[0], original.keys[0]);
            assert_eq!(next.keys[2], original.keys[2]);
            assert!(engine.undo().unwrap());
            assert_eq!(engine.snapshot(), before);
            assert!(!engine.undo().unwrap());
        }
    }
}

#[test]
fn mapped_effect_keyboard_navigation_uses_fractional_projected_times_in_reverse_order() {
    for reverse in [false, true] {
        let (engine, catalogue) = fixture(reverse);
        let mut controls = controls(&catalogue);
        let s = state(&engine, 6.5);
        controls.choose_parameter(&s, 7, 1, "p0001");
        for (key, expected) in [(Key::J, 2.), (Key::K, 11.)] {
            let mut out = vec![];
            assert!(controls.navigate_keys(
                &s,
                Some(7),
                &Input {
                    keys: vec![(key, Default::default())],
                    ..Default::default()
                },
                &mut out
            ));
            assert!(matches!(out.as_slice(),[Action::Seek(frame)] if *frame==expected));
        }
    }
}

#[test]
fn mapped_property_navigation_selects_local_identity_and_toggle_never_casts_the_playhead() {
    let (mut engine, _) = fixture(false);
    let s = state(&engine, 6.5);
    let mut panels = Panels::default();
    let mut out = vec![];
    panels::key_navigation::apply_property_choice(
        &mut panels,
        &s,
        7,
        Property::Position,
        None,
        panels::key_navigation::Choice::Seek(6.5),
        &mut out,
    );
    assert!(matches!(out.as_slice(),[Action::Seek(frame)] if *frame==6.5));
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    };
    assert_eq!(
        panels.selected_keys.local_selections(),
        vec![(target.clone(), vec![3])]
    );
    assert_eq!(panels.animation_controls.user_key(), None);
    out.clear();
    panels::key_navigation::apply_property_choice(
        &mut panels,
        &s,
        7,
        Property::Position,
        None,
        panels::key_navigation::Choice::Toggle,
        &mut out,
    );
    assert!(
        matches!(out.as_slice(),[Action::Edit(Command::TrackAt {target:actual,frame,edit:TrackEdit::ToggleKey})] if actual==&target&&*frame==6.5)
    );
    apply(&mut engine, out);
    assert!(!engine.project().layers[0]
        .transform
        .position
        .keys
        .iter()
        .any(|key| key.frame == 3));
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project().layers[0].transform.position.keys.len(), 3);
}

#[test]
fn seeking_coincident_spatial_keys_does_not_keep_an_unrelated_previous_key_selected() {
    let (engine, _) = fixture(false);
    let mut project = engine.snapshot();
    let track = &mut project.layers[0].transform.position;
    track.keys[1].value = track.keys[0].value;
    track.enable_roving(&[3]).unwrap();
    let spatial = track.spatial_animation.as_ref().unwrap();
    let last = spatial.keys().last().unwrap().id;
    let engine = Engine::new(project).unwrap();
    let s = state(&engine, 6.5);
    let mut panels = Panels::default();
    let target = KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    };
    panels
        .selected_keys
        .apply_addressed(
            &s,
            [panels::key_selection::SpatialKey { target, id: last }.into()],
            panels::key_selection::Mode::Replace,
        )
        .unwrap();
    assert!(!panels.selected_keys.spatial_selections().is_empty());
    let mut out = vec![];
    panels::key_navigation::apply_property_choice(
        &mut panels,
        &s,
        7,
        Property::Position,
        None,
        panels::key_navigation::Choice::Seek(2.),
        &mut out,
    );
    assert!(matches!(out.as_slice(),[Action::Seek(frame)] if *frame==2.));
    assert!(panels.selected_keys.spatial_selections().is_empty());
    assert!(panels.selected_keys.local_selections().is_empty());
    assert_eq!(panels.animation_controls.user_key(), None);
}

#[test]
fn mapped_effect_rail_toggles_at_fractional_composition_frame_and_disables_fractional_local_frame()
{
    let (mut engine, _) = fixture(false);
    let parameter = engine.project().layers[0].effects[0].params["p0001"].clone();
    for (at, enabled) in [(6.5, true), (6.6, false)] {
        let s = state(&engine, at);
        let mut list = PaintList::default();
        let mut channel = None;
        let mut out = vec![];
        let row = Rect::new(0., 0., 420., 26.);
        let rail = panels::key_navigation::rail(&list.styles(), row);
        let point = [rail.min[0] + rail.width() * 0.5, 13.];
        key_navigation::parameter_navigation_in_rail(
            &mut channel,
            &mut list,
            row,
            rail,
            &s,
            &pointer(point, true, false),
            7,
            1,
            "p0001",
            &parameter,
            property_authoring::local(&s, 7, at).unwrap(),
            false,
            &mut out,
        );
        if enabled {
            assert!(
                matches!(out.as_slice(),[Action::Edit(Command::TrackAt {target,frame,edit:TrackEdit::ToggleKey})] if target==&effect_target()&&*frame==at)
            );
            apply(&mut engine, out);
            assert_eq!(
                engine.project().layers[0].effects[0].params["p0001"]
                    .track
                    .keys
                    .len(),
                2
            );
        } else {
            assert!(out.is_empty());
        }
    }
}

#[test]
fn invalid_clock_and_offset_do_not_sample_or_generate_rounded_healthy_edits() {
    let (engine, _) = fixture(false);
    for field in ["clock", "offset"] {
        let mut s = state(&engine, 6.5);
        if field == "clock" {
            s["project"]["layers"][0]["time_mapping"]["clock"] = json!({"broken":true});
        } else {
            s["project"]["layers"][0]["timeline"]["offset_frame"] = json!(0.5);
        }
        assert!(property_authoring::local(&s, 7, 6.5).is_err());
        assert!(panels::animation_data::sample(&s, 7, Property::Position, None, 6.5).is_none());
    }
    let mut s = state(&engine, 6.5);
    s["frame"] = Value::Null;
    assert!(
        matches!(property_authoring::at_playhead(&s,Command::SetScalar {object:7,property:Property::Opacity,frame:0,value:0.5}),
        Command::TrackAt {frame,..} if frame.is_nan())
    );
}

#[test]
fn invalid_clock_cancels_an_active_numeric_gesture_once_without_committing() {
    let (mut engine, catalogue) = fixture(false);
    let before = engine.snapshot();
    let mut controls = controls(&catalogue);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let good = state(&engine, 6.5);
    draw(
        &mut controls,
        &mut atlas,
        &good,
        &Input::default(),
        Locale::En,
    );
    let rect = controls.test_rects[&(7, 1, "p0001".into(), 0)];
    let point = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    for input in [
        pointer(point, true, false),
        pointer([point[0] + 20., point[1]], false, false),
        pointer([point[0] + 20., point[1]], false, false),
        pointer([point[0] + 30., point[1]], false, false),
    ] {
        let (_, actions) = draw(
            &mut controls,
            &mut atlas,
            &state(&engine, 6.5),
            &input,
            Locale::En,
        );
        apply(&mut engine, actions);
    }
    assert!(engine.gesture_active());
    assert_ne!(engine.snapshot(), before);
    let mut bad = state(&engine, 6.5);
    bad["project"]["layers"][0]["time_mapping"]["clock"] = json!({"broken":true});
    let (_, actions) = draw(
        &mut controls,
        &mut atlas,
        &bad,
        &Input::default(),
        Locale::En,
    );
    assert!(matches!(actions.as_slice(), [Action::History(4)]));
    apply(&mut engine, actions);
    assert!(!engine.gesture_active());
    assert_eq!(engine.snapshot(), before);
    assert!(!engine.can_undo());
    assert!(draw(
        &mut controls,
        &mut atlas,
        &bad,
        &Input::default(),
        Locale::En
    )
    .1
    .is_empty());
}

#[test]
fn mapped_clock_faults_reach_default_file_and_error_stderr_with_bounded_lifetime_sites() {
    const CHILD: &str = "MOTION_TRACK_AT_CLOCK_FAULT_CHILD";
    const NAME:&str="panels::effect_controls::mapped_authoring_tests::mapped_clock_faults_reach_default_file_and_error_stderr_with_bounded_lifetime_sites";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let (engine, catalogue) = fixture(false);
        let healthy = state(&engine, 6.5);
        let mut bad = healthy.clone();
        bad["project"]["layers"][0]["time_mapping"]["clock"] = json!({"broken":true});
        let mut controls = controls(&catalogue);
        controls.choose_parameter(&healthy, 7, 1, "p0001");
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let parameter: EffectParam = serde_json::from_value(
            healthy["project"]["layers"][0]["effects"][0]["params"]["p0001"].clone(),
        )
        .unwrap();
        let selection_faults: Vec<_> = (0..5)
            .map(|case| {
                let mut state = healthy.clone();
                let track = &mut state["timeline_layers"][0]["properties"]["position"];
                match case {
                    0 => track["keys"] = json!("broken"),
                    1 => track["keys"][0]["frame"] = Value::Null,
                    2 => track["keys"][1]["local_frame"] = Value::Null,
                    3 => track["spatial_keys"] = json!("broken"),
                    4 => track["spatial_keys"] = json!([{"frame":6.5,"local_frame":3.,"id":null}]),
                    _ => unreachable!(),
                }
                state
            })
            .collect();
        for _ in 0..120 {
            assert!(
                panels::animation_data::sample(&bad, 7, Property::Position, None, 6.5).is_none()
            );
            assert!(draw(
                &mut controls,
                &mut atlas,
                &bad,
                &Input::default(),
                Locale::En
            )
            .1
            .is_empty());
            let mut out = vec![];
            assert!(!controls.navigate_keys(&bad, Some(7), &Input::default(), &mut out));
            let mut paint = PaintList::default();
            let row = Rect::new(0., 0., 420., 26.);
            let rail = panels::key_navigation::rail(&paint.styles(), row);
            assert!(!key_navigation::parameter_navigation_in_rail(
                &mut None,
                &mut paint,
                row,
                rail,
                &bad,
                &Input::default(),
                7,
                1,
                "p0001",
                &parameter,
                3.,
                false,
                &mut out
            ));
            assert!(out.is_empty());
            for malformed in &selection_faults {
                let mut panels = Panels::default();
                let mut actions = vec![];
                panels::key_navigation::apply_property_choice(
                    &mut panels,
                    malformed,
                    7,
                    Property::Position,
                    None,
                    panels::key_navigation::Choice::Seek(6.5),
                    &mut actions,
                );
                assert!(matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==6.5));
                assert!(panels.selected_keys.local_selections().is_empty());
                assert!(panels.selected_keys.spatial_selections().is_empty());
            }
            // Existing panel counters intentionally span the process lifetime.
            // A healthy neighbouring row cannot reset malformed-row evidence.
            assert!(
                panels::animation_data::sample(&healthy, 7, Property::Position, None, 6.5)
                    .is_some()
            );
            draw(
                &mut controls,
                &mut atlas,
                &healthy,
                &Input::default(),
                Locale::En,
            );
            assert!(!controls.navigate_keys(&healthy, Some(7), &Input::default(), &mut out));
        }
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in [
            "animation.property_time",
            "effects.property_time",
            "effects.navigation_times",
            "effects.rail_times",
            "navigation.property_key_store",
            "navigation.property_key_time",
            "navigation.property_key_address",
            "navigation.spatial_key_store",
            "navigation.spatial_key_address",
        ] {
            let records: Vec<_> = text
                .lines()
                .filter(|line| line.contains(&format!("site={site} ")))
                .collect();
            assert_eq!(records.len(), 2, "{site}: {text}");
            assert!(records[0].contains("WARN") && records[0].contains("count=1 "));
            assert!(records[1].contains("ERROR") && records[1].contains("count=60 "));
            assert!(records
                .iter()
                .all(|line| line.contains("object=7") && line.contains("reason=")));
        }
        if let Some(path) = std::env::var_os("MOTION_TRACK_AT_FAULT_LOG_COPY") {
            std::fs::write(path, &text).unwrap();
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
    for site in [
        "animation.property_time",
        "effects.property_time",
        "effects.navigation_times",
        "effects.rail_times",
        "navigation.property_key_store",
        "navigation.property_key_time",
        "navigation.property_key_address",
        "navigation.spatial_key_store",
        "navigation.spatial_key_address",
    ] {
        assert!(stderr.contains(&format!(
            "persistent panel degradation site={site} count=60 "
        )));
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
}

#[cfg(target_os = "windows")]
mod native;
