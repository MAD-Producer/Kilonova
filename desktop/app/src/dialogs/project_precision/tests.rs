use super::*;
use motion_ui::input::Modifiers;

fn state(precision: ChannelPrecision) -> Value {
    let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
    project.working_precision = precision;
    serde_json::json!({"project":project})
}
fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn click(rect: Rect) -> Input {
    let point = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(dialog: &mut Dialog, atlas: &mut TextAtlas, input: Input, locale: Locale) -> Option<Event> {
    let mut paint = PaintList::default();
    let event = dialog.render(&mut paint, atlas, [900., 600.], &input, locale);
    assert!(!paint.shapes.is_empty() || matches!(event, Some(Event::Cancelled)));
    event
}

#[test]
fn four_distinct_options_are_draft_only_until_default_apply_in_both_locales() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for (index, precision) in VALUES.into_iter().enumerate() {
            let source = state(ChannelPrecision::Unorm8);
            let original = source.clone();
            let mut dialog = Dialog::project_precision(&source).unwrap();
            assert!(draw(&mut dialog, &mut atlas, key(Key::Space), locale).is_none());
            for _ in 0..index {
                assert!(draw(&mut dialog, &mut atlas, key(Key::Down), locale).is_none());
            }
            assert!(
                draw(&mut dialog, &mut atlas, key(Key::Enter), locale).is_none(),
                "choosing must not also Apply"
            );
            assert_eq!(source, original);
            assert!(
                matches!(draw(&mut dialog, &mut atlas, key(Key::Enter), locale),
                Some(Event::ProjectPrecision(selected)) if selected == precision)
            );
        }
        for precision in VALUES {
            let mut dialog = Dialog::project_precision(&state(precision)).unwrap();
            assert!(
                matches!(draw(&mut dialog, &mut atlas, key(Key::Enter), locale),
                Some(Event::ProjectPrecision(selected)) if selected == precision),
                "opening must read the saved mode"
            );
        }
    }
}

#[test]
fn popup_owns_footer_escape_focus_loss_and_ime_without_accidental_apply() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut dialog = Dialog::project_precision(&state(ChannelPrecision::Float32)).unwrap();
    let sheet = PaintList::default().styles();
    let layout = Layout::new(&sheet, [900., 600.]);
    assert!(draw(&mut dialog, &mut atlas, key(Key::Space), Locale::En).is_none());
    assert!(dialog.project_precision.as_ref().unwrap().dropdown.open);
    assert!(draw(&mut dialog, &mut atlas, click(layout.apply), Locale::En).is_none());
    assert!(!dialog.project_precision.as_ref().unwrap().dropdown.open);
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
            key_repeats: vec![true],
            ..key(Key::Enter)
        },
        Input {
            composing: true,
            ..key(Key::Escape)
        },
    ] {
        assert!(draw(&mut dialog, &mut atlas, input, Locale::En).is_none());
        assert!(!dialog.project_precision.as_ref().unwrap().dropdown.open);
    }
    draw(&mut dialog, &mut atlas, key(Key::Space), Locale::En);
    assert!(draw(&mut dialog, &mut atlas, key(Key::Escape), Locale::En).is_none());
    assert!(!dialog.project_precision.as_ref().unwrap().dropdown.open);
    draw(&mut dialog, &mut atlas, key(Key::Space), Locale::En);
    assert!(
        draw(
            &mut dialog,
            &mut atlas,
            Input {
                focus_lost: true,
                ..Default::default()
            },
            Locale::En
        )
        .is_none()
    );
    assert!(!dialog.project_precision.as_ref().unwrap().dropdown.open);
    assert!(matches!(
        draw(&mut dialog, &mut atlas, click(layout.cancel), Locale::En),
        Some(Event::Cancelled)
    ));
}

#[test]
fn required_project_precision_is_never_inferred_from_missing_or_invalid_metadata() {
    for invalid in [
        Value::Null,
        serde_json::json!({"project":{}}),
        serde_json::json!({"project":{"working_precision":"16bit"}}),
    ] {
        assert!(
            Dialog::project_precision(&invalid)
                .err()
                .unwrap()
                .contains("project precision field is invalid")
        );
    }
}
