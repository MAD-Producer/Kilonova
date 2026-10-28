//! Mask parameter constraints and viewer boundary controls.

/// Preserve proportions when the edited axis has a nonzero baseline. At zero,
/// a common delta avoids changing the opposite value merely by starting a drag.
/// Limit the common ratio/delta before writing either component.
pub(super) fn linked_feather(base: [f32; 2], axis: usize, value: f32, maximum: f32) -> [f32; 2] {
    let other = 1 - axis;
    let mut result = base;
    if base[axis] > 0. {
        let ratio = (f64::from(value) / f64::from(base[axis])).min(if base[other] > 0. {
            f64::from(maximum) / f64::from(base[other])
        } else {
            f64::INFINITY
        });
        result[axis] = (f64::from(base[axis]) * ratio) as f32;
        result[other] = (f64::from(base[other]) * ratio) as f32;
    } else {
        let delta = (value - base[axis]).min(maximum - base[other]);
        result[axis] += delta;
        result[other] += delta;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use motion_ui::dock::Panel;
    #[cfg(target_os = "windows")]
    mod native;

    fn fixture() -> motion_core::Engine {
        let mut project = motion_core::Project::new(320, 180, 30, 90).unwrap();
        let mut layer = Layer::solid(7, "Feather", [320., 180.], [160., 90., 0.], [1.; 4]);
        let mut mask = LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: true,
                nodes: [[40., 40.], [280., 40.], [280., 140.], [40., 140.]]
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                    })
                    .collect(),
            },
        );
        mask.feather = Track::constant([8., 13.]);
        mask.feather_linked = true;
        layer.masks.push(mask);
        project.layers.push(layer);
        let mut engine = motion_core::Engine::new(project).unwrap();
        engine
            .apply(Command::TrimLayerClip {
                object: 7,
                in_frame: 0,
                out_frame: 60,
            })
            .unwrap();
        engine
            .apply(Command::MoveLayerClip {
                object: 7,
                in_frame: 20,
            })
            .unwrap();
        for frame in [21, 60] {
            engine
                .apply(Command::Mask {
                    object: 7,
                    action: MaskAction::Set {
                        mask: 1,
                        property: MaskProperty::Feather,
                        frame,
                        value: vec![8., 13.],
                        animated: Some(true),
                    },
                })
                .unwrap();
        }
        engine
    }
    fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Edit(command) => engine.apply(command).unwrap(),
                Action::History(2) => engine.begin_gesture().unwrap(),
                Action::History(3) => engine.end_gesture(true).unwrap(),
                Action::History(4) => engine.end_gesture(false).unwrap(),
                Action::Seek(_) => {}
                _ => panic!("unexpected feather action: {action:?}"),
            }
        }
    }

    #[test]
    fn linked_numeric_gesture_updates_both_axes_and_cancels_the_complete_track_on_context_loss() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let styles = PaintList::default().styles();
        for host in [Panel::Properties, Panel::Timeline, Panel::Curves] {
            for finish in [
                "release",
                "escape",
                "focus",
                "frame",
                "root",
                "composition",
                "locked",
                "mask",
                "link",
                "lost_button",
                "new_press",
            ] {
                let mut engine = fixture();
                let before = engine.snapshot();
                let mut controls = Controls::default();
                let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":engine.project(),"frame":33});
                let fields = mask_inspector::property_row(
                    &styles,
                    Rect::new(0., 0., 640., 200.),
                    0.,
                    0,
                    2,
                    host,
                );
                let point = [fields.number.min[0] + 8., fields.number.min[1] + 8.];
                let mut actions = vec![];
                controls.mask_number(
                    &mut PaintList::default(),
                    &mut atlas,
                    fields.number,
                    &state,
                    mask_keys::Owner {
                        object: 7,
                        locked: false,
                        offset: 20,
                    },
                    1,
                    MaskProperty::Feather,
                    true,
                    0,
                    &[8., 13.],
                    [0., 32000.],
                    host,
                    &Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                );
                assert!(controls.parameter.is_some());
                apply(&mut engine, actions);
                let mut actions = vec![];
                controls.parameter_input(
                    &state,
                    &Input {
                        mouse: [point[0] + 20., point[1]],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                    3.,
                );
                apply(&mut engine, actions);
                assert_eq!(
                    engine.project().layers[0].masks[0]
                        .feather
                        .sample(13.)
                        .unwrap(),
                    [28., 45.5]
                );
                let mut changed = state.clone();
                let mut input = Input {
                    mouse: [point[0] + 999., point[1]],
                    released: Some((MouseButton::Left, [point[0] + 30., point[1]])),
                    ..Default::default()
                };
                match finish {
                    "escape" => input.keys.push((Key::Escape, Default::default())),
                    "focus" => input.focus_lost = true,
                    "frame" => changed["frame"] = serde_json::json!(34),
                    "root" => changed["root"] = serde_json::json!("other"),
                    "composition" => changed["composition"] = serde_json::json!("other"),
                    "locked" => changed["project"]["layers"][0]["locked"] = serde_json::json!(true),
                    "lost_button" => input.released = None,
                    "new_press" => {
                        input.released = None;
                        input.held = Some(MouseButton::Left);
                        input.pressed = Some((MouseButton::Left, input.mouse));
                    }
                    "mask" => changed["project"]["layers"][0]["masks"] = serde_json::json!([]),
                    "link" => {
                        changed["project"]["layers"][0]["masks"][0]["feather_linked"] =
                            serde_json::json!(false)
                    }
                    _ => {}
                }
                let mut actions = vec![];
                controls.parameter_input(&changed, &input, &mut actions, 3.);
                apply(&mut engine, actions);
                assert!(controls.parameter.is_none());
                if matches!(finish, "release" | "lost_button" | "new_press") {
                    let keys = &engine.project().layers[0].masks[0].feather.keys;
                    let expected = if finish == "release" {
                        [38., 61.75]
                    } else {
                        [28., 45.5]
                    };
                    assert_eq!((keys[1].frame, keys[1].value), (13, expected));
                    assert_eq!(keys[0], before.layers[0].masks[0].feather.keys[0]);
                    assert_eq!(keys[2], before.layers[0].masks[0].feather.keys[1]);
                    engine.undo().unwrap();
                }
                let mut late = vec![];
                controls.parameter_input(
                    &state,
                    &Input {
                        mouse: [point[0] + 1200., point[1]],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut late,
                    3.,
                );
                assert!(
                    late.is_empty(),
                    "completed gesture must never resume at a later cursor position"
                );
                assert_eq!(engine.snapshot(), before, "host={host:?} finish={finish}");
            }
        }
    }

    #[test]
    fn linked_feather_preserves_ratio_or_zero_axis_delta_with_a_shared_limit() {
        assert_eq!(linked_feather([8., 13.], 0, 16., 32000.), [16., 26.]);
        assert_eq!(linked_feather([8., 13.], 1, 26., 32000.), [16., 26.]);
        assert_eq!(linked_feather([0., 0.], 0, 20., 32000.), [20., 20.]);
        assert_eq!(linked_feather([0., 13.], 0, 20., 32000.), [20., 33.]);
        assert_eq!(
            linked_feather([8., 16.], 0, 32000., 32000.),
            [16000., 32000.]
        );
        assert_eq!(linked_feather([0., 31990.], 0, 20., 32000.), [10., 32000.]);
        assert_eq!(linked_feather([8., 13.], 0, 0., 32000.), [0., 0.]);
    }

    #[test]
    fn feather_link_checkbox_stays_inline_and_dispatches_only_for_an_unlocked_owner() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for host in [Panel::Properties, Panel::Masks, Panel::Timeline] {
            for width in if host == Panel::Timeline {
                vec![320., 400., 640.]
            } else {
                vec![160., 180., 220., 286., 400., 720.]
            } {
                for locked in [false, true] {
                    let mut engine = fixture();
                    let before = engine.snapshot();
                    let mut controls = Controls::default();
                    let mut paint = PaintList::default();
                    let styles = paint.styles();
                    let area = Rect::new(17., 31., width, 200.);
                    let x = mask_inspector::property_row(&styles, area, 90., 0, 2, host);
                    let y = mask_inspector::property_row(&styles, area, 90., 1, 2, host);
                    let slot = styles.metric("icon.slot", 20.);
                    let link = Rect::new(
                        x.number.min[0] - slot,
                        90.,
                        slot,
                        styles.metric("row.height", 22.),
                    );
                    assert!(x.watch.max[0] <= link.min[0]);
                    assert!(x.caption.max[0] <= link.min[0]);
                    assert_eq!(link.max[0], x.number.min[0]);
                    assert!(x.number.max[0] <= y.number.min[0]);
                    assert!(y.number.max[0] <= area.max[0]);
                    let point = [link.min[0] + slot * 0.5, link.min[1] + link.height() * 0.5];
                    let mut actions = vec![];
                    controls.feather_link(
                        &mut paint,
                        &mut atlas,
                        area,
                        90.,
                        mask_keys::Owner {
                            object: 7,
                            locked,
                            offset: 20,
                        },
                        &before.layers[0].masks[0],
                        host,
                        &Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            ..Default::default()
                        },
                        &mut actions,
                    );
                    assert_eq!(actions.len(), usize::from(!locked));
                    apply(&mut engine, actions);
                    if !locked {
                        assert!(!engine.project().layers[0].masks[0].feather_linked);
                        assert_eq!(
                            engine.project().layers[0].masks[0].feather,
                            before.layers[0].masks[0].feather
                        );
                        engine.undo().unwrap();
                    }
                    assert_eq!(engine.snapshot(), before);
                }
            }
        }
    }
}
