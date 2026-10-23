use super::*;
use crate::i18n::Locale;
use motion_core::{Ease, Engine, Keyframe, Layer, Project, Property};
use motion_ui::{
    dock,
    input::{Modifiers, Rect},
    paint::PaintList,
    text::TextAtlas,
};

fn notice(locale: Locale) -> &'static str {
    match locale {
        Locale::Zh => "删除目标：图层",
        Locale::En => "Delete target: layers",
    }
}
fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    for id in [7, 8, 9] {
        let mut layer = Layer::solid(id, "Delete fixture", [100.; 2], [0.; 3], [1.; 4]);
        layer.transform.position.keys = vec![Keyframe {
            frame: 30,
            value: [20., 10., 0.],
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        }];
        project.layers.push(layer);
    }
    Engine::new(project).unwrap()
}
fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"delete-feedback","composition":motion_core::composition::MAIN_COMPOSITION,
        "revision":engine.revision(),"frame":30,"project":engine.project(),
        "timeline_layers":engine.project().timeline_layers(30.).unwrap()})
}
fn setup(locale: Locale, focus: Panel, multiple: bool) -> Panels {
    Panels {
        locale,
        focused: Some(focus),
        selected: Some(7),
        selected_layers: if multiple { vec![7, 8] } else { vec![7] },
        ..Default::default()
    }
}
fn input(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}
fn invoke(p: &mut Panels, s: &Value, input: &Input, owner: Option<Panel>) -> Vec<Action> {
    let mut out = Vec::new();
    route(input, p, s, false, owner, &mut out);
    out
}
fn apply(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::History(0) => {
                assert!(engine.undo().unwrap());
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }
}

#[test]
fn layer_delete_reports_its_target_and_real_single_and_multi_delete_undo_once() {
    for locale in [Locale::En, Locale::Zh] {
        for focus in [Panel::Timeline, Panel::Composition] {
            for owner in [None, Some(focus)] {
                for multiple in [false, true] {
                    for key in [Key::Delete, Key::Backspace] {
                        let mut engine = fixture();
                        let original = engine.project().clone();
                        let mut p = setup(locale, focus, multiple);
                        let actions = invoke(&mut p, &state(&engine), &input(key), owner);
                        assert_eq!(actions.len(), 1);
                        assert!(
                            matches!(actions[0], Action::Edit(Edit::Delete { object: 7 }))
                                || matches!(&actions[0],Action::EditBatch(commands) if commands.len()==2 && commands.iter().all(|command|matches!(command,Edit::Delete{object:7|8})))
                        );
                        apply(&mut engine, actions);
                        assert_eq!(engine.project().layers.len(), if multiple { 1 } else { 2 });
                        assert!(engine.project().layers.iter().any(|layer| layer.id == 9));
                        let saved = tempfile::tempdir().unwrap();
                        motion_core::storage::save(saved.path(), engine.project()).unwrap();
                        let reopened = motion_core::storage::load(saved.path()).unwrap();
                        assert_eq!(&reopened, engine.project());
                        assert!(engine.undo().unwrap());
                        assert_eq!(engine.project(), &original);
                        assert!(!engine.can_undo());
                        assert_eq!(
                            p.message.as_deref(),
                            Some(notice(locale)),
                            "{locale:?} {focus:?} {owner:?} multiple={multiple} key={key:?}"
                        );
                    }
                }
            }
        }
    }
}

fn draw(
    p: &mut Panels,
    s: &Value,
    input: &Input,
    floating: bool,
    size: [f32; 2],
    atlas: &mut TextAtlas,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        super::super::floating_chrome(&mut paint, atlas, Panel::Timeline, size, p, s, input)
    } else {
        super::super::chrome(
            &mut paint,
            atlas,
            size,
            &dock::Node::dock(Panel::Timeline.dock()),
            p,
            s,
            input,
        )
    };
    (paint, actions)
}
fn assert_status(
    paint: &PaintList,
    atlas: &mut TextAtlas,
    text: &str,
    size: [f32; 2],
    floating: bool,
) {
    let mut expected = PaintList::default();
    let height = if floating { 22. } else { 20. };
    super::super::helpers::label(
        &mut expected,
        atlas,
        Rect::new(8., size[1] - height, size[0] - 16., height),
        text,
        true,
    );
    assert!(!expected.glyphs.is_empty());
    assert_eq!(
        expected.glyphs.len(),
        text.chars().filter(|c| !c.is_whitespace()).count()
    );
    let actual = paint
        .glyphs
        .iter()
        .filter(|glyph| glyph.position[1] >= size[1] - 20.)
        .collect::<Vec<_>>();
    assert_eq!(format!("{actual:?}"), format!("{:?}", expected.glyphs));
}

