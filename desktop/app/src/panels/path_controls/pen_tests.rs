use super::*;

#[test]
fn pen_placement_alt_handles_commit_cancel_and_save_roundtrip() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mask in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for origin in [[0., 0.], [90., 60.]] {
                let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
                let base = VectorPath {
                    id: 1,
                    closed: false,
                    nodes: vec![
                        PathNode {
                            id: 1,
                            geometry: Track::constant([-200., -100., 0., 0., 0., 0.]),
                        },
                        PathNode {
                            id: 2,
                            geometry: Track::constant([-100., -50., 0., 0., 0., 0.]),
                        },
                    ],
                };
                let mut layer = Layer::solid(7, "Pen fixture", [100., 100.], [0.; 3], [1.; 4]);
                if !mask {
                    layer.content = Content::Vector {
                        vector: new_vector(base),
                    };
                }
                project.layers.push(layer);
                let mut engine = motion_core::Engine::new(project).unwrap();
                let before = engine.project().clone();
                let state = serde_json::json!({"root":"pen-fixture", "revision":engine.revision(),
                    "frame":0, "project":engine.project(), "source_transforms":[{"id":7,
                    "mvp":DMat4::from_scale(glam::DVec3::new(2./640., 2./480., 1.)).to_cols_array()}]});
                let view = Rect::new(origin[0], origin[1], 640., 480.);
                let mut controls = Controls::default();
                controls.tool = if mask { Tool::Mask } else { Tool::Pen };
                let mut actions = vec![];
                let mut draw =
                    |controls: &mut Controls, input: Input, actions: &mut Vec<Action>| {
                        controls.draw(
                            &mut PaintList::default(),
                            &mut atlas,
                            view,
                            &state,
                            Some(7),
                            &input,
                            locale,
                            actions,
                        );
                    };
                let at = |x: f32, y: f32| [origin[0] + x, origin[1] + y];
                draw(
                    &mut controls,
                    Input {
                        mouse: at(100., 100.),
                        pressed: Some((MouseButton::Left, at(100., 100.))),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                );
                draw(
                    &mut controls,
                    Input {
                        mouse: at(120., 110.),
                        held: Some(MouseButton::Left),
                        modifiers: motion_ui::input::Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    &mut actions,
                );
                let g = controls.draft.as_ref().unwrap().path.nodes[0]
                    .geometry
                    .value;
                assert_eq!(&g[2..], &[0., 0., 20., 10.]);
                // Releasing Alt restores symmetry during this placement.
                draw(
                    &mut controls,
                    Input {
                        mouse: at(130., 115.),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                );
                assert_eq!(
                    &controls.draft.as_ref().unwrap().path.nodes[0]
                        .geometry
                        .value[2..],
                    &[-30., -15., 30., 15.]
                );
                // Alt can be pressed again without moving the opposite handle.
                draw(
                    &mut controls,
                    Input {
                        mouse: at(140., 120.),
                        released: Some((MouseButton::Left, at(140., 120.))),
                        modifiers: motion_ui::input::Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    &mut actions,
                );
                let first = controls.draft.as_ref().unwrap().path.nodes[0]
                    .geometry
                    .value;
                assert_eq!(&first[2..], &[-30., -15., 40., 20.]);
                // A later held button outside the canvas must not rewrite the last node.
                draw(
                    &mut controls,
                    Input {
                        mouse: at(700., 500.),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                );
                assert_eq!(
                    controls.draft.as_ref().unwrap().path.nodes[0]
                        .geometry
                        .value,
                    first
                );
                for point in [at(220., 220.), at(320., 130.)] {
                    draw(
                        &mut controls,
                        Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            released: Some((MouseButton::Left, point)),
                            ..Default::default()
                        },
                        &mut actions,
                    );
                }
                // Clicking the first node closes the path and publishes exactly one edit.
                draw(
                    &mut controls,
                    Input {
                        mouse: at(100., 100.),
                        pressed: Some((MouseButton::Left, at(100., 100.))),
                        ..Default::default()
                    },
                    &mut actions,
                );
                let edits: Vec<_> = actions
                    .into_iter()
                    .filter_map(|a| {
                        if let Action::Edit(c) = a {
                            Some(c)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(edits.len(), 1);
                engine.apply(edits.into_iter().next().unwrap()).unwrap();
                let root = tempfile::tempdir().unwrap();
                motion_core::storage::save(root.path(), engine.project()).unwrap();
                assert_eq!(
                    motion_core::storage::load(root.path()).unwrap(),
                    *engine.project()
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
                let mut canceled = Controls::default();
                canceled.tool = controls.tool;
                draw(
                    &mut canceled,
                    Input {
                        mouse: at(100., 100.),
                        pressed: Some((MouseButton::Left, at(100., 100.))),
                        ..Default::default()
                    },
                    &mut vec![],
                );
                assert!(canceled.draft.is_some());
                canceled.cancel();
                let mut out = vec![];
                canceled.finish(true, &mut out);
                assert!(canceled.draft.is_none() && out.is_empty());
                assert_eq!(engine.project(), &before);
            }
        }
    }
}
