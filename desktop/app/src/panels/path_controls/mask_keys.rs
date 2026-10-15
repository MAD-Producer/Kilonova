//! Mask keys use composition time and the shared timeline's viewport.
use super::*;
use motion_core::{Ease, Easing};
use motion_ui::{
    icons::Icon,
    widgets::{Dropdown, DropdownItem},
};

#[derive(Clone, Copy)]
pub(super) struct Owner {
    pub object: u64,
    pub locked: bool,
    pub offset: i32,
}
impl From<&Layer> for Owner {
    fn from(layer: &Layer) -> Self {
        Self {
            object: layer.id,
            locked: layer.locked,
            offset: layer.timeline.map_or(0, |t| t.offset_frame),
        }
    }
}
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
pub(super) fn frames(mask: &LayerMask, index: u8) -> Vec<i32> {
    match index {
        0 => mask.opacity.keys.iter().map(|k| k.frame).collect(),
        1 => mask.feather.keys.iter().map(|k| k.frame).collect(),
        _ => mask.expansion.keys.iter().map(|k| k.frame).collect(),
    }
}
pub(super) fn values(mask: &LayerMask, index: u8, local: f64) -> Vec<f32> {
    match index {
        0 => vec![mask.opacity.sample(local)],
        1 => mask.feather.sample(local).to_vec(),
        _ => vec![mask.expansion.sample(local)],
    }
}
pub(super) fn animated(mask: &LayerMask, index: u8) -> bool {
    match index {
        0 => mask.opacity.is_animated(),
        1 => mask.feather.is_animated(),
        _ => mask.expansion.is_animated(),
    }
}
struct Drag {
    channel: Channel,
    from: u32,
    origin: u32,
    start: f32,
    begun: bool,
    lane: Rect,
    ppf: f32,
    offset: f32,
}
struct Menu {
    channel: Channel,
    frame: u32,
    anchor: Rect,
    dropdown: Dropdown,
    opened: bool,
    existing: bool,
    can_curve: bool,
    values: Vec<f32>,
}
#[derive(Default)]
pub(super) struct Controls {
    context: Option<(String, String, u64)>,
    active: Option<Channel>,
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
impl Controls {
    pub(super) fn editing(&self) -> bool {
        self.drag.is_some() || self.menu.is_some()
    }
    pub(super) fn modal(&self) -> bool {
        self.menu.is_some()
    }
    pub(super) fn clear_selection(&mut self) {
        self.active = None;
        self.selected = None;
    }
    pub(super) fn cancel(&mut self) -> bool {
        self.menu = None;
        self.clear_selection();
        self.drag.take().is_some_and(|d| d.begun)
    }
    pub(super) fn owner(&self) -> Option<u64> {
        self.drag
            .as_ref()
            .map(|d| d.channel.object)
            .or_else(|| self.active.as_ref().map(|c| c.object))
    }
    pub(super) fn prepare(
        &mut self,
        state: &Value,
        layer: &Layer,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        self.prepare_owner(state, Owner::from(layer), &layer.masks, input, out);
    }
    pub(super) fn prepare_owner(
        &mut self,
        state: &Value,
        owner: Owner,
        masks: &[LayerMask],
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            owner.object,
        );
        let key_frames = |channel: &Channel| {
            masks
                .iter()
                .find(|m| m.id == channel.mask)
                .map(|m| frames(m, channel.property))
        };
        let missing = self.drag.as_ref().is_some_and(|d| {
            key_frames(&d.channel).is_none_or(|keys| {
                !keys
                    .iter()
                    .any(|f| i64::from(*f) + i64::from(owner.offset) == i64::from(d.from))
            })
        }) || self.menu.as_ref().is_some_and(|m| {
            m.existing
                && key_frames(&m.channel).is_none_or(|keys| {
                    !keys
                        .iter()
                        .any(|f| i64::from(*f) + i64::from(owner.offset) == i64::from(m.frame))
                })
        });
        if let Some(menu) = &mut self.menu {
            menu.can_curve = menu.existing
                && key_frames(&menu.channel).is_some_and(|keys| {
                    keys.iter()
                        .any(|f| i64::from(*f) + i64::from(owner.offset) > i64::from(menu.frame))
                });
        }
        if self.context.as_ref() != Some(&context)
            || owner.locked
            || missing
            || input.focus_lost
            || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.context = Some(context);
    }
    pub(super) fn choose(
        &mut self,
        state: &Value,
        owner: Owner,
        mask: u64,
        index: u8,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            owner.object,
        );
        if self.context.as_ref() != Some(&context) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.context = Some(context);
        }
        self.active = Some(Channel {
            object: owner.object,
            mask,
            property: index,
        });
        self.selected = None;
    }
    pub(super) fn watch(
        &mut self,
        list: &mut PaintList,
        rect: Rect,
        state: &Value,
        owner: Owner,
        mask: &LayerMask,
        index: u8,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        crate::panels::timeline::property_icon(
            list,
            rect,
            Icon::Stopwatch,
            animated(mask, index),
            !owner.locked,
        );
        if !owner.locked && click(input, rect) {
            out.push(command(
                &Channel {
                    object: owner.object,
                    mask: mask.id,
                    property: index,
                },
                MaskAction::Animate {
                    mask: mask.id,
                    property: property(index),
                    frame: state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32,
                    enabled: !animated(mask, index),
                },
            ));
        }
    }
    pub(super) fn lane(
        &mut self,
        list: &mut PaintList,
        lane: Rect,
        ppf: f32,
        offset: f32,
        state: &Value,
        owner: Owner,
        mask: &LayerMask,
        index: u8,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let channel = Channel {
            object: owner.object,
            mask: mask.id,
            property: index,
        };
        let stored = frames(mask, index);
        let total = state["project"]["frames"].as_u64().unwrap_or(1).max(1) as u32;
        let colors = list.styles().colors();
        let mut keys = list.child();
        let mut hit = None;
        let cy = lane.min[1] + lane.height() * 0.5;
        for local in &stored {
            let time = i64::from(*local) + i64::from(owner.offset);
            if time < 0 || time >= i64::from(total) {
                continue;
            }
            let x = lane.min[0] + (time as f32 - offset) * ppf;
            if x < lane.min[0] || x >= lane.max[0] {
                continue;
            }
            let selected = self
                .selected
                .as_ref()
                .is_some_and(|(c, f)| *c == channel && i64::from(*f) == time);
            let current = (time as f64 - state["frame"].as_f64().unwrap_or(0.)).abs() < 0.001;
            paint::diamond(
                &mut keys,
                [x, cy],
                4.,
                if selected || current {
                    colors.KEYFRAME
                } else {
                    colors.ACCENT
                },
            );
            if distance(input.mouse, [x, cy]) <= 7. {
                hit = Some(time as u32);
            }
        }
        keys.clip_to(lane);
        list.append(keys);
        if let Some((button, point)) = input.pressed.filter(|(_, p)| lane.contains(*p)) {
            self.active = Some(channel.clone());
            if let Some(time) = hit {
                self.selected = Some((channel.clone(), time));
                if button == MouseButton::Left && !owner.locked {
                    self.drag = Some(Drag {
                        channel: channel.clone(),
                        from: time,
                        origin: time,
                        start: point[0],
                        begun: false,
                        lane,
                        ppf,
                        offset,
                    });
                    out.push(Action::Seek(f64::from(time)));
                }
            } else {
                self.selected = None;
            }
            if button == MouseButton::Right && !owner.locked {
                let time = hit.unwrap_or_else(|| {
                    (((point[0] - lane.min[0]) / ppf + offset)
                        .round()
                        .clamp(0., (total - 1) as f32)) as u32
                });
                self.menu = Some(Menu {
                    channel,
                    frame: time,
                    anchor: Rect::new(point[0], point[1], 200., 1.),
                    dropdown: Dropdown::default(),
                    opened: true,
                    existing: hit.is_some(),
                    can_curve: hit.is_some()
                        && stored
                            .iter()
                            .any(|f| i64::from(*f) + i64::from(owner.offset) > i64::from(time)),
                    values: values(mask, index, f64::from(time) - f64::from(owner.offset)),
                });
            }
        }
    }
    pub(super) fn process_drag(
        &mut self,
        state: &Value,
        owner: Owner,
        masks: &[LayerMask],
        body: Rect,
        ppf: f32,
        offset: f32,
        input: &Input,
        out: &mut Vec<Action>,
        threshold: f32,
    ) {
        self.prepare_owner(state, owner, masks, input, out);
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let Some(mask) = masks.iter().find(|m| m.id == drag.channel.mask) else {
            if drag.begun {
                out.push(Action::History(4));
            }
            return;
        };
        if drag.ppf != ppf
            || drag.offset != offset
            || drag.lane.min[1] < body.min[1]
            || drag.lane.max[1] > body.max[1]
            || !ppf.is_finite()
            || ppf <= 0.
        {
            if drag.begun {
                out.push(Action::History(4));
            }
            return;
        }
        let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
        if input.dragging() || release {
            let delta = input.mouse[0] - drag.start;
            let total = state["project"]["frames"].as_u64().unwrap_or(1).max(1) as u32;
            let target = (drag.origin as f32 + delta / ppf)
                .round()
                .clamp(0., (total - 1) as f32) as u32;
            let occupied = frames(mask, drag.channel.property)
                .iter()
                .any(|f| i64::from(*f) + i64::from(owner.offset) == i64::from(target));
            if (drag.begun
                || motion_ui::widgets::numeric_behavior::started(
                    delta,
                    input.pointer_travel,
                    threshold,
                ))
                && target != drag.from
                && !occupied
            {
                if !drag.begun {
                    out.push(Action::History(2));
                    drag.begun = true;
                }
                out.push(command(
                    &drag.channel,
                    MaskAction::MoveKey {
                        mask: drag.channel.mask,
                        property: property(drag.channel.property),
                        from: drag.from,
                        to: target,
                    },
                ));
                drag.from = target;
                self.selected = Some((drag.channel.clone(), target));
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
    pub(super) fn navigate(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        let Some(channel) = self.active.clone().filter(|c| Some(c.object) == selected) else {
            return false;
        };
        let Some(layer) = super::layer(state, channel.object) else {
            return false;
        };
        let Some(mask) = layer.masks.iter().find(|m| m.id == channel.mask) else {
            return false;
        };
        let offset = i64::from(Owner::from(&layer).offset);
        let total = state["project"]["frames"].as_u64().unwrap_or(1) as i64;
        let keys: Vec<_> = frames(mask, channel.property)
            .into_iter()
            .map(|f| i64::from(f) + offset)
            .filter(|f| *f >= 0 && *f < total)
            .collect();
        let mut at = state["frame"].as_f64().unwrap_or(0.);
        let mut consumed = false;
        for (key, mods, _) in input
            .key_presses()
            .filter(|(k, m, _)| matches!(k, Key::J | Key::K) && !m.control && !m.alt && !m.shift)
        {
            let _ = mods;
            consumed = true;
            let target = if key == Key::J {
                keys.iter().copied().filter(|f| (*f as f64) < at).max()
            } else {
                keys.iter().copied().find(|f| (*f as f64) > at)
            };
            if let Some(frame) = target {
                at = frame as f64;
                self.selected = Some((channel.clone(), frame as u32));
                out.push(Action::Seek(at));
            }
        }
        consumed
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
        if selected != Some(channel.object) {
            self.clear_selection();
            return true;
        }
        let Some(layer) = super::layer(state, channel.object).filter(|l| !l.locked) else {
            return true;
        };
        let valid = self.context.as_ref()
            == Some(&(
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                channel.object,
            ));
        let local = i64::from(frame) - i64::from(Owner::from(&layer).offset);
        if valid
            && layer
                .masks
                .iter()
                .find(|m| m.id == channel.mask)
                .is_some_and(|m| {
                    frames(m, channel.property)
                        .iter()
                        .any(|f| i64::from(*f) == local)
                })
        {
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
        let labels = if locale == Locale::Zh {
            [
                if menu.existing {
                    "删除关键帧"
                } else {
                    "添加关键帧"
                },
                "线性",
                "保持",
                "缓入",
                "缓出",
                "缓入缓出",
            ]
        } else {
            [
                if menu.existing {
                    "Delete keyframe"
                } else {
                    "Add keyframe"
                },
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
                    if menu.existing {
                        MaskAction::DeleteKey {
                            mask: menu.channel.mask,
                            property: property(menu.channel.property),
                            frame: menu.frame,
                        }
                    } else {
                        MaskAction::Set {
                            mask: menu.channel.mask,
                            property: property(menu.channel.property),
                            frame: menu.frame,
                            value: menu.values.clone(),
                            animated: Some(true),
                        }
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
impl Controls {
    fn row(
        &mut self,
        list: &mut PaintList,
        area: Rect,
        y: f32,
        state: &Value,
        layer: &Layer,
        mask: &LayerMask,
        index: u8,
        _keys: &[i32],
        _values: &[f32],
        input: &Input,
        out: &mut Vec<Action>,
    ) -> f32 {
        let owner = Owner::from(layer);
        self.watch(
            list,
            Rect::new(area.min[0] + 14., y, 20., 24.),
            state,
            owner,
            mask,
            index,
            input,
            out,
        );
        let total = state["project"]["frames"].as_u64().unwrap_or(1).max(1);
        let lane = Rect::new(
            area.min[0] + 80.,
            y + 24.,
            (area.width() - 96.).max(1.),
            22.,
        );
        let ppf = lane.width() / (total - 1).max(1) as f32;
        self.lane(list, lane, ppf, 0., state, owner, mask, index, input, out);
        self.process_drag(
            state,
            owner,
            &layer.masks,
            area,
            ppf,
            0.,
            input,
            out,
            list.styles().metric("motion.drag_threshold", 3.),
        );
        28.
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
            let x = 80. + 42. / 119. * 304.;
            assert!(draw(
                &mut controls,
                &engine,
                42,
                index,
                &press(MouseButton::Right, x, 35.),
                &mut atlas
            )
            .is_empty());
            let actions = draw(
                &mut controls,
                &engine,
                42,
                index,
                &Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                },
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
                &press(MouseButton::Left, 210., 99.),
                Locale::Zh,
                &mut actions,
            );
            assert!(controls.parameter.is_some());
            apply(&mut engine, actions);
            let mut actions = vec![];
            controls.parameter_input(
                &state,
                &Input {
                    mouse: [230., 99.],
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
                    mouse: [240., 99.],
                    released: Some((MouseButton::Left, [240., 99.])),
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
    fn delete_shortcut_in_mask_timeline_deletes_only_the_selected_key_in_main_and_floating_windows()
    {
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
            engine
                .apply(Command::Mask {
                    object: 7,
                    action: MaskAction::Set {
                        mask: 1,
                        property: MaskProperty::Opacity,
                        frame: 60,
                        value: vec![80.],
                        animated: None,
                    },
                })
                .unwrap();
            let before = engine.project().clone();
            let mut panels = crate::panels::Panels::default();
            panels.selected = Some(7);
            panels.focused = Some(Panel::Timeline);
            panels.timeline.expanded.insert(7);
            panels.timeline.collapsed_transforms.insert(7);
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"frame":21,"revision":engine.revision(),"project":engine.project()});
            let layout = Node::dock(Panel::Timeline.dock());
            let top = if floating {
                30.
            } else {
                crate::panels::frames(&layout, [1280., 700.])[0].content.min[1]
            };
            let point = [640. + 21. / 120. * 632., top + 64. + 4. * 22. + 11.];
            let mut render = |panels: &mut crate::panels::Panels, input: &Input| {
                if floating {
                    crate::panels::floating_chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        Panel::Timeline,
                        [1280., 700.],
                        panels,
                        &state,
                        input,
                    )
                } else {
                    crate::panels::chrome(
                        &mut PaintList::default(),
                        &mut atlas,
                        [1280., 700.],
                        &layout,
                        panels,
                        &state,
                        input,
                    )
                }
            };
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
            let navigation = render(
                &mut panels,
                &Input {
                    keys: vec![(Key::K, Default::default())],
                    ..Default::default()
                },
            );
            assert!(matches!(&navigation[..],[Action::Seek(frame)] if *frame==60.));
            let actions = render(
                &mut panels,
                &Input {
                    keys: vec![(Key::Delete, Default::default())],
                    ..Default::default()
                },
            );
            assert!(
                matches!(
                    &actions[..],
                    [Action::Edit(Command::Mask {
                        action: MaskAction::DeleteKey { .. },
                        ..
                    })]
                ),
                "{actions:?}"
            );
            apply(&mut engine, actions);
            assert_eq!(engine.project().layers.len(), 1);
            assert_eq!(engine.project().layers[0].masks[0].opacity.keys.len(), 1);
            assert_eq!(engine.project().layers[0].masks[0].opacity.keys[0].frame, 9);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
