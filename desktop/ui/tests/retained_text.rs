use motion_ui::{
    paint::{self, Glyph, PaintList, PaintOp},
    theme::Color,
    Rect,
};

#[test]
fn removing_obscured_glyphs_keeps_shapes_text_and_clipping_in_creation_order() {
    let glyph = |x| Glyph {
        atlas_size: None,
        uv: [0., 0., 1., 1.],
        position: [x, 0.],
        size: [8., 8.],
        color: Color::rgb(255, 255, 255),
    };
    let mut list = PaintList::default();
    list.push_glyph(glyph(0.));
    paint::rect(
        &mut list,
        Rect::new(0., 0., 40., 40.),
        Color::rgb(30, 30, 30),
    );
    list.push_glyph(glyph(12.));
    paint::rect(
        &mut list,
        Rect::new(0., 0., 20., 20.),
        Color::rgb(50, 50, 50),
    );
    list.push_glyph(glyph(24.));
    list.retain_glyphs(|g| g.position[0] != 12.);
    assert_eq!(
        list.order,
        vec![
            PaintOp::Glyph(0),
            PaintOp::Shape(0),
            PaintOp::Shape(1),
            PaintOp::Glyph(1)
        ]
    );
    list.clip_to(Rect::new(0., 0., 40., 40.));
    assert_eq!(
        list.order,
        vec![
            PaintOp::Glyph(0),
            PaintOp::Shape(0),
            PaintOp::Shape(1),
            PaintOp::Glyph(1)
        ]
    );
    let mut popup = PaintList::default();
    popup.push_glyph(glyph(2.));
    list.append(popup);
    assert_eq!(list.order.last(), Some(&PaintOp::Glyph(2)));
}

fn sample_glyph(id: usize) -> Glyph {
    Glyph {
        uv: [0., 0., 1., 1.],
        atlas_size: Some([64, 64]),
        position: [id as f32, 0.],
        size: [8., 8.],
        color: Color::grey(220),
    }
}

#[test]
fn every_retention_mask_preserves_predicate_order_and_interleaved_creation_order() {
    for len in 0..=8 {
        for mask in 0usize..(1 << len) {
            for ordering in 0..3 {
                let mut list = PaintList::default();
                paint::rect(&mut list, Rect::new(0., 0., 8., 8.), Color::grey(30));
                for id in 0..len {
                    list.push_glyph(sample_glyph(id));
                    if id % 2 == 0 {
                        paint::rect(&mut list, Rect::new(0., 0., 4., 4.), Color::grey(50));
                    }
                }
                if ordering == 1 {
                    list.order.reverse();
                }
                if ordering == 2 && len > 1 {
                    let index = list
                        .order
                        .iter()
                        .rposition(|op| matches!(op, PaintOp::Glyph(_)))
                        .unwrap();
                    list.order[index] = PaintOp::Glyph(0);
                }
                let kept: Vec<_> = (0..len).filter(|id| mask & (1 << id) != 0).collect();
                let expected: Vec<_> = list
                    .order
                    .iter()
                    .filter_map(|op| match op {
                        PaintOp::Shape(index) => Some(PaintOp::Shape(*index)),
                        PaintOp::Glyph(index) => {
                            kept.iter().position(|id| id == index).map(PaintOp::Glyph)
                        }
                        PaintOp::Image(_) => unreachable!("this text fixture contains no bitmaps"),
                    })
                    .collect();
                let colors = list.colors.clone();
                let mut visits = vec![];
                list.retain_glyphs(|glyph| {
                    let id = glyph.position[0] as usize;
                    visits.push(id);
                    mask & (1 << id) != 0
                });
                assert_eq!(visits, (0..len).collect::<Vec<_>>());
                assert_eq!(
                    list.glyphs
                        .iter()
                        .map(|g| g.position[0] as usize)
                        .collect::<Vec<_>>(),
                    kept
                );
                assert_eq!(
                    list.order, expected,
                    "len={len} mask={mask} ordering={ordering}"
                );
                assert_eq!(list.colors, colors);
            }
        }
    }
}

fn panic_text(cause: Box<dyn std::any::Any + Send>) -> String {
    cause
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
        .expect("string panic")
}

#[test]
fn invalid_glyph_indices_keep_the_original_bounds_panic_after_the_predicate() {
    for len in [0, 1] {
        for keep in [false, true] {
            let mut list = PaintList::default();
            paint::rect(&mut list, Rect::new(0., 0., 4., 4.), Color::grey(30));
            for id in 0..len {
                list.push_glyph(sample_glyph(id));
            }
            list.order[0] = PaintOp::Glyph(len + 2);
            let mut visits = 0;
            let cause = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                list.retain_glyphs(|_| {
                    visits += 1;
                    keep
                })
            }))
            .unwrap_err();
            assert_eq!(visits, len);
            assert_eq!(
                panic_text(cause),
                format!(
                    "index out of bounds: the len is {len} but the index is {}",
                    len + 2
                )
            );
        }
    }
}

#[test]
fn predicate_panic_preserves_the_primary_cause_and_existing_partial_compaction() {
    let mut list = PaintList::default();
    for id in 0..4 {
        list.push_glyph(sample_glyph(id));
    }
    let order = list.order.clone();
    let mut visits = vec![];
    let cause = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        list.retain_glyphs(|g| {
            let id = g.position[0] as usize;
            visits.push(id);
            if id == 2 {
                panic!("retention predicate failure");
            }
            id != 1
        })
    }))
    .unwrap_err();
    assert_eq!(panic_text(cause), "retention predicate failure");
    assert_eq!(visits, vec![0, 1, 2]);
    assert_eq!(
        list.glyphs
            .iter()
            .map(|g| g.position[0] as usize)
            .collect::<Vec<_>>(),
        vec![0, 2, 3]
    );
    assert_eq!(list.order, order);
}
