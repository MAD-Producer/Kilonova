use super::*;
use crate::panels::property_authoring;
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    AudioLevels, Ease, KeyframeTarget, TrackEdit, TrackValue,
};
use serde_json::json;
mod faults;
#[cfg(target_os = "windows")]
mod native;

fn harness(floating: bool, locale: Locale, reverse: bool) -> Harness {
    let mut h = Harness::new(floating, locale);
    h.atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
    let mut p: Project = serde_json::from_value(h.state["project"].clone()).unwrap();
    let layer = &mut p.layers[0];
    layer.timeline = Some(LayerTimeline {
        in_frame: 2,
        out_frame: 120,
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
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: Some(
            TimeRemap::new(vec![TimeRemapKey {
                frame: -6.,
                source_frame: 43.,
                easing: Default::default(),
            }])
            .unwrap(),
        ),
    });
    let mut levels = AudioLevels {
        balance: 0.3,
        ..Default::default()
    };
    for (f, v) in [(-6, -6.), (-3, -12.), (0, -18.)] {
        levels.left.upsert(f, v, Ease::Linear).unwrap();
        levels.right.upsert(f, v * 0.5, Ease::Linear).unwrap();
    }
    layer.audio_levels = Some(levels);
    let root = h._folder.path().join("mapped-project");
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::copy(
        h._folder.path().join("project/assets/tone.wav"),
        root.join("assets/tone.wav"),
    )
    .unwrap();
    motion_core::storage::save(&root, &p).unwrap();
    h.engine = Engine::start(root, motion_desktop_media::platform(), 8 << 30).unwrap();
    h.state = h.engine.seek(6.5).unwrap();
    h.panels.focused = Some(Panel::Timeline);
    h
}
fn target(channel: AudioChannel) -> KeyframeTarget {
    KeyframeTarget::Audio { object: 7, channel }
}
fn rail(h: &Harness, control: Control, slot: usize) -> [f32; 2] {
    let styles = PaintList::default().styles();
    let field = h.fields(control);
    [
        h.area().min[0]
            + styles.metric("padding.panel", 8.)
            + (slot as f32 + 0.5) * styles.metric("icon.slot", 20.),
        field.watch.min[1] + field.watch.height() * 0.5,
    ]
}
fn expected(before: &Value, channel: &str, value: f64) -> Value {
    let mut expected = before.clone();
    expected["layers"][0]["audio_levels"][channel]["keys"][1]["value"] = json!(value);
    expected
}

#[test]
fn mapped_audio_numeric_typing_preserves_signed_key_identity_and_all_other_channels_in_both_hosts()
{
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let mut h = harness(floating, locale, reverse);
                let before = h.state["project"].clone();
                h.type_number(Control::Left, "-17.25");
                assert_eq!(h.state["project"], expected(&before, "left", -17.25));
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                assert_eq!(
                    h.engine.history(1).unwrap()["project"],
                    expected(&before, "left", -17.25)
                );
                h.engine.save().unwrap();
                assert_eq!(
                    serde_json::to_value(
                        motion_core::storage::load(&h._folder.path().join("mapped-project"))
                            .unwrap()
                    )
                    .unwrap(),
                    h.state["project"]
                );
            }
        }
    }
}

