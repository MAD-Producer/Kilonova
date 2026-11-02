//! Real mini controls must preserve continuous times and stable identities.
use super::mapped_tests::{apply, click, draw, pointer, quadratic};
use super::*;
use motion_core::{Engine, KeyframeTarget, Layer, Project, SpatialKeyEdit, Track};
use serde_json::json;
#[cfg(target_os = "windows")]
mod native;
mod worker;

fn target() -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    }
}
fn fixture(mode: &str, roving: bool) -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut l = Layer::solid(
        7,
        "Continuous mini",
        [100.; 2],
        [320., 180., 0.],
        [0.2, 0.5, 0.8, 1.],
    );
    l.transform.position = Track::constant([320., 180., 0.]);
    for (f, x) in [(0, 0.), (4, 1.), (10, 4.), (20, 8.)] {
        l.transform
            .position
            .upsert(f, [320. + x, 180., 0.], Ease::Linear)
            .unwrap();
    }
    l.transform.position.enable_roving(&[4]).unwrap();
    if !roving {
        l.transform
            .position
            .spatial_animation
            .as_mut()
            .unwrap()
            .set_fixed_time(2, 4.125)
            .unwrap();
    }
    if mode != "plain" {
        use motion_core::layer_time::*;
        l.timeline = Some(motion_core::LayerTimeline {
            in_frame: 0,
            out_frame: 120,
            offset_frame: 8,
        });
        l.time_mapping = Some(LayerTimeMapping {
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
    }
    p.layers.push(l);
    Engine::new(p).unwrap()
}
fn projected(e: &Engine, local: f64) -> f64 {
    e.project().layers[0]
        .property_composition_frame(local)
        .unwrap()
}
fn state(e: &Engine, frame: f64) -> Value {
    json!({"root":"spatial-mini","composition":motion_core::MAIN_COMPOSITION,"document_generation":1,
        "revision":e.revision(),"frame":frame,"project":e.project(),
        "timeline_layers":e.project().timeline_layers(frame).unwrap(),
        "sampledLayers":[{"id":7,"position":e.project().layers[0].transform.position.sample(e.project().layers[0].local_frame(frame)).unwrap()}]})
}
fn ready(s: &Value) -> Controls {
    let mut c = Controls::default();
    c.select_address(s, 7, Property::Position, None, time::Address::Spatial(2));
    c
}
fn interval(e: &Engine) -> [time::KeyTime; 2] {
    let a = e.project().layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap();
    let keys: Vec<_> = a.prepared().keys().collect();
    [1, 2].map(|i| time::KeyTime {
        address: time::Address::Spatial(keys[i].0),
        local: keys[i].1,
    })
}

#[test]
fn mini_spatial_navigation_move_delete_add_and_presets_never_alias_an_integer_frame() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mode in ["plain", "forward", "reverse"] {
        for locale in [Locale::En, Locale::Zh] {
            let mut e = fixture(mode, false);
            let before = e.snapshot();
            let at = projected(&e, 4.125);
            let s = state(&e, at);
            for (point, id, local) in [
                (
                    [28., 126.],
                    if mode == "reverse" { 3 } else { 1 },
                    if mode == "reverse" { 10. } else { 0. },
                ),
                (
                    [60., 126.],
                    if mode == "reverse" { 1 } else { 3 },
                    if mode == "reverse" { 0. } else { 10. },
                ),
            ] {
                let mut c = ready(&s);
                let a = draw(&mut c, &mut atlas, &s, &click(point), locale);
                assert!(
                    matches!(a.as_slice(),[Action::Seek(f)] if *f==projected(&e,local)),
                    "{a:?}"
                );
                assert_eq!(c.take_spatial_selection().unwrap().unwrap().id, id);
                assert!(c.user_key().is_none());
            }
            let mut c = ready(&s);
            let a = draw(&mut c, &mut atlas, &s, &click([230., 182.]), locale);
            assert!(
                matches!(a.as_slice(),[Action::Edit(Command::SpatialKeyframes {target:t,edit:SpatialKeyEdit::Curve {key:2,..}})] if *t==target()),
                "{a:?}"
            );
            apply(&mut e, a);
            let mut expected = before.clone();
            expected.layers[0]
                .transform
                .position
                .spatial_animation
                .as_mut()
                .unwrap()
                .set_easing(
                    2,
                    Easing {
                        ease: Ease::Linear,
                        curve: Some(Curve {
                            space: CurveSpace::Progress,
                            shape: CurveShape::Quadratic {
                                control: [0.5, 0.],
                                start: 0.,
                                end: 1.,
                            },
                        }),
                    },
                )
                .unwrap();
            assert_eq!(e.project(), &expected);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            let s = state(&e, projected(&e, 4.625));
            let mut c = ready(&s);
            let a = draw(&mut c, &mut atlas, &s, &click([100., 154.]), locale);
            assert!(
                matches!(a.as_slice(),[Action::Edit(Command::SpatialKeyframes {edit:SpatialKeyEdit::Time {key:2,time},..})] if *time==s["frame"].as_f64().unwrap())
            );
            apply(&mut e, a);
            assert_eq!(interval(&e)[0].local, 4.625);
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            let mut c = ready(&s);
            let a = draw(&mut c, &mut atlas, &s, &click([450., 154.]), locale);
            apply(&mut e, a);
            assert_eq!(
                e.project().layers[0]
                    .transform
                    .position
                    .spatial_animation
                    .as_ref()
                    .unwrap()
                    .keys()
                    .iter()
                    .map(|k| k.id)
                    .collect::<Vec<_>>(),
                [1, 3, 4]
            );
            assert_eq!(c.take_spatial_selection(), Some(None));
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
            let mut c = ready(&s);
            let a = draw(&mut c, &mut atlas, &s, &click([450., 98.]), locale);
            assert!(
                matches!(a.as_slice(),[Action::EditBatch(commands)] if commands.len()==2 && commands.iter().all(|c|matches!(c,Command::TrackAt {frame,..} if *frame==s["frame"].as_f64().unwrap())))
            );
            apply(&mut e, a);
            let a = e.project().layers[0]
                .transform
                .position
                .spatial_animation
                .as_ref()
                .unwrap();
            assert_eq!(a.keys().len(), 5);
            assert!(a.prepared().keys().any(|(id, t)| id == 5 && t == 4.625));
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}

#[test]
fn spatial_curve_rejections_preserve_full_project_and_never_relax_roving_constraints() {
    let mut e = fixture("forward", true);
    let before = e.snapshot();
    for (key, easing) in [
        (0, quadratic()),
        (99, quadratic()),
        (4, quadratic()),
        (1, quadratic()),
        (2, quadratic()),
    ] {
        assert!(e
            .apply(Command::SpatialKeyframes {
                target: target(),
                edit: SpatialKeyEdit::Curve { key, easing }
            })
            .is_err());
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
    }
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let s = state(&e, projected(&e, 2.5));
    let mut c = ready(&s);
    assert!(draw(&mut c, &mut atlas, &s, &click([230., 182.]), Locale::Zh).is_empty());
    assert_eq!(e.project(), &before);
    let cmd = Command::SpatialKeyframes {
        target: target(),
        edit: SpatialKeyEdit::Curve {
            key: 3,
            easing: quadratic(),
        },
    };
    assert!(!cmd.composition_transaction());
    let encoded = serde_json::to_value(&cmd).unwrap();
    let parsed = motion_core::CommandRegistry::parse(&encoded.to_string()).unwrap();
    let Command::InComposition {
        composition,
        command,
    } = &parsed[0]
    else {
        panic!("missing composition routing: {:?}", parsed[0]);
    };
    assert_eq!(composition, motion_core::MAIN_COMPOSITION);
    assert_eq!(serde_json::to_value(command).unwrap(), encoded);
    let catalogue = motion_core::CommandRegistry::catalogue();
    let schema = &catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "spatial_keyframes")
        .unwrap()["inputSchema"]["properties"]["edit"]["oneOf"];
    let variant = schema
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["properties"]["kind"]["const"] == "curve")
        .unwrap();
    assert_eq!(variant["required"], json!(["kind", "key", "easing"]));
    assert_eq!(variant["properties"]["key"]["minimum"], 1);
}

