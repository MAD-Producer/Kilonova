use super::*;

#[test]
fn hidden_authored_handles_do_not_capture_clicks_and_pen_and_mask_can_commit_new_geometry_and_undo()
{
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mask in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let base = VectorPath {
                id: 1,
                closed: true,
                nodes: [[-200., -100.], [-100., -50.], [-80., 40.]]
                    .into_iter()
                    .enumerate()
                    .map(|(i, point)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([point[0], point[1], 0., 0., 0., 0.]),
                    })
                    .collect(),
            };
            let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
            let mut layer =
                Layer::solid(7, "Hidden handle fixture", [100., 100.], [0.; 3], [1.; 4]);
            if mask {
                layer.masks.push(LayerMask::new(1, base.clone()));
            } else {
                layer.content = Content::Vector {
                    vector: new_vector(base.clone()),
                };
            }
            project.layers.push(layer);
            let mut engine = motion_core::Engine::new(project).unwrap();
            let before = engine.project().clone();
            let mut state = serde_json::json!({"root":"hidden-handles-fixture","document_generation":4,"composition":"comp-main","revision":engine.revision(),"frame":0,
                "project":engine.project(),"source_transforms":[{"id":7,"mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}]});
            if mask {
                state["mask_layers"] = serde_json::json!([{"id":7,"masks":[{"id":1,"path":{"id":1,"closed":true,
                "nodes":base.nodes.iter().map(|node|serde_json::json!({"id":node.id,"geometry":node.geometry.value})).collect::<Vec<_>>()}}]}]);
            }
            let view = Rect::new(0., 0., 640., 480.);
            let projection = layer_projection(&state, 7, view).unwrap();
            let offset = if mask { [50., 50.] } else { [0., 0.] };
            let old = base.nodes[0].geometry.value;
            let hidden = projection
                .screen([old[0] - offset[0], old[1] - offset[1]])
                .unwrap();
            let mut controls = Controls::default();
            controls.tool = if mask { Tool::Mask } else { Tool::Pen };
            if mask {
                controls.mask = Some(1);
            }
            let mut out = vec![];
            for (index, point) in [hidden, [350., 240.], [400., 300.]].into_iter().enumerate() {
                controls.draw_with_handles(
                    &mut PaintList::default(),
                    &mut atlas,
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
                    locale,
                    false,
                    &mut out,
                );
                assert!(
                    controls.draft.is_some() && controls.drag.is_none(),
                    "hidden authored nodes must not receive the new click"
                );
                if index == 0 {
                    assert_eq!(out.len(), 1);
                    assert!(
                        matches!(out.pop(),Some(Action::Seek(frame)) if frame == 0.),
                        "new drafts retain the existing view-only playback stop/seek boundary"
                    );
                }
                assert!(out.is_empty());
                controls.draw_with_handles(
                    &mut PaintList::default(),
                    &mut atlas,
                    view,
                    view,
                    &state,
                    Some(7),
                    &Input {
                        mouse: point,
                        released: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                    locale,
                    false,
                    &mut out,
                );
                assert!(out.is_empty());
            }
            controls.draw_with_handles(
                &mut PaintList::default(),
                &mut atlas,
                view,
                view,
                &state,
                Some(7),
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
                locale,
                false,
                &mut out,
            );
            assert_eq!(out.len(), 1);
            let Action::Edit(command) = out.remove(0) else {
                panic!("expected one authored command")
            };
            engine.apply(command).unwrap();
            assert_ne!(engine.project(), &before);
            if mask {
                assert_eq!(engine.project().layers[0].masks.len(), 2);
                assert_eq!(
                    engine.project().layers[0].masks[0],
                    before.layers[0].masks[0]
                );
            } else {
                let Content::Vector { vector } = &engine.project().layers[0].content else {
                    unreachable!()
                };
                let VectorSource::Paths { paths } = &vector.source else {
                    unreachable!()
                };
                assert_eq!(paths.len(), 2);
                assert_eq!(paths[0], base);
                assert_eq!(paths[1].nodes.len(), 3);
            }
            assert!(engine.undo().unwrap());
            assert_eq!(engine.project(), &before);
        }
    }
}
