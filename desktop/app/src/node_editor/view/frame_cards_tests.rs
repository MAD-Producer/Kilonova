use super::super::tests::{press, release, simple};
use super::*;
use motion_ui::input::Modifiers;
use std::cell::RefCell;

#[derive(Default)]
struct Probe {
    builds: usize,
    painted: Option<String>,
}
thread_local! {
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}
pub(super) fn record_build() {
    PROBE.with(|state| {
        if let Some(probe) = state.borrow_mut().as_mut() {
            probe.builds = probe.builds.saturating_add(1);
        }
    });
}
fn geometry(cards: &[Card]) -> String {
    let mut text = String::new();
    use std::fmt::Write;
    for card in cards {
        writeln!(text, "{} {:?}", card.id, card.rect).unwrap();
        for socket in &card.sockets {
            writeln!(
                text,
                "{:?} {} {:?} {:?} {:?} {}",
                socket.port,
                socket.output,
                socket.ty,
                socket.spec,
                socket.point,
                socket.connectable
            )
            .unwrap();
        }
    }
    text
}
pub(super) fn record_paint(cards: &[Card]) {
    PROBE.with(|state| {
        if let Some(probe) = state.borrow_mut().as_mut() {
            assert!(probe.painted.is_none(), "one canvas paint per node draw");
            probe.painted = Some(geometry(cards));
        }
    });
}
fn draw(
    editor: &mut NodeEditor,
    atlas: &mut TextAtlas,
    area: Rect,
    input: Input,
    language: Language,
) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PROBE.with(|state| *state.borrow_mut() = None);
        }
    }
    PROBE.with(|state| assert!(state.borrow_mut().replace(Probe::default()).is_none()));
    let reset = Reset;
    editor.draw(&mut PaintList::default(), atlas, area, &input, language);
    let probe = PROBE.with(|state| state.borrow_mut().take().unwrap());
    drop(reset);
    // Rebuild independently after the measured draw, outside the counter scope.
    assert_eq!(
        probe.painted.unwrap(),
        geometry(&editor.cards(editor.last_canvas)),
        "paint must use the current document, ports and viewport"
    );
    probe.builds
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

#[test]
fn unchanged_draws_build_card_geometry_once_per_frame() {
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for size in [[900., 600.], [1600., 1000.]] {
            let mut editor = simple();
            let before = editor.snapshot();
            let area = Rect::new(0., 0., size[0], size[1]);
            for point in [[-100.; 2], [400., 200.], [500., 500.]] {
                let builds = draw(
                    &mut editor,
                    &mut atlas,
                    area,
                    Input {
                        mouse: point,
                        ..Default::default()
                    },
                    language,
                );
                assert_eq!(builds, 1, "unchanged node geometry should be built once");
            }
            assert_eq!(editor.document, before.document);
            assert_eq!(editor.revision, before.revision);
            assert!(editor.undo.is_empty());
        }
    }
}

#[test]
fn navigation_rebuilds_current_geometry_without_editing_the_document() {
    for language in [Language::English, Language::Chinese] {
        let mut editor = simple();
        let before = editor.snapshot();
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        let area = Rect::new(0., 0., 1600., 1000.);
        draw(&mut editor, &mut atlas, area, Input::default(), language);
        let mouse = center(editor.last_canvas);
        for modifiers in [
            Modifiers::default(),
            Modifiers {
                control: true,
                ..Default::default()
            },
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ] {
            let builds = draw(
                &mut editor,
                &mut atlas,
                area,
                Input {
                    mouse,
                    wheel: [8., 25.],
                    modifiers,
                    ..Default::default()
                },
                language,
            );
            assert!(builds >= 2, "changed viewport needs fresh geometry");
        }
        draw(
            &mut editor,
            &mut atlas,
            area,
            press(mouse, MouseButton::Middle),
            language,
        );
        draw(
            &mut editor,
            &mut atlas,
            area,
            Input {
                mouse: [mouse[0] + 50., mouse[1] - 25.],
                held: Some(MouseButton::Middle),
                ..Default::default()
            },
            language,
        );
        draw(
            &mut editor,
            &mut atlas,
            area,
            release([mouse[0] + 50., mouse[1] - 25.], MouseButton::Middle),
            language,
        );
        assert_eq!(editor.document, before.document);
        assert_eq!(editor.revision, before.revision);
        assert!(editor.undo.is_empty());
    }
}

