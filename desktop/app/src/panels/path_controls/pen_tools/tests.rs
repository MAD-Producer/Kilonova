use super::*;
use motion_core::{Ease, Engine, Project};

fn path(closed: bool) -> VectorPath {
    VectorPath {
        tangents: Default::default(), id: 3,
        closed,
        nodes: [
            (11, [-90., -60., -15., -5., 60., -70.]),
            (42, [90., 60., -55., -60., 20., 8.]),
            (99, [-90., 70., 0., 0., 0., 0.]),
        ]
        .into_iter()
        .map(|(id, g)| PathNode {
            id,
            geometry: Track::constant(g),
        })
        .collect(),
    }
}
fn layer(mask: bool, closed: bool) -> Layer {
    let mut l = Layer::solid(7, "Pen tools", [640., 480.], [0.; 3], [1.; 4]);
    if mask {
        l.masks.push(LayerMask::new(5, path(closed)));
    } else {
        l.content = Content::Vector {
            vector: new_vector(path(closed)),
        };
    }
    l
}
fn point(mask: bool, node: u64) -> Point {
    Point {
        mask: mask.then_some(5),
        item: None,
        path: 3,
        node,
    }
}
fn target(l: &Layer, mask: bool) -> &VectorPath {
    if mask {
        &l.masks[0].path
    } else {
        let Content::Vector { vector } = &l.content else {
            panic!()
        };
        let VectorSource::Paths { paths } = &vector.source else {
            panic!()
        };
        &paths[0]
    }
}
fn assert_close(a: [f32; 2], b: [f32; 2]) {
    assert!(distance(a, b) < 0.00015, "{a:?} != {b:?}");
}
fn shape_equal(before: &VectorPath, after: &VectorPath, index: usize, t: f32, frame: f64) {
    let a = before.nodes[index].geometry.sample(frame).unwrap();
    let b = before.nodes[(index + 1) % before.nodes.len()]
        .geometry
        .sample(frame).unwrap();
    let left = after.nodes[index].geometry.sample(frame).unwrap();
    let middle = after.nodes[index + 1].geometry.sample(frame).unwrap();
    let right = after.nodes[(index + 2) % after.nodes.len()]
        .geometry
        .sample(frame).unwrap();
    for i in 0..=1000 {
        let u = i as f32 / 1000.;
        let actual = if u <= t {
            cubic(left, middle, u / t)
        } else {
            cubic(middle, right, (u - t) / (1. - t))
        };
        assert_close(actual, cubic(a, b, u));
    }
}
#[test]
fn subdivide_preserves_curves_open_and_closed_edges_and_unrelated_tracks() {
    for mask in [false, true] {
        for closed in [false, true] {
            for index in 0..if closed { 3 } else { 2 } {
                for t in [0.07, 0.37, 0.93] {
                    let l = layer(mask, closed);
                    let before = target(&l, mask).clone();
                    let p = point(mask, before.nodes[index].id);
                    let mut project = Project::new(640, 480, 30, 120).unwrap();
                    project.layers.push(l);
                    let mut engine = Engine::new(project.clone()).unwrap();
                    engine
                        .apply(
                            command(
                                &engine.project().layers[0],
                                p,
                                Edit::Add {
                                    next: before.nodes[(index + 1) % 3].id,
                                    t,
                                },
                                0,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                    let after = target(&engine.project().layers[0], mask);
                    assert_eq!(after.nodes[index + 1].id, 100);
                    assert_eq!(after.closed, closed);
                    shape_equal(&before, after, index, t, 0.);
                    for n in &before.nodes {
                        let actual = after.nodes.iter().find(|x| x.id == n.id).unwrap();
                        if n.id != p.node && n.id != before.nodes[(index + 1) % 3].id {
                            assert_eq!(actual, n);
                        }
                    }
                    let dir = tempfile::tempdir().unwrap();
                    motion_core::storage::save(dir.path(), engine.project()).unwrap();
                    assert_eq!(
                        &motion_core::storage::load(dir.path()).unwrap(),
                        engine.project()
                    );
                    assert!(engine.undo().unwrap());
                    assert_eq!(engine.project(), &project);
                }
            }
        }
    }
}
#[test]
fn subdivision_preserves_matching_temporal_curves_at_fractional_frames() {
    for static_end in [false, true] {
        let mut before = path(false);
        for (i, node) in before.nodes.iter_mut().take(2).enumerate() {
            if static_end && i == 1 {
                continue;
            }
            let g = node.geometry.value;
            for (frame, delta) in [(-8, 0.), (13, 90.), (41, -50.)] {
                let mut value = g;
                value[0] += delta;
                value[1] -= delta * 0.7;
                node.geometry.upsert(frame, value, Ease::InOut).unwrap();
            }
        }
        let mut after = before.clone();
        edit_path(&mut after, 11, Edit::Add { next: 42, t: 0.37 }, 0).unwrap();
        for i in -40..=200 {
            shape_equal(&before, &after, 0, 0.37, f64::from(i) / 4.);
        }
        assert_eq!(after.nodes[3], before.nodes[2]);
    }
}
#[test]
fn independent_animation_clocks_preserve_all_authored_poses_and_endpoint_timing() {
    for mask in [false, true] {
        for closed in [false, true] {
            for index in 0..if closed { 3 } else { 2 } {
                let mut l = layer(mask, closed);
                l.timeline = Some(motion_core::LayerTimeline {
                    in_frame: 0,
                    out_frame: 120,
                    offset_frame: 12,
                });
                let mut animated = path(closed);
                for (i, node) in animated.nodes.iter_mut().enumerate() {
                    for (frame, delta) in [
                        (-8 + i as i32, 0.),
                        (13 + i as i32 * 2, 90.),
                        (41 + i as i32 * 3, -50.),
                    ] {
                        let mut g = node.geometry.value;
                        g[0] += delta;
                        g[1] -= delta * 0.7;
                        node.geometry
                            .upsert(frame, g, if i == 1 { Ease::Hold } else { Ease::InOut })
                            .unwrap();
                    }
                    if i == 2 {
                        node.geometry.keys[0].curve = Some(motion_core::Curve {
                            space: motion_core::CurveSpace::Progress,
                            shape: motion_core::CurveShape::Cubic {
                                control1: [0.2, 0.1],
                                control2: [0.7, 0.9],
                                start: 0.,
                                end: 1.,
                            },
                        });
                    }
                }
                if mask {
                    l.masks[0].path = animated.clone();
                } else {
                    l.content = Content::Vector {
                        vector: new_vector(animated.clone()),
                    };
                }
                let mut project = Project::new(640, 480, 30, 120).unwrap();
                project.layers.push(l);
                let mut engine = Engine::new(project.clone()).unwrap();
                let t = 0.37;
                engine
                    .apply(
                        command(
                            &engine.project().layers[0],
                            point(mask, animated.nodes[index].id),
                            Edit::Add {
                                next: animated.nodes[(index + 1) % 3].id,
                                t,
                            },
                            30,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let after = target(&engine.project().layers[0], mask);
                let expected: std::collections::BTreeSet<_> = animated
                    .nodes
                    .iter()
                    .flat_map(|n| n.geometry.keys.iter().map(|k| k.frame))
                    .chain([18])
                    .collect();
                let middle = &after.nodes[index + 1].geometry;
                assert_eq!(
                    middle
                        .keys
                        .iter()
                        .map(|k| k.frame)
                        .collect::<std::collections::BTreeSet<_>>(),
                    expected
                );
                for frame in expected {
                    shape_equal(&animated, after, index, t, f64::from(frame));
                }
                for old in &animated.nodes {
                    let new = after.nodes.iter().find(|n| n.id == old.id).unwrap();
                    if old.id != animated.nodes[index].id
                        && old.id != animated.nodes[(index + 1) % 3].id
                    {
                        assert_eq!(new, old);
                        continue;
                    }
                    assert_eq!(new.geometry.keys.len(), old.geometry.keys.len());
                    let untouched = if old.id == animated.nodes[index].id {
                        2..4
                    } else {
                        4..6
                    };
                    assert_eq!(
                        new.geometry.value[untouched.clone()],
                        old.geometry.value[untouched.clone()]
                    );
                    for (a, b) in old.geometry.keys.iter().zip(&new.geometry.keys) {
                        assert_eq!(
                            (a.frame, a.ease, a.curve, &a.spatial),
                            (b.frame, b.ease, b.curve, &b.spatial)
                        );
                        assert_eq!(&a.value[..2], &b.value[..2]);
                        assert_eq!(a.value[untouched.clone()], b.value[untouched.clone()]);
                    }
                }
                for pair in middle.keys.windows(2) {
                    assert_eq!(pair[0].ease, Ease::Linear);
                    assert_eq!(pair[0].curve, None);
                    let frame = f64::from(pair[0].frame)
                        + (f64::from(pair[1].frame) - f64::from(pair[0].frame)) * 0.5;
                    let actual = middle.sample(frame).unwrap();
                    for (axis, value) in actual.into_iter().enumerate() {
                        assert!(
                            (value - (pair[0].value[axis] + pair[1].value[axis]) * 0.5).abs()
                                < 0.0001
                        );
                    }
                }
                // Adding again must accept the newly generated vertex's distinct clock.
                let mut twice = after.clone();
                edit_path(
                    &mut twice,
                    animated.nodes[index].id,
                    Edit::Add { next: 100, t: 0.5 },
                    18,
                )
                .unwrap();
                for k in &middle.keys {
                    shape_equal(after, &twice, index, 0.5, f64::from(k.frame));
                }
                let changed = engine.project().clone();
                let dir = tempfile::tempdir().unwrap();
                motion_core::storage::save(dir.path(), &changed).unwrap();
                assert_eq!(motion_core::storage::load(dir.path()).unwrap(), changed);
                assert!(engine.undo().unwrap());
                assert_eq!(engine.project(), &project);
                assert!(engine.redo().unwrap());
                assert_eq!(engine.project(), &changed);
            }
        }
    }
}

#[test]
fn independent_clock_union_obeys_key_budget_without_partial_mutation() {
    let mut l = layer(false, false);
    let Content::Vector { vector } = &mut l.content else {
        panic!()
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        panic!()
    };
    let count = motion_core::MAX_FRAMES as i32;
    for (i, node) in paths[0].nodes.iter_mut().take(2).enumerate() {
        node.geometry.keys = (0..count)
            .filter(|frame| frame % 2 == i as i32)
            .map(|frame| motion_core::Keyframe {
                frame,
                value: node.geometry.value,
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            })
            .collect();
        node.geometry.validate_local().unwrap();
    }
    // Each endpoint is valid; their union plus a new local edit frame exceeds one track.
    let before = l.clone();
    let error = command(
        &l,
        point(false, 11),
        Edit::Add { next: 42, t: 0.5 },
        motion_core::MAX_FRAMES,
    )
    .unwrap_err();
    assert!(error.to_string().contains("new vertex keyframe budget"));
    assert_eq!(l, before);
}

#[test]
fn stale_or_unrepresentable_edits_are_rejected_without_mutating_authored_data() {
    let mut l = layer(false, false);
    let before = l.clone();
    for edit in [
        Edit::Add { next: 99, t: 0.5 },
        Edit::Add { next: 42, t: 0. },
        Edit::Add { next: 42, t: 1. },
        Edit::Add {
            next: 42,
            t: f32::NAN,
        },
    ] {
        assert!(command(&l, point(false, 11), edit, 0).is_err());
        assert_eq!(l, before);
    }
    assert!(command(&l, point(false, 99), Edit::Add { next: 11, t: 0.5 }, 0).is_err());
    l.locked = true;
    assert!(command(&l, point(false, 11), Edit::Delete, 0).is_err());
    l.locked = false;
    let Content::Vector { vector } = &mut l.content else {
        panic!()
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        panic!()
    };
    paths[0].nodes[2].id = u64::MAX;
    let before = l.clone();
    let error = command(&l, point(false, 11), Edit::Add { next: 42, t: 0.5 }, 0).unwrap_err();
    assert!(error.to_string().contains("pen node ID overflow"));
    assert_eq!(l, before);
}
#[test]
fn conversion_preserves_anchor_lengths_and_other_keyframes_and_delete_opens_small_paths() {
    let mut p = path(true);
    let before = p.clone();
    edit_path(&mut p, 42, Edit::Smooth, 0).unwrap();
    let g = p.nodes[1].geometry.value;
    let old = before.nodes[1].geometry.value;
    assert_eq!(&g[..2], &old[..2]);
    assert!((g[2].hypot(g[3]) - old[2].hypot(old[3])).abs() < 0.0001);
    assert!((g[4].hypot(g[5]) - old[4].hypot(old[5])).abs() < 0.0001);
    assert!((g[2] * g[5] - g[3] * g[4]).abs() < 0.0001);
    assert!(g[2] * g[4] + g[3] * g[5] < 0.);
    assert_eq!(p.nodes[0], before.nodes[0]);
    assert_eq!(p.nodes[2], before.nodes[2]);
    let mut changed = g;
    changed[0] += 50.;
    p.nodes[1].geometry.upsert(0, g, Ease::Linear).unwrap();
    p.nodes[1]
        .geometry
        .upsert(30, changed, Ease::InOut)
        .unwrap();
    let key = p.nodes[1].geometry.keys[1].clone();
    edit_path(&mut p, 42, Edit::Corner, 0).unwrap();
    assert_eq!(p.nodes[1].geometry.sample(0.).unwrap()[2..], [0.; 4]);
    assert_eq!(p.nodes[1].geometry.keys[1], key);
    edit_path(&mut p, 42, Edit::Delete, 0).unwrap();
    assert!(!p.closed);
    assert_eq!(p.nodes, [before.nodes[0].clone(), before.nodes[2].clone()]);
}
#[test]
fn segment_picking_uses_zoomed_cubic_geometry_and_ignores_open_closing_edge() {
    for zoom in [0.5, 1., 2.] {
        let view = Rect::new(83., 47., 640. * zoom, 480. * zoom);
        let projection = Projection::new(
            DMat4::from_scale(glam::DVec3::new(2. / 640., 2. / 480., 1.)).to_cols_array(),
            view,
            [640., 480.],
        )
        .unwrap();
        let p = path(false);
        let sampled = |closed| {
            serde_json::json!({"id":3,"closed":closed,"nodes":p.nodes.iter().map(|n|
            serde_json::json!({"id":n.id,"geometry":n.geometry.value})).collect::<Vec<_>>()})
        };
        for t in [0.13, 0.37, 0.81] {
            let mouse = projection
                .screen(cubic(
                    p.nodes[0].geometry.value,
                    p.nodes[1].geometry.value,
                    t,
                ))
                .unwrap();
            let (point, next, actual) =
                segment(&[sampled(false)], &projection, [0.; 2], mouse).unwrap();
            assert_eq!((point.node, next), (11, 42));
            assert!((actual - t).abs() < 0.00005);
        }
        let closing = projection
            .screen(cubic(
                p.nodes[2].geometry.value,
                p.nodes[0].geometry.value,
                0.5,
            ))
            .unwrap();
        assert!(segment(&[sampled(false)], &projection, [0.; 2], closing).is_none());
        let (point, next, _) = segment(&[sampled(true)], &projection, [0.; 2], closing).unwrap();
        assert_eq!((point.node, next), (99, 11));
    }
}

struct Harness {
    engine: Engine,
    controls: super::super::Controls,
    atlas: TextAtlas,
    view: Rect,
    locale: Locale,
    mask: bool,
}
impl Harness {
    fn new(mask: bool, closed: bool, locale: Locale, zoom: f32) -> Self {
        let mut project = Project::new(640, 480, 30, 120).unwrap();
        project.layers.push(layer(mask, closed));
        Self {
            engine: Engine::new(project).unwrap(),
            controls: super::super::Controls {
                tool: if mask { Tool::Mask } else { Tool::Pen },
                mask: mask.then_some(5),
                ..Default::default()
            },
            atlas: TextAtlas::from_system(12.).unwrap(),
            view: Rect::new(80., 45., 640. * zoom, 480. * zoom),
            locale,
            mask,
        }
    }
    fn state(&self) -> Value {
        let l = &self.engine.project().layers[0];
        serde_json::json!({"root":"pen-tools","composition":"main","frame":0,"revision":self.engine.revision(),
            "project":self.engine.project(),"source_transforms":[{"id":7,
                "mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}],
            "mask_layers":[{"id":7,"masks":l.masks.iter().map(|m|serde_json::json!({"id":m.id,"path":{
                "id":m.path.id,"closed":m.path.closed,"nodes":m.path.nodes.iter().map(|n|
                    serde_json::json!({"id":n.id,"geometry":n.geometry.sample(0.).unwrap()})).collect::<Vec<_>>()}})).collect::<Vec<_>>()}]})
    }
    fn screen(&self, p: [f32; 2]) -> [f32; 2] {
        let p = if self.mask {
            [p[0] - 320., p[1] - 240.]
        } else {
            p
        };
        layer_projection(&self.state(), 7, self.view)
            .unwrap()
            .screen(p)
            .unwrap()
    }
    fn draw(&mut self, input: Input) -> Vec<Action> {
        let mut actions = vec![];
        let state = self.state();
        self.controls.draw(
            &mut PaintList::default(),
            &mut self.atlas,
            self.view,
            &state,
            Some(7),
            &input,
            self.locale,
            &mut actions,
        );
        actions
    }
    fn apply(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Edit(command) => self.engine.apply(command).unwrap(),
                Action::EditBatch(commands) => {
                    self.engine.apply_batch(commands).unwrap();
                }
                Action::History(0) => {
                    assert!(self.engine.undo().unwrap());
                }
                other => panic!("unexpected action: {other:?}"),
            }
        }
    }
    fn press(&mut self, p: [f32; 2]) {
        let actions = self.draw(Input {
            mouse: p,
            pressed: Some((MouseButton::Left, p)),
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        assert!(actions.is_empty(), "edit must wait for its release");
    }
    fn release(&mut self, p: [f32; 2]) -> Vec<Action> {
        self.draw(Input {
            mouse: p,
            released: Some((MouseButton::Left, p)),
            ..Default::default()
        })
    }
}
#[test]
fn release_commits_once_and_cancel_or_stale_context_never_edits() {
    for mask in [false, true] {
        for closed in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for zoom in [0.5, 1., 2.] {
                    let mut h = Harness::new(mask, closed, locale, zoom);
                    // Keep source masks in the visible portion of the layer.
                    if mask {
                        let mut project = h.engine.project().clone();
                        for n in &mut project.layers[0].masks[0].path.nodes {
                            n.geometry.value[0] += 320.;
                            n.geometry.value[1] += 240.;
                        }
                        h.engine = Engine::new(project).unwrap();
                    }
                    let before = h.engine.project().clone();
                    let p = target(&before.layers[0], mask);
                    let mouse = h.screen(cubic(
                        p.nodes[0].geometry.value,
                        p.nodes[1].geometry.value,
                        0.37,
                    ));
                    h.controls.pen_tools.mode = Mode::Add;
                    h.press(mouse);
                    assert!(h.controls.pen_tools.pending.is_some());
                    assert_eq!(h.engine.project(), &before);
                    let actions = h.release(mouse);
                    assert_eq!(actions.len(), 1);
                    h.apply(actions);
                    assert_eq!(target(&h.engine.project().layers[0], mask).nodes.len(), 4);
                    assert!(
                        h.release(mouse).is_empty(),
                        "a duplicate release must not repeat an edit"
                    );
                    h.engine.undo().unwrap();
                    assert_eq!(h.engine.project(), &before);
                    for cancel in [
                        Input {
                            focus_lost: true,
                            ..Default::default()
                        },
                        Input {
                            keys: vec![(Key::Escape, Default::default())],
                            ..Default::default()
                        },
                        Input::default(),
                    ] {
                        h.press(mouse);
                        assert!(h.draw(cancel).is_empty());
                        assert!(h.release(mouse).is_empty());
                        assert_eq!(h.engine.project(), &before);
                    }
                    h.press(mouse);
                    let mut state = h.state();
                    state["revision"] = serde_json::json!(h.engine.revision() + 1);
                    let mut actions = vec![];
                    h.controls.draw(
                        &mut PaintList::default(),
                        &mut h.atlas,
                        h.view,
                        &state,
                        Some(7),
                        &Input {
                            released: Some((MouseButton::Left, mouse)),
                            mouse,
                            ..Default::default()
                        },
                        locale,
                        &mut actions,
                    );
                    assert!(actions.is_empty());
                    assert_eq!(h.engine.project(), &before);
                }
            }
        }
    }
}
#[test]
fn tool_dropdown_owns_its_clicks_and_escape_leaves_the_path_unchanged() {
    for locale in [Locale::Zh, Locale::En] {
        let mut h = Harness::new(false, false, locale, 1.);
        let before = h.engine.project().clone();
        let point = [h.view.min[0] + 20., h.view.min[1] + 15.];
        h.press(point);
        assert!(h.controls.pen_tools.menu.open);
        assert!(h.release(point).is_empty());
        assert!(h.controls.draft.is_none());
        let actions = h.draw(Input {
            keys: vec![
                (Key::Down, Default::default()),
                (Key::Enter, Default::default()),
            ],
            ..Default::default()
        });
        assert!(actions.is_empty());
        assert_eq!(h.controls.pen_tools.mode, Mode::Add);
        assert!(!h.controls.pen_tools.menu.open);
        h.press(point);
        assert!(h
            .draw(Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            })
            .is_empty());
        assert!(!h.controls.pen_tools.menu.open);
        assert_eq!(h.engine.project(), &before);
        assert!(h
            .draw(Input {
                pressed: Some((MouseButton::Middle, point)),
                ..Default::default()
            })
            .is_empty());
        assert!(!h.controls.pen_tools.menu.open);
        assert!(
            h.controls.wants_keyboard(),
            "middle-clicking the tool area still focuses the viewport"
        );
    }
}

#[test]
fn grouped_topology_targets_authored_item_and_keeps_siblings_transforms_and_repeaters() {
    use motion_core::vector::groups::{GroupItem, Repeater, VectorGroup};
    let mut l = layer(false, true);
    let Content::Vector { vector } = &mut l.content else {
        panic!()
    };
    *vector = VectorGroup::wrap(vector.clone(), l.size);
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    group.transform.rotation.value = 12.;
    let mut sibling = group.items[0].clone();
    let GroupItem::Geometry { id, .. } = &mut sibling else {
        panic!()
    };
    *id = 8;
    group.items.push(sibling.clone());
    group.items.push(GroupItem::Repeater {
        id: 19,
        name: "Copies".into(),
        repeater: Repeater::default(),
    });
    let before = vector.clone();
    let command = command(
        &l,
        Point {
            item: Some(2),
            ..point(false, 11)
        },
        Edit::Add { next: 42, t: 0.37 },
        0,
    )
    .unwrap();
    let Command::Vector {
        action: VectorAction::Replace { mut vector },
        ..
    } = command
    else {
        panic!()
    };
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    assert_eq!(group.items[1], sibling);
    let VectorSource::Group { group: old } = &before.source else {
        panic!()
    };
    assert_eq!(group.transform, old.transform);
    assert_eq!(group.items[2], old.items[2]);
    let GroupItem::Geometry { vector: child, .. } = &mut group.items[0] else {
        panic!()
    };
    let VectorSource::Paths { paths } = &child.source else {
        panic!()
    };
    assert_eq!(paths[0].nodes.len(), 4);
    let GroupItem::Geometry {
        vector: original, ..
    } = &old.items[0]
    else {
        panic!()
    };
    let VectorSource::Paths { paths: previous } = &original.source else {
        panic!()
    };
    shape_equal(&previous[0], &paths[0], 0, 0.37, 0.);
}

#[test]
fn existing_parametric_conversion_is_required_then_pen_edits_preserve_paint_and_undo() {
    let mut l = layer(false, true);
    l.content = Content::Vector {
        vector: VectorContent::shape(motion_core::vector::ShapeKind::Rectangle),
    };
    let mut project = Project::new(640, 480, 30, 120).unwrap();
    project.layers.push(l);
    let mut engine = Engine::new(project.clone()).unwrap();
    assert!(command(
        &engine.project().layers[0],
        Point {
            path: 1,
            node: 1,
            ..point(false, 11)
        },
        Edit::Corner,
        0
    )
    .is_err());
    engine
        .apply(Command::Vector {
            object: 7,
            action: VectorAction::ConvertToPath { frame: 0 },
        })
        .unwrap();
    let converted = engine.project().clone();
    let p = target(&converted.layers[0], false);
    let point = Point {
        path: p.id,
        node: p.nodes[0].id,
        ..point(false, 11)
    };
    engine
        .apply(
            command(
                &engine.project().layers[0],
                point,
                Edit::Add {
                    next: p.nodes[1].id,
                    t: 0.5,
                },
                0,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        target(&engine.project().layers[0], false).nodes.len(),
        p.nodes.len() + 1
    );
    let Content::Vector { vector: before } = &converted.layers[0].content else {
        panic!()
    };
    let Content::Vector { vector: after } = &engine.project().layers[0].content else {
        panic!()
    };
    assert_eq!(before.fill, after.fill);
    assert_eq!(before.stroke, after.stroke);
    assert_eq!(before.fill_rule, after.fill_rule);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &converted);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &project);
}

#[test]
fn rejection_reaches_default_file_and_error_stderr_with_threshold_suppression_and_reset() {
    const CHILD: &str = "MOTION_PEN_TOOLS_FAILURE_CHILD";
    const TEST:&str="panels::path_controls::pen_tools::tests::rejection_reaches_default_file_and_error_stderr_with_threshold_suppression_and_reset";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut h = Harness::new(false, false, Locale::Zh, 1.);
        let mut project = h.engine.project().clone();
        let Content::Vector { vector } = &mut project.layers[0].content else {
            panic!()
        };
        let VectorSource::Paths { paths } = &mut vector.source else {
            panic!()
        };
        paths[0].nodes[2].id = u64::MAX;
        h.engine = Engine::new(project.clone()).unwrap();
        h.controls.pen_tools.mode = Mode::Add;
        let path = target(&project.layers[0], false);
        let mouse = h.screen(cubic(
            path.nodes[0].geometry.value,
            path.nodes[1].geometry.value,
            0.37,
        ));
        let errors = || {
            std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .lines()
                .filter(|l| l.contains("ERROR") && l.contains("site=path.pen_edit "))
                .count()
        };
        for _ in 0..59 {
            h.press(mouse);
        }
        assert_eq!(errors(), 0);
        h.press(mouse);
        assert_eq!(errors(), 1);
        for _ in 0..80 {
            h.press(mouse);
        }
        assert_eq!(errors(), 1);
        assert!(h
            .controls
            .notice
            .as_deref()
            .unwrap()
            .contains("pen node ID overflow"));
        // A normal miss starts a fresh consecutive-attempt observation window.
        let blank = h.screen([160., 100.]);
        h.press(blank);
        for _ in 0..59 {
            h.press(mouse);
        }
        assert_eq!(errors(), 1);
        h.press(mouse);
        assert_eq!(errors(), 2);
        assert_eq!(h.engine.project(), &project);
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        assert!(file.contains(
            "object=7 mask=None item=None path=3 node=11 mode=Add frame=0 phase=prepare reason="
        ));
        assert!(file.contains("pen edit rejected:"));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("site=path.pen_edit ")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("pen edit rejected"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("persistent degradation"));
}
