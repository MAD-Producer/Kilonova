use super::*;
use crate::panels::{animation_controls::TrackSelection, keyboard, property_focus};

fn left_caption(h: &Harness) -> Rect {
    let rows = crate::panels::timeline::rows(&h.state);
    let layer = rows.iter().find(|row| row.id == 7).unwrap();
    let plan = crate::panels::timeline_layout::LayerRows::new(
        layer,
        &h.panels.timeline,
        22.,
        h.panels.locale,
    );
    let styles = PaintList::default().styles();
    let area = h.area();
    // The video layer has a transform heading before the audio subtree. Read
    // the same row plan as painting instead of using the audio-only fixture.
    row(
        &styles,
        Rect::new(
            area.min[0],
            area.min[1],
            GUTTER.min(area.width() * 0.52),
            area.height(),
        ),
        area.min[1] + 64. + (plan.audio.start + 2) as f32 * styles.metric("row.height", 22.)
            - h.panels.timeline.scroll,
        styles.metric("row.height", 22.),
    )
    .caption
}

#[test]
fn actual_audio_caption_replaces_transform_owner_and_stale_scope_cannot_delete_reused_layers() {
    for floating in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let mut h = Harness::with_video(floating, locale, true);
            for frame in [40, 80] {
                h.run(vec![Action::Edit(Command::AudioLevels {
                    object: 7,
                    action: AudioLevelsAction::Set {
                        channel: AudioChannel::Left,
                        frame,
                        value: -6.,
                        animated: Some(true),
                    },
                })]);
            }
            h.run(vec![Action::Edit(Command::Animate {
                object: 7,
                property: Property::Position,
                axis: None,
                frame: 50,
                enabled: true,
            })]);
            for frame in [50, 90] {
                h.run(vec![Action::Edit(Command::SetVector {
                    object: 7,
                    property: Property::Position,
                    frame,
                    value: [frame as f32; 3],
                })]);
            }
            h.state = h.engine.seek(22.).unwrap();
            h.panels.focused = Some(Panel::Timeline);
            let before = h.state["project"].clone();
            let owner = floating.then_some(Panel::Timeline);
            for _ in 0..2 {
                h.panels
                    .animation_controls
                    .choose_track_in(&h.state, 7, Property::Position, None);
                property_focus::settle(&mut h.panels);
                let actions = h.draw(Input {
                    keys: vec![(Key::K, Default::default())],
                    ..Default::default()
                });
                assert!(
                    matches!(actions.as_slice(), [Action::Seek(50.)]),
                    "{actions:?}"
                );
                h.click(left_caption(&h));
                assert_eq!(
                    h.panels
                        .animation_controls
                        .track_selection(&h.state, Some(7)),
                    TrackSelection::Invalid
                );
                let actions = h.draw(Input {
                    keys: vec![(Key::K, Default::default())],
                    ..Default::default()
                });
                assert!(
                    matches!(actions.as_slice(), [Action::Seek(40.)]),
                    "{actions:?}"
                );
                for focus in [
                    Panel::Properties,
                    Panel::EffectControls,
                    Panel::Vector,
                    Panel::Audio,
                    Panel::Curves,
                    Panel::Composition,
                ] {
                    h.panels.focused = Some(focus);
                    let mut out = vec![];
                    keyboard::route(
                        &Input {
                            keys: vec![(Key::K, Default::default())],
                            ..Default::default()
                        },
                        &mut h.panels,
                        &h.state,
                        false,
                        floating.then_some(focus),
                        &mut out,
                    );
                    assert!(
                        matches!(out.as_slice(), [Action::Seek(40.)]),
                        "{focus:?}: {out:?}"
                    );
                }
                h.panels.focused = Some(Panel::Timeline);
            }
            for case in 0..4 {
                h.panels.select_only(Some(7));
                h.click(left_caption(&h));
                let mut changed = h.state.clone();
                match case {
                    0 => changed["root"] = "another-project".into(),
                    1 => changed["composition"] = "another-composition".into(),
                    2 => h.panels.selected = Some(8),
                    _ => changed["project"]["layers"] = serde_json::json!([]),
                }
                for pass in 0..2 {
                    for key in [Key::J, Key::K, Key::Delete] {
                        let mut actions = vec![];
                        keyboard::route(
                            &Input {
                                keys: vec![(key, Default::default())],
                                ..Default::default()
                            },
                            &mut h.panels,
                            &changed,
                            false,
                            owner,
                            &mut actions,
                        );
                        assert!(
                            actions.is_empty(),
                            "float={floating} case={case} pass={pass} key={key:?}: {actions:?}"
                        );
                    }
                    let mut actions = vec![];
                    prepare(&mut h.panels, &changed, &Input::default(), &mut actions);
                    assert!(actions.is_empty());
                }
            }
            assert_eq!(h.engine.state().unwrap()["project"], before);
            h.engine.save().unwrap();
            assert_eq!(
                serde_json::to_value(
                    motion_core::storage::load(&h._folder.path().join("project")).unwrap()
                )
                .unwrap(),
                before
            );
        }
    }
}
