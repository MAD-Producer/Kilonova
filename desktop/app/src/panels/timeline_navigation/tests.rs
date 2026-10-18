use super::*;
use std::time::Duration;

#[test]
fn viewport_changes_place_the_playhead_at_its_current_time_without_pixel_easing() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 1200., 500.);
    let mut state = scene();
    state["frame"] = serde_json::json!(150);
    let mut panels = Panels::default();
    let mut render = |panels: &mut Panels| {
        let mut paint = PaintList::default();
        timeline::render(
            &mut paint,
            &mut atlas,
            area,
            panels,
            &state,
            &Input::default(),
            &mut vec![],
        );
        paint
    };
    render(&mut panels);
    panels.timeline.zoom = 4.;
    panels.timeline.offset = 100.;
    let paint = render(&mut panels);
    let track = panels.timeline.zoom_track.unwrap();
    let expected = track.min[0] + 50. * track.width() / 1200. * 4.;
    let line_x = |paint: &PaintList| {
        paint
            .shapes
            .iter()
            .zip(&paint.colors)
            .find_map(|(shape, color)| {
                if *color != paint.styles().color("focus") {
                    return None;
                }
                match shape {
                    motion_ui::paint::Shape::Procedural { area, .. }
                        if (area.width() - 1.).abs() < 0.001
                            && area.min[1] == 36.
                            && area.max[1] == 482. =>
                    {
                        Some(area.min[0])
                    }
                    _ => None,
                }
            })
    };
    assert!((line_x(&paint).unwrap() - expected).abs() < 0.001);
    panels.timeline.offset = 400.;
    assert!(
        line_x(&render(&mut panels)).is_none(),
        "offscreen time must not leave a stale visible playhead"
    );
}