#[test]
fn real_main_and_floating_status_paint_the_target_and_keep_it_after_state_refresh() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for floating in [false, true] {
            for size in [[900., 600.], [1600., 1000.]] {
                let mut engine = fixture();
                let original = engine.project().clone();
                let mut p = setup(locale, Panel::Timeline, false);
                let (paint, actions) = draw(
                    &mut p,
                    &state(&engine),
                    &input(Key::Delete),
                    floating,
                    size,
                    &mut atlas,
                );
                assert_eq!(actions.len(), 1);
                assert_status(&paint, &mut atlas, notice(locale), size, floating);
                apply(&mut engine, actions);
                let (paint, actions) = draw(
                    &mut p,
                    &state(&engine),
                    &Input::default(),
                    floating,
                    size,
                    &mut atlas,
                );
                assert!(actions.is_empty());
                assert_status(&paint, &mut atlas, notice(locale), size, floating);
                assert_eq!(p.selected, None);
                assert!(engine.undo().unwrap());
                assert_eq!(engine.project(), &original);
            }
        }
    }
}

#[test]
fn key_delete_clears_only_the_previous_layer_notice_and_does_not_delete_a_layer() {
    for old_locale in [Locale::En, Locale::Zh] {
        for locale in [Locale::En, Locale::Zh] {
            let mut engine = fixture();
            let original = engine.project().clone();
            let s = state(&engine);
            let mut p = setup(locale, Panel::Timeline, false);
            p.message = Some(notice(old_locale).into());
            p.animation_controls
                .select_key_in(&s, 7, Property::Position, None, 30);
            let actions = invoke(&mut p, &s, &input(Key::Delete), None);
            assert!(matches!(
                actions.as_slice(),
                [Action::Edit(Edit::DeleteKey {
                    object: 7,
                    frame: 30,
                    ..
                })]
            ));
            assert!(p.message.is_none());
            apply(&mut engine, actions);
            assert_eq!(engine.project().layers.len(), 3);
            assert!(engine.project().layers[0]
                .transform
                .position
                .keys
                .is_empty());
            assert!(engine.undo().unwrap());
            assert_eq!(engine.project(), &original);
            p.message = Some("retain another message".into());
            p.animation_controls
                .select_key_in(&s, 7, Property::Position, None, 30);
            assert_eq!(invoke(&mut p, &s, &input(Key::Delete), None).len(), 1);
            assert_eq!(p.message.as_deref(), Some("retain another message"));
        }
    }
}

#[test]
fn blocked_inputs_and_existing_action_prefixes_do_not_publish_a_layer_target() {
    let engine = fixture();
    for locale in [Locale::En, Locale::Zh] {
        for case in 0..7 {
            let mut p = setup(locale, Panel::Timeline, false);
            p.message = Some("retain another message".into());
            let mut s = state(&engine);
            let mut event = input(Key::Delete);
            match case {
                0 => event.composing = true,
                1 => event.ime_consumed = true,
                2 => event.key_repeats = vec![true],
                3 => p.timeline.timeline_search = true,
                4 => s["project"]["layers"][0]["locked"] = true.into(),
                5 => {
                    p.selected = None;
                    p.selected_layers.clear();
                }
                6 => event.keys[0].1.control = true,
                _ => unreachable!(),
            }
            assert!(invoke(&mut p, &s, &event, None).is_empty(), "case {case}");
            assert_eq!(p.message.as_deref(), Some("retain another message"));
        }
        let mut p = setup(locale, Panel::Timeline, false);
        p.message = Some("retain another message".into());
        let mut prefix = vec![Action::Edit(Edit::Delete { object: 9 })];
        route(
            &Input::default(),
            &mut p,
            &state(&engine),
            false,
            None,
            &mut prefix,
        );
        assert_eq!(prefix.len(), 1);
        assert_eq!(p.message.as_deref(), Some("retain another message"));
    }
    assert!(!engine.can_undo());
}

