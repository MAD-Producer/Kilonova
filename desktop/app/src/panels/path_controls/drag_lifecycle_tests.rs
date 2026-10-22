use super::*;

fn fixture(mask: bool) -> motion_core::Engine {
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    let path = VectorPath {
        id: 3,
        closed: true,
        nodes: [
            (11, [0., 0., -20., 0., 20., 0.]),
            (42, [70., 20., 0., 0., 0., 0.]),
            (99, [30., 70., 0., 0., 0., 0.]),
        ]
        .into_iter()
        .map(|(id, g)| PathNode {
            id,
            geometry: Track::constant(g),
        })
        .collect(),
    };
    let mut layer = Layer::solid(7, "Drag fixture", [100.; 2], [0.; 3], [1.; 4]);
    if mask {
        layer.masks.push(LayerMask::new(1, path));
    } else {
        layer.content = Content::Vector {
            vector: new_vector(path),
        };
    }
    project.layers.push(layer);
    motion_core::Engine::new(project).unwrap()
}
fn state(engine: &motion_core::Engine, mask: bool) -> Value {
    let layer = &engine.project().layers[0];
    let path = if mask {
        &layer.masks[0].path
    } else {
        let Content::Vector {
            vector:
                VectorContent {
                    source: VectorSource::Paths { paths },
                    ..
                },
        } = &layer.content
        else {
            panic!()
        };
        &paths[0]
    };
    let nodes: Vec<_> = path
        .nodes
        .iter()
        .map(|n| serde_json::json!({"id":n.id,"geometry":n.geometry.sample(0.).unwrap()}))
        .collect();
    let path = serde_json::json!({"id":path.id,"closed":true,"nodes":nodes});
    serde_json::json!({"root":"drag-fixture","revision":engine.revision(),"frame":0,
        "project":engine.project(),"source_transforms":[{"id":7,"mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}],
        "vector_layers":[{"id":7,"paths":[path.clone()]}],
        "mask_layers":[{"id":7,"masks":[{"id":1,"path":path}]}]})
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(c) => engine.apply(c).unwrap(),
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            a => panic!("unexpected {a:?}"),
        }
    }
}
fn draw(
    controls: &mut Controls,
    engine: &motion_core::Engine,
    mask: bool,
    atlas: &mut TextAtlas,
    input: Input,
) -> Vec<Action> {
    let mut out = vec![];
    controls.draw(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 640., 480.),
        &state(engine, mask),
        Some(7),
        &input,
        Locale::Zh,
        &mut out,
    );
    out
}

#[test]
fn delayed_release_uses_release_position_and_never_follows_later_motion() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mask in [false, true] {
        for axis in [0, 2, 4] {
            let mut engine = fixture(mask);
            let before = engine.project().clone();
            let mut controls = Controls {
                tool: if mask { Tool::Mask } else { Tool::Pen },
                mask: mask.then_some(1),
                selected: Some((3, 11)),
                ..Default::default()
            };
            let offset = if mask { 50. } else { 0. };
            let start = [
                320. - offset
                    + match axis {
                        2 => -20.,
                        4 => 20.,
                        _ => 0.,
                    },
                240. - offset,
            ];
            apply(
                &mut engine,
                draw(
                    &mut controls,
                    &fixture(mask),
                    mask,
                    &mut atlas,
                    Input {
                        mouse: start,
                        pressed: Some((MouseButton::Left, start)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                ),
            );
            assert!(controls.drag.is_some());
            let release = [start[0] + 15., start[1] + 10.];
            let mut input = Input::default();
            motion_ui::ui::accumulate(
                &mut input,
                motion_ui::input::Event::MouseReleased {
                    position: release,
                    button: MouseButton::Left,
                },
            );
            motion_ui::ui::accumulate(
                &mut input,
                motion_ui::input::Event::MouseMoved {
                    position: [600., 400.],
                },
            );
            let actions = draw(&mut controls, &engine, mask, &mut atlas, input);
            apply(&mut engine, actions);
            let after = state(&engine, mask);
            let g = geometry(&after["vector_layers"][0]["paths"][0]["nodes"][0]).unwrap();
            let mut expected = [0., 0., -20., 0., 20., 0.];
            if axis == 0 {
                expected[0] = 15.;
                expected[1] = 10.;
            } else {
                let delta = [if axis == 2 { -5. } else { 35. }, 10.];
                move_handle(&mut expected, axis, delta, false);
            }
            for (actual, expected) in g.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 0.001,
                    "mask={mask} axis={axis} {g:?}"
                );
            }
            assert!(controls.drag.is_none());
            let settled = engine.project().clone();
            assert!(draw(
                &mut controls,
                &engine,
                mask,
                &mut atlas,
                Input {
                    mouse: [550., 350.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                }
            )
            .is_empty());
            assert_eq!(engine.project(), &settled);
            let root = tempfile::tempdir().unwrap();
            motion_core::storage::save(root.path(), engine.project()).unwrap();
            assert_eq!(motion_core::storage::load(root.path()).unwrap(), settled);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &settled);
        }
    }
}