#[test]
fn edge_pan_is_time_based_bounded_and_keeps_layer_drag_anchor() {
    let styles = motion_ui::style::default_stylesheet_arc();
    let track = Rect::new(600., 100., 600., 400.);
    let mut view = types::TimelineState {
        zoom: 4.,
        offset: 400.,
        ..Default::default()
    };
    view.moving = Some(types::MovingLayers {
        start_mouse: 850.,
        initial: 550,
        last: 550,
        layers: vec![(7, 550, 100)],
    });
    let now = Instant::now();
    view.edge_pan_at = Some(now - Duration::from_millis(25));
    let input = Input {
        mouse: [590., 250.],
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    let g = Geometry::new(track, 18., 1200., &view, &styles);
    update_at(&g, &mut view, &input, 1200., true, now);
    assert!((view.offset - 392.5).abs() < 0.001);
    assert!((view.moving.as_ref().unwrap().start_mouse - 865.).abs() < 0.001);
    // An unchanged pointer continues panning; stopping/releasing resets the clock.
    update_at(
        &g,
        &mut view,
        &input,
        1200.,
        true,
        now + Duration::from_millis(25),
    );
    assert!(view.offset < 392.5);
    let before_ack = view.offset;
    update_at(
        &g,
        &mut view,
        &Input::default(),
        1200.,
        true,
        now + Duration::from_millis(30),
    );
    assert_eq!(view.offset, before_ack);
    assert!(view.edge_pan_at.is_some());
    update_at(
        &g,
        &mut view,
        &Input {
            released: Some((MouseButton::Left, input.mouse)),
            ..Default::default()
        },
        1200.,
        true,
        now + Duration::from_secs(1),
    );
    assert!(view.edge_pan_at.is_none());
    view.offset = 0.;
    view.edge_pan_at = Some(now);
    update_at(
        &g,
        &mut view,
        &input,
        1200.,
        true,
        now + Duration::from_secs(1),
    );
    assert_eq!(view.offset, 0.);
    view.offset = 900.;
    let right = Input {
        mouse: [1210., 250.],
        ..input
    };
    update_at(
        &g,
        &mut view,
        &right,
        1200.,
        true,
        now + Duration::from_secs(2),
    );
    assert_eq!(view.offset, 900.);
}

fn scene() -> Value {
    serde_json::json!({"frame":0,"project":{"frames":1200,"fps":30,"layers":[{"id":7,"name":"Layer","visible":true}]},"timeline_layers":[{"object":7,"in_frame":100,"out_frame":200}]})
}
fn draw(
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    input: &Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let mut actions = vec![];
    timeline::render(
        &mut paint,
        atlas,
        area,
        panels,
        &scene(),
        input,
        &mut actions,
    );
    (paint, actions)
}

#[test]
fn actual_scrollbar_drag_and_trough_click_only_navigate() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for origin in [0., 250.] {
        let area = Rect::new(origin, 100., 1200., 500.);
        let mut panels = Panels::default();
        panels.select_only(Some(7));
        panels.timeline.zoom = 4.;
        draw(&mut atlas, area, &mut panels, &Input::default());
        let track = panels.timeline.zoom_track.unwrap();
        let styles = motion_ui::style::default_stylesheet_arc();
        let g = Geometry::new(
            track,
            styles.metric("field.height", 18.),
            1200.,
            &panels.timeline,
            &styles,
        );
        let mouse = [g.thumb.min[0] + 5., g.lane.min[1] + 5.];
        let press = Input {
            mouse,
            pressed: Some((MouseButton::Left, mouse)),
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        assert!(draw(&mut atlas, area, &mut panels, &press).1.is_empty());
        let dragged = [g.lane.max[0] - 5., mouse[1]];
        assert!(draw(
            &mut atlas,
            area,
            &mut panels,
            &Input {
                mouse: dragged,
                held: Some(MouseButton::Left),
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!((panels.timeline.offset - 900.).abs() < 0.001);
        // Neutral acknowledged-frame paints must neither move nor release a grab.
        assert!(draw(&mut atlas, area, &mut panels, &Input::default())
            .1
            .is_empty());
        assert!((panels.timeline.offset - 900.).abs() < 0.001);
        assert!(panels.timeline.horizontal_drag.is_some());

        assert!(draw(
            &mut atlas,
            area,
            &mut panels,
            &Input {
                mouse: dragged,
                released: Some((MouseButton::Left, dragged)),
                ..Default::default()
            }
        )
        .1
        .is_empty());
        assert!(panels.timeline.horizontal_drag.is_none());
        assert_eq!(panels.selected, Some(7));
        assert!(panels.timeline.trimming.is_none() && panels.timeline.moving.is_none());
        let point = [track.min[0] + track.width() * 0.5, mouse[1]];
        draw(
            &mut atlas,
            area,
            &mut panels,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        );
        assert!((panels.timeline.offset - 450.).abs() < 0.001);
        assert!(super::super::cancel_interaction(&mut panels).is_empty());
        assert!(panels.timeline.horizontal_drag.is_none());
    }
}

#[test]
fn offscreen_layer_handles_do_not_paint_or_edit_the_property_gutter() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 1200., 500.);
    let mut panels = Panels::default();
    panels.select_only(Some(7));
    panels.timeline.zoom = 4.;
    panels.timeline.offset = 400.;
    // Layer out-frame 200 maps well into the property gutter at this zoom.
    let x = GUTTER.min(1200. * 0.52)
        + (200. - 400.) * (1200. - GUTTER.min(1200. * 0.52) - 8.) / 1200. * 4.;
    let mouse = [x, 75.];
    let (paint, actions) = draw(
        &mut atlas,
        area,
        &mut panels,
        &Input {
            mouse,
            pressed: Some((MouseButton::Left, mouse)),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    assert!(panels.timeline.trimming.is_none());
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::History(_) | Action::Edit(Command::SetLayerRange { .. })
    )));
    let accent = paint.styles().color("focus");
    assert!(!paint.shapes.iter().zip(&paint.colors).any(|(shape,color)| matches!(shape,motion_ui::paint::Shape::Quad {points} if (points[0][0]-(x-4.)).abs()<0.01 && points[0][1]>64.) && *color==accent));
}

#[test]
fn zoom_button_paints_the_new_layer_coordinates_in_the_same_frame() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 1200., 500.);
    let mut panels = Panels::default();
    let track = Rect::new(
        GUTTER.min(1200. * 0.52),
        0.,
        1200. - GUTTER.min(1200. * 0.52) - 8.,
        500.,
    );
    let mouse = [track.max[0] - 146. + 86. + 5., 8.];
    let (paint, actions) = draw(
        &mut atlas,
        area,
        &mut panels,
        &Input {
            mouse,
            pressed: Some((MouseButton::Left, mouse)),
            ..Default::default()
        },
    );
    assert!(actions.is_empty());
    assert_eq!(panels.timeline.zoom, 1.25);
    let x = track.min[0] + (200. - 120.) * track.width() / 1200. * 1.25;
    assert!(paint.shapes.iter().any(|shape| matches!(shape,motion_ui::paint::Shape::Quad {points} if (points[1][0]-x).abs()<0.01 && (points[0][1]-69.).abs()<0.01)));
}
