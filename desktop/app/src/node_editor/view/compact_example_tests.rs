use super::super::tests::frame;
use super::*;

#[test]
fn compact_example_root_cards_do_not_overlap() {
    let mut atlas = TextAtlas::from_system(13.).unwrap();
    for id in ["soft_sphere", "soft_sphere_motion", "floating_lights"] {
        let graph = motion_graph::builtin_examples()
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .load()
            .unwrap();
        let mut editor = NodeEditor::new(graph);
        frame(&mut editor, &mut atlas, Input::default());
        let cards = editor.cards(editor.canvas_bounds());
        for (index, a) in cards.iter().enumerate() {
            for b in &cards[index + 1..] {
                let overlaps = a.rect.min[0] < b.rect.max[0]
                    && b.rect.min[0] < a.rect.max[0]
                    && a.rect.min[1] < b.rect.max[1]
                    && b.rect.min[1] < a.rect.max[1];
                assert!(!overlaps, "{id}: {} overlaps {}", a.id, b.id);
            }
        }
    }
}