#[test]
fn fixed_spatial_curve_changes_the_sample_at_exact_local_time_and_preserves_all_key_times() {
    for mode in ["plain", "forward", "reverse"] {
        let mut e = fixture(mode, false);
        let old = e.snapshot();
        let times: Vec<_> = old.layers[0]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .prepared()
            .keys()
            .collect();
        e.apply(Command::SpatialKeyframes {
            target: target(),
            edit: SpatialKeyEdit::Curve {
                key: 2,
                easing: quadratic(),
            },
        })
        .unwrap();
        let t = &e.project().layers[0].transform.position;
        assert_eq!(
            t.spatial_animation
                .as_ref()
                .unwrap()
                .prepared()
                .keys()
                .collect::<Vec<_>>(),
            times
        );
        // Quadratic control (0.5,0.25) has progress 0.375 at normalized t=0.5.
        // Geometry is x=321..324, hence the independently computed x=322.125.
        assert_eq!(t.sample((4.125 + 10.) / 2.).unwrap(), [322.125, 180., 0.]);
        let s = state(&e, projected(&e, (4.125 + 10.) / 2.));
        assert_eq!(
            s["sampledLayers"][0]["position"],
            json!([322.125, 180., 0.])
        );
        let mut bad = quadratic();
        if let Some(Curve {
            shape: CurveShape::Quadratic { control, .. },
            ..
        }) = &mut bad.curve
        {
            control[0] = f64::NAN;
        }
        let before = e.snapshot();
        assert!(e
            .apply(Command::SpatialKeyframes {
                target: target(),
                edit: SpatialKeyEdit::Curve {
                    key: 2,
                    easing: bad
                }
            })
            .is_err());
        assert_eq!(e.project(), &before);
    }
}