#[test]
fn minimap_drag_and_cancel_refresh_the_final_canvas_geometry() {
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for cancel in [
            Input {
                keys: vec![(Key::Escape, Modifiers::default())],
                ..Default::default()
            },
            Input {
                focus_lost: true,
                ..Default::default()
            },
            Input::default(),
        ] {
            let mut editor = simple();
            editor.view_preferences.minimap = true;
            let area = Rect::new(0., 0., 1600., 1000.);
            draw(&mut editor, &mut atlas, area, Input::default(), language);
            let before = editor.snapshot();
            let view = editor.view;
            let styles = PaintList::default().styles();
            let map = editor.minimap(editor.last_canvas, &styles).unwrap();
            let point = [map.area.min[0] + 12., map.area.min[1] + 12.];
            draw(
                &mut editor,
                &mut atlas,
                area,
                press(point, MouseButton::Left),
                language,
            );
            assert!(editor.minimap_drag.is_some());
            assert_ne!((editor.view.pan, editor.view.zoom), (view.pan, view.zoom));
            draw(
                &mut editor,
                &mut atlas,
                area,
                Input {
                    mouse: center(map.area),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                language,
            );
            draw(&mut editor, &mut atlas, area, cancel, language);
            assert!(editor.minimap_drag.is_none());
            assert_eq!((editor.view.pan, editor.view.zoom), (view.pan, view.zoom));
            assert_eq!(editor.document, before.document);
            assert_eq!(editor.revision, before.revision);
            assert!(editor.undo.is_empty());
        }
    }
}

#[test]
fn uncommitted_node_moves_release_and_cancel_keep_fresh_geometry_and_history() {
    let mut editor = simple();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 1600., 1000.);
    draw(
        &mut editor,
        &mut atlas,
        area,
        Input::default(),
        Language::English,
    );
    let before = editor.snapshot();
    let card = editor
        .cards(editor.last_canvas)
        .into_iter()
        .find(|card| card.id == "number")
        .unwrap();
    let start = [card.rect.min[0] + 40., card.rect.min[1] + 8.];
    let end = [start[0] + 35., start[1] + 20.];
    // A press and newer pointer position may arrive in the same frame.
    let mut input = press(start, MouseButton::Left);
    input.mouse = end;
    assert!(draw(&mut editor, &mut atlas, area, input, Language::English) >= 2);
    assert_eq!(
        editor.revision, before.revision,
        "held drag is not committed yet"
    );
    assert_ne!(editor.document, before.document);
    draw(
        &mut editor,
        &mut atlas,
        area,
        release(end, MouseButton::Left),
        Language::English,
    );
    assert_eq!(editor.undo.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before.document);
    draw(
        &mut editor,
        &mut atlas,
        area,
        press(start, MouseButton::Left),
        Language::English,
    );
    draw(
        &mut editor,
        &mut atlas,
        area,
        Input {
            mouse: end,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        Language::English,
    );
    draw(
        &mut editor,
        &mut atlas,
        area,
        Input {
            keys: vec![(Key::Escape, Modifiers::default())],
            ..Default::default()
        },
        Language::English,
    );
    assert_eq!(editor.document, before.document);
    assert!(editor.undo.is_empty());
}

#[test]
fn parent_navigation_invalidates_geometry_even_when_view_and_revision_match() {
    for language in [Language::English, Language::Chinese] {
        let mut editor = simple();
        editor.select(Some("number"), false);
        editor.select(Some("math"), true);
        assert!(editor.group_selected().unwrap());
        let group = editor.primary.clone().unwrap();
        assert!(editor.enter_group(&group));
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        let area = Rect::new(0., 0., 1600., 1000.);
        draw(&mut editor, &mut atlas, area, Input::default(), language);
        editor.view = editor.parents.last().unwrap().view;
        let layout = editor.guided_layout(area, PaintList::default().styles().as_ref());
        let point = center(Rect::new(
            layout.toolbar.min[0] + 6.,
            layout.toolbar.max[1] - 27.,
            85.,
            25.,
        ));
        let revision = editor.revision;
        let view = editor.view;
        let preview = editor.preview_revision;
        let builds = draw(
            &mut editor,
            &mut atlas,
            area,
            press(point, MouseButton::Left),
            language,
        );
        assert!(editor.parents.is_empty());
        assert_eq!(editor.revision, revision);
        assert_eq!((editor.view.pan, editor.view.zoom), (view.pan, view.zoom));
        assert_ne!(editor.preview_revision, preview);
        assert!(
            builds >= 2,
            "group navigation must invalidate before hit testing"
        );
        draw(
            &mut editor,
            &mut atlas,
            area,
            release(point, MouseButton::Left),
            language,
        );
    }
}
