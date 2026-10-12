use motion_ui::{
    paint::{self, Glyph, PaintList, PaintOp},
    theme::Color,
    Rect,
};

#[test]
fn removing_obscured_glyphs_keeps_shapes_text_and_clipping_in_creation_order() {
    let glyph = |x| Glyph {
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
