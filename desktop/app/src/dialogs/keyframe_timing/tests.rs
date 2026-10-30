use super::*;
use motion_core::{
    Command, Ease, Engine, Keyframe, KeyframeSelection, KeyframeTarget, Layer, Project, Property,
};
use motion_ui::input::Modifiers;

fn fixture() -> (Engine, Value, Vec<KeyframeSelection>) {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Timing", [64.; 2], [0.; 3], [1.; 4]);
    for frame in [0, 30, 60] {
        layer.transform.position.keys.push(Keyframe {
            auto_bezier: false,
            frame,
            value: [frame as f32 / 30. * 100., 0., 0.],
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        });
        layer.transform.opacity.keys.push(Keyframe {
            auto_bezier: false,
            frame,
            value: frame as f32 / 60.,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        });
    }
    project.layers.push(layer);
    let engine = Engine::new(project).unwrap();
    let state = serde_json::json!({"root":"timing-project","composition":engine.project().composition_id,"document_generation":1,"revision":engine.revision(),"project":engine.project()});
    let selected = [Property::Position, Property::Opacity]
        .map(|property| KeyframeSelection {
            target: KeyframeTarget::Property {
                object: 7,
                property,
                axis: None,
            },
            frames: vec![30],
        })
        .to_vec();
    (engine, state, selected)
}
fn dialog(state: &Value, selected: Vec<KeyframeSelection>) -> Dialog {
    Dialog::keyframe_timing(
        state,
        crate::layer_clipboard::KeyframeSource::of(state),
        selected,
        0,
    )
    .unwrap()
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn draw(dialog: &mut Dialog, atlas: &mut TextAtlas, input: Input, locale: Locale) -> Option<Event> {
    dialog.render(
        &mut PaintList::default(),
        atlas,
        [1280., 800.],
        &input,
        locale,
    )
}

#[test]
fn defaults_use_property_units_and_unit_switches_keep_independent_uncommitted_drafts() {
    let (engine, state, selected) = fixture();
    let before = engine.snapshot();
    let mut d = dialog(&state, selected);
    let editor = d.keyframe_timing.as_mut().unwrap();
    assert_eq!(editor.units, 0);
    assert_eq!(editor.profiles[0].units.len(), 2);
    assert_eq!(
        editor.profiles[0].incoming.speed, None,
        "mixed physical speeds must not display an arbitrary common value"
    );
    assert_eq!(editor.profiles[1].incoming.speed, Some(1.));
    editor.speeds[0][0].text = "12.5".into();
    editor.units = 1;
    editor.speeds[1][0].text = "0.25".into();
    assert_eq!(
        editor.settings(Locale::En).unwrap().incoming.unwrap().speed,
        Some(0.25)
    );
    editor.units = 0;
    assert_eq!(
        editor.settings(Locale::Zh).unwrap().incoming.unwrap().speed,
        Some(12.5)
    );
    assert_eq!(engine.project(), &before);
    assert!(!engine.can_undo());
}

#[test]
fn keyboard_numeric_edit_emits_one_typed_batch_only_after_apply_and_cancel_has_no_history() {
    for locale in [Locale::En, Locale::Zh] {
        let (mut engine, state, selected) = fixture();
        let before = engine.snapshot();
        let mut d = dialog(&state, selected.clone());
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        // Tab follows interpolation, units, incoming checkbox, incoming speed.
        for _ in 0..4 {
            assert!(draw(&mut d, &mut atlas, key(Key::Tab), locale).is_none());
        }
        assert!(
            draw(
                &mut d,
                &mut atlas,
                Input {
                    text: "25".into(),
                    ..Default::default()
                },
                locale
            )
            .is_none()
        );
        assert!(
            draw(&mut d, &mut atlas, key(Key::Enter), locale).is_none(),
            "first Enter finishes the field only"
        );
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
        let Some(Event::KeyframeTiming {
            selections,
            settings,
            ..
        }) = draw(&mut d, &mut atlas, key(Key::Enter), locale)
        else {
            panic!("missing Apply event")
        };
        assert_eq!(selections, selected);
        assert_eq!(settings.units, VelocityUnits::PropertyPerSecond);
        assert_eq!(settings.incoming.unwrap().speed, Some(25.));
        assert_eq!(settings.outgoing.unwrap().speed, None);
        engine
            .apply(Command::KeyframeTiming {
                selections,
                settings,
            })
            .unwrap();
        assert_ne!(engine.project(), &before);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
        let mut cancelled = dialog(&state, selected);
        cancelled.keyframe_timing.as_mut().unwrap().speeds[0][0].text = "99".into();
        assert!(matches!(
            draw(&mut cancelled, &mut atlas, key(Key::Escape), locale),
            Some(Event::Cancelled)
        ));
        assert_eq!(engine.project(), &before);
        assert!(!engine.can_undo());
    }
}

#[test]
fn invalid_numbers_and_no_changes_remain_local_and_ime_cannot_apply_the_draft() {
    let (engine, state, selected) = fixture();
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut d = dialog(&state, selected.clone());
        for bad in ["NaN", "inf", "1e999", "x"] {
            d.keyframe_timing.as_mut().unwrap().speeds[0][0].text = bad.into();
            assert!(draw(&mut d, &mut atlas, key(Key::Enter), locale).is_none());
            assert!(d.keyframe_timing.as_ref().unwrap().error.is_some());
        }
        d.keyframe_timing.as_mut().unwrap().speeds[0][0].text = "20".into();
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
        ] {
            assert!(draw(&mut d, &mut atlas, input, locale).is_none());
        }
        let mut d = dialog(&state, selected.clone());
        assert!(draw(&mut d, &mut atlas, key(Key::Enter), locale).is_none());
        assert!(d.keyframe_timing.as_ref().unwrap().error.is_some());
    }
    assert!(!engine.can_undo());
}

