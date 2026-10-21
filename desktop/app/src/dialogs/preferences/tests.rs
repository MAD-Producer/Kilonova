use super::*;
use motion_ui::input::Modifiers;

fn draw(
    editor: &mut Editor,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    input: Input,
) -> (PaintList, Option<Event>) {
    let mut list = PaintList::default();
    let event = editor.draw(&mut list, atlas, size, &input, Locale::En, None);
    (list, event)
}
fn click(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
#[test]
fn preferences_dropdown_owns_clicks_escape_and_focus_without_changing_live_settings() {
    let workspace = Workspace::default();
    let original = workspace.preferences();
    let mut editor = Editor::new(&workspace);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let size = [900., 600.];
    let sheet = motion_ui::style::default_stylesheet();
    let pad = sheet.metric("preferences.margin", 16.);
    let row = sheet.metric("row.height", 22.);
    let top = (size[1] - sheet.metric("preferences.max_height", 492.)) * 0.5;
    let first = top + pad + row * 2. + sheet.metric("preferences.section_gap", 12.) + row * 0.5;
    assert!(draw(&mut editor, &mut atlas, size, click([570., first]))
        .1
        .is_none());
    assert!(editor.language.open);
    let footer = top + sheet.metric("preferences.max_height", 492.) - pad - row * 0.5;
    assert!(
        draw(&mut editor, &mut atlas, size, click([650., footer]))
            .1
            .is_none(),
        "closing an overlay must not click Apply"
    );
    assert!(!editor.language.open);
    draw(&mut editor, &mut atlas, size, click([570., first]));
    let esc = || Input {
        keys: vec![(Key::Escape, Modifiers::default())],
        ..Default::default()
    };
    let (paint, event) = draw(&mut editor, &mut atlas, size, esc());
    assert!(event.is_none() && !editor.language.open && !paint.shapes.is_empty());
    assert!(matches!(
        draw(&mut editor, &mut atlas, size, esc()).1,
        Some(Event::Cancelled)
    ));
    assert!(draw(
        &mut editor,
        &mut atlas,
        size,
        Input {
            composing: true,
            ..esc()
        }
    )
    .1
    .is_none());
    draw(&mut editor, &mut atlas, size, click([570., first]));
    let (paint, event) = draw(
        &mut editor,
        &mut atlas,
        size,
        Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert!(event.is_none() && !editor.language.open && !paint.shapes.is_empty());
    assert_eq!(workspace.preferences(), original);
}

#[test]
fn compact_preferences_scroll_reaches_controls_without_hidden_hits_and_reset_is_draft_only() {
    let workspace = Workspace::default();
    let original = workspace.preferences();
    let mut editor = Editor::new(&workspace);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let size = [360., 400.];
    // Three categories now share the compact navigation row.
    draw(&mut editor, &mut atlas, size, click([180., 99.]));
    assert!(editor.page == Page::Interface);
    let (normal, _) = draw(&mut editor, &mut atlas, size, Input::default());
    draw(
        &mut editor,
        &mut atlas,
        size,
        Input {
            mouse: [150., 160.],
            wheel: [0., 1000.],
            ..Default::default()
        },
    );
    assert!(editor.scroll.offset > 0.);
    let (scrolled, _) = draw(&mut editor, &mut atlas, size, Input::default());
    assert_ne!(
        normal.glyphs.iter().map(|g| g.position).collect::<Vec<_>>(),
        scrolled
            .glyphs
            .iter()
            .map(|g| g.position)
            .collect::<Vec<_>>(),
        "scrolling moves visible text"
    );
    let before = editor.draft.clone();
    // Keep this outside content and outside all three category hit targets.
    draw(&mut editor, &mut atlas, size, click([250., 80.]));
    assert!(
        !editor.language.open && !editor.preview.open,
        "scrolled-out dropdowns cannot receive clicks"
    );
    assert_eq!(
        editor.draft, before,
        "the clipped-out content cannot receive a press"
    );
    draw(&mut editor, &mut atlas, size, click([250., 183.]));
    assert!(
        !editor.draft.columns.matte,
        "the visible checkbox retains its normal hit target"
    );
    draw(&mut editor, &mut atlas, size, click([70., 99.]));
    assert!(editor.page == Page::General);
    assert!(
        !editor.draft.columns.matte,
        "switching categories retains the draft"
    );
    editor.draft.node_view.grid = !original.node_view.grid;
    draw(&mut editor, &mut atlas, size, click([70., 357.]));
    assert_eq!(editor.draft, Preferences::default());
    assert_eq!(workspace.preferences(), original);
}

#[test]
fn shortcut_capture_owns_footer_escape_ime_and_supports_local_undo_without_project_edits() {
    use crate::shortcuts::{Command, Scope};
    let workspace = Workspace::default();
    let original = workspace.preferences();
    let mut editor = Editor::new(&workspace);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let size = [900., 600.];
    let (clicked, _) = draw(&mut editor, &mut atlas, size, click([250., 193.]));
    assert!(editor.page == Page::Shortcuts);
    let (settled, _) = draw(&mut editor, &mut atlas, size, click([250., 193.]));
    assert_eq!(
        clicked.colors, settled.colors,
        "selection and content must agree in the click frame"
    );
    draw(&mut editor, &mut atlas, size, click([365., 243.]));
    assert!(editor.shortcuts.capturing);
    assert!(draw(&mut editor, &mut atlas, size, click([650., 519.]))
        .1
        .is_none());
    assert!(
        editor.shortcuts.capturing,
        "Apply must not receive a capture-owned click"
    );
    let press = |key, modifiers| Input {
        keys: vec![(key, modifiers)],
        ..Default::default()
    };
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    let mut ime = press(Key::B, ctrl);
    ime.composing = true;
    draw(&mut editor, &mut atlas, size, ime);
    assert_eq!(editor.draft, original);
    let (paint, event) = draw(
        &mut editor,
        &mut atlas,
        size,
        press(Key::Escape, Default::default()),
    );
    assert!(event.is_none() && !paint.shapes.is_empty() && !editor.shortcuts.capturing);
    draw(&mut editor, &mut atlas, size, click([365., 243.]));
    draw(&mut editor, &mut atlas, size, press(Key::B, ctrl));
    assert!(!editor.shortcuts.capturing);
    assert_eq!(
        editor
            .draft
            .shortcuts
            .resolve(Scope::Application, Key::B, ctrl, false),
        Some(Command::SelectAll)
    );
    let changed = editor.draft.shortcuts.clone();
    draw(&mut editor, &mut atlas, size, press(Key::Z, ctrl));
    assert_eq!(editor.draft, original);
    draw(
        &mut editor,
        &mut atlas,
        size,
        press(
            Key::Z,
            Modifiers {
                shift: true,
                ..ctrl
            },
        ),
    );
    assert_eq!(editor.draft.shortcuts, changed);
    draw(&mut editor, &mut atlas, size, click([365., 243.]));
    draw(
        &mut editor,
        &mut atlas,
        size,
        Input {
            focus_lost: true,
            ..Default::default()
        },
    );
    assert!(!editor.shortcuts.capturing && editor.draft.shortcuts == changed);
    draw(&mut editor, &mut atlas, size, click([570., 137.]));
    assert!(editor.shortcuts.owns_input());
    assert!(draw(&mut editor, &mut atlas, size, click([650., 519.]))
        .1
        .is_none());
    assert_eq!(workspace.preferences(), original);
    assert!(matches!(
        draw(
            &mut editor,
            &mut atlas,
            size,
            press(Key::Escape, Default::default())
        )
        .1,
        Some(Event::Cancelled)
    ));
}
