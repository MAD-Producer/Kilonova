use super::*;
use motion_core::{Ease, Engine, Layer, Project};
use motion_ui::input::Modifiers;
#[path = "error_tests.rs"]
mod error_tests;

fn property(property: Property) -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property,
        axis: None,
    }
}
fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Comparison", [100., 50.], [0.; 3], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 15,
        out_frame: 120,
        offset_frame: 15,
    });
    for (f, x, o) in [(0, 0., 0.2), (30, 100., 0.5), (60, 0., 0.8)] {
        layer
            .transform
            .position
            .upsert(f, [x, 10., 0.], Ease::Linear)
            .unwrap();
        layer.transform.opacity.upsert(f, o, Ease::Linear).unwrap();
    }
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"comparison-fixture","composition":motion_core::composition::MAIN_COMPOSITION,
        "document_generation":1,"revision":engine.revision(),"frame":30,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(30.).unwrap()})
}
fn setup(state: &Value, locale: Locale) -> Panels {
    let mut p = Panels {
        locale,
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    p.select_only(Some(7));
    p.timeline.set_graph_mode(true);
    p.timeline.comparison.sync(state, locale);
    p.timeline
        .comparison
        .add(
            state,
            locale,
            &[property(Property::Position), property(Property::Opacity)],
        )
        .unwrap();
    p.timeline.comparison.enabled = true;
    p
}
fn draw(
    p: &mut Panels,
    s: &Value,
    input: &Input,
    atlas: &mut TextAtlas,
    floating: bool,
) -> Vec<Action> {
    let mut list = PaintList::default();
    let mut actions = vec![];
    if floating {
        actions = super::super::floating_chrome(
            &mut list,
            atlas,
            Panel::Timeline,
            [1200., 620.],
            p,
            s,
            input,
        );
    } else {
        actions = super::super::chrome(
            &mut list,
            atlas,
            [1200., 850.],
            &dock::Node::dock(Panel::Timeline.dock()),
            p,
            s,
            input,
        );
    }
    actions
}
#[test]
fn collection_expands_components_deduplicates_and_keeps_unit_groups_separate_without_editing() {
    let e = fixture();
    let s = state(&e);
    let before = s.clone();
    let mut c = Controls::default();
    c.sync(&s, Locale::En);
    c.add(
        &s,
        Locale::En,
        &[property(Property::Position), property(Property::Opacity)],
    )
    .unwrap();
    c.add(&s, Locale::En, &[property(Property::Position)])
        .unwrap();
    assert_eq!(c.entries.len(), 3);
    assert_eq!(
        c.entries
            .iter()
            .map(|e| e.data.as_ref().unwrap().unit.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2
    );
    c.entries[0].visible = false;
    let mut changed = s.clone();
    changed["frame"] = serde_json::json!(80);
    c.sync(&changed, Locale::En);
    assert!(!c.entries[0].visible);
    assert_eq!(s, before);
    assert_eq!(e.history_diagnostics(), fixture().history_diagnostics());
    changed["document_generation"] = serde_json::json!(2);
    c.sync(&changed, Locale::En);
    assert!(
        c.entries.is_empty(),
        "reused layer IDs in another document cannot inherit a display set"
    );
}
#[test]
fn local_offsets_physical_units_and_signed_separated_axes_follow_actual_samples() {
    let mut e = fixture();
    let s = state(&e);
    let position = data::load(&s, &property(Property::Position), Locale::En).unwrap();
    assert_eq!(position.frames, [15, 45, 75]);
    assert_eq!(position.sample(30., 0, 30., false).unwrap(), 50.);
    assert_eq!(position.sample(30., 0, 30., true).unwrap(), 100.);
    let opacity = data::load(&s, &property(Property::Opacity), Locale::En).unwrap();
    assert!((opacity.sample(30., 0, 30., false).unwrap() - 35.).abs() < 0.0001);
    assert!((opacity.sample(30., 0, 30., true).unwrap() - 30.).abs() < 0.0001);
    let mut project = e.project().clone();
    project.layers[0].transform.position.separate().unwrap();
    e = Engine::new(project).unwrap();
    let s = state(&e);
    let x = data::load(
        &s,
        &KeyframeTarget::Property {
            object: 7,
            property: Property::Position,
            axis: Some(motion_core::Axis::X),
        },
        Locale::Zh,
    )
    .unwrap();
    assert_eq!(x.dimensions, [0]);
    assert_eq!(x.sample(60., 0, 30., true).unwrap(), -100.);
    assert_eq!(x.unit, position.unit);
}
#[test]
fn native_effect_values_do_not_share_axes_without_the_same_parameter_identity() {
    let mut s = state(&fixture());
    let track = serde_json::json!({"value":[0.,0.,0.,0.],"keys":[
        {"frame":0,"value":[0.,0.,0.,0.],"ease":"linear"},
        {"frame":30,"value":[2.,0.,0.,0.],"ease":"linear"}]});
    s["project"]["layers"][0]["effects"] = serde_json::json!([
        {"id":1,"effect":"effect_a","hash":"hash_a","params":{"amount":{"kind":"float","animatable":true,"implemented":true,"min":0.,"max":1.,"track":track}}},
        {"id":2,"effect":"effect_b","hash":"hash_b","params":{"amount":{"kind":"float","animatable":true,"implemented":true,"min":0.,"max":1.,"track":track}}}]);
    let a = data::load(
        &s,
        &KeyframeTarget::Effect {
            object: 7,
            effect: 1,
            param: "amount".into(),
        },
        Locale::En,
    )
    .unwrap();
    let b = data::load(
        &s,
        &KeyframeTarget::Effect {
            object: 7,
            effect: 2,
            param: "amount".into(),
        },
        Locale::En,
    )
    .unwrap();
    assert_ne!(a.unit, b.unit);
    assert_eq!(a.dimensions, [0]);
    assert_eq!(a.sample(30., 0, 30., false).unwrap(), 1.);
    assert_eq!(a.sample(40., 0, 30., false).unwrap(), 1.);
    assert_eq!(a.sample(40., 0, 30., true).unwrap(), 0.);
    assert!(
        !a.unit.title(false).contains("hash_a"),
        "runtime identities are not product labels"
    );
}
#[test]
fn clicks_select_the_authored_channel_and_single_key_drag_uses_one_atomic_existing_command() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut engine = fixture();
            let s = state(&engine);
            let before = engine.project().clone();
            let mut panels = setup(&s, locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            assert!(draw(&mut panels, &s, &Input::default(), &mut atlas, floating).is_empty());
            let point = panels
                .timeline
                .comparison
                .marks
                .iter()
                .find(|(c, f, _)| c.target == property(Property::Opacity) && *f == 45)
                .unwrap()
                .2;
            let press = Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            };
            let mut actions = draw(&mut panels, &s, &press, &mut atlas, floating);
            assert_eq!(
                panels.selected_keys.primary().unwrap().target,
                property(Property::Opacity)
            );
            let track = panels.timeline.zoom_track.unwrap();
            let dx = track.width() / 120. * 10.;
            let moved = [point[0] + dx, point[1]];
            actions.extend(draw(
                &mut panels,
                &s,
                &Input {
                    mouse: moved,
                    held: Some(MouseButton::Left),
                    pointer_travel: dx,
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &mut atlas,
                floating,
            ));
            assert_eq!(engine.project(), &before, "preview must not edit");
            actions.extend(draw(
                &mut panels,
                &s,
                &Input {
                    mouse: moved,
                    released: Some((MouseButton::Left, moved)),
                    ..Default::default()
                },
                &mut atlas,
                floating,
            ));
            let commands = actions
                .into_iter()
                .filter_map(|a| {
                    if let Action::EditorAction { commands, .. } = a {
                        Some(commands)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(commands.len(), 1, "one release creates one transaction");
            engine
                .apply_batch(commands.into_iter().next().unwrap())
                .unwrap();
            assert!(engine.project().layers[0]
                .transform
                .opacity
                .keys
                .iter()
                .any(|k| k.frame == 40));
            assert_eq!(
                engine.project().layers[0].transform.position,
                before.layers[0].transform.position
            );
            let edited = engine.project().clone();
            let temp = tempfile::tempdir().unwrap();
            motion_core::storage::save(temp.path(), &edited).unwrap();
            assert_eq!(motion_core::storage::load(temp.path()).unwrap(), edited);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &edited);
        }
    }
}
#[test]
fn samples_reuse_the_revision_cache_while_visibility_and_graph_modes_leave_the_project_unchanged() {
    let engine = fixture();
    let s = state(&engine);
    let mut p = setup(&s, Locale::En);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(&mut p, &s, &Input::default(), &mut atlas, false);
    let old = p.timeline.comparison.entries[0]
        .points
        .as_ref()
        .unwrap()
        .1
        .clone();
    let mut later = s.clone();
    later["frame"] = serde_json::json!(33);
    draw(&mut p, &later, &Input::default(), &mut atlas, false);
    assert!(Arc::ptr_eq(
        &old,
        &p.timeline.comparison.entries[0].points.as_ref().unwrap().1
    ));
    let rect = p.timeline.comparison.legend_hits[0].1;
    let point = [rect.min[0] + 5., rect.min[1] + 5.];
    assert!(draw(
        &mut p,
        &later,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        &mut atlas,
        false
    )
    .is_empty());
    assert!(!p.timeline.comparison.entries[0].visible);
    p.timeline.speed_graph = true;
    assert!(draw(&mut p, &later, &Input::default(), &mut atlas, false).is_empty());
    assert_eq!(engine.project(), &fixture().project().clone());
    assert_eq!(s["project"], later["project"]);
}

#[test]
fn stale_keys_partial_add_limits_and_removed_dimensions_preserve_authored_state() {
    let s = state(&fixture());
    let mut p = setup(&s, Locale::En);
    p.selected_keys
        .apply(
            &s,
            [super::super::key_selection::Key {
                target: property(Property::Opacity),
                frame: 45,
            }],
            super::super::key_selection::Mode::Replace,
        )
        .unwrap();
    let mut other = s.clone();
    other["document_generation"] = serde_json::json!(2);
    assert!(selected(&p, &other).is_none());
    let original = p
        .timeline
        .comparison
        .entries
        .iter()
        .map(|e| e.channel.clone())
        .collect::<Vec<_>>();
    assert!(matches!(
        p.timeline.comparison.add(
            &s,
            Locale::En,
            &[
                property(Property::Scale),
                KeyframeTarget::Property {
                    object: 999,
                    property: Property::Position,
                    axis: None
                }
            ]
        ),
        Err(Unavailable::Missing)
    ));
    assert!(
        original
            == p.timeline
                .comparison
                .entries
                .iter()
                .map(|e| e.channel.clone())
                .collect::<Vec<_>>()
    );
    let layer = s["project"]["layers"][0].clone();
    let mut many = s.clone();
    let layers = (100..230)
        .map(|id| {
            let mut l = layer.clone();
            l["id"] = serde_json::json!(id);
            l
        })
        .collect::<Vec<_>>();
    many["project"]["layers"] = serde_json::json!(layers);
    let targets = (100..230)
        .map(|object| KeyframeTarget::Property {
            object,
            property: Property::Opacity,
            axis: None,
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        p.timeline.comparison.add(&many, Locale::En, &targets),
        Err(Unavailable::Limit)
    ));
    assert!(
        original
            == p.timeline
                .comparison
                .entries
                .iter()
                .map(|e| e.channel.clone())
                .collect::<Vec<_>>()
    );
    let mut three = s.clone();
    three["project"]["layers"][0]["three_d"] = serde_json::json!(true);
    let mut c = Controls::default();
    c.sync(&three, Locale::En);
    c.add(&three, Locale::En, &[property(Property::Position)])
        .unwrap();
    assert_eq!(c.entries.len(), 3);
    let mut two = s.clone();
    two["revision"] = serde_json::json!(1);
    c.sync(&two, Locale::En);
    assert!(c.entries[2]
        .data
        .as_ref()
        .err()
        .is_some_and(|e| matches!(e.as_ref(), Unavailable::Unsupported)));
    assert_eq!(s, state(&fixture()));
}

#[test]
fn compact_controls_keep_a_visible_plot_and_own_popup_keys_and_closing_clicks() {
    let engine = fixture();
    let s = state(&engine);
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let mut p = setup(&s, locale);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            p.selected_keys
                .apply(
                    &s,
                    [super::super::key_selection::Key {
                        target: property(Property::Opacity),
                        frame: 45,
                    }],
                    super::super::key_selection::Mode::Replace,
                )
                .unwrap();
            let original_selection = p.selected_keys.selections();
            let render = |p: &mut Panels, input: &Input, atlas: &mut TextAtlas| {
                let mut list = PaintList::default();
                if floating {
                    super::super::floating_chrome(
                        &mut list,
                        atlas,
                        Panel::Timeline,
                        [720., 300.],
                        p,
                        &s,
                        input,
                    )
                } else {
                    super::super::chrome(
                        &mut list,
                        atlas,
                        [900., 600.],
                        &dock::Node::dock(Panel::Timeline.dock()),
                        p,
                        &s,
                        input,
                    )
                }
            };
            assert!(render(&mut p, &Input::default(), &mut atlas).is_empty());
            assert!(!p.timeline.comparison.marks.is_empty());
            let anchor = p.timeline.comparison.test_anchor(2);
            let point = [anchor.min[0] + 3., anchor.min[1] + 3.];
            assert!(render(
                &mut p,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(p.timeline.comparison.channels_open);
            assert_eq!(p.selected_keys.selections(), original_selection);
            let selected = p.selected_keys.selections();
            // Focus Add, Clear, then the first checkbox. Enter must not rename
            // the layer, move a key or leak Space to playback.
            for _ in 0..2 {
                assert!(render(
                    &mut p,
                    &Input {
                        keys: vec![(Key::Tab, Default::default())],
                        ..Default::default()
                    },
                    &mut atlas
                )
                .is_empty());
            }
            assert_eq!(p.timeline.comparison.bar_focus, Some(5));
            assert!(render(
                &mut p,
                &Input {
                    keys: vec![(Key::Space, Default::default())],
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(!p.timeline.comparison.entries[0].visible);
            assert_eq!(p.selected_keys.selections(), selected);
            let outside = [20., 120.];
            assert!(render(
                &mut p,
                &Input {
                    mouse: outside,
                    pressed: Some((MouseButton::Left, outside)),
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(!p.timeline.comparison.channels_open);
            let mode = p.timeline.comparison.test_anchor(1);
            let mode_point = [mode.min[0] + 3., mode.min[1] + 3.];
            assert!(render(
                &mut p,
                &Input {
                    mouse: mode_point,
                    pressed: Some((MouseButton::Left, mode_point)),
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert_eq!(p.selected_keys.selections(), original_selection);
            assert!(render(
                &mut p,
                &Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert_eq!(p.selected_keys.selections(), selected);
            assert!(render(
                &mut p,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(p.timeline.comparison.channels_open);
            assert!(render(
                &mut p,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(!p.timeline.comparison.channels_open);
            p.timeline.comparison.bar_focus = Some(0);
            assert!(render(
                &mut p,
                &Input {
                    keys: vec![(Key::Space, Default::default())],
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
            assert!(!p.timeline.comparison.enabled);
            assert_eq!(engine.project(), fixture().project());
        }
    }
}

#[test]
fn wide_legend_reserves_the_same_time_viewport_for_ruler_keys_zoom_and_navigation() {
    let mut project = fixture().project().clone();
    project.layers[0]
        .transform
        .opacity
        .upsert(90, 0.7, Ease::Linear)
        .unwrap();
    let engine = Engine::new(project).unwrap();
    let s = state(&engine);
    let before = engine.project().clone();
    let mut p = setup(&s, Locale::En);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    assert!(draw(&mut p, &s, &Input::default(), &mut atlas, false).is_empty());
    let track = p.timeline.zoom_track.unwrap();
    assert!(
        p.timeline
            .comparison
            .marks
            .iter()
            .any(|(c, frame, _)| c.target == property(Property::Opacity) && *frame == 105),
        "fit must retain the late key beside the legend"
    );
    for (_, frame, point) in &p.timeline.comparison.marks {
        let expected = track.min[0] + *frame as f32 / 120. * track.width();
        assert!(
            (point[0] - expected).abs() < 0.001,
            "key x must follow the shared ruler mapping"
        );
        assert!(point[0] <= track.max[0]);
    }
    assert_eq!(engine.project(), &before);
}
