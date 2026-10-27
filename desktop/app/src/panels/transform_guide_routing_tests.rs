use super::tests::fixture;
use super::*;
use motion_core::{Guide, GuideAxis};

#[test]
fn composition_preserves_grip_priority_and_specialized_tools_near_guides() {
    let (_folder, engine, _, _) = fixture(false);
    let state = engine
        .edit(
            serde_json::to_string(&Command::SetGuides {
                guides: vec![Guide {
                    id: 1,
                    axis: GuideAxis::Y,
                    position: 560.,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    let area = Rect::new(0., 0., 360., 320.);
    let sheet = PaintList::default().styles();
    let viewport = crate::panels::styled_viewer(area, &sheet);
    let mut atlas = motion_ui::text::TextAtlas::from_system(18.).unwrap();
    let mut panels = crate::panels::Panels::default();
    panels.select_only(Some(7));
    let mut out = vec![];
    crate::panels::composition::render(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &mut panels,
        &state,
        &Input::default(),
        &mut out,
    );
    let map = panels
        .composition
        .navigation
        .mapping(viewport, [1920., 1080.])
        .unwrap();
    let pose = Pose::new(&state, 7, map.image).unwrap();
    let point = mid(pose.corners[1], pose.corners[2]);
    crate::panels::composition::render(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &mut panels,
        &state,
        &Input {
            pressed: Some((MouseButton::Left, point)),
            mouse: point,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        &mut out,
    );
    assert!(panels.transform_controls.gesture_active());
    assert!(!panels.viewer_guides.dragging());
    assert!(out.is_empty());
    panels.transform_controls.cancel();
    crate::panels::tools::choose(
        &mut panels,
        crate::panels::tools::Active::Zoom,
        &state,
        &mut out,
    );
    let point = map.screen([960., 560.]);
    let before = panels.composition.navigation;
    crate::panels::composition::render(
        &mut PaintList::default(),
        &mut atlas,
        area,
        &mut panels,
        &state,
        &Input {
            pressed: Some((MouseButton::Left, point)),
            mouse: point,
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        &mut out,
    );
    assert!(!panels.viewer_guides.dragging());
    assert_ne!(panels.composition.navigation, before);
    assert!(out.is_empty());
}