#[test]
fn remapped_delete_keeps_its_target_and_undo_retires_only_our_notice() {
    use crate::shortcuts::bindings::Bindings;
    use std::sync::Arc;
    for locale in [Locale::En, Locale::Zh] {
        let mut engine = fixture();
        let original = engine.project().clone();
        let mut p = setup(locale, Panel::Timeline, false);
        let mut bindings = Bindings::default();
        let chord = KeyChord::new(Key::B, true, false, true);
        bindings
            .bind(Scope::Application, Command::Delete, Some(chord))
            .unwrap();
        p.shortcut_bindings = Arc::new(bindings);
        assert!(invoke(&mut p, &state(&engine), &input(Key::Delete), None).is_empty());
        assert!(p.message.is_none());
        let event = Input {
            keys: vec![(chord.key, chord.modifiers)],
            ..Default::default()
        };
        let actions = invoke(&mut p, &state(&engine), &event, None);
        assert!(matches!(
            actions.as_slice(),
            [Action::Edit(Edit::Delete { object: 7 })]
        ));
        assert_eq!(p.message.as_deref(), Some(notice(locale)));
        apply(&mut engine, actions);
        let undo = Input {
            keys: vec![(
                Key::Z,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let actions = invoke(&mut p, &state(&engine), &undo, None);
        assert!(matches!(actions.as_slice(), [Action::History(0)]));
        assert!(p.message.is_none());
        apply(&mut engine, actions);
        assert_eq!(engine.project(), &original);
    }
}

#[test]
fn rejected_core_delete_keeps_state_and_error_snapshot_outranks_the_target() {
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut engine = fixture();
            let mut p = setup(locale, Panel::Timeline, false);
            let mut actions = invoke(&mut p, &state(&engine), &input(Key::Delete), None);
            assert_eq!(p.message.as_deref(), Some(notice(locale)));
            // Another editor removes the same layer after the UI snapshot.
            engine.apply(Edit::Delete { object: 7 }).unwrap();
            let before = engine.project().clone();
            let revision = engine.revision();
            let error = match actions.pop().unwrap() {
                Action::Edit(command) => engine.apply(command).unwrap_err(),
                other => panic!("unexpected action: {other:?}"),
            };
            assert_eq!(engine.project(), &before);
            assert_eq!(engine.revision(), revision);
            let mut s = state(&engine);
            s["renderError"] = error.to_string().into();
            let original_error = s["renderError"].clone();
            let size = [900., 600.];
            let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
            let (paint, actions) = draw(&mut p, &s, &Input::default(), floating, size, &mut atlas);
            assert!(actions.is_empty());
            assert_eq!(s["renderError"], original_error);
            assert_eq!(p.message.as_deref(), Some(notice(locale)));
            let mut expected = PaintList::default();
            let height = if floating { 22. } else { 20. };
            let style = expected
                .styles()
                .resolve("error", Default::default())
                .unwrap();
            motion_ui::style::text_style(
                &mut expected,
                &mut atlas,
                Rect::new(8., size[1] - height, size[0] - 16., height),
                &locale.error(s["renderError"].as_str().unwrap()),
                style,
                motion_ui::ui::Align::Left,
            );
            assert!(!expected.glyphs.is_empty());
            let actual = paint
                .glyphs
                .iter()
                .filter(|glyph| glyph.position[1] >= size[1] - 20.)
                .collect::<Vec<_>>();
            assert_eq!(
                format!("{actual:?}"),
                format!("{:?}", expected.glyphs),
                "floating={floating} locale={locale:?}"
            );
        }
    }
}

#[test]
fn committed_host_delete_with_a_real_sample_failure_publishes_and_displays_the_original_error() {
    use std::sync::Arc;
    for locale in [Locale::En, Locale::Zh] {
        let engine = fixture();
        let mut p = setup(locale, Panel::Timeline, false);
        let actions = invoke(&mut p, &state(&engine), &input(Key::Delete), None);
        assert_eq!(p.message.as_deref(), Some(notice(locale)));
        let commands = actions
            .into_iter()
            .map(|action| match action {
                Action::Edit(command) => command,
                other => panic!("unexpected action: {other:?}"),
            })
            .collect::<Vec<_>>();
        let temp = tempfile::tempdir().unwrap();
        let id = motion_host::session::open(
            temp.path().into(),
            engine.project().clone(),
            Arc::new(motion_desktop_media::DesktopPlatform::new()),
        )
        .unwrap();
        let result = motion_host::session::with_session(id, |session| {
            // Controlled invalid sample after the real edit has committed. This
            // exercises the host's actual Ok(snapshot)+renderError publication.
            session.frame = 1000.;
            session.apply_command_batch(commands)
        });
        motion_host::session::close(id);
        let snapshot = result.unwrap();
        let cause = snapshot["renderError"].as_str().unwrap();
        assert!(cause.contains("invalid sample time"), "{cause}");
        assert_eq!(snapshot["project"]["layers"].as_array().unwrap().len(), 2);
        assert!(snapshot["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|layer| layer["id"] != 7));
        let original_error = snapshot["renderError"].clone();
        for floating in [false, true] {
            for size in [[900., 600.], [1600., 1000.]] {
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let (paint, actions) = draw(
                    &mut p,
                    &snapshot,
                    &Input::default(),
                    floating,
                    size,
                    &mut atlas,
                );
                assert!(actions.is_empty());
                let mut expected = PaintList::default();
                let height = if floating { 22. } else { 20. };
                let style = expected
                    .styles()
                    .resolve("error", Default::default())
                    .unwrap();
                motion_ui::style::text_style(
                    &mut expected,
                    &mut atlas,
                    Rect::new(8., size[1] - height, size[0] - 16., height),
                    &locale.error(cause),
                    style,
                    motion_ui::ui::Align::Left,
                );
                assert!(!expected.glyphs.is_empty());
                let actual = paint
                    .glyphs
                    .iter()
                    .filter(|glyph| glyph.position[1] >= size[1] - 20.)
                    .collect::<Vec<_>>();
                assert_eq!(
                    format!("{actual:?}"),
                    format!("{:?}", expected.glyphs),
                    "floating={floating} locale={locale:?} size={size:?}"
                );
                assert_eq!(snapshot["renderError"], original_error);
                assert_eq!(p.message.as_deref(), Some(notice(locale)));
            }
        }
    }
}
