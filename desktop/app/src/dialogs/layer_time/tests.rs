use super::*;
use motion_core::{AudioAsset, AudioClip, Engine, LayerTimeline, Project, Track};
use motion_ui::input::Modifiers;

pub(crate) fn fixture(audio: bool) -> Engine {
    let mut p = Project::new(640, 360, 30, 240).unwrap();
    for id in [7, 8] {
        let mut l = Layer::solid(id, "Time", [64.; 2], [0.; 3], [1.; 4]);
        l.timeline = Some(LayerTimeline {
            in_frame: 10,
            out_frame: 90,
            offset_frame: 10,
        });
        l.transform.position = Track::constant([0.; 3]);
        l.transform
            .position
            .upsert(3, [10., 20., 0.], motion_core::Ease::Hold)
            .unwrap();
        p.layers.push(l);
    }
    if audio {
        p.layers.truncate(1);
        p.audio_assets.push(AudioAsset {
            id: 1,
            path: "assets/tone.wav".into(),
            bytes: 64,
            mime: "audio/wav".into(),
            track: 0,
            sample_rate: 48000,
            channels: 2,
            sample_frames: 384000,
            duration_us: 8_000_000,
        });
        p.layers[0].content = Content::Audio {
            audio: AudioClip::new(1),
        };
        p.layers[0].size = [0.; 2];
        p.layers[0].transform = motion_core::Transform::new([0.; 3]);
    }
    Engine::new(p).unwrap()
}
pub(crate) fn state(e: &Engine) -> Value {
    serde_json::json!({"root":"layer-time","composition":e.project().composition_id,"document_generation":3,
        "revision":e.revision(),"frame":30.,"project":e.project(),"timeline_layers":e.project().timeline_layers(30.).unwrap()})
}
fn dialog(s: &Value, objects: Vec<u64>) -> Dialog {
    Dialog::layer_time(s, crate::layer_clipboard::KeyframeSource::of(s), objects, 0).unwrap()
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn draw(d: &mut Dialog, atlas: &mut TextAtlas, input: Input, locale: Locale) -> Option<Event> {
    d.render(
        &mut PaintList::default(),
        atlas,
        [1000., 800.],
        &input,
        locale,
    )
}
#[test]
fn decimal_duration_percentages_are_exact_bounded_and_unchanged_rationals_are_lossless() {
    for (text, ratio) in [
        ("150", [3, 2]),
        ("100.000000", [1, 1]),
        ("0.1", [1, 1000]),
        ("100000", [1000, 1]),
        ("125.25", [501, 400]),
    ] {
        assert_eq!(percentage(text).unwrap().ratio(), ratio);
    }
    for bad in [
        "0",
        "-100",
        "NaN",
        "inf",
        "1e3",
        "100000.1",
        "0.01",
        "1.0000001",
        "",
        "1.2.3",
    ] {
        assert!(percentage(bad).is_err(), "{bad}");
    }
    let mut e = fixture(false);
    e.apply(Command::LayerTime {
        object: 7,
        action: LayerTimeAction::Stretch {
            stretch: TimeStretch::new(77, 89).unwrap(),
            anchor_frame: 10.,
            resize_clip: false,
        },
    })
    .unwrap();
    let mut d = dialog(&state(&e), vec![7]);
    assert!(d
        .layer_time
        .as_mut()
        .unwrap()
        .commands()
        .unwrap()
        .is_empty());
    assert_eq!(
        e.project().layers[0]
            .time_mapping
            .as_ref()
            .unwrap()
            .clock
            .stretch()
            .ratio(),
        [77, 89]
    );
}
#[test]
fn multi_layer_stretch_reverse_preserve_author_keys_and_have_one_complete_undo() {
    for locale in [Locale::Zh, Locale::En] {
        let mut e = fixture(false);
        let before = e.snapshot();
        let s = state(&e);
        let mut d = dialog(&s, vec![7, 8]);
        let editor = d.layer_time.as_mut().unwrap();
        editor.stretch.text = "150".into();
        editor.direction = 2;
        let commands = editor.commands().unwrap();
        assert_eq!(commands.len(), 4);
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        e.apply_batch(commands).unwrap();
        for (old, new) in before.layers.iter().zip(&e.project().layers) {
            assert_eq!(old.transform, new.transform);
            assert_eq!(
                new.interval(240),
                motion_core::ClipRange {
                    in_frame: 10.,
                    out_frame: 130.
                }
            );
            assert_eq!(
                new.time_mapping.as_ref().unwrap().clock.direction(),
                TimeDirection::Reverse
            );
            assert_eq!(new.checked_local_frame(10.).unwrap(), 80.);
            assert_eq!(new.checked_local_frame(130.).unwrap(), 0.);
        }
        let after = e.snapshot();
        let folder = tempfile::tempdir().unwrap();
        motion_core::storage::save(folder.path(), &after).unwrap();
        assert_eq!(motion_core::storage::load(folder.path()).unwrap(), after);
        assert!(e.undo().unwrap());
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        let mut cancelled = dialog(&s, vec![7, 8]);
        cancelled.layer_time.as_mut().unwrap().stretch.text = "200".into();
        assert!(matches!(
            draw(
                &mut cancelled,
                &mut TextAtlas::from_system(12.).unwrap(),
                key(Key::Escape),
                locale
            ),
            Some(Event::Cancelled)
        ));
        assert_eq!(e.project(), &before);
    }
}
#[test]
fn remap_table_edits_source_time_preserves_custom_easing_and_rejects_collisions_atomically() {
    let mut e = fixture(true);
    let before = e.snapshot();
    let s = state(&e);
    let mut d = dialog(&s, vec![7]);
    let editor = d.layer_time.as_mut().unwrap();
    assert!(editor.remappable());
    editor.remap = 1;
    editor.select_key(1).unwrap();
    editor.key_edit.as_mut().unwrap().source.text = "20.5".into();
    editor.keys[0].easing = Easing {
        ease: motion_core::Ease::Hold,
        curve: None,
    };
    let commands = editor.commands().unwrap();
    assert_eq!(commands.len(), 1);
    e.apply_batch(commands).unwrap();
    assert_eq!(
        e.project().layers[0].source_time_sample(50.).unwrap().frame,
        0.
    );
    assert_eq!(
        e.project().layers[0].source_time_sample(90.).unwrap().frame,
        20.5
    );
    assert_eq!(e.project().layers[0].transform, before.layers[0].transform);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    editor.key_edit.as_mut().unwrap().frame.text = "0".into();
    assert!(editor.commands().is_err());
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    editor.key_edit.as_mut().unwrap().frame.text = "NaN".into();
    assert!(editor.commands().is_err());
    editor.remap = 0;
    assert!(
        editor.commands().unwrap().is_empty(),
        "disabled remap retains but ignores invalid draft"
    );
    editor.remap = 1;
    editor.key_edit.as_mut().unwrap().frame.text = "80".into();
    editor.add_key().unwrap();
    assert_eq!(editor.keys.len(), 3);
    assert_eq!(editor.keys[2].frame, 20.);
    assert!(editor.add_key().is_err());
}
#[test]
fn modal_keyboard_ime_repeat_focus_and_popup_ownership_do_not_commit_or_leak() {
    let e = fixture(true);
    let s = state(&e);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut d = dialog(&s, vec![7]);
        draw(&mut d, &mut atlas, key(Key::Tab), locale);
        draw(
            &mut d,
            &mut atlas,
            Input {
                text: "150".into(),
                ..Default::default()
            },
            locale,
        );
        assert!(draw(&mut d, &mut atlas, key(Key::Enter), locale).is_none());
        for input in [
            Input {
                composing: true,
                ..key(Key::Enter)
            },
            Input {
                ime_consumed: true,
                ..key(Key::Enter)
            },
            Input {
                focus_lost: true,
                ..key(Key::Enter)
            },
            Input {
                key_repeats: vec![true],
                ..key(Key::Enter)
            },
        ] {
            assert!(draw(&mut d, &mut atlas, input, locale).is_none());
        }
        let Some(Event::LayerTime { commands, .. }) =
            draw(&mut d, &mut atlas, key(Key::Enter), locale)
        else {
            panic!("missing Apply")
        };
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0],
            Command::LayerTime {
                action: LayerTimeAction::Stretch { .. },
                ..
            }
        ));
        let mut d = dialog(&s, vec![7]);
        d.layer_time.as_mut().unwrap().menus[0].open = true;
        assert!(draw(&mut d, &mut atlas, key(Key::Escape), locale).is_none());
        assert!(matches!(
            draw(&mut d, &mut atlas, key(Key::Escape), locale),
            Some(Event::Cancelled)
        ));
        assert!(!e.can_undo());
    }
}
#[test]
fn selection_identity_locks_readonly_and_whole_batch_constraints_are_enforced() {
    let e = fixture(false);
    let s = state(&e);
    for fault in [
        "locked",
        "readonly",
        "document_generation",
        "revision",
        "root",
        "composition",
    ] {
        let mut bad = s.clone();
        match fault {
            "locked" => bad["project"]["layers"][1]["locked"] = true.into(),
            "readonly" => bad["observing"] = true.into(),
            field => bad[field] = Value::Null,
        };
        assert!(!eligible(&bad, &[7, 8]), "{fault}");
        assert!(Dialog::layer_time(
            &bad,
            crate::layer_clipboard::KeyframeSource::of(&bad),
            vec![7, 8],
            0
        )
        .is_err());
    }
    assert!(Dialog::layer_time(
        &s,
        crate::layer_clipboard::KeyframeSource::of(&s),
        vec![7, 7],
        0
    )
    .is_err());
    let mut d = dialog(&s, vec![7, 8]);
    let editor = d.layer_time.as_mut().unwrap();
    editor.remap = 1;
    assert!(
        editor.commands().is_err(),
        "source remap requires one source-bearing layer"
    );
    editor.remap = 0;
    editor.stretch.text = "100000".into();
    assert!(editor.commands().is_err());
    assert!(!e.can_undo());
    editor.stretch.text = "150".into();
    assert!(editor.commands().is_ok());
}

