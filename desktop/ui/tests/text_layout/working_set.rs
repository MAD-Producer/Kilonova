use super::*;
use motion_ui::{
    paint::PaintList,
    style::{default_stylesheet, States},
    ui::{self, Align},
    Rect,
};

#[path = "working_set_gpu.rs"]
mod gpu;

fn labels(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| {
            if count == 200 {
                format!("Property {n:03} cafe\u{301} / 位置 / سلام")
            } else {
                format!("R{n:04}")
            }
        })
        .collect()
}

fn same_layout(
    actual: &motion_ui::text::layout::LineLayout,
    reference: &motion_ui::text::layout::LineLayout,
) {
    assert_eq!(actual.advance, reference.advance);
    assert_eq!(actual.grapheme_boundaries, reference.grapheme_boundaries);
    assert_eq!(actual.missing_glyphs, reference.missing_glyphs);
    assert_eq!(actual.glyphs.len(), reference.glyphs.len());
    for (a, b) in actual.glyphs.iter().zip(&reference.glyphs) {
        assert_eq!(
            (a.face, a.id, a.cluster, a.source),
            (b.face, b.id, b.cluster, b.source)
        );
        assert_eq!((a.position, a.advance), (b.position, b.advance));
    }
}

#[test]
fn different_complete_labels_reuse_the_budgeted_working_set_without_allocations() {
    for count in [200, 1000] {
        let labels = labels(count);
        let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
        for label in &labels {
            atlas.layout_line(label).unwrap();
        }
        for _ in 0..3 {
            let allocated = allocations(|| {
                for label in &labels {
                    std::hint::black_box(atlas.layout_line(label).unwrap());
                }
            });
            eprintln!("complete distinct_labels={count} allocations={allocated}");
            assert_eq!(
                allocated, 0,
                "retained distinct complete labels must reuse layouts"
            );
        }
        let mut reference = TextAtlas::from_system_language(12., false).unwrap();
        assert_eq!(atlas.selected_family, reference.selected_family);
        for label in &labels {
            same_layout(
                &atlas.layout_line(label).unwrap(),
                &reference.layout_line(label).unwrap(),
            );
        }
    }
}

#[test]
fn different_fitted_labels_reuse_both_truncated_and_complete_working_sets() {
    for count in [200, 1000] {
        let labels = labels(count);
        for width in [2., 100.] {
            let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
            for label in &labels {
                atlas.fit_line(label, width).unwrap();
            }
            for _ in 0..3 {
                let allocated = allocations(|| {
                    for label in &labels {
                        std::hint::black_box(atlas.fit_line(label, width).unwrap());
                    }
                });
                eprintln!(
                    "fitted distinct_labels={count} width_em={width} allocations={allocated}"
                );
                assert_eq!(
                    allocated, 0,
                    "retained distinct fitted labels must reuse layouts"
                );
            }
            let mut reference = TextAtlas::from_system_language(12., false).unwrap();
            assert_eq!(atlas.selected_family, reference.selected_family);
            for label in &labels {
                let a = atlas.fit_line(label, width).unwrap();
                let b = reference.fit_line(label, width).unwrap();
                assert_eq!((a.prefix_end, a.ellipsis), (b.prefix_end, b.ellipsis));
                same_layout(&a.layout, &b.layout);
            }
        }
    }
}

#[test]
fn different_plain_and_styled_label_controls_do_not_allocate_per_warmed_frame() {
    for count in [200, 1000] {
        let labels = labels(count);
        for width in [48., 480.] {
            let mut atlas = TextAtlas::from_system_language(12., false).unwrap();
            let sheet = default_stylesheet();
            let style = sheet.resolve("label", States::default()).unwrap();
            let mut list = PaintList::default();
            let mut frame = || {
                list.clear();
                for (row, label) in labels.iter().enumerate() {
                    let area = Rect::new(0., row as f32 * 22., width, 22.);
                    ui::text(
                        &mut list,
                        &mut atlas,
                        area,
                        label,
                        style.foreground,
                        Align::Left,
                    );
                    motion_ui::style::text_style(
                        &mut list,
                        &mut atlas,
                        area,
                        label,
                        style,
                        Align::Right,
                    );
                }
            };
            frame();
            for _ in 0..3 {
                let allocated = allocations(&mut frame);
                eprintln!("draw distinct_labels={count} width_px={width} allocations={allocated}");
                assert_eq!(
                    allocated, 0,
                    "different warmed labels may not allocate per control"
                );
            }
            assert!(!list.glyphs.is_empty());
        }
    }
}
