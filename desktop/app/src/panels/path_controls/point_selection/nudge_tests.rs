use super::*;
#[test]
fn authored_and_mask_nodes_nudge_without_moving_tangents_or_other_points() {
    for mask in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let engine = crate::engine::Engine::start(
            root.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let path = VectorPath {
            id: 1,
            closed: true,
            nodes: (1..=3)
                .map(|id| PathNode {
                    id,
                    geometry: Track::constant([id as f32 * 20., id as f32 * 10., -2., -3., 4., 5.]),
                })
                .collect(),
        };
        let mut layer = Layer::solid(7, "Path nudge", [200., 100.], [960., 540., 0.], [1.; 4]);
        if mask {
            layer.masks.push(LayerMask {
                id: 1,
                name: "Mask".into(),
                path: path.clone(),
                mode: MaskMode::Add,
                enabled: true,
                inverted: false,
                opacity: Track::constant(100.),
                feather: Track::constant([0.; 2]),
                expansion: Track::constant(0.),
            });
        } else {
            layer.content = Content::Vector {
                vector: new_vector(path.clone()),
            };
        }
        let state = engine
            .edit(serde_json::to_string(&Command::Add { layer }).unwrap())
            .unwrap();
        let before = state["project"].clone();
        let mut controls = Controls::default();
        controls.tool = if mask { Tool::Mask } else { Tool::Pen };
        for node in [1, 3] {
            controls.point_selection.points.insert(Point {
                mask: mask.then_some(1),
                item: None,
                path: 1,
                node,
            });
        }
        let commands = controls
            .nudge(&state, Some(7), [10., -1.])
            .unwrap()
            .unwrap();
        assert!(!commands.is_empty());
        let updated = engine
            .edit(serde_json::to_string(&commands).unwrap())
            .unwrap();
        let layer: Layer = serde_json::from_value(updated["project"]["layers"][0].clone()).unwrap();
        let actual = if mask {
            layer.masks[0].path.clone()
        } else {
            let Content::Vector { vector } = layer.content else {
                panic!()
            };
            let VectorSource::Paths { paths } = vector.source else {
                panic!()
            };
            paths[0].clone()
        };
        for i in 0..3 {
            let mut expected = path.nodes[i].geometry.value;
            if i != 1 {
                expected[0] += 10.;
                expected[1] -= 1.;
            }
            assert_eq!(actual.nodes[i].geometry.value, expected);
        }
        engine.history(0).unwrap();
        assert_eq!(engine.state().unwrap()["project"], before);
        let mut missing = state.clone();
        missing["project"]["layers"] = serde_json::json!([]);
        assert!(controls
            .nudge(&missing, Some(7), [1., 0.])
            .unwrap()
            .is_err());
    }
}