#[test]
fn custom_remap_curves_survive_open_and_numeric_edit_without_roundtrip_flattening() {
    let mut e = fixture(true);
    let curve = motion_core::Curve {
        space: motion_core::CurveSpace::Progress,
        shape: motion_core::CurveShape::Cubic {
            control1: [0.25, 0.05],
            control2: [0.75, 0.95],
            start: 0.,
            end: 1.,
        },
    };
    e.apply(Command::LayerTime {
        object: 7,
        action: LayerTimeAction::Remap {
            remap: Some(
                TimeRemap::new(vec![
                    TimeRemapKey {
                        frame: -0.5,
                        source_frame: 2.25,
                        easing: Easing {
                            ease: motion_core::Ease::Linear,
                            curve: Some(curve),
                        },
                    },
                    TimeRemapKey {
                        frame: 80.5,
                        source_frame: 90.25,
                        easing: Easing::default(),
                    },
                ])
                .unwrap(),
            ),
        },
    })
    .unwrap();
    let before = e.snapshot();
    let mut d = dialog(&state(&e), vec![7]);
    let editor = d.layer_time.as_mut().unwrap();
    editor.remap = 1;
    assert!(editor.commands().unwrap().is_empty());
    editor.select_key(1).unwrap();
    editor.key_edit.as_mut().unwrap().source.text = "100.75".into();
    e.apply_batch(editor.commands().unwrap()).unwrap();
    let keys = e.project().layers[0]
        .time_mapping
        .as_ref()
        .unwrap()
        .remap
        .as_ref()
        .unwrap()
        .keys();
    assert_eq!(
        keys[0],
        before.layers[0]
            .time_mapping
            .as_ref()
            .unwrap()
            .remap
            .as_ref()
            .unwrap()
            .keys()[0]
    );
    assert_eq!(keys[0].easing.curve, Some(curve));
    assert_eq!(keys[1].source_frame, 100.75);
    assert!(e.undo().unwrap());
    assert_eq!(e.project(), &before);
}
