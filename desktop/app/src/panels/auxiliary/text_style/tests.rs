use super::*;
use motion_ui::input::Modifiers;
#[cfg(target_os = "windows")]
mod native;

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn press(rect: Rect) -> Input {
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
fn draw(
    panel: Panel,
    floating: bool,
    p: &mut Panels,
    state: &Value,
    atlas: &mut TextAtlas,
    input: &Input,
) -> Vec<Action> {
    if floating {
        crate::panels::floating_chrome(
            &mut PaintList::default(),
            atlas,
            panel,
            [300., 460.],
            p,
            state,
            input,
        )
    } else {
        crate::panels::chrome(
            &mut PaintList::default(),
            atlas,
            [300., 460.],
            &dock::Node::dock(panel.dock()),
            p,
            state,
            input,
        )
    }
}
fn state() -> Value {
    let mut layer = motion_core::Layer::solid(7, "Text", [320., 48.], [0.; 3], [1.; 4]);
    layer.content = motion_core::Content::Text {
        text: "First\nSecond".into(),
        font: motion_media::fonts::FontStore::builtin_id(),
        style: TextStyle::new(24., 320),
        color: [1.; 4],
        raster_asset: 1,
    };
    serde_json::json!({"root":"text-style","composition":"main","revision":0,"document_generation":1,
        "project":{"layers":[layer],"fonts":[]}})
}
fn edit_number(
    panel: Panel,
    floating: bool,
    field: usize,
    p: &mut Panels,
    s: &Value,
    atlas: &mut TextAtlas,
    value: &str,
) {
    draw(panel, floating, p, s, atlas, &Input::default());
    let rect = p.auxiliary.text_style.fields[field];
    let click = press(rect);
    let point = click.mouse;
    assert!(draw(panel, floating, p, s, atlas, &click).is_empty());
    assert!(draw(
        panel,
        floating,
        p,
        s,
        atlas,
        &Input {
            mouse: point,
            released: Some((MouseButton::Left, point)),
            ..Default::default()
        }
    )
    .is_empty());
    assert!(draw(
        panel,
        floating,
        p,
        s,
        atlas,
        &Input {
            keys: vec![(
                Key::A,
                Modifiers {
                    control: true,
                    ..Default::default()
                }
            )],
            ..Default::default()
        }
    )
    .is_empty());
    assert!(draw(
        panel,
        floating,
        p,
        s,
        atlas,
        &Input {
            text: value.into(),
            ..Default::default()
        }
    )
    .is_empty());
    assert!(draw(panel, floating, p, s, atlas, &key(Key::Enter)).is_empty());
}
#[test]
fn text_and_paragraph_controls_edit_one_draft_and_explicit_apply_in_both_hosts_and_languages() {
    for locale in [Locale::Zh, Locale::En] {
        for floating in [false, true] {
            let s = state();
            let mut p = Panels::default();
            p.locale = locale;
            p.selected = Some(7);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            edit_number(Panel::Character, floating, 0, &mut p, &s, &mut atlas, "48");
            assert_eq!(
                p.auxiliary
                    .text_style
                    .draft
                    .as_ref()
                    .unwrap()
                    .style
                    .font_size,
                48.
            );
            edit_number(Panel::Paragraph, floating, 1, &mut p, &s, &mut atlas, "16");
            assert_eq!(
                p.auxiliary
                    .text_style
                    .draft
                    .as_ref()
                    .unwrap()
                    .style
                    .indent_left,
                16.
            );
            let rect = p.auxiliary.text_style.alignment_rect.unwrap();
            assert!(draw(
                Panel::Paragraph,
                floating,
                &mut p,
                &s,
                &mut atlas,
                &press(rect)
            )
            .is_empty());
            assert!(draw(
                Panel::Paragraph,
                floating,
                &mut p,
                &s,
                &mut atlas,
                &key(Key::Down)
            )
            .is_empty());
            assert!(draw(
                Panel::Paragraph,
                floating,
                &mut p,
                &s,
                &mut atlas,
                &key(Key::Enter)
            )
            .is_empty());
            assert_eq!(
                p.auxiliary
                    .text_style
                    .draft
                    .as_ref()
                    .unwrap()
                    .style
                    .alignment,
                TextAlign::Center
            );
            let rect = p.auxiliary.text_style.apply_rect.unwrap();
            let actions = draw(
                Panel::Paragraph,
                floating,
                &mut p,
                &s,
                &mut atlas,
                &press(rect),
            );
            let [Action::TextApply {
                request, source, ..
            }] = actions.as_slice()
            else {
                panic!("{actions:?}")
            };
            assert_eq!(request["style"]["font_size"], 48.);
            assert_eq!(request["style"]["indent_left"], 16.);
            assert_eq!(request["style"]["alignment"], "center");
            assert_eq!(*source, crate::layer_clipboard::KeyframeSource::of(&s));
            assert_eq!(s, state(), "draft controls never mutate the input snapshot");
        }
    }
}
#[test]
fn cancellation_and_invalid_or_stale_drafts_never_dispatch_partial_edits() {
    let s = state();
    let mut p = Panels::default();
    p.selected = Some(7);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    edit_number(Panel::Character, false, 1, &mut p, &s, &mut atlas, "-1");
    let rect = p.auxiliary.text_style.apply_rect.unwrap();
    assert!(draw(
        Panel::Character,
        false,
        &mut p,
        &s,
        &mut atlas,
        &press(rect)
    )
    .is_empty());
    assert!(p
        .auxiliary
        .text_style
        .draft
        .as_ref()
        .unwrap()
        .message
        .is_some());
    let rect = p.auxiliary.text_style.cancel_rect.unwrap();
    assert!(draw(
        Panel::Character,
        false,
        &mut p,
        &s,
        &mut atlas,
        &press(rect)
    )
    .is_empty());
    assert!(p.auxiliary.text_style.draft.is_none());
    edit_number(Panel::Character, false, 0, &mut p, &s, &mut atlas, "48");
    let mut changed = s.clone();
    changed["revision"] = 1.into();
    draw(
        Panel::Character,
        false,
        &mut p,
        &changed,
        &mut atlas,
        &Input::default(),
    );
    let rect = p.auxiliary.text_style.apply_rect.unwrap();
    assert!(draw(
        Panel::Character,
        false,
        &mut p,
        &changed,
        &mut atlas,
        &press(rect)
    )
    .is_empty());
    assert_eq!(
        p.auxiliary
            .text_style
            .draft
            .as_ref()
            .unwrap()
            .style
            .font_size,
        48.
    );
    assert!(p
        .auxiliary
        .text_style
        .draft
        .as_ref()
        .unwrap()
        .message
        .is_some());
}

#[test]
fn text_style_discrete_fields_retain_invalid_input_and_ime_focus_stay_local() {
    for floating in [false, true] {
        let s = state();
        let mut p = Panels::default();
        p.selected = Some(7);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        edit_number(
            Panel::Paragraph,
            floating,
            0,
            &mut p,
            &s,
            &mut atlas,
            "280.5",
        );
        let controls = &p.auxiliary.text_style;
        assert_eq!(controls.draft.as_ref().unwrap().style.width, 320);
        let editor = &controls
            .numeric
            .as_ref()
            .expect("invalid integer retains focus")
            .1
            .text;
        assert!(editor.active());
        assert_eq!(editor.text, "280.5");
        assert!(editor.error.is_some());
        let rect = controls.apply_rect.unwrap();
        assert!(draw(
            Panel::Paragraph,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &press(rect)
        )
        .is_empty());
        draw(
            Panel::Paragraph,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &key(Key::Escape),
        );
        assert!(p.auxiliary.text_style.numeric.is_none());
        edit_number(Panel::Paragraph, floating, 0, &mut p, &s, &mut atlas, "280");
        assert_eq!(
            p.auxiliary.text_style.draft.as_ref().unwrap().style.width,
            280
        );
        draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &Input::default(),
        );
        let rect = p.auxiliary.text_style.source_rect.unwrap();
        draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &press(rect),
        );
        let chord = |key| Input {
            keys: vec![(
                key,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &chord(Key::A),
        );
        draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &Input {
                text: "中文\nEdited".into(),
                ..Default::default()
            },
        );
        assert_eq!(
            p.auxiliary.text_style.draft.as_ref().unwrap().text.text,
            "中文\nEdited"
        );
        assert!(draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &chord(Key::Z)
        )
        .is_empty());
        assert_eq!(
            p.auxiliary.text_style.draft.as_ref().unwrap().text.text,
            "First\nSecond"
        );
        draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &chord(Key::Y),
        );
        let mut composing = chord(Key::Enter);
        composing.composing = true;
        assert!(draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &composing
        )
        .is_empty());
        assert!(p.auxiliary.text_style.draft.as_ref().unwrap().text.active());
        assert!(draw(
            Panel::Character,
            floating,
            &mut p,
            &s,
            &mut atlas,
            &Input {
                focus_lost: true,
                ..Default::default()
            }
        )
        .is_empty());
        assert!(!p.auxiliary.text_style.draft.as_ref().unwrap().text.active());
        assert_eq!(
            p.auxiliary.text_style.draft.as_ref().unwrap().text.text,
            "中文\nEdited"
        );
        assert_eq!(s, state());
    }
}
