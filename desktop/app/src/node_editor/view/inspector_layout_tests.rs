use super::super::tests::{press, simple};
use super::*;

#[test]
fn compact_inspector_connections_navigate_disconnect_and_undo_at_each_width() {
    let styles = motion_ui::style::default_stylesheet();
    let pad = styles.metric("padding.panel", 8.);
    let row = styles.metric("node.dropdown.row.height", 28.);
    let gap = styles.metric("padding", 5.);
    let action = styles.metric("auxiliary.action.width", 28.);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for language in [Language::Chinese, Language::English] {
        for width in [220., 360., 720.] {
            let mut editor = simple();
            editor.select(Some("output"), false);
            let area = Rect::new(137., 49., width, 520.);
            let source = [
                area.min[0] + pad + styles.metric("label.column", 72.) + gap * 2.,
                area.min[1] + pad + row * 3. + gap + row * 0.5,
            ];
            let before = editor.document.clone();
            let revision = editor.revision;
            editor.draw_inspector(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &press(source, MouseButton::Left),
                language,
            );
            assert_eq!(
                editor.primary.as_deref(),
                Some("color"),
                "{width} {language:?}"
            );
            assert_eq!(editor.document, before);
            assert_eq!(editor.revision, revision, "navigation is view state");
            editor.select(Some("output"), false);
            let detach = [area.max[0] - pad - action * 0.5, source[1]];
            let result = editor.draw_inspector(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &press(detach, MouseButton::Left),
                language,
            );
            assert!(result.semantic_changed);
            assert!(editor.document.connections.is_empty());
            assert!(editor.undo());
            assert_eq!(editor.document, before);
        }
    }
}
