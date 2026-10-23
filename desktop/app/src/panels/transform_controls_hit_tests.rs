use super::*;

#[test]
fn projected_small_layer_corners_are_captured_at_each_zoom_and_offset() {
    let (_folder, engine, _, _) = super::tests::fixture(false);
    let state = engine
        .edit(
            serde_json::to_string(&Command::Add {
                layer: motion_core::Layer::solid(8, "Small", [4.; 2], [960., 540., 0.], [1.; 4]),
            })
            .unwrap(),
        )
        .unwrap();
    for view in [
        Rect::new(0., 0., 1920., 1080.),
        Rect::new(200., 80., 960., 540.),
        Rect::new(-350., -180., 3840., 2160.),
        Rect::new(200., 80., 96., 54.),
    ] {
        let pose = Pose::new(&state, 8, view).unwrap();
        for (index, point) in pose.corners.into_iter().enumerate() {
            for alt in [false, true] {
                let mut controls = Controls::default();
                let mut actions = Vec::new();
                controls.draw_in_viewport(
                    &mut PaintList::default(),
                    view,
                    view,
                    &state,
                    Some(8),
                    &Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        held: Some(MouseButton::Left),
                        modifiers: motion_ui::input::Modifiers {
                            alt,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    &mut actions,
                );
                assert_eq!(
                    controls.drag.as_ref().map(|drag| drag.handle),
                    Some(Handle::Scale(true, true, Some(index))),
                    "view={view:?} corner={index} alt={alt}"
                );
                assert!(controls.cursor.is_some());
                assert!(actions.is_empty());
            }
        }
    }
    assert_eq!(engine.state().unwrap()["project"], state["project"]);
}

#[test]
fn overlapping_handles_choose_the_closest_geometric_target() {
    let (_folder, _engine, state, view) = super::tests::fixture(false);
    let mut pose = Pose::new(&state, 7, view).unwrap();
    pose.corners = [[100., 100.], [104., 100.], [104., 104.], [100., 104.]];
    pose.pivot = [102., 102.];
    pose.rotate = [102., 96.];
    let styles = PaintList::default().styles();
    for (point, expected) in [
        (pose.corners[2], Handle::Scale(true, true, Some(2))),
        (pose.corners[1], Handle::Scale(true, true, Some(1))),
        ([102., 104.], Handle::Scale(false, true, Some(6))),
        (pose.rotate, Handle::Rotate),
    ] {
        assert_eq!(pose.handle(point, Tool::Move, &styles), Some(expected));
    }
}

#[test]
fn alt_does_not_implicitly_start_an_anchor_gesture() {
    let (_folder, _engine, state, view) = super::tests::fixture(false);
    let pose = Pose::new(&state, 7, view).unwrap();
    for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
        for alt in [false, true] {
            let mut controls = Controls {
                tool,
                ..Default::default()
            };
            let mut actions = Vec::new();
            controls.draw_in_viewport(
                &mut PaintList::default(),
                view,
                view,
                &state,
                Some(7),
                &Input {
                    mouse: pose.pivot,
                    pressed: Some((MouseButton::Left, pose.pivot)),
                    held: Some(MouseButton::Left),
                    modifiers: motion_ui::input::Modifiers {
                        alt,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &mut actions,
            );
            assert!(
                controls.drag.is_none(),
                "non-anchor tool captured the pivot, alt={alt}"
            );
            assert!(controls.pick.is_some());
            assert!(actions.is_empty());
        }
    }
    for alt in [false, true] {
        let mut controls = Controls {
            tool: Tool::Anchor,
            ..Default::default()
        };
        let mut actions = Vec::new();
        controls.draw_in_viewport(
            &mut PaintList::default(),
            view,
            view,
            &state,
            Some(7),
            &Input {
                mouse: pose.pivot,
                pressed: Some((MouseButton::Left, pose.pivot)),
                held: Some(MouseButton::Left),
                modifiers: motion_ui::input::Modifiers {
                    alt,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!(
            controls.drag.as_ref().map(|drag| drag.handle),
            Some(Handle::Anchor)
        );
        assert!(actions.is_empty());
    }
}

#[test]
fn radii_preserve_default_boundaries_and_follow_window_overrides() {
    let (_folder, _engine, state, view) = super::tests::fixture(false);
    let mut pose = Pose::new(&state, 7, view).unwrap();
    pose.corners = [[100., 100.], [300., 100.], [300., 300.], [100., 300.]];
    pose.pivot = [200., 200.];
    pose.rotate = [200., 50.];
    let styles = PaintList::default().styles();
    let expanded = styles
        .with_overrides(
            br#"{"tokens":{"sizes":{
        "transform.anchor.hit_radius":24,"transform.rotate.hit_radius":18,
        "transform.corner.hit_radius":16,"transform.edge.hit_radius":14
    }}}"#,
        )
        .unwrap();
    for (token, radius, center, tool, handle) in [
        (
            "transform.anchor.hit_radius",
            12.,
            pose.pivot,
            Tool::Anchor,
            Handle::Anchor,
        ),
        (
            "transform.rotate.hit_radius",
            9.,
            pose.rotate,
            Tool::Move,
            Handle::Rotate,
        ),
        (
            "transform.corner.hit_radius",
            8.,
            pose.corners[0],
            Tool::Move,
            Handle::Scale(true, true, Some(0)),
        ),
        (
            "transform.edge.hit_radius",
            7.,
            [200., 100.],
            Tool::Move,
            Handle::Scale(false, true, Some(4)),
        ),
    ] {
        assert_eq!(styles.metric(token, -1.), radius);
        let inside = [center[0], center[1] + radius - 0.25];
        let boundary = [center[0], center[1] + radius];
        assert_eq!(pose.handle(inside, tool, &styles), Some(handle));
        assert_eq!(pose.handle(boundary, tool, &styles), None);
        assert_eq!(pose.handle(boundary, tool, &expanded), Some(handle));
    }
}

#[test]
fn exact_ties_are_stable_and_locked_or_invalid_points_have_no_target() {
    let (_folder, _engine, state, view) = super::tests::fixture(false);
    let mut pose = Pose::new(&state, 7, view).unwrap();
    pose.corners = [[100., 100.]; 4];
    pose.pivot = [100., 100.];
    pose.rotate = pose.pivot;
    let styles = PaintList::default().styles();
    assert_eq!(
        pose.handle(pose.pivot, Tool::Move, &styles),
        Some(Handle::Rotate)
    );
    assert_eq!(
        pose.handle(pose.pivot, Tool::Anchor, &styles),
        Some(Handle::Anchor)
    );
    assert_eq!(pose.handle([f32::NAN, 100.], Tool::Move, &styles), None);
    assert_eq!(pose.handle([10_000.; 2], Tool::Anchor, &styles), None);
    pose.locked = true;
    assert_eq!(pose.handle(pose.pivot, Tool::Anchor, &styles), None);
}

#[test]
fn cursor_highlight_and_press_use_the_same_window_hit_style() {
    let (_folder, _engine, state, view) = super::tests::fixture(false);
    let pose = Pose::new(&state, 7, view).unwrap();
    let point = [pose.corners[0][0] + 10., pose.corners[0][1]];
    let styles = PaintList::default().styles();
    let expanded = styles
        .with_overrides(br#"{"tokens":{"sizes":{"transform.corner.hit_radius":16}}}"#)
        .unwrap();
    for expanded in [None, Some(expanded)] {
        let mut list = PaintList::default();
        let enlarged = expanded.is_some();
        list.stylesheet = expanded.map(std::sync::Arc::new);
        let mut controls = Controls::default();
        let mut actions = Vec::new();
        controls.draw_in_viewport(
            &mut list,
            view,
            view,
            &state,
            Some(7),
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!(controls.cursor.is_some(), enlarged);
        assert_eq!(
            controls.drag.as_ref().map(|drag| drag.handle),
            enlarged.then_some(Handle::Scale(true, true, Some(0)))
        );
        let foreground = list.styles().colors().TEXT;
        let highlighted: Vec<_> = list
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                paint::Shape::Procedural {
                    area,
                    kind: paint::Pattern::Box { border, .. },
                    ..
                } if *border == foreground => Some(*area),
                _ => None,
            })
            .collect();
        assert_eq!(
            highlighted.len(),
            usize::from(enlarged),
            "hover highlight must agree with the captured grip"
        );
        if enlarged {
            assert!(highlighted[0].contains(pose.corners[0]));
        }
        assert!(actions.is_empty());
    }
}