#[test]
fn actual_mini_readout_and_fit_selection_keep_fractional_spatial_intervals_in_both_directions() {
    use crate::panels::{
        graph_ranges::{Bounds, Controls as Ranges},
        graph_view::Range,
        key_selection::{AddressedKey, Mode, Set, SpatialKey},
    };
    for mode in ["plain", "forward", "reverse"] {
        for locale in [Locale::En, Locale::Zh] {
            let e = fixture(mode, false);
            let s = state(&e, projected(&e, 4.125));
            let [start, end] = interval(&e);
            let mut view = mini::View::default();
            let t = 0.37;
            let local = start.local + (end.local - start.local) * t;
            let at = projected(&e, local);
            let (title, lines) = view.actual_readout_interval(
                &s,
                locale,
                &target(),
                start,
                Some(end),
                t,
                false,
                "T".into(),
                vec![],
            );
            assert!(title.contains(&format!("{at:.2} f")), "{title} {lines:?}");
            let data = crate::panels::curve_comparison::data::load(&s, &target(), locale).unwrap();
            for &component in &data.dimensions {
                let value = data.sample(at, component, 30., false).unwrap();
                assert!(lines.iter().any(|l| l.contains(
                    &crate::panels::graph_ranges::value_label(
                        f64::from(value),
                        &data.unit.title(false)
                    )
                )));
            }
            let mut set = Set::default();
            set.apply_addressed(
                &s,
                [
                    AddressedKey::Spatial(SpatialKey {
                        target: target(),
                        id: 2,
                    }),
                    AddressedKey::Spatial(SpatialKey {
                        target: target(),
                        id: 3,
                    }),
                ],
                Mode::Replace,
            )
            .unwrap();
            let mut ranges = Ranges::default();
            let context = mini::Context {
                ranges: &mut ranges,
                selection: &set,
                effect_key: None,
                focused: None,
            };
            let all = Bounds {
                x: Range::new(0., 1.).unwrap(),
                y: Range::new(-0.1, 1.1).unwrap(),
            };
            assert_eq!(
                mini::selection_interval(
                    &s,
                    &target(),
                    start,
                    Some(end),
                    &context,
                    None,
                    Easing::default(),
                    all
                ),
                Some(all)
            );
            assert_eq!(
                view.actual_readout_interval(
                    &s,
                    locale,
                    &target(),
                    end,
                    None,
                    0.5,
                    false,
                    "T".into(),
                    vec![]
                ),
                ("T".into(), vec![])
            );
        }
    }
}
