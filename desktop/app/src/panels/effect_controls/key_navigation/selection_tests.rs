use super::*;
use crate::panels::{animation_controls::TrackSelection, keyboard, property_focus, Panels};
use motion_core::Property;
use motion_ui::dock::Panel;

#[test]
fn property_focus_replaces_effect_and_transform_owners_and_consumes_repeated_stale_commands() {
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    let mut layer = motion_core::Layer::solid(7, "Properties", [100.; 2], [0.; 3], [1.; 4]);
    layer.transform.position.keys = [10, 90]
        .into_iter()
        .map(|frame| motion_core::Keyframe {
            frame,
            value: [frame as f32; 3],
            ease: motion_core::Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    project.layers.push(layer);
    let parameter = EffectParam {
        kind: motion_effects::ParamKind::Float,
        animatable: true,
        implemented: true,
        min: 0.,
        max: 100.,
        default: [0.; 4],
        curve: None,
        track: motion_core::Track {
            keys: [20, 80]
                .into_iter()
                .map(|frame| motion_core::Keyframe {
                    frame,
                    value: [frame as f32; 4],
                    ease: motion_core::Ease::Linear,
                    curve: None,
                    spatial: None,
                })
                .collect(),
            ..motion_core::Track::constant([0.; 4])
        },
    };
    let mut original = serde_json::json!({"root":"fixture","composition":"main","frame":40,
        "project":project,"timeline_layers":project.timeline_layers(40.)});
    original["project"]["layers"][0]["effects"] =
        serde_json::json!([{"id":1,"params":{"amount":parameter}}]);
    for locale in [Locale::En, Locale::Zh] {
        for inspector in [false, true] {
            for floating in [false, true] {
                let panel = if inspector {
                    Panel::Properties
                } else {
                    Panel::EffectControls
                };
                let mut panels = Panels {
                    locale,
                    focused: Some(panel),
                    ..Default::default()
                };
                panels.select_only(Some(7));
                let owner = floating.then_some(panel);
                for _ in 0..2 {
                    panels.animation_controls.choose_track_in(
                        &original,
                        7,
                        Property::Position,
                        None,
                    );
                    property_focus::settle(&mut panels);
                    if inspector {
                        panels
                            .properties
                            .effects
                            .choose_parameter(&original, 7, 1, "amount")
                    } else {
                        panels
                            .effect_controls
                            .choose_parameter(&original, 7, 1, "amount")
                    }
                    property_focus::settle(&mut panels);
                    assert_eq!(
                        panels
                            .animation_controls
                            .track_selection(&original, Some(7)),
                        TrackSelection::Invalid
                    );
                    let mut out = vec![];
                    keyboard::route(
                        &Input {
                            keys: vec![(Key::K, Default::default())],
                            ..Default::default()
                        },
                        &mut panels,
                        &original,
                        false,
                        owner,
                        &mut out,
                    );
                    assert!(matches!(out.as_slice(), [Action::Seek(80.)]), "{out:?}");
                    for focus in [
                        Panel::Timeline,
                        Panel::Curves,
                        Panel::Properties,
                        Panel::EffectControls,
                        Panel::Masks,
                        Panel::Vector,
                        Panel::Audio,
                        Panel::Composition,
                    ] {
                        panels.focused = Some(focus);
                        let mut out = vec![];
                        keyboard::route(
                            &Input {
                                keys: vec![(Key::K, Default::default())],
                                ..Default::default()
                            },
                            &mut panels,
                            &original,
                            false,
                            floating.then_some(focus),
                            &mut out,
                        );
                        assert!(
                            matches!(out.as_slice(), [Action::Seek(80.)]),
                            "{focus:?}: {out:?}"
                        );
                    }
                    panels.focused = Some(panel);
                    panels.animation_controls.choose_track_in(
                        &original,
                        7,
                        Property::Position,
                        None,
                    );
                    property_focus::settle(&mut panels);
                    assert!(
                        !panels.effect_controls.has_key_channel()
                            && !panels.properties.effects.has_key_channel()
                    );
                }
                for case in 0..4 {
                    if inspector {
                        panels
                            .properties
                            .effects
                            .choose_parameter(&original, 7, 1, "amount")
                    } else {
                        panels
                            .effect_controls
                            .choose_parameter(&original, 7, 1, "amount")
                    }
                    property_focus::settle(&mut panels);
                    let mut changed = original.clone();
                    match case {
                        0 => changed["root"] = "another-project".into(),
                        1 => changed["composition"] = "another-composition".into(),
                        2 => changed["project"]["layers"][0]["effects"] = serde_json::json!([]),
                        _ => changed["project"]["layers"] = serde_json::json!([]),
                    }
                    for _ in 0..2 {
                        for key in [Key::J, Key::K, Key::Delete] {
                            let mut out = vec![];
                            keyboard::route(
                                &Input {
                                    keys: vec![(key, Default::default())],
                                    ..Default::default()
                                },
                                &mut panels,
                                &changed,
                                false,
                                owner,
                                &mut out,
                            );
                            assert!(out.is_empty(),"inspector={inspector} floating={floating} case={case} key={key:?}: {out:?}");
                        }
                    }
                }
            }
        }
    }
}
