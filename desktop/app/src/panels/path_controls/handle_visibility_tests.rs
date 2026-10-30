use super::*;

#[test]
fn mask_handles_remain_visible_but_only_the_selected_node_can_be_dragged() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for origin in [[0., 0.], [90., 60.]] {
            let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
            let path = VectorPath {
                tangents: Default::default(), id: 1,
                closed: true,
                nodes: [
                    (11, [100., 100., -20., 0., 20., 0.]),
                    (42, [220., 100., -20., 0., 20., 0.]),
                    (99, [160., 220., -20., 0., 20., 0.]),
                ]
                .into_iter()
                .map(|(id, g)| PathNode {
                    id,
                    geometry: Track::constant(g),
                })
                .collect(),
            };
            let mut layer = Layer::solid(7, "Mask handles", [100.; 2], [0.; 3], [1.; 4]);
            layer.masks.push(LayerMask::new(1, path));
            project.layers.push(layer);
            let mut engine = motion_core::Engine::new(project).unwrap();
            let before = engine.project().clone();
            let view = Rect::new(origin[0], origin[1], 640., 480.);
            let state = |engine: &motion_core::Engine| {
                let nodes: Vec<_> = engine.project().layers[0].masks[0]
                    .path
                    .nodes
                    .iter()
                    .map(|n| serde_json::json!({"id":n.id,"geometry":n.geometry.value}))
                    .collect();
                serde_json::json!({"root":"handles","project":engine.project(),"frame":0,"revision":engine.revision(),"source_transforms":[{"id":7,"mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}],"mask_layers":[{"id":7,"masks":[{"id":1,"path":{"id":1,"closed":true,"nodes":nodes}}]}]})
            };
            let mut controls = Controls {
                tool: Tool::Mask,
                mask: Some(1),
                selected: Some((1, 42)),
                ..Default::default()
            };
            let mut render =
                |controls: &mut Controls, engine: &motion_core::Engine, input: &Input| {
                    let mut paint = PaintList::default();
                    let mut out = vec![];
                    controls.draw(
                        &mut paint,
                        &mut atlas,
                        view,
                        &state(engine),
                        Some(7),
                        input,
                        locale,
                        &mut out,
                    );
                    (paint, out)
                };
            let (paint, _) = render(&mut controls, &engine, &Input::default());
            let handles = paint
                .shapes
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    matches!(
                        s,
                        motion_ui::paint::Shape::Procedural {
                            kind: motion_ui::paint::Pattern::Diamond,
                            ..
                        }
                    )
                    .then_some(paint.colors[i])
                })
                .collect::<Vec<_>>();
            assert_eq!(handles.len(), 6, "both handles of every node are shown");
            assert!(
                handles[2].0[3] > handles[0].0[3],
                "the selected ID 42, not ordinal 42, is emphasized"
            );
            let projection = layer_projection(&state(&engine), 7, view).unwrap();
            let unselected = projection.screen([70., 50.]).unwrap(); // node 11 outgoing, mask-centered coordinates
            let (_, out) = render(
                &mut controls,
                &engine,
                &Input {
                    mouse: unselected,
                    pressed: Some((MouseButton::Left, unselected)),
                    ..Default::default()
                },
            );
            assert!(
                controls.drag.is_none(),
                "showing an unselected handle does not make it editable"
            );
            assert!(out
                .iter()
                .all(|a| !matches!(a, Action::Edit(_) | Action::History(_))));
            controls.cancel();
            controls.selected = Some((1, 42));
            let selected = projection.screen([190., 50.]).unwrap();
            render(
                &mut controls,
                &engine,
                &Input {
                    mouse: selected,
                    pressed: Some((MouseButton::Left, selected)),
                    ..Default::default()
                },
            );
            assert!(controls
                .drag
                .as_ref()
                .is_some_and(|d| d.node == 42 && d.axis == 4));
            let end = [selected[0] + 15., selected[1] + 10.];
            let (_, out) = render(
                &mut controls,
                &engine,
                &Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            for action in out {
                match action {
                    Action::Edit(c) => engine.apply(c).unwrap(),
                    Action::History(2) => engine.begin_gesture().unwrap(),
                    _ => {}
                }
            }
            let (_, out) = render(
                &mut controls,
                &engine,
                &Input {
                    mouse: end,
                    released: Some((MouseButton::Left, end)),
                    ..Default::default()
                },
            );
            for action in out {
                match action {
                    Action::Edit(c) => engine.apply(c).unwrap(),
                    Action::History(3) => engine.end_gesture(true).unwrap(),
                    _ => {}
                }
            }
            let changed = &engine.project().layers[0].masks[0].path.nodes;
            assert_eq!(changed[0], before.layers[0].masks[0].path.nodes[0]);
            assert_eq!(changed[2], before.layers[0].masks[0].path.nodes[2]);
            assert_ne!(changed[1], before.layers[0].masks[0].path.nodes[1]);
            let temp = tempfile::tempdir().unwrap();
            motion_core::storage::save(temp.path(), engine.project()).unwrap();
            assert_eq!(
                motion_core::storage::load(temp.path()).unwrap(),
                *engine.project()
            );
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
