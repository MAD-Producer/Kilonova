use super::*;
use crate::i18n::Locale;
use crate::panels::{chrome, floating_chrome};
use motion_ui::{dock::Node, input::Modifiers, paint::PaintList, text::TextAtlas};

fn chord() -> Input {
    Input {
        keys: vec![(
            Key::M,
            Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
        )],
        ..Default::default()
    }
}

fn message(locale: Locale) -> &'static str {
    match locale {
        Locale::Zh => "此版本暂不支持视频导出",
        Locale::En => "Video export is unavailable in this build",
    }
}

fn state() -> Value {
    serde_json::json!({"root":"project", "composition":"main", "revision":1,
        "frame":0, "project":{"frames":120, "fps":30, "layers":[]}})
}

#[test]
fn export_is_dispatched_or_explained_for_each_locale_and_window_owner() {
    let state = state();
    let before = state.clone();
    for locale in [Locale::En, Locale::Zh] {
        for panel in [
            Panel::Timeline,
            Panel::Composition,
            Panel::Nodes,
            Panel::NodePreview,
        ] {
            for owner in [None, Some(panel)] {
                let mut panels = Panels {
                    locale,
                    focused: Some(panel),
                    ..Default::default()
                };
                let mut out = Vec::new();
                route(
                    &chord(),
                    &mut panels,
                    &state,
                    matches!(panel, Panel::Nodes | Panel::NodePreview),
                    owner,
                    &mut out,
                );
                if cfg!(feature = "ffmpeg") {
                    assert!(matches!(out.as_slice(), [Action::ExportMovie]));
                    assert!(panels.message.is_none());
                } else {
                    assert!(out.is_empty());
                    assert_eq!(panels.message.as_deref(), Some(message(locale)));
                    panels.message = None;
                    route(
                        &chord(),
                        &mut panels,
                        &state,
                        matches!(panel, Panel::Nodes | Panel::NodePreview),
                        owner,
                        &mut out,
                    );
                    assert_eq!(
                        panels.message.as_deref(),
                        Some(message(locale)),
                        "later press must also explain unavailability"
                    );
                    assert!(out.is_empty());
                }
                assert_eq!(state, before);
            }
        }
    }
}

#[test]
fn ime_text_ownership_and_repeat_do_not_replace_feedback_or_export() {
    for locale in [Locale::En, Locale::Zh] {
        for owner in [None, Some(Panel::Timeline)] {
            for suppressed in 0..4 {
                let mut panels = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    message: Some("keep current".into()),
                    ..Default::default()
                };
                let mut input = chord();
                match suppressed {
                    0 => input.composing = true,
                    1 => input.ime_consumed = true,
                    2 => panels.project.search = true,
                    _ => input.key_repeats = vec![true],
                }
                let mut out = Vec::new();
                route(&input, &mut panels, &state(), false, owner, &mut out);
                assert!(out.is_empty());
                assert_eq!(panels.message.as_deref(), Some("keep current"));
            }
        }
    }
}

#[cfg(not(feature = "ffmpeg"))]
#[test]
fn unavailable_feedback_is_painted_in_main_and_floating_status_bars() {
    use crate::panels::{helpers::label, Rect};
    for locale in [Locale::En, Locale::Zh] {
        for size in [[900., 600.], [1600., 1000.]] {
            for floating in [false, true] {
                let mut panels = Panels {
                    locale,
                    focused: Some(Panel::Timeline),
                    ..Default::default()
                };
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let mut paint = PaintList::default();
                let state = state();
                let before = state.clone();
                let mut out = Vec::new();
                route(
                    &chord(),
                    &mut panels,
                    &state,
                    false,
                    floating.then_some(Panel::Timeline),
                    &mut out,
                );
                let actions = if floating {
                    floating_chrome(
                        &mut paint,
                        &mut atlas,
                        Panel::Timeline,
                        size,
                        &mut panels,
                        &state,
                        &Input::default(),
                    )
                } else {
                    chrome(
                        &mut paint,
                        &mut atlas,
                        size,
                        &Node::dock(Panel::Timeline.dock()),
                        &mut panels,
                        &state,
                        &Input::default(),
                    )
                };
                assert!(out.is_empty());
                assert!(actions.is_empty());
                assert_eq!(state, before);
                let mut expected = PaintList::default();
                let height = if floating { 22. } else { 20. };
                label(
                    &mut expected,
                    &mut atlas,
                    Rect::new(8., size[1] - height, size[0] - 16., height),
                    message(locale),
                    true,
                );
                assert!(
                    !expected.glyphs.is_empty(),
                    "locale={locale:?} size={size:?} floating={floating} diagnostics={}",
                    font_diagnostics(&mut atlas, &expected)
                );
                let actual: Vec<_> = paint
                    .glyphs
                    .iter()
                    .filter(|g| g.position[1] >= size[1] - 20.)
                    .collect();
                assert_eq!(
                    format!("{actual:?}"),
                    format!("{:?}", expected.glyphs),
                    "locale={locale:?} size={size:?} floating={floating}"
                );
            }
        }
    }
}

fn font_diagnostics(atlas: &mut TextAtlas, paint: &PaintList) -> String {
    let size = paint
        .styles()
        .resolve("label.small.muted", Default::default())
        .unwrap()
        .font_size;
    let supported = ['此', '版', '本', '中'].map(|c| (c, atlas.supports(c)));
    let glyph = atlas.glyph_at('此', size);
    let families = atlas
        .families
        .iter()
        .filter(|family| {
            [
                "PingFang", "Heiti", "Hiragino", "Songti", "Noto", "CJK", "YaHei",
            ]
            .iter()
            .any(|part| family.contains(part))
        })
        .take(16)
        .map(|family| family.chars().take(64).collect::<String>())
        .collect::<Vec<_>>();
    format!("family={:?} supported={supported:?} font_size={} line_height={} ascent={} glyph_uv={:?} glyph_offset={:?} glyph_advance={} candidates={families:?}",
        atlas.selected_family, atlas.font_size(), atlas.line_height, atlas.ascent,
        glyph.uv, glyph.offset, glyph.advance)
}
