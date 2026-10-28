use super::super::tests::{apply, fixture};
use super::*;

fn state(engine: &motion_core::Engine) -> Value {
    let layer = &engine.project().layers[0];
    let sample = layer.masks[0]
        .sample(layer.local_frame(33.))
        .unwrap()
        .unwrap();
    serde_json::json!({"root":"boundary-fixture","composition":"main","frame":33.,
        "revision":engine.revision(),"project":engine.project(),
        "mask_layers":[{"id":7,"masks":[{"id":1,"path":{"id":1,"closed":true,
            "nodes":sample.nodes.iter().enumerate().map(|(i,g)|serde_json::json!({"id":i+1,"geometry":g})).collect::<Vec<_>>()}}]}]})
}
fn projection(zoom: f32, rotated: bool) -> Projection {
    let scale = DMat4::from_scale(glam::DVec3::new(2. / 320., 2. / 180., 1.));
    let parent = if rotated {
        DMat4::from_rotation_z(0.25)
    } else {
        DMat4::IDENTITY
    };
    Projection::new(
        (scale * parent).to_cols_array(),
        Rect::new(
            900. - 320. * zoom,
            550. - 180. * zoom,
            640. * zoom,
            360. * zoom,
        ),
        [320., 180.],
    )
    .unwrap()
}
fn draw(
    controls: &mut Controls,
    state: &Value,
    projection: &Projection,
    input: &Input,
    visible: bool,
) -> Vec<Action> {
    let mut paint = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut actions = vec![];
    let layer = layer(state, 7);
    controls.mask_boundaries(
        &mut paint,
        &mut atlas,
        Rect::new(0., 0., 1800., 1100.),
        Some(projection),
        layer.as_ref(),
        state,
        input,
        Locale::Zh,
        visible,
        &mut actions,
    );
    actions
}
fn press(
    controls: &mut Controls,
    state: &Value,
    projection: &Projection,
    feather: bool,
    axis: usize,
) -> Grip {
    assert!(draw(controls, state, projection, &Input::default(), true).is_empty());
    let grip = controls
        .boundaries
        .grips
        .iter()
        .find(|g| {
            matches!(g.property, MaskProperty::Feather) == feather && g.axis == axis && g.sign > 0.
        })
        .unwrap()
        .clone();
    let actions = draw(
        controls,
        state,
        projection,
        &Input {
            mouse: grip.screen,
            pressed: Some((MouseButton::Left, grip.screen)),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        true,
    );
    assert!(matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==33.));
    assert!(controls.parameter.is_some());
    assert!(controls.boundaries.capture.is_some());
    grip
}
fn moved(projection: &Projection, grip: &Grip, delta: f32) -> [f32; 2] {
    let mut local = projection.local(grip.screen).unwrap();
    local[grip.axis] += delta;
    projection.screen(local).unwrap()
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn transformed_feather_and_expansion_grips_share_numeric_history_and_release_coordinates() {
    for zoom in [0.5, 1., 2.] {
        for rotated in [false, true] {
            for feather in [false, true] {
                for axis in [0, 1] {
                    let mut engine = fixture();
                    let before = engine.snapshot();
                    let mut controls = Controls {
                        tool: Tool::Mask,
                        mask: Some(1),
                        ..Default::default()
                    };
                    let projection = projection(zoom, rotated);
                    let grip = press(&mut controls, &state(&engine), &projection, feather, axis);
                    let at = moved(&projection, &grip, 9.);
                    let actions = draw(
                        &mut controls,
                        &state(&engine),
                        &projection,
                        &Input {
                            mouse: at,
                            held: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        true,
                    );
                    apply(&mut engine, actions);
                    let at = moved(&projection, &grip, 15.);
                    let actions = draw(
                        &mut controls,
                        &state(&engine),
                        &projection,
                        &Input {
                            mouse: [10000.; 2],
                            released: Some((MouseButton::Left, at)),
                            ..Default::default()
                        },
                        true,
                    );
                    apply(&mut engine, actions);
                    assert!(controls.parameter.is_none() && controls.boundaries.capture.is_none());
                    let mask = &engine.project().layers[0].masks[0];
                    if feather {
                        let value = mask.feather.sample(13.).unwrap();
                        let expected = super::super::linked_feather(
                            [8., 13.],
                            axis,
                            [8., 13.][axis] + 20.,
                            32000.,
                        );
                        for i in [0, 1] {
                            close(value[i], expected[i]);
                        }
                        assert_eq!(mask.feather.keys.len(), 3);
                        assert_eq!(
                            mask.feather.keys[0],
                            before.layers[0].masks[0].feather.keys[0]
                        );
                        assert_eq!(
                            mask.feather.keys[2],
                            before.layers[0].masks[0].feather.keys[1]
                        );
                    } else {
                        close(mask.expansion.sample(13.).unwrap(), 15.);
                    }
                    engine.undo().unwrap();
                    assert_eq!(
                        engine.snapshot(),
                        before,
                        "zoom={zoom} rotated={rotated} feather={feather} axis={axis}"
                    );
                }
            }
        }
    }
}

#[test]
fn boundary_capture_cancels_complete_tracks_on_stale_context_and_never_resumes_after_release() {
    for finish in [
        "escape",
        "focus",
        "frame",
        "root",
        "composition",
        "lock",
        "mask",
        "disabled",
        "link",
        "nodes",
        "matrix",
        "view",
        "hidden",
        "observing",
        "lost_button",
        "new_press",
    ] {
        let mut engine = fixture();
        let before = engine.snapshot();
        let mut controls = Controls {
            tool: Tool::Mask,
            mask: Some(1),
            ..Default::default()
        };
        let mut projection = projection(1., false);
        let grip = press(&mut controls, &state(&engine), &projection, true, 0);
        let actions = draw(
            &mut controls,
            &state(&engine),
            &projection,
            &Input {
                mouse: moved(&projection, &grip, 6.),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            true,
        );
        apply(&mut engine, actions);
        let mut changed = state(&engine);
        let mut input = Input {
            mouse: moved(&projection, &grip, 50.),
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        let mut visible = true;
        match finish {
            "escape" => input.keys.push((Key::Escape, Default::default())),
            "focus" => input.focus_lost = true,
            "frame" => changed["frame"] = serde_json::json!(33.5),
            "root" => changed["root"] = serde_json::json!("different"),
            "composition" => changed["composition"] = serde_json::json!("different"),
            "lock" => changed["project"]["layers"][0]["locked"] = serde_json::json!(true),
            "mask" => changed["project"]["layers"][0]["masks"] = serde_json::json!([]),
            "disabled" => {
                changed["project"]["layers"][0]["masks"][0]["enabled"] = serde_json::json!(false)
            }
            "link" => {
                changed["project"]["layers"][0]["masks"][0]["feather_linked"] =
                    serde_json::json!(false)
            }
            "nodes" => {
                changed["project"]["layers"][0]["masks"][0]["path"]["nodes"][0]["geometry"]["value"]
                    [0] = serde_json::json!(50.)
            }
            "matrix" => projection.matrix = DMat4::from_rotation_z(0.1) * projection.matrix,
            "view" => projection.image.min[0] += 1.,
            "hidden" => visible = false,
            "observing" => changed["observing"] = serde_json::json!(true),
            "lost_button" => input.held = None,
            "new_press" => input.pressed = Some((MouseButton::Left, input.mouse)),
            _ => unreachable!(),
        }
        let actions = draw(&mut controls, &changed, &projection, &input, visible);
        apply(&mut engine, actions);
        assert!(
            controls.parameter.is_none() && controls.boundaries.capture.is_none(),
            "{finish}"
        );
        if matches!(finish, "lost_button" | "new_press") {
            let value = engine.project().layers[0].masks[0]
                .feather
                .sample(13.)
                .unwrap();
            close(value[0], 16.);
            close(value[1], 26.);
            engine.undo().unwrap();
        }
        assert_eq!(engine.snapshot(), before, "{finish}");
        let actions = draw(
            &mut controls,
            &state(&engine),
            &projection,
            &Input {
                mouse: [10000.; 2],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            true,
        );
        assert!(actions.is_empty(), "later cursor motion resumed {finish}");
    }
}

#[test]
fn a_coalesced_release_can_commit_a_drag_and_an_empty_section_can_recover() {
    for expansion in [0., -500.] {
        let mut engine = fixture();
        engine
            .apply(Command::Mask {
                object: 7,
                action: MaskAction::Set {
                    mask: 1,
                    property: MaskProperty::Expansion,
                    frame: 33,
                    value: vec![expansion],
                    animated: None,
                },
            })
            .unwrap();
        let before = engine.snapshot();
        let mut controls = Controls {
            tool: Tool::Mask,
            mask: Some(1),
            ..Default::default()
        };
        let projection = projection(1., false);
        let grip = press(&mut controls, &state(&engine), &projection, false, 0);
        assert_eq!(grip.recovery, expansion < 0.);
        let actions = draw(
            &mut controls,
            &state(&engine),
            &projection,
            &Input {
                released: Some((
                    MouseButton::Left,
                    moved(&projection, &grip, 12. - expansion),
                )),
                ..Default::default()
            },
            true,
        );
        apply(&mut engine, actions);
        close(
            engine.project().layers[0].masks[0]
                .expansion
                .sample(13.)
                .unwrap(),
            12.,
        );
        engine.undo().unwrap();
        assert_eq!(engine.snapshot(), before);
    }
}
