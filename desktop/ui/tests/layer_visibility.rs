use motion_ui::{
    paint::Shape,
    ui::{self, LayerColumn, LayerRow},
    Input, MouseButton, PaintList, Rect, TextAtlas,
};

#[test]
fn fractional_scroll_draws_and_hits_both_partial_rows_without_leaking_outside() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let height = list.styles().metric("row.height", 22.);
    let area = Rect::new(10., 20., 180., height * 2.);
    let rows: Vec<_> = (1..=5)
        .map(|id| LayerRow {
            id,
            name: format!("Layer {id}"),
            color: list.styles().color("accent"),
            visible: true,
            locked: false,
            selected: true,
            camera: false,
            active: true,
        })
        .collect();
    for fraction in [0.1, 0.5, 0.9] {
        for (point, expected, column) in [
            ([80., area.min[1] + 0.5], Some(1), LayerColumn::Name),
            ([24., area.min[1] + 0.5], Some(1), LayerColumn::Visibility),
            ([40., area.max[1] - 0.5], Some(3), LayerColumn::Lock),
            ([80., area.max[1] - 0.5], Some(3), LayerColumn::Name),
            ([80., area.min[1] - 0.5], None, LayerColumn::Name),
            ([80., area.max[1] + 0.5], None, LayerColumn::Name),
        ] {
            list.clear();
            let hit = ui::layer_list(
                &mut list,
                &mut atlas,
                area,
                &rows,
                height * fraction,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
            );
            assert_eq!(hit.row, expected, "offset={fraction}, point={point:?}");
            assert_eq!(hit.column, column);
            let selected = list.styles().colors().ACCENT.with_alpha(0.18);
            assert_eq!(list.colors.iter().filter(|color| **color == selected).count(), 3);
            for shape in &list.shapes {
                match shape {
                    Shape::Quad { points } => check_points(points, area),
                    Shape::Triangles { points } => check_points(points, area),
                    Shape::Line { clip: Some(clip), .. }
                    | Shape::Stroke { clip: Some(clip), .. }
                    | Shape::Procedural { clip, .. } => assert!(area.contains_rect(*clip)),
                    Shape::Line { points, clip: None }
                    | Shape::Stroke { points, clip: None, .. } => check_points(points, area),
                }
            }
        }
    }
}

fn check_points(points: &[[f32; 2]], area: Rect) {
    for point in points {
        assert!(point[0] >= area.min[0] - 0.001 && point[0] <= area.max[0] + 0.001);
        assert!(point[1] >= area.min[1] - 0.001 && point[1] <= area.max[1] + 0.001);
    }
}
