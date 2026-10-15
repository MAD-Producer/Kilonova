//! Mask tracks keep their stored local times; UI commands use composition time.
use super::*;
use motion_core::{Ease, Easing};
use motion_ui::{
    icons::{self, Icon},
    widgets::{Dropdown, DropdownItem},
};

#[derive(Clone, PartialEq, Eq)]
struct Channel {
    object: u64,
    mask: u64,
    property: u8,
}

fn property(index: u8) -> MaskProperty {
    match index {
        0 => MaskProperty::Opacity,
        1 => MaskProperty::Feather,
        _ => MaskProperty::Expansion,
    }
}
struct Drag {
    channel: Channel,
    from: u32,
    origin: u32,
    start: f32,
    begun: bool,
    lane: Rect,
}
struct Menu {
    channel: Channel,
    frame: u32,
    anchor: Rect,
    dropdown: Dropdown,
    opened: bool,
    can_curve: bool,
}
#[derive(Default)]
pub(super) struct Controls {
    context: Option<(String, String, u64)>,
    selected: Option<(Channel, u32)>,
    drag: Option<Drag>,
    menu: Option<Menu>,
}
fn command(channel: &Channel, action: MaskAction) -> Action {
    Action::Edit(Command::Mask {
        object: channel.object,
        action,
    })
}
fn icon(
    list: &mut PaintList,
    input: &Input,
    rect: Rect,
    kind: Icon,
    active: bool,
    enabled: bool,
) -> bool {
    let colors = list.styles().colors();
    if rect.contains(input.mouse) && enabled {
        paint::rect(list, rect, colors.SUNKEN);
    }
    icons::draw(
        list,
        rect,
        kind,
        13.,
        if !enabled {
            colors.TEXT_MUTED.with_alpha(0.4)
        } else if active {
            colors.ACCENT
        } else {
            colors.TEXT_MUTED
        },
    );
    enabled && click(input, rect)
}
impl Controls {
    pub(super) fn editing(&self) -> bool {
        self.drag.is_some() || self.menu.is_some()
    }
    pub(super) fn modal(&self) -> bool {
        self.menu.is_some()
    }
    pub(super) fn cancel(&mut self) -> bool {
        self.menu = None;
        self.selected = None;
        self.drag.take().is_some_and(|d| d.begun)
    }
    pub(super) fn delete_selected(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        if !input.key_presses().any(|(k, m, repeat)| {
            !repeat && matches!(k, Key::Delete | Key::Backspace) && !m.control && !m.alt && !m.shift
        }) {
            return false;
        }
        let Some((channel, frame)) = self.selected.take() else {
            return false;
        };
        if self.context.as_ref()
            != Some(&(
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                channel.object,
            ))
            || selected != Some(channel.object)
        {
            return true;
        }
        let Some(layer) = super::layer(state, channel.object).filter(|l| !l.locked) else {
            return true;
        };
        let Some(mask) = layer.masks.iter().find(|m| m.id == channel.mask) else {
            return true;
        };
        let local = i64::from(frame)
            - i64::from(
                layer
                    .clip(state["project"]["frames"].as_u64().unwrap_or(1) as u32)
                    .offset_frame,
            );
        let exists = match channel.property {
            0 => mask
                .opacity
                .keys
                .iter()
                .any(|k| i64::from(k.frame) == local),
            1 => mask
                .feather
                .keys
                .iter()
                .any(|k| i64::from(k.frame) == local),
            _ => mask
                .expansion
                .keys
                .iter()
                .any(|k| i64::from(k.frame) == local),
        };
        if exists {
            out.push(command(
                &channel,
                MaskAction::DeleteKey {
                    mask: channel.mask,
                    property: property(channel.property),
                    frame,
                },
            ));
        }
        true
    }
    pub(super) fn prepare(
        &mut self,
        state: &Value,
        layer: &Layer,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            layer.id,
        );
        let frames = |channel: &Channel| -> Option<Vec<i32>> {
            let mask = layer.masks.iter().find(|m| m.id == channel.mask)?;
            Some(match channel.property {
                0 => mask.opacity.keys.iter().map(|k| k.frame).collect(),
                1 => mask.feather.keys.iter().map(|k| k.frame).collect(),
                _ => mask.expansion.keys.iter().map(|k| k.frame).collect(),
            })
        };
        let offset = i64::from(
            layer
                .clip(state["project"]["frames"].as_u64().unwrap_or(1) as u32)
                .offset_frame,
        );
        let missing = self.drag.as_ref().is_some_and(|drag| {
            frames(&drag.channel).is_none_or(|keys| {
                !keys
                    .iter()
                    .any(|f| i64::from(*f) + offset == i64::from(drag.from))
            })
        }) || self.menu.as_ref().is_some_and(|menu| {
            frames(&menu.channel).is_none_or(|keys| {
                !keys
                    .iter()
                    .any(|f| i64::from(*f) + offset == i64::from(menu.frame))
            })
        });
        if let Some(menu) = &mut self.menu {
            menu.can_curve = frames(&menu.channel).is_some_and(|keys| {
                keys.iter()
                    .any(|f| i64::from(*f) + offset > i64::from(menu.frame))
            });
        }
        if self.context.as_ref() != Some(&context)
            || layer.locked
            || missing
            || input.focus_lost
            || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.selected = None;
        }
        self.context = Some(context);
    }
    pub(super) fn row(
        &mut self,
        list: &mut PaintList,
        area: Rect,
        y: f32,
        state: &Value,
        layer: &Layer,
        mask: &LayerMask,
        index: u8,
        keys: &[i32],
        values: &[f32],
        input: &Input,
        out: &mut Vec<Action>,
    ) -> f32 {
        let channel = Channel {
            object: layer.id,
            mask: mask.id,
            property: index,
        };
        let total = state["project"]["frames"]
            .as_u64()
            .unwrap_or(1)
            .max(1)
            .min(u32::MAX as u64) as u32;
        let frame = (state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32).min(total - 1);
        let offset = i64::from(layer.clip(total).offset_frame);
        let local_keys = keys;
        let keys: Vec<u32> = keys
            .iter()
            .filter_map(|key| {
                u32::try_from(i64::from(*key) + offset)
                    .ok()
                    .filter(|f| *f < total)
            })
            .collect();
        let animated = match index {
            0 => mask.opacity.is_animated(),
            1 => mask.feather.is_animated(),
            _ => mask.expansion.is_animated(),
        };
        if icon(
            list,
            input,
            Rect::new(area.min[0] + 14., y, 20., 24.),
            Icon::Stopwatch,
            animated,
            !layer.locked,
        ) {
            out.push(command(
                &channel,
                MaskAction::Animate {
                    mask: mask.id,
                    property: property(index),
                    frame,
                    enabled: !animated,
                },
            ));
        }
        if !animated {
            return 28.;
        }
        let nav_y = y + 24.;
        for (i, kind) in [Icon::Previous, Icon::Key, Icon::Next]
            .into_iter()
            .enumerate()
        {
            let target = if i == 0 {
                keys.iter().copied().filter(|f| *f < frame).max()
            } else {
                keys.iter().copied().find(|f| *f > frame)
            };
            if icon(
                list,
                input,
                Rect::new(area.min[0] + 14. + i as f32 * 20., nav_y, 20., 22.),
                kind,
                i == 1 && keys.contains(&frame),
                !layer.locked && (i == 1 || target.is_some()),
            ) {
                if i == 1 {
                    out.push(command(
                        &channel,
                        if keys.contains(&frame) {
                            MaskAction::DeleteKey {
                                mask: mask.id,
                                property: property(index),
                                frame,
                            }
                        } else {
                            MaskAction::Set {
                                mask: mask.id,
                                property: property(index),
                                frame,
                                value: values.to_vec(),
                                animated: Some(true),
                            }
                        },
                    ));
                } else if let Some(target) = target {
                    out.push(Action::Seek(f64::from(target)));
                }
            }
        }
        let lane = Rect::new(area.min[0] + 80., nav_y, (area.width() - 96.).max(1.), 22.);
        let x = |f: u32| lane.min[0] + f as f32 / (total - 1).max(1) as f32 * lane.width();
        let colors = list.styles().colors();
        paint::line(
            list,
            vec![
                [lane.min[0], lane.min[1] + 11.],
                [lane.max[0], lane.min[1] + 11.],
            ],
            colors.BORDER,
        );
        paint::line(
            list,
            vec![[x(frame), lane.min[1]], [x(frame), lane.max[1]]],
            colors.ACCENT.with_alpha(0.5),
        );
        let mut hit = None;
        for key in &keys {
            let center = [x(*key), nav_y + 11.];
            let selected = self
                .selected
                .as_ref()
                .is_some_and(|(c, f)| *c == channel && *f == *key);
            let color = if selected || *key == frame {
                colors.ACCENT
            } else {
                colors.TEXT_MUTED
            };
            paint::diamond(list, center, 4., color);
            if distance(input.mouse, center) <= 7. {
                hit = Some(*key);
            }
        }
        if let Some((button, point)) = input.pressed.filter(|(_, p)| lane.contains(*p)) {
            if let Some(key) = hit {
                self.selected = Some((channel.clone(), key));
                if button == MouseButton::Left && !layer.locked {
                    self.drag = Some(Drag {
                        channel: channel.clone(),
                        from: key,
                        origin: key,
                        start: point[0],
                        begun: false,
                        lane,
                    });
                    out.push(Action::Seek(f64::from(key)));
                } else if button == MouseButton::Right && !layer.locked {
                    self.menu = Some(Menu {
                        channel: channel.clone(),
                        frame: key,
                        anchor: Rect::new(point[0], point[1], 180., 1.),
                        dropdown: Dropdown::default(),
                        opened: true,
                        can_curve: local_keys
                            .iter()
                            .any(|f| i64::from(*f) + offset > i64::from(key)),
                    });
                }
            } else if button == MouseButton::Left {
                out.push(Action::Seek(
                    (((point[0] - lane.min[0]) / lane.width()) * (total - 1) as f32)
                        .round()
                        .clamp(0., (total - 1) as f32) as f64,
                ));
            }
        }
        if self.drag.as_ref().is_some_and(|d| d.channel == channel) {
            let mut drag = self.drag.take().unwrap();
            if !keys.contains(&drag.from) || layer.locked || lane != drag.lane {
                if drag.begun {
                    out.push(Action::History(4));
                }
            } else {
                let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
                if input.dragging() || release {
                    let delta = input.mouse[0] - drag.start;
                    let target = (drag.origin as f32 + delta / lane.width() * (total - 1) as f32)
                        .round()
                        .clamp(0., (total - 1) as f32) as u32;
                    if (drag.begun
                        || delta.abs() >= list.styles().metric("motion.drag_threshold", 3.))
                        && target != drag.from
                        && !keys.contains(&target)
                    {
                        if !drag.begun {
                            out.push(Action::History(2));
                            drag.begun = true;
                        }
                        out.push(command(
                            &channel,
                            MaskAction::MoveKey {
                                mask: mask.id,
                                property: property(index),
                                from: drag.from,
                                to: target,
                            },
                        ));
                        drag.from = target;
                        self.selected = Some((channel.clone(), target));
                    }
                }
                if release {
                    if drag.begun {
                        out.push(Action::History(3));
                    }
                } else {
                    self.drag = Some(drag);
                }
            }
        }
        50.
    }
    pub(super) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let zh = locale == Locale::Zh;
        let labels = if zh {
            ["删除关键帧", "线性", "保持", "缓入", "缓出", "缓入缓出"]
        } else {
            [
                "Delete keyframe",
                "Linear",
                "Hold",
                "Ease in",
                "Ease out",
                "Ease in/out",
            ]
        };
        let items: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| DropdownItem {
                label,
                enabled: index == 0 || menu.can_curve,
            })
            .collect();
        let first = Input {
            mouse: input.mouse,
            pressed: Some((
                MouseButton::Left,
                [menu.anchor.min[0] + 1., menu.anchor.min[1]],
            )),
            ..Default::default()
        };
        let choice = menu.dropdown.draw_grouped(
            list,
            atlas,
            menu.anchor,
            area,
            &items,
            0,
            if menu.opened { &first } else { input },
            true,
            true,
            &[1],
            true,
        );
        menu.opened = false;
        if let Some(index) = choice {
            out.push(command(
                &menu.channel,
                if index == 0 {
                    MaskAction::DeleteKey {
                        mask: menu.channel.mask,
                        property: property(menu.channel.property),
                        frame: menu.frame,
                    }
                } else {
                    MaskAction::Curve {
                        mask: menu.channel.mask,
                        property: property(menu.channel.property),
                        frame: menu.frame,
                        easing: Easing {
                            ease: [Ease::Linear, Ease::Hold, Ease::In, Ease::Out, Ease::InOut]
                                [index - 1],
                            curve: None,
                        },
                    }
                },
            ));
            self.menu = None;
        } else if !menu.dropdown.open {
            self.menu = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> motion_core::Engine {
        let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
        let mut layer = Layer::solid(7, "Masked", [100.; 2], [320., 180., 0.], [1.; 4]);
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 12,
            out_frame: 120,
            offset_frame: 12,
        });
        let path = VectorPath {
            id: 1,
            closed: true,
            nodes: [[0., 0.], [100., 0.], [0., 100.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        };
        let mut mask = LayerMask::new(1, path);
        mask.opacity = Track::constant(50.);
        mask.feather = Track::constant([8., 13.]);
        layer.masks.push(mask);
        project.layers.push(layer);
        motion_core::Engine::new(project).unwrap()
    }
    fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Edit(command) => engine.apply(command).unwrap(),
                Action::History(2) => engine.begin_gesture().unwrap(),
                Action::History(3) => engine.end_gesture(true).unwrap(),
                Action::History(4) => engine.end_gesture(false).unwrap(),
                Action::Seek(_) => {}
                _ => panic!("unexpected mask action"),
            }
        }
    }
    fn draw(
        controls: &mut Controls,
        engine: &motion_core::Engine,
        frame: u32,
        index: u8,
        input: &Input,
        atlas: &mut TextAtlas,
    ) -> Vec<Action> {
        let layer = &engine.project().layers[0];
        let mask = &layer.masks[0];
        let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":engine.project(),"frame":frame});
        let local = f64::from(frame) - 12.;
        let (keys, values) = match index {
            0 => (
                mask.opacity
                    .keys
                    .iter()
                    .map(|k| k.frame)
                    .collect::<Vec<_>>(),
                vec![mask.opacity.sample(local)],
            ),
            1 => (
                mask.feather.keys.iter().map(|k| k.frame).collect(),
                mask.feather.sample(local).to_vec(),
            ),
            _ => (
                mask.expansion.keys.iter().map(|k| k.frame).collect(),
                vec![mask.expansion.sample(local)],
            ),
        };
        let mut out = vec![];
        controls.prepare(&state, layer, input, &mut out);
        let modal = controls.modal();
        let neutral = Input::default();
        let mut list = PaintList::default();
        controls.row(
            &mut list,
            Rect::new(0., 0., 400., 300.),
            0.,
            &state,
            layer,
            mask,
            index,
            &keys,
            &values,
            if modal { &neutral } else { input },
            &mut out,
        );
        controls.overlay(
            &mut list,
            atlas,
            Rect::new(0., 0., 400., 300.),
            input,
            Locale::En,
            &mut out,
        );
        out
    }
    fn press(button: MouseButton, x: f32, y: f32) -> Input {
        Input {
            mouse: [x, y],
            pressed: Some((button, [x, y])),
            held: Some(button),
            ..Default::default()
        }
    }
    #[test]
    fn mask_ui_animates_all_channels_at_local_time_and_key_toggle_undoes() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for index in 0..3 {
            let mut engine = engine();
            let original = engine.project().clone();
            let mut controls = Controls::default();
            let actions = draw(
                &mut controls,
                &engine,
                21,
                index,
                &press(MouseButton::Left, 24., 12.),
                &mut atlas,
            );
            assert!(matches!(
                &actions[..],
                [Action::Edit(Command::Mask {
                    action: MaskAction::Animate {
                        frame: 21,
                        enabled: true,
                        ..
                    },
                    ..
                })]
            ));
            apply(&mut engine, actions);
            let animated = engine.project().clone();
            let actions = draw(
                &mut controls,
                &engine,
                42,
                index,
                &press(MouseButton::Left, 44., 35.),
                &mut atlas,
            );
            apply(&mut engine, actions);
            let mask = &engine.project().layers[0].masks[0];
            let frames = match index {
                0 => mask
                    .opacity
                    .keys
                    .iter()
                    .map(|k| k.frame)
                    .collect::<Vec<_>>(),
                1 => mask.feather.keys.iter().map(|k| k.frame).collect(),
                _ => mask.expansion.keys.iter().map(|k| k.frame).collect(),
            };
            assert_eq!(frames, vec![9, 30]);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &animated);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &original);
        }
    }
    #[test]
    fn mask_key_drag_preserves_other_keys_and_easing_and_commits_or_cancels_once() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for cancel in [false, true] {
            let mut engine = engine();
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
                .apply(Command::Mask {
                    object: 7,
                    action: MaskAction::Curve {
                        mask: 1,
                        property: MaskProperty::Feather,
                        frame: 21,
                        easing: Easing {
                            ease: Ease::InOut,
                            curve: None,
                        },
                    },
                })
                .unwrap();
            let before = engine.project().clone();
            let mut controls = Controls::default();
            let x = |frame: u32| 80. + frame as f32 / 119. * 304.;
            let actions = draw(
                &mut controls,
                &engine,
                21,
                1,
                &press(MouseButton::Left, x(21) + 2., 35.),
                &mut atlas,
            );
            apply(&mut engine, actions);
            // Pointer-to-key offset must remain stable across intermediate host snapshots.
            for frame in [30, 42] {
                let actions = draw(
                    &mut controls,
                    &engine,
                    21,
                    1,
                    &Input {
                        mouse: [x(frame) + 2., 35.],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut atlas,
                );
                apply(&mut engine, actions);
            }
            let actions = draw(
                &mut controls,
                &engine,
                21,
                1,
                &if cancel {
                    Input {
                        focus_lost: true,
                        ..Default::default()
                    }
                } else {
                    Input {
                        mouse: [x(45) + 2., 35.],
                        released: Some((MouseButton::Left, [x(45) + 2., 35.])),
                        ..Default::default()
                    }
                },
                &mut atlas,
            );
            apply(&mut engine, actions);
            if cancel {
                assert_eq!(engine.project(), &before);
            } else {
                let keys = &engine.project().layers[0].masks[0].feather.keys;
                assert_eq!(
                    (keys[0].frame, keys[0].value, keys[0].ease),
                    (33, [8., 13.], Ease::InOut)
                );
                assert_eq!(keys[1], before.layers[0].masks[0].feather.keys[1]);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
            }
            assert!(!controls.editing());
        }
    }
    #[test]
    fn mask_key_context_menu_changes_interpolation_and_delete_is_undoable() {
        let mut engine = engine();
        let mut controls = Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let actions = draw(
            &mut controls,
            &engine,
            21,
            0,
            &press(MouseButton::Left, 24., 12.),
            &mut atlas,
        );
        apply(&mut engine, actions);
        engine
            .apply(Command::Mask {
                object: 7,
                action: MaskAction::Set {
                    mask: 1,
                    property: MaskProperty::Opacity,
                    frame: 60,
                    value: vec![80.],
                    animated: Some(true),
                },
            })
            .unwrap();
        let x = 80. + 21. / 119. * 304.;
        let actions = draw(
            &mut controls,
            &engine,
            21,
            0,
            &press(MouseButton::Right, x, 35.),
            &mut atlas,
        );
        assert!(actions.is_empty());
        for _ in 0..5 {
            assert!(draw(
                &mut controls,
                &engine,
                21,
                0,
                &Input {
                    keys: vec![(Key::Down, Default::default())],
                    ..Default::default()
                },
                &mut atlas
            )
            .is_empty());
        }
        let actions = draw(
            &mut controls,
            &engine,
            21,
            0,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            &mut atlas,
        );
        assert!(matches!(
            &actions[..],
            [Action::Edit(Command::Mask {
                action: MaskAction::Curve {
                    easing: Easing {
                        ease: Ease::InOut,
                        ..
                    },
                    ..
                },
                ..
            })]
        ));
        apply(&mut engine, actions);
        let before = engine.project().clone();
        let actions = draw(
            &mut controls,
            &engine,
            21,
            0,
            &press(MouseButton::Right, x, 35.),
            &mut atlas,
        );
        assert!(actions.is_empty());
        let actions = draw(
            &mut controls,
            &engine,
            21,
            0,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut engine, actions);
        assert_eq!(engine.project().layers[0].masks[0].opacity.keys.len(), 1);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
    #[test]
    fn animated_feather_numeric_drag_preserves_y_and_cancels_when_its_context_changes() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for cancel in ["commit", "composition", "locked", "frame"] {
            let mut engine = engine();
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
            let before = engine.project().clone();
            let mut controls = crate::panels::path_controls::Controls::default();
            controls.masks_open = true;
            let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":engine.project(),"frame":33});
            let mut actions = vec![];
            controls.inspector_section(
                false,
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 400., 600.),
                &mut 0.,
                &engine.project().layers[0],
                &state,
                &press(MouseButton::Left, 210., 168.),
                Locale::Zh,
                &mut actions,
            );
            assert!(controls.parameter.is_some());
            apply(&mut engine, actions);
            let mut actions = vec![];
            controls.parameter_input(
                &state,
                &Input {
                    mouse: [230., 168.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                &mut actions,
                3.,
            );
            apply(&mut engine, actions);
            assert_eq!(
                engine.project().layers[0].masks[0].feather.keys[1].value,
                [28., 13.]
            );
            let mut changed = state.clone();
            match cancel {
                "composition" => changed["composition"] = serde_json::json!("child"),
                "locked" => changed["project"]["layers"][0]["locked"] = serde_json::json!(true),
                "frame" => changed["frame"] = serde_json::json!(34),
                _ => {}
            }
            let mut actions = vec![];
            controls.parameter_input(
                &changed,
                &Input {
                    mouse: [240., 168.],
                    released: Some((MouseButton::Left, [240., 168.])),
                    ..Default::default()
                },
                &mut actions,
                3.,
            );
            apply(&mut engine, actions);
            if cancel == "commit" {
                let keys = &engine.project().layers[0].masks[0].feather.keys;
                assert_eq!((keys[1].frame, keys[1].value), (21, [38., 13.]));
                assert_eq!(keys[0], before.layers[0].masks[0].feather.keys[0]);
                assert_eq!(keys[2], before.layers[0].masks[0].feather.keys[1]);
                engine.undo().unwrap();
            }
            assert_eq!(engine.project(), &before);
            assert!(controls.parameter.is_none());
        }
    }
    #[test]
    fn delete_shortcut_in_mask_panel_deletes_only_the_selected_key_in_main_and_floating_windows() {
        use motion_ui::dock::{Node, Panel};
        for floating in [false, true] {
            let mut engine = engine();
            engine
                .apply(Command::Mask {
                    object: 7,
                    action: MaskAction::Animate {
                        mask: 1,
                        property: MaskProperty::Opacity,
                        frame: 21,
                        enabled: true,
                    },
                })
                .unwrap();
            let before = engine.project().clone();
            let mut panels = crate::panels::Panels::default();
            panels.selected = Some(7);
            panels.focused = Some(Panel::Masks);
            panels.path_controls.masks_open = true;
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"frame":21,"project":engine.project()});
            let mut render = |panels: &mut crate::panels::Panels, input: &Input| {
                if floating {
                    crate::panels::floating_chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        Panel::Masks,
                        [400., 600.],
                        panels,
                        &state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        [400., 600.],
                        &Node::dock(Panel::Masks.dock()),
                        panels,
                        &state,
                        input,
                    )
                }
            };
            // Derive the content origin from the same chrome used for hit testing.
            let top = if floating {
                30.
            } else {
                crate::panels::frames(&Node::dock(Panel::Masks.dock()), [400., 600.])[0]
                    .content
                    .min[1]
            };
            let point = [80. + 21. / 119. * 304., top + 8. + 128. + 35.];
            apply(
                &mut engine,
                render(&mut panels, &press(MouseButton::Left, point[0], point[1])),
            );
            apply(
                &mut engine,
                render(
                    &mut panels,
                    &Input {
                        mouse: point,
                        released: Some((MouseButton::Left, point)),
                        ..Default::default()
                    },
                ),
            );
            let actions = render(
                &mut panels,
                &Input {
                    keys: vec![(Key::Delete, Default::default())],
                    ..Default::default()
                },
            );
            assert!(matches!(
                &actions[..],
                [Action::Edit(Command::Mask {
                    action: MaskAction::DeleteKey { .. },
                    ..
                })]
            ));
            apply(&mut engine, actions);
            assert_eq!(engine.project().layers.len(), 1);
            assert!(engine.project().layers[0].masks[0].opacity.keys.is_empty());
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