#[test]
fn dropdown_enter_and_escape_belong_to_the_popup_before_the_dialog() {
    let (_, state, selected) = fixture();
    let mut d = dialog(&state, selected);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    assert!(draw(&mut d, &mut atlas, key(Key::Tab), Locale::Zh).is_none());
    assert!(draw(&mut d, &mut atlas, key(Key::Space), Locale::Zh).is_none());
    assert!(d.keyframe_timing.as_ref().unwrap().interpolation_menu.open);
    assert!(draw(&mut d, &mut atlas, key(Key::Escape), Locale::Zh).is_none());
    assert!(!d.keyframe_timing.as_ref().unwrap().interpolation_menu.open);
    assert!(draw(&mut d, &mut atlas, key(Key::Space), Locale::Zh).is_none());
    assert!(draw(&mut d, &mut atlas, key(Key::Down), Locale::Zh).is_none());
    assert!(draw(&mut d, &mut atlas, key(Key::Enter), Locale::Zh).is_none());
    assert!(!d.keyframe_timing.as_ref().unwrap().interpolation_menu.open);
    assert_eq!(d.keyframe_timing.as_ref().unwrap().interpolation, 1);
}

#[test]
fn stale_identity_and_missing_keys_cannot_open_an_edit_and_layout_stays_inside_small_hosts() {
    let (_, state, selected) = fixture();
    let source = crate::layer_clipboard::KeyframeSource::of(&state);
    let mut stale = state.clone();
    stale["revision"] = 99.into();
    assert!(
        Dialog::keyframe_timing(&stale, source, selected.clone(), 0)
            .err()
            .unwrap()
            .contains("stale")
    );
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for size in [[900., 600.], [1600., 1000.], [360., 320.]] {
        let mut d = dialog(&state, selected.clone());
        let mut list = PaintList::default();
        let sheet = list.styles();
        let layout = Layout::new(&sheet, size);
        assert!(layout.card.min[0] >= 0. && layout.card.min[1] >= 0.);
        assert!(layout.card.max[0] <= size[0] && layout.card.max[1] <= size[1]);
        assert!(layout.body.max[1] <= layout.error.min[1]);
        assert!(layout.error.max[1] < layout.apply.min[1]);
        assert!(
            d.render(&mut list, &mut atlas, size, &Input::default(), Locale::En)
                .is_none()
        );
    }
}

#[test]
fn keyboard_focus_scrolls_small_floating_forms_and_vector_speed_errors_stay_in_the_dialog() {
    let (_, state, selected) = fixture();
    let mut d = dialog(&state, selected);
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    let size = [360., 320.];
    for _ in 0..7 {
        assert!(
            d.render(
                &mut PaintList::default(),
                &mut atlas,
                size,
                &key(Key::Tab),
                Locale::Zh
            )
            .is_none()
        );
    }
    let editor = d.keyframe_timing.as_ref().unwrap();
    assert_eq!(editor.focus, Some(6));
    assert!(editor.scroll.offset > 0.);
    assert!(
        d.render(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &Input {
                text: "-10".into(),
                ..Default::default()
            },
            Locale::Zh
        )
        .is_none()
    );
    assert!(
        d.render(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &key(Key::Enter),
            Locale::Zh
        )
        .is_none()
    );
    assert!(
        d.render(
            &mut PaintList::default(),
            &mut atlas,
            size,
            &key(Key::Enter),
            Locale::Zh
        )
        .is_none()
    );
    assert!(
        d.keyframe_timing
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("向量速率不能为负")
    );
}