#[test]
fn lost_button_or_focus_finishes_capture_without_reusing_it_for_another_press() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mask in [false, true] {
        for focus_lost in [false, true] {
            let mut engine = fixture(mask);
            let mut controls = Controls {
                tool: if mask { Tool::Mask } else { Tool::Pen },
                mask: mask.then_some(1),
                ..Default::default()
            };
            let offset = if mask { 50. } else { 0. };
            let start = [320. - offset, 240. - offset];
            let actions = draw(
                &mut controls,
                &engine,
                mask,
                &mut atlas,
                Input {
                    mouse: start,
                    pressed: Some((MouseButton::Left, start)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            apply(&mut engine, actions);
            let actions = draw(
                &mut controls,
                &engine,
                mask,
                &mut atlas,
                Input {
                    mouse: [start[0] + 10., start[1] + 10.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            );
            apply(&mut engine, actions);
            assert!(controls.drag.as_ref().unwrap().begun);
            let actions = draw(
                &mut controls,
                &engine,
                mask,
                &mut atlas,
                Input {
                    mouse: [600., 400.],
                    focus_lost,
                    ..Default::default()
                },
            );
            apply(&mut engine, actions);
            assert!(
                controls.drag.is_none(),
                "stale capture mask={mask} focus_lost={focus_lost}"
            );
            assert!(!controls.gesture_active());
            assert!(draw(
                &mut controls,
                &engine,
                mask,
                &mut atlas,
                Input {
                    mouse: [500., 300.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                }
            )
            .is_empty());
            engine.begin_gesture().unwrap();
            engine.end_gesture(false).unwrap();
        }
    }
}

#[test]
fn new_pen_handle_placement_stops_at_release_even_when_motion_is_coalesced() {
    let mut atlas=TextAtlas::from_system(12.).unwrap();
    for mask in [false,true] {
        let engine=fixture(mask);
        let mut controls=Controls {tool:if mask {Tool::Mask}else{Tool::Pen},..Default::default()};
        draw(&mut controls,&engine,mask,&mut atlas,Input {mouse:[100.,100.],pressed:Some((MouseButton::Left,[100.,100.])),held:Some(MouseButton::Left),..Default::default()});
        assert!(controls.draft.as_ref().unwrap().placing);
        draw(&mut controls,&engine,mask,&mut atlas,Input {mouse:[600.,400.],released:Some((MouseButton::Left,[120.,110.])),..Default::default()});
        let draft=controls.draft.as_ref().unwrap();
        assert!(!draft.placing);
        let geometry=draft.path.nodes[0].geometry.value;
        assert!((geometry[4]-20.).abs()<0.001 && (geometry[5]-10.).abs()<0.001,"{geometry:?}");
        draw(&mut controls,&engine,mask,&mut atlas,Input {mouse:[500.,400.],held:Some(MouseButton::Left),..Default::default()});
        assert_eq!(controls.draft.as_ref().unwrap().path.nodes[0].geometry.value,geometry);
    }
}
