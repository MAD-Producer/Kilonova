use super::*;
use motion_ui::input::Modifiers;

fn path(closed: bool) -> VectorPath {
    VectorPath {
        id: 3,
        closed,
        nodes: [
            (11, [-90., -60., -12., 0., 12., 0.]),
            (42, [90., -60., -12., 0., 12., 0.]),
            (99, [90., 60., 0., 0., 0., 0.]),
            (123, [-90., 60., 0., 0., 0., 0.]),
        ]
        .into_iter()
        .map(|(id, g)| PathNode {
            id,
            geometry: Track::constant(g),
        })
        .collect(),
    }
}
struct Harness {
    engine: motion_core::Engine,
    controls: Controls,
    atlas: TextAtlas,
    view: Rect,
    locale: Locale,
    frame: u32,
}
impl Harness {
    fn new(mask: bool, closed: bool, locale: Locale, scale: f32) -> Self {
        let mut p = motion_core::Project::new(640, 480, 30, 120).unwrap();
        let mut l = Layer::solid(7, "Point selection", [100.; 2], [0.; 3], [1.; 4]);
        if mask {
            l.masks.push(LayerMask::new(1, path(closed)));
        } else {
            l.content = Content::Vector {
                vector: new_vector(path(closed)),
            };
        }
        p.layers.push(l);
        Self {
            engine: motion_core::Engine::new(p).unwrap(),
            controls: Controls {
                tool: if mask { Tool::Mask } else { Tool::Pen },
                mask: mask.then_some(1),
                ..Default::default()
            },
            atlas: TextAtlas::from_system(12.).unwrap(),
            view: Rect::new(83., 47., 640. * scale, 480. * scale),
            locale,
            frame: 0,
        }
    }
    fn state(&self) -> Value {
        let layer = &self.engine.project().layers[0];
        let masks:Vec<_> = layer.masks.iter().map(|m|serde_json::json!({"id":m.id,"path":{
            "id":m.path.id,"closed":m.path.closed,"nodes":m.path.nodes.iter().map(|n|
                serde_json::json!({"id":n.id,"geometry":n.geometry.sample(layer.local_frame(f64::from(self.frame))),
                    "animated":n.geometry.is_animated()})).collect::<Vec<_>>()}})).collect();
        serde_json::json!({"root":"point-selection-fixture","composition":"comp-main","frame":self.frame,
            "revision":self.engine.revision(),"project":self.engine.project(),"mask_layers":[{"id":7,"masks":masks}],
            "source_transforms":[{"id":7,"mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}]})
    }
    fn position(&self, point: [f32; 2]) -> [f32; 2] {
        let offset = if self.controls.tool == Tool::Mask {
            50.
        } else {
            0.
        };
        layer_projection(&self.state(), 7, self.view)
            .unwrap()
            .screen([point[0] - offset, point[1] - offset])
            .unwrap()
    }
    fn draw(&mut self, input: Input) -> Vec<Action> {
        let mut out = vec![];
        let state = self.state();
        self.controls.draw(
            &mut PaintList::default(),
            &mut self.atlas,
            self.view,
            &state,
            Some(7),
            &input,
            self.locale,
            &mut out,
        );
        out
    }
    fn input(&mut self, input: Input) -> Vec<i32> {
        let actions = self.draw(input);
        let mut history = vec![];
        for action in actions {
            match action {
                Action::Edit(c) => self.engine.apply(c).unwrap(),
                Action::EditBatch(cs) => {
                    self.engine.apply_batch(cs).unwrap();
                }
                Action::History(h) => {
                    history.push(h);
                    match h {
                        0 => {
                            self.engine.undo().unwrap();
                        }
                        1 => {
                            self.engine.redo().unwrap();
                        }
                        2 => self.engine.begin_gesture().unwrap(),
                        3 | 4 => self.engine.end_gesture(h == 3).unwrap(),
                        _ => panic!("unexpected history {h}"),
                    }
                }
                other => panic!("unexpected path action {other:?}"),
            }
        }
        history
    }
    fn click(&mut self, point: [f32; 2], mods: Modifiers) {
        let p = self.position(point);
        self.input(Input {
            mouse: p,
            pressed: Some((MouseButton::Left, p)),
            held: Some(MouseButton::Left),
            modifiers: mods,
            ..Default::default()
        });
        assert!(self
            .input(Input {
                mouse: p,
                released: Some((MouseButton::Left, p)),
                ..Default::default()
            })
            .is_empty());
    }
    fn values(&self) -> Vec<[f32; 6]> {
        if self.controls.tool == Tool::Mask {
            self.engine.project().layers[0].masks[0]
                .path
                .nodes
                .iter()
                .map(|n| n.geometry.sample(f64::from(self.frame)))
                .collect()
        } else {
            source_geometry::paths(&self.engine.project().layers[0], f64::from(self.frame)).unwrap()
                [0]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| geometry(n).unwrap())
                .collect()
        }
    }
    fn persist_undo(&mut self, before: &motion_core::Project) {
        let dir = tempfile::tempdir().unwrap();
        motion_core::storage::save(dir.path(), self.engine.project()).unwrap();
        assert_eq!(
            motion_core::storage::load(dir.path()).unwrap(),
            *self.engine.project()
        );
        self.engine.undo().unwrap();
        assert_eq!(self.engine.project(), before);
    }
}
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}
fn control() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

#[test]
fn point_add_toggle_group_drag_release_and_one_undo_at_multiple_zooms() {
    for mask in [false, true] {
        for closed in [false, true] {
            for locale in [Locale::Zh, Locale::En] {
                for scale in [0.5, 1., 2.] {
                    let mut h = Harness::new(mask, closed, locale, scale);
                    let before = h.engine.project().clone();
                    h.click([-90., -60.], Modifiers::default());
                    h.click([90., -60.], shift());
                    assert_eq!(h.controls.point_selection.points.len(), 2);
                    h.click([90., -60.], shift());
                    assert_eq!(h.controls.point_selection.points.len(), 1);
                    h.click([90., -60.], shift());
                    let values = h.values();
                    let start = h.position([-90., -60.]);
                    h.input(Input {
                        mouse: start,
                        pressed: Some((MouseButton::Left, start)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    });
                    let end = h.position([-65., -45.]);
                    assert_eq!(
                        h.input(Input {
                            mouse: [9000., 9000.],
                            released: Some((MouseButton::Left, end)),
                            ..Default::default()
                        }),
                        vec![2, 3]
                    );
                    let after = h.values();
                    for index in 0..2 {
                        assert!((after[index][0] - values[index][0] - 25.).abs() < 0.002);
                        assert!((after[index][1] - values[index][1] - 15.).abs() < 0.002);
                        assert_eq!(
                            &after[index][2..],
                            &values[index][2..],
                            "relative handles stay intact"
                        );
                    }
                    assert_eq!(&after[2..], &values[2..]);
                    assert!(h
                        .input(Input {
                            mouse: [200., 180.],
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        })
                        .is_empty());
                    assert_eq!(h.values(), after, "motion after release cannot move nodes");
                    h.persist_undo(&before);
                }
            }
        }
    }
}

#[test]
fn segment_moves_endpoints_and_handles_only_and_closed_edge_is_pickable() {
    for mask in [false, true] {
        for closed in [false, true] {
            let mut h = Harness::new(mask, closed, Locale::Zh, 1.);
            let before = h.engine.project().clone();
            let values = h.values();
            let start = h.position([0., -60.]);
            h.input(Input {
                mouse: start,
                pressed: Some((MouseButton::Left, start)),
                held: Some(MouseButton::Left),
                ..Default::default()
            });
            assert_eq!(h.controls.point_selection.points.len(), 2);
            assert!(h.controls.draft.is_none());
            let end = h.position([10., -40.]);
            assert_eq!(
                h.input(Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                }),
                vec![2, 3]
            );
            let after = h.values();
            for index in 0..2 {
                assert!((after[index][0] - values[index][0] - 10.).abs() < 0.002);
                assert_eq!(&after[index][2..], &values[index][2..]);
            }
            assert_eq!(&after[2..], &values[2..]);
            h.persist_undo(&before);
            let closing = h.position([-90., 0.]);
            let actions = h.draw(Input {
                mouse: closing,
                pressed: Some((MouseButton::Left, closing)),
                held: Some(MouseButton::Left),
                ..Default::default()
            });
            if closed {
                assert!(actions.is_empty());
            } else {
                assert!(
                    matches!(actions.as_slice(), [Action::Seek(0.)]),
                    "open edge continues normal pen drafting: {actions:?}"
                );
            }
            assert_eq!(
                h.controls.point_selection.editing(),
                closed,
                "only closed path has the closing segment"
            );
        }
    }
}

#[test]
fn marquee_replace_add_toggle_cancel_delete_and_save_reopen() {
    for mask in [false, true] {
        for scale in [0.5, 2.] {
            let mut h = Harness::new(mask, true, Locale::En, scale);
            let before = h.engine.project().clone();
            let a = h.position([-110., -80.]);
            let b = h.position([110., -40.]);
            for mods in [
                control(),
                shift(),
                Modifiers {
                    control: true,
                    shift: true,
                    alt: false,
                },
            ] {
                let prev = h.controls.point_selection.points.clone();
                h.input(Input {
                    mouse: a,
                    pressed: Some((MouseButton::Left, a)),
                    held: Some(MouseButton::Left),
                    modifiers: mods,
                    ..Default::default()
                });
                h.input(Input {
                    mouse: b,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                });
                assert!(h.controls.point_selection.editing());
                h.input(Input {
                    keys: vec![(Key::Escape, Modifiers::default())],
                    mouse: b,
                    ..Default::default()
                });
                assert_eq!(h.controls.point_selection.points, prev);
                h.input(Input {
                    mouse: a,
                    pressed: Some((MouseButton::Left, a)),
                    held: Some(MouseButton::Left),
                    modifiers: mods,
                    ..Default::default()
                });
                assert!(h
                    .input(Input {
                        mouse: [9000., 9000.],
                        released: Some((MouseButton::Left, b)),
                        ..Default::default()
                    })
                    .is_empty());
                assert_eq!(
                    h.controls.point_selection.points.len(),
                    if mods.control && mods.shift { 0 } else { 2 }
                );
            }
            h.click([-90., -60.], Modifiers::default());
            h.click([90., -60.], shift());
            h.input(Input {
                keys: vec![(Key::Delete, Modifiers::default())],
                ..Default::default()
            });
            assert_eq!(h.values().len(), 2);
            assert!(h.controls.point_selection.points.is_empty());
            h.persist_undo(&before);
        }
    }
}

#[test]
fn animated_masks_with_identical_node_ids_move_together_without_pose_overwrite() {
    let mut h = Harness::new(true, true, Locale::Zh, 1.);
    let mut project = h.engine.project().clone();
    let mut mask = project.layers[0].masks[0].clone();
    mask.id = 2;
    for n in &mut mask.path.nodes {
        n.geometry.value[0] += 240.;
    }
    project.layers[0].masks.push(mask);
    for m in &mut project.layers[0].masks {
        for n in &mut m.path.nodes {
            let value = n.geometry.value;
            n.geometry.set_animated(0, true).unwrap();
            let mut end = value;
            end[1] += 40.;
            n.geometry.set_at(30, end).unwrap();
        }
    }
    h.engine = motion_core::Engine::new(project).unwrap();
    h.frame = 15;
    let before = h.engine.project().clone();
    h.click([-90., -40.], Modifiers::default());
    h.click([90., -40.], shift());
    h.click([150., -40.], shift());
    assert_eq!(h.controls.point_selection.points.len(), 3);
    let start = h.position([150., -40.]);
    let end = h.position([170., -30.]);
    h.input(Input {
        mouse: start,
        pressed: Some((MouseButton::Left, start)),
        held: Some(MouseButton::Left),
        ..Default::default()
    });
    h.input(Input {
        mouse: end,
        released: Some((MouseButton::Left, end)),
        ..Default::default()
    });
    for (m_index, m) in h.engine.project().layers[0].masks.iter().enumerate() {
        for (n_index, n) in m.path.nodes.iter().enumerate() {
            let original = &before.layers[0].masks[m_index].path.nodes[n_index].geometry;
            let edited = (m_index == 0 && n_index < 2) || (m_index == 1 && n_index == 0);
            let a = original.sample(15.);
            let b = n.geometry.sample(15.);
            assert!((b[0] - a[0] - if edited { 20. } else { 0. }).abs() < 0.002);
            assert!((b[1] - a[1] - if edited { 10. } else { 0. }).abs() < 0.002);
            assert_eq!(n.geometry.sample(0.), original.sample(0.));
            assert_eq!(n.geometry.sample(30.), original.sample(30.));
        }
    }
    h.persist_undo(&before);
}

#[test]
fn moving_selection_cancels_on_escape_focus_loss_time_change_and_lock() {
    for reason in 0..4 {
        let mut h = Harness::new(false, true, Locale::Zh, 1.);
        h.click([-90., -60.], Modifiers::default());
        h.click([90., -60.], shift());
        let before = h.engine.project().clone();
        let start = h.position([-90., -60.]);
        let end = h.position([-70., -40.]);
        h.input(Input {
            mouse: start,
            pressed: Some((MouseButton::Left, start)),
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        assert_eq!(
            h.input(Input {
                mouse: end,
                held: Some(MouseButton::Left),
                ..Default::default()
            }),
            vec![2]
        );
        let mut input = Input {
            mouse: end,
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        match reason {
            0 => input.keys.push((Key::Escape, Modifiers::default())),
            1 => input.focus_lost = true,
            2 => h.frame = 1,
            3 => {
                // A readonly snapshot must cancel, without adding a lock edit to the active gesture.
                let mut state = h.state();
                state["project"]["layers"][0]["locked"] = Value::Bool(true);
                let mut out = vec![];
                h.controls.draw(
                    &mut PaintList::default(),
                    &mut h.atlas,
                    h.view,
                    &state,
                    Some(7),
                    &input,
                    h.locale,
                    &mut out,
                );
                assert!(matches!(out.as_slice(), [Action::History(4)]));
                h.engine.end_gesture(false).unwrap();
                assert_eq!(h.engine.project(), &before);
                continue;
            }
            _ => unreachable!(),
        }
        assert_eq!(h.input(input), vec![4]);
        assert_eq!(h.engine.project(), &before);
        assert!(!h.controls.point_selection.editing());
    }
}

#[test]
fn invalid_batch_is_reported_without_partial_edits_and_recovery_is_possible() {
    let mut h = Harness::new(false, true, Locale::Zh, 1.);
    h.click([-90., -60.], Modifiers::default());
    h.click([90., -60.], shift());
    let before = h.engine.project().clone();
    let start = h.position([-90., -60.]);
    h.input(Input {
        mouse: start,
        pressed: Some((MouseButton::Left, start)),
        held: Some(MouseButton::Left),
        ..Default::default()
    });
    assert!(h
        .input(Input {
            mouse: [40000., 0.],
            held: Some(MouseButton::Left),
            ..Default::default()
        })
        .is_empty());
    assert_eq!(h.engine.project(), &before);
    let end = h.position([-70., -40.]);
    assert_eq!(
        h.input(Input {
            mouse: end,
            released: Some((MouseButton::Left, end)),
            ..Default::default()
        }),
        vec![2, 3]
    );
    h.persist_undo(&before);
}

#[test]
fn grouped_selection_uses_authored_owners_and_world_translation_before_repeaters() {
    use motion_core::vector::groups::{GroupItem, Repeater, VectorGroup};
    let mut h = Harness::new(false, true, Locale::En, 1.);
    let mut project = h.engine.project().clone();
    let mut vector = VectorGroup::wrap(new_vector(path(true)), [180., 120.]);
    project.version = 20;
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    let mut second = group.items[0].clone();
    let GroupItem::Geometry { id, position, .. } = &mut second else {
        panic!()
    };
    *id = 42;
    *position = Track::constant([240., 0.]);
    group.items.push(second);
    group.transform.rotation = Track::constant(12.);
    group.items.push(GroupItem::Repeater {
        id: 50,
        name: "Copies".into(),
        repeater: Repeater::default(),
    });
    project.layers[0].content = Content::Vector { vector };
    h.engine = motion_core::Engine::new(project).unwrap();
    let before = h.engine.project().clone();
    let paths = source_geometry::paths(&before.layers[0], 0.).unwrap();
    assert_eq!(paths.len(), 2);
    let base = layer_projection(&h.state(), 7, h.view).unwrap();
    let positions: Vec<_> = paths
        .iter()
        .map(|p| {
            source_geometry::projection(&base, p)
                .unwrap()
                .screen([-90., -60.])
                .unwrap()
        })
        .collect();
    for (i, p) in positions.iter().enumerate() {
        h.input(Input {
            mouse: *p,
            pressed: Some((MouseButton::Left, *p)),
            held: Some(MouseButton::Left),
            modifiers: if i == 0 {
                Modifiers::default()
            } else {
                shift()
            },
            ..Default::default()
        });
        h.input(Input {
            mouse: *p,
            released: Some((MouseButton::Left, *p)),
            ..Default::default()
        });
    }
    assert_eq!(h.controls.point_selection.points.len(), 2);
    h.input(Input {
        mouse: positions[0],
        pressed: Some((MouseButton::Left, positions[0])),
        held: Some(MouseButton::Left),
        ..Default::default()
    });
    let end = [positions[0][0] + 20., positions[0][1] + 15.];
    h.input(Input {
        mouse: end,
        released: Some((MouseButton::Left, end)),
        ..Default::default()
    });
    let after = source_geometry::paths(&h.engine.project().layers[0], 0.).unwrap();
    for (index, path) in after.iter().enumerate() {
        let g = geometry(&path["nodes"][0]).unwrap();
        let point = source_geometry::projection(&base, path)
            .unwrap()
            .screen([g[0], g[1]])
            .unwrap();
        assert!(
            distance(
                point,
                [positions[index][0] + 20., positions[index][1] + 15.]
            ) < 0.003
        );
        assert_eq!(&g[2..], &[-12., 0., 12., 0.]);
    }
    h.persist_undo(&before);
    h.input(Input {
        keys: vec![(Key::Delete, Modifiers::default())],
        ..Default::default()
    });
    let after = source_geometry::paths(&h.engine.project().layers[0], 0.).unwrap();
    assert!(after
        .iter()
        .all(|p| p["nodes"].as_array().unwrap().len() == 3));
    let Content::Vector { vector } = &h.engine.project().layers[0].content else {
        panic!()
    };
    let VectorSource::Group { group } = &vector.source else {
        panic!()
    };
    assert!(matches!(group.items[2], GroupItem::Repeater { id: 50, .. }));
    h.persist_undo(&before);
}

#[test]
fn animated_vector_batch_keeps_existing_keys_and_failed_second_command_rolls_back() {
    let mut h = Harness::new(false, true, Locale::En, 1.);
    let mut project = h.engine.project().clone();
    let Content::Vector { vector } = &mut project.layers[0].content else {
        panic!()
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        panic!()
    };
    for n in &mut paths[0].nodes {
        let mut end = n.geometry.value;
        end[1] += 40.;
        n.geometry.set_animated(0, true).unwrap();
        n.geometry.set_at(30, end).unwrap();
    }
    h.engine = motion_core::Engine::new(project).unwrap();
    h.frame = 15;
    h.click([-90., -40.], Modifiers::default());
    h.click([90., -40.], shift());
    let before = h.engine.project().clone();
    let original = h.values();
    let a = h.position([-90., -40.]);
    let b = h.position([-70., -30.]);
    h.input(Input {
        mouse: a,
        pressed: Some((MouseButton::Left, a)),
        held: Some(MouseButton::Left),
        ..Default::default()
    });
    let actions = h.draw(Input {
        mouse: b,
        released: Some((MouseButton::Left, b)),
        ..Default::default()
    });
    assert!(
        matches!(actions.as_slice(),[Action::History(2),Action::EditBatch(cs),Action::History(3)] if cs.len()==2)
    );
    let Action::EditBatch(commands) = &actions[1] else {
        panic!()
    };
    let mut invalid = commands.clone();
    let Command::Vector {
        action: VectorAction::SetNode { node, .. },
        ..
    } = &mut invalid[1]
    else {
        panic!()
    };
    *node = 99999;
    assert!(h.engine.apply_batch(invalid).is_err());
    assert_eq!(h.engine.project(), &before);
    h.engine.apply_batch(commands.clone()).unwrap();
    let after = h.values();
    for i in 0..2 {
        assert!((after[i][0] - original[i][0] - 20.).abs() < 0.002);
        assert!((after[i][1] - original[i][1] - 10.).abs() < 0.002);
    }
    let Content::Vector { vector } = &h.engine.project().layers[0].content else {
        panic!()
    };
    let VectorSource::Paths { paths } = &vector.source else {
        panic!()
    };
    for node in &paths[0].nodes[..2] {
        assert_eq!(
            node.geometry
                .keys
                .iter()
                .map(|k| k.frame)
                .collect::<Vec<_>>(),
            vec![0, 15, 30]
        );
    }
    assert_eq!(&after[2..], &original[2..]);
    h.persist_undo(&before);
}

#[test]
fn context_changes_and_unavailable_projection_cancel_without_reusing_selection() {
    for reason in 0..4 {
        let mut h = Harness::new(false, true, Locale::Zh, 1.);
        h.click([-90., -60.], Modifiers::default());
        h.click([90., -60.], shift());
        let before = h.engine.project().clone();
        let a = h.position([-90., -60.]);
        let b = h.position([-70., -40.]);
        h.input(Input {
            mouse: a,
            pressed: Some((MouseButton::Left, a)),
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        h.input(Input {
            mouse: b,
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        let mut state = h.state();
        match reason {
            0 => state["root"] = "other-project".into(),
            1 => state["composition"] = "comp-other".into(),
            2 => state["source_transforms"] = serde_json::json!([]),
            3 => state["observing"] = true.into(),
            _ => unreachable!(),
        }
        let mut actions = vec![];
        h.controls.draw(
            &mut PaintList::default(),
            &mut h.atlas,
            h.view,
            &state,
            Some(7),
            &Input {
                mouse: b,
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            h.locale,
            &mut actions,
        );
        assert!(
            matches!(actions.as_slice(), [Action::History(4)]),
            "{actions:?}"
        );
        h.engine.end_gesture(false).unwrap();
        assert_eq!(h.engine.project(), &before);
        if reason < 2 {
            assert!(h.controls.point_selection.points.is_empty());
        }
    }
}

#[test]
fn composing_text_never_deletes_selected_points_or_layer() {
    let mut h = Harness::new(false, true, Locale::Zh, 1.);
    h.click([-90., -60.], Modifiers::default());
    h.click([90., -60.], shift());
    let before = h.engine.project().clone();
    assert!(h
        .draw(Input {
            composing: true,
            keys: vec![(Key::Delete, Modifiers::default())],
            text: "未提交".into(),
            ..Default::default()
        })
        .is_empty());
    assert_eq!(h.engine.project(), &before);
}

#[test]
fn failed_translation_is_visible_at_default_level_and_resets_after_recovery() {
    const CHILD: &str = "MOTION_POINT_SELECTION_FAILURE_CHILD";
    const TEST:&str="panels::path_controls::point_selection::tests::failed_translation_is_visible_at_default_level_and_resets_after_recovery";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut h = Harness::new(false, true, Locale::Zh, 1.);
        h.click([-90., -60.], Modifiers::default());
        h.click([90., -60.], shift());
        let before = h.engine.project().clone();
        let start = h.position([-90., -60.]);
        h.input(Input {
            mouse: start,
            pressed: Some((MouseButton::Left, start)),
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        let errors = || {
            std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .lines()
                .filter(|l| l.contains("ERROR") && l.contains("site=path.point_selection "))
                .count()
        };
        for _ in 0..59 {
            h.input(Input {
                mouse: [40000., 0.],
                held: Some(MouseButton::Left),
                ..Default::default()
            });
        }
        assert_eq!(errors(), 0);
        assert_eq!(h.engine.project(), &before);
        h.input(Input {
            mouse: [40000., 0.],
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        assert_eq!(errors(), 1);
        for _ in 0..80 {
            h.input(Input {
                mouse: [40000., 0.],
                held: Some(MouseButton::Left),
                ..Default::default()
            });
        }
        assert_eq!(errors(), 1);
        // A valid no-op is recovery, and must not create history.
        assert!(h
            .input(Input {
                mouse: start,
                held: Some(MouseButton::Left),
                ..Default::default()
            })
            .is_empty());
        for _ in 0..59 {
            h.input(Input {
                mouse: [40000., 0.],
                held: Some(MouseButton::Left),
                ..Default::default()
            });
        }
        assert_eq!(errors(), 1);
        h.input(Input {
            mouse: [40000., 0.],
            held: Some(MouseButton::Left),
            ..Default::default()
        });
        assert_eq!(errors(), 2);
        assert_eq!(h.engine.project(), &before);
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        assert!(
            file.contains(
                "object=7 phase=move selected=2 reason=point move cannot unproject pointer"
            ),
            "{file}"
        );
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
            .matches("site=path.point_selection ")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("path point selection rejected"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("persistent degradation"));
}
