use super::super::tests::simple;
use super::*;
use std::cell::Cell;

thread_local! {
    static HEIGHT_RESOLUTIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(super) fn record_height_resolution() {
    HEIGHT_RESOLUTIONS.with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value.saturating_add(1)));
        }
    });
}

fn measured_paint(
    editor: &NodeEditor,
    atlas: &mut TextAtlas,
    cards: &[Card],
    area: Rect,
    language: Language,
) -> (String, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            HEIGHT_RESOLUTIONS.with(|count| count.set(None));
        }
    }
    HEIGHT_RESOLUTIONS.with(|count| assert!(count.replace(Some(0)).is_none()));
    let reset = Reset;
    let mut paint = PaintList::default();
    editor.paint_canvas(&mut paint, atlas, cards, area, &Input::default(), language);
    let resolutions = HEIGHT_RESOLUTIONS.with(|count| count.get().unwrap());
    drop(reset);
    assert!(!paint.shapes.is_empty());
    (
        format!(
            "{:?}",
            (paint.shapes, paint.colors, paint.glyphs, paint.order)
        ),
        resolutions,
    )
}

#[test]
fn builtin_canvas_paints_reuse_exact_frame_heights_without_resolving_ports_again() {
    let mut comparisons = 0;
    let mut painted_cards = 0;
    for language in [Language::English, Language::Chinese] {
        let mut atlas =
            TextAtlas::from_system_language(12., language == Language::Chinese).unwrap();
        for example in motion_graph::builtin_examples() {
            for collapsed in [false, true] {
                let mut editor = NodeEditor::new(example.load().unwrap());
                for (index, node) in editor.document.nodes.iter_mut().enumerate() {
                    node.collapsed = collapsed;
                    node.position = [(index % 4) as f32 * 230., (index / 4) as f32 * 280.];
                }
                for zoom in [0.33, 0.85, 2.75] {
                    editor.view.zoom = zoom;
                    let area = Rect::new(17., 33., 1600., 1000.);
                    let cards = editor.cards(area);
                    for (card, node) in cards.iter().zip(&editor.document.nodes) {
                        assert_eq!(
                            card.world_height.to_bits(),
                            node_height(node, &editor.document).to_bits()
                        );
                    }
                    let (fresh, resolutions) =
                        measured_paint(&editor, &mut atlas, &cards, area, language);
                    assert_eq!(
                        resolutions, 0,
                        "example={} collapsed={collapsed} zoom={zoom}",
                        example.id
                    );
                    // Force the retained original calculation on identical cards.
                    // This exercises the actual painter, not a metadata-only oracle.
                    let mut original_cards = editor.cards(area);
                    for card in &mut original_cards {
                        card.source_node = std::ptr::null();
                    }
                    let (original, original_resolutions) =
                        measured_paint(&editor, &mut atlas, &original_cards, area, language);
                    assert_eq!(fresh, original, "complete paint example={}", example.id);
                    comparisons += 1;
                    painted_cards += original_resolutions;
                }
            }
        }
    }
    assert_eq!(comparisons, motion_graph::builtin_examples().len() * 12);
    assert!(painted_cards > 0);
    println!("complete canvas comparisons={comparisons} original_height_resolutions={painted_cards} reused_height_resolutions=0");
}

#[test]
fn identity_mismatch_and_duplicate_ids_keep_the_original_height_and_complete_paint() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut editor = simple();
    editor.view.pan = [20., 20.];
    editor.view.zoom = 1.;
    for (index, node) in editor.document.nodes.iter_mut().enumerate() {
        node.position = [index as f32 * 240., 20.];
    }
    let area = Rect::new(0., 0., 1600., 1000.);
    for duplicate in [false, true] {
        if duplicate {
            let duplicate_id = editor.document.nodes[0].id.clone();
            editor.document.nodes[1].id = duplicate_id;
        }
        let cards = editor.cards(area);
        let (expected, expected_resolutions) =
            measured_paint(&editor, &mut atlas, &cards, area, Language::English);
        assert_eq!(expected_resolutions, usize::from(duplicate));
        let mut mismatched = editor.cards(area);
        for card in &mut mismatched {
            card.source_node = std::ptr::null();
            card.world_height = f32::NAN;
        }
        let (actual, resolutions) =
            measured_paint(&editor, &mut atlas, &mismatched, area, Language::English);
        assert_eq!(resolutions, cards.len());
        assert_eq!(
            actual, expected,
            "mismatched metadata must not change any paint, duplicate={duplicate}"
        );
    }
}