#[test]
fn mapped_audio_rails_toggle_and_seek_fractional_projections_without_legacy_integer_selection() {
    for reverse in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for floating in [false, true] {
                let mut h = harness(floating, locale, reverse);
                let before = h.state["project"].clone();
                for (slot, time, local) in [
                    (0, 2., if reverse { 0 } else { -6 }),
                    (2, 11., if reverse { -6 } else { 0 }),
                ] {
                    let point = rail(&h, Control::Left, slot);
                    let actions = h.draw(pointer(point, true, false));
                    assert!(
                        matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==time),
                        "{actions:?}"
                    );
                    assert_eq!(
                        h.panels.selected_keys.local_selections(),
                        vec![(target(AudioChannel::Left), vec![local])]
                    );
                    assert!(h.panels.audio_controls.selection.is_none());
                }
                let point = rail(&h, Control::Left, 1);
                let actions = h.draw(pointer(point, true, false));
                assert!(
                    matches!(actions.as_slice(),[Action::Edit(Command::TrackAt{target:actual,frame:6.5,edit:TrackEdit::ToggleKey})] if actual==&target(AudioChannel::Left)),
                    "{actions:?}"
                );
                h.run(actions);
                let mut removed = before.clone();
                removed["layers"][0]["audio_levels"]["left"]["keys"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
                assert_eq!(h.state["project"], removed);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                // A fractional local time cannot select a nearby integer key.
                h.state = h.engine.seek(8.75).unwrap();
                let point = rail(&h, Control::Left, 1);
                assert!(h.draw(pointer(point, true, false)).is_empty());
                assert_eq!(h.engine.state().unwrap()["project"], before);
                h.state = h.engine.seek(6.5).unwrap();
                let watch = h.fields(Control::Left).watch;
                h.click(watch);
                let mut frozen = before.clone();
                frozen["layers"][0]["audio_levels"]["left"] =
                    serde_json::to_value(Track::constant(-12.)).unwrap();
                assert_eq!(h.state["project"], frozen);
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
            }
        }
    }
}

#[test]
fn mapped_audio_keyboard_navigation_projects_channels_through_the_property_clock_not_source_remap()
{
    for reverse in [false, true] {
        let mut h = harness(false, Locale::En, reverse);
        h.panels.audio_controls.active_object = Some(7);
        h.panels.audio_controls.active_control = Some(Control::Left);
        h.panels.audio_controls.context = Some((
            h.state["root"].as_str().unwrap().into(),
            h.state["composition"].as_str().unwrap().into(),
        ));
        for (command, expected) in [
            (crate::shortcuts::Command::PreviousKey, 2.),
            (crate::shortcuts::Command::NextKey, 11.),
        ] {
            let mut actions = vec![];
            assert!(shortcut(command, &mut h.panels, &h.state, &mut actions));
            assert!(
                matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==expected),
                "{actions:?}"
            );
        }
    }
}

#[test]
fn mapped_audio_scrubs_cancel_once_when_the_time_context_changes_and_static_gain_stays_global() {
    for reverse in [false, true] {
        for floating in [false, true] {
            let mut h = harness(floating, Locale::En, reverse);
            let before = h.state["project"].clone();
            let r = h.field(Control::Left);
            let start = [r.min[0] + 5., r.min[1] + 10.];
            let end = [start[0] + 20., start[1]];
            let actions = h.draw(pointer(start, true, false));
            h.run(actions);
            let actions = h.draw(pointer(end, false, false));
            assert!(actions
                .iter()
                .any(|a| matches!(a, Action::Edit(Command::TrackAt { frame: 6.5, .. }))));
            h.run(actions);
            assert_ne!(h.state["project"], before);
            h.state = h.engine.seek(11.).unwrap();
            let actions = h.draw(pointer(end, false, false));
            assert!(
                matches!(actions.as_slice(), [Action::History(4)]),
                "{actions:?}"
            );
            h.run(actions);
            assert_eq!(h.state["project"], before);
            assert!(h.draw(pointer(end, false, true)).is_empty());
            assert_eq!(h.engine.state().unwrap()["history"]["undoEntries"], 0);
            // Gain does not author a key at the current composition time.
            let s = h.engine.state().unwrap();
            assert!(authoring::numeric_time(&s, 7, Control::Gain)
                .unwrap()
                .is_none());
            assert!(matches!(
                property_authoring::at_playhead(
                    &s,
                    Command::SetAudio {
                        object: 7,
                        volume: Some(0.5),
                        muted: None
                    }
                ),
                Command::SetAudio { .. }
            ));
        }
    }
}
