//! Shape-content keys share the timeline clock, viewport and gesture protocol.
use super::*;
use motion_core::vector::groups::KeyEdit;
use motion_core::{Ease, Easing};
use motion_ui::widgets::{Dropdown, DropdownItem};

#[derive(Clone, PartialEq, Eq)]
struct Channel {
    object: u64,
    item: u64,
    parameter: String,
}
struct Drag {
    channel: Channel,
    from: u32,
    origin: u32,
    start: f32,
    begun: bool,
    copy: bool,
    lane: Rect,
    ppf: f32,
    offset: f32,
    layer_offset: i32,
}
struct Menu {
    channel: Channel,
    frame: u32,
    anchor: Rect,
    dropdown: Dropdown,
    opened: bool,
    existing: bool,
    can_curve: bool,
    parameter: parameters::Parameter,
}
#[derive(Default)]
pub(in crate::panels) struct Controls {
    selection_intent: bool,
    context: Option<(String, String)>,
    pub(super) cache: SourceCache,
    active: Option<Channel>,
    selected: Option<(Channel, u32)>,
    drag: Option<Drag>,
    menu: Option<Menu>,
}
pub(super) type SourceCache =
    std::cell::RefCell<Option<(String, String, u64, u64, std::sync::Arc<Layer>)>>;
/// Project revision is stable across seeks; cache authored tracks, not sampled values.
pub(super) fn cached_layer(
    state: &Value,
    object: u64,
    cache: &SourceCache,
) -> Option<std::sync::Arc<Layer>> {
    let root = state["root"].as_str().unwrap_or("");
    let comp = state["composition"].as_str().unwrap_or("");
    let revision = state["revision"].as_u64();
    if let Some(revision) = revision {
        if let Some((r, c, o, v, layer)) = cache.borrow().as_ref() {
            if r == root && c == comp && *o == object && *v == revision {
                return Some(layer.clone());
            }
        }
    }
    let layer = std::sync::Arc::new(super::super::layer(state, object)?);
    if let Some(revision) = revision {
        *cache.borrow_mut() = Some((root.into(), comp.into(), object, revision, layer.clone()));
    }
    Some(layer)
}
fn snapshot(
    state: &Value,
    c: &Channel,
    frame: u32,
    cache: &SourceCache,
) -> Option<(std::sync::Arc<Layer>, parameters::Parameter)> {
    let l = cached_layer(state, c.object, cache)?;
    let Content::Vector { vector } = &l.content else {
        return None;
    };
    let p = parameters::lookup_source(
        vector,
        l.size,
        c.item,
        &c.parameter,
        l.local_frame(f64::from(frame)),
        Locale::En,
    )?;
    Some((l, p))
}

fn command(c: &Channel, edit: KeyEdit) -> Action {
    Action::Edit(Command::Vector {
        object: c.object,
        action: VectorAction::EditGroupKey {
            item: c.item,
            parameter: c.parameter.clone(),
            edit,
        },
    })
}
impl Controls {
    pub(in crate::panels) fn selected_in(&self, state: &Value, object: u64, item: u64, parameter: &str) -> bool {
        self.active.as_ref().is_some_and(|channel|
            channel.object == object && channel.item == item && channel.parameter == parameter)
            && self.context.as_ref().is_some_and(|(root, composition)|
                root == state["root"].as_str().unwrap_or("")
                    && composition == state["composition"].as_str().unwrap_or(""))
    }

    pub(in crate::panels) fn take_selection_intent(&mut self) -> bool {
        std::mem::take(&mut self.selection_intent)
    }
    pub(super) fn navigation(
        &mut self,
        list: &mut PaintList,
        row: Rect,
        state: &Value,
        object: u64,
        item: u64,
        parameter: &parameters::Parameter,
        input: &Input,
        enabled: bool,
        out: &mut Vec<Action>,
    ) {
        use crate::panels::key_navigation::{self as navigation, Choice};
        let Some(layer) = cached_layer(state, object, &self.cache) else { return };
        let offset = layer.timeline.map_or(0, |timeline| timeline.offset_frame);
        let at = state["frame"].as_f64().unwrap_or(0.);
        let last = crate::panels::last_frame(state);
        let adjacent = navigation::neighbours(parameter.keys.iter()
            .map(|(frame, _)| f64::from(*frame) + f64::from(offset)), at, last);
        let frame = navigation::editable_frame(at, last);
        let editable = enabled && !layer.locked && state["observing"] != true
            && !parameter.static_value && frame.is_some();
        let rect = navigation::rail(&list.styles(), row);
        let Some(choice) = navigation::draw(list, rect, input, adjacent, editable) else { return };
        self.prepare(state, input, out);
        self.choose(object, item, parameter);
        let channel = Channel { object, item, parameter: parameter.id.clone() };
        match choice {
            Choice::Seek(frame) => {
                self.selected = Some((channel, frame as u32));
                out.push(Action::Seek(frame));
            }
            Choice::Toggle => {
                let Some(frame) = frame.filter(|_| editable) else { return };
                out.push(if adjacent.current {
                    command(&channel, KeyEdit::Delete { frame })
                } else {
                    Action::Edit(parameter.set(object, item, frame, Some(true)))
                });
                self.selected = (!adjacent.current).then_some((channel, frame));
            }
        }
    }
    pub(in crate::panels) fn editing(&self) -> bool {
        self.drag.is_some() || self.menu.is_some()
    }
    pub(in crate::panels) fn modal(&self) -> bool {
        self.menu.is_some()
    }
    pub(in crate::panels) fn clear_selection(&mut self) {
        self.selection_intent = false;
        self.active = None;
        self.selected = None;
    }
    pub(in crate::panels) fn cancel(&mut self) -> bool {
        self.menu = None;
        self.clear_selection();
        self.drag.take().is_some_and(|d| d.begun)
    }
    pub(in crate::panels) fn choose(&mut self, object: u64, item: u64, p: &parameters::Parameter) {
        self.selection_intent = true;
        self.active = Some(Channel {
            object,
            item,
            parameter: p.id.clone(),
        });
        self.selected = None;
    }
    pub(in crate::panels) fn prepare(
        &mut self,
        state: &Value,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
        );
        let invalid = self.drag.as_ref().is_some_and(|d| {
            snapshot(state, &d.channel, d.from, &self.cache).is_none_or(|(l, p)| {
                l.locked
                    || l.timeline.map_or(0, |t| t.offset_frame) != d.layer_offset
                    || !p.keys.iter().any(|(f, _)| {
                        i64::from(*f) + i64::from(d.layer_offset) == i64::from(d.from)
                    })
            })
        }) || self.menu.as_ref().is_some_and(|m| {
            snapshot(state, &m.channel, m.frame, &self.cache).is_none_or(|(l, p)| {
                l.locked
                    || (m.existing
                        && !p.keys.iter().any(|(f, _)| {
                            i64::from(*f) + i64::from(l.timeline.map_or(0, |t| t.offset_frame))
                                == i64::from(m.frame)
                        }))
            })
        });
        if self.context.as_ref() != Some(&context)
            || invalid
            || state["observing"] == true
            || input.focus_lost
            || input.keys.iter().any(|(k, _)| *k == Key::Escape)
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        self.context = Some(context);
        if let Some(m) = &mut self.menu {
            if let Some((l, p)) = snapshot(state, &m.channel, m.frame, &self.cache) {
                let off = l.timeline.map_or(0, |t| t.offset_frame);
                m.can_curve = m.existing
                    && !p.discrete
                    && p.keys
                        .iter()
                        .any(|(f, _)| i64::from(*f) + i64::from(off) > i64::from(m.frame));
                m.parameter = p;
            }
        }
    }
    pub(in crate::panels) fn lane(
        &mut self,
        list: &mut PaintList,
        lane: Rect,
        ppf: f32,
        offset: f32,
        state: &Value,
        object: u64,
        item: u64,
        p: &parameters::Parameter,
        layer_offset: i32,
        locked: bool,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        if p.static_value {
            return;
        }
        let c = Channel {
            object,
            item,
            parameter: p.id.clone(),
        };
        let total = state["project"]["frames"].as_u64().unwrap_or(1).max(1) as u32;
        let styles = list.styles();
        let colors = styles.colors();
        let radius = styles.metric("graph.handle_radius", 4.);
        let hit_radius = radius + styles.metric("motion.drag_threshold", 3.);
        let cy = lane.min[1] + lane.height() * 0.5;
        let mut paint = list.child();
        let mut hit = None;
        for (f, _) in &p.keys {
            let time = i64::from(*f) + i64::from(layer_offset);
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
                .is_some_and(|(channel, f)| *channel == c && i64::from(*f) == time);
            paint::diamond(
                &mut paint,
                [x, cy],
                radius,
                if selected || (time as f64 - state["frame"].as_f64().unwrap_or(0.)).abs() < 0.001 {
                    colors.KEYFRAME
                } else {
                    colors.ACCENT
                },
            );
            if distance(input.mouse, [x, cy]) <= hit_radius {
                hit = Some(time as u32);
            }
        }
        paint.clip_to(lane);
        list.append(paint);
        if let Some((button, point)) = input.pressed.filter(|(_, point)| lane.contains(*point)) {
            self.active = Some(c.clone());
            self.selection_intent = true;
            self.selected = hit.map(|f| (c.clone(), f));
            if button == MouseButton::Left {
                if let Some(frame) = hit.filter(|_| !locked) {
                    self.drag = Some(Drag {
                        channel: c.clone(),
                        from: frame,
                        origin: frame,
                        start: point[0],
                        begun: false,
                        copy: input.modifiers.control,
                        lane,
                        ppf,
                        offset,
                        layer_offset,
                    });
                    out.push(Action::Seek(f64::from(frame)));
                }
            }
            if button == MouseButton::Right && !locked {
                let frame = hit.unwrap_or_else(|| {
                    (((point[0] - lane.min[0]) / ppf + offset)
                        .round()
                        .clamp(0., (total - 1) as f32)) as u32
                });
                if let Some((_, parameter)) = snapshot(state, &c, frame, &self.cache) {
                    self.menu = Some(Menu {
                        channel: c,
                        frame,
                        anchor: Rect::new(
                            point[0],
                            point[1],
                            styles.metric("dropdown.menu_min_width", 160.),
                            styles.metric("border", 1.),
                        ),
                        dropdown: Dropdown::default(),
                        opened: true,
                        existing: hit.is_some(),
                        can_curve: hit.is_some()
                            && !p.discrete
                            && p.keys.iter().any(|(f, _)| {
                                i64::from(*f) + i64::from(layer_offset) > i64::from(frame)
                            }),
                        parameter,
                    });
                }
            }
        }
    }
    pub(in crate::panels) fn process(
        &mut self,
        list: &PaintList,
        state: &Value,
        selected: Option<u64>,
        body: Rect,
        ppf: f32,
        offset: f32,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        self.prepare(state, input, out);
        let Some(mut d) = self.drag.take() else {
            return;
        };
        let Some((l, p)) = snapshot(state, &d.channel, d.from, &self.cache) else {
            if d.begun {
                out.push(Action::History(4));
            }
            return;
        };
        if selected != Some(d.channel.object)
            || l.locked
            || d.ppf != ppf
            || d.offset != offset
            || d.lane.min[1] < body.min[1]
            || d.lane.max[1] > body.max[1]
            || !ppf.is_finite()
            || ppf <= 0.
        {
            if d.begun {
                out.push(Action::History(4));
            }
            return;
        }
        let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
        if input.dragging() || release {
            let delta = input.mouse[0] - d.start;
            let total = state["project"]["frames"].as_u64().unwrap_or(1).max(1) as u32;
            let target = (d.origin as f32 + delta / ppf)
                .round()
                .clamp(0., (total - 1) as f32) as u32;
            let occupied = p
                .keys
                .iter()
                .any(|(f, _)| i64::from(*f) + i64::from(d.layer_offset) == i64::from(target));
            if (d.begun
                || motion_ui::widgets::numeric_behavior::started(
                    delta,
                    input.pointer_travel,
                    list.styles().metric("motion.drag_threshold", 3.),
                ))
                && target != d.from
                && !occupied
            {
                let copy = d.copy && !d.begun;
                if !d.begun {
                    out.push(Action::History(2));
                    d.begun = true;
                }
                out.push(command(
                    &d.channel,
                    if copy {
                        KeyEdit::Copy {
                            from: d.from,
                            to: target,
                        }
                    } else {
                        KeyEdit::Move {
                            from: d.from,
                            to: target,
                        }
                    },
                ));
                d.from = target;
                self.selected = Some((d.channel.clone(), target));
            }
        }
        if release {
            if d.begun {
                out.push(Action::History(3));
            }
        } else {
            self.drag = Some(d);
        }
    }
    pub(in crate::panels) fn shortcuts(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
        );
        if self.context.as_ref() != Some(&context) {
            let had_key = self.selected.is_some();
            let had_track = self.active.is_some();
            if self.cancel() {
                out.push(Action::History(4));
            }
            return had_track && input.key_presses().any(|(k, m, _)|
                matches!(k, Key::J | Key::K) && !m.control && !m.alt && !m.shift)
                || had_key
                && input
                    .key_presses()
                    .any(|(k, _, _)| matches!(k, Key::Delete | Key::Backspace));
        }
        if input.key_presses().any(|(k, m, r)| {
            !r && matches!(k, Key::Delete | Key::Backspace) && !m.control && !m.alt && !m.shift
        }) {
            if let Some((c, f)) = self.selected.take() {
                if selected == Some(c.object)
                    && snapshot(state, &c, f, &self.cache).is_some_and(|(l, p)| {
                        !l.locked
                            && p.keys.iter().any(|(at, _)| {
                                i64::from(*at) + i64::from(l.timeline.map_or(0, |t| t.offset_frame))
                                    == i64::from(f)
                            })
                    })
                {
                    out.push(command(&c, KeyEdit::Delete { frame: f }));
                }
                return true;
            }
        }
        let Some(c) = self.active.clone().filter(|c| Some(c.object) == selected) else {
            return false;
        };
        let Some((l, p)) = snapshot(
            state,
            &c,
            state["frame"].as_f64().unwrap_or(0.).max(0.) as u32,
            &self.cache,
        ) else {
            return false;
        };
        let total = state["project"]["frames"].as_i64().unwrap_or(1);
        let mut at = state["frame"].as_f64().unwrap_or(0.);
        let mut used = false;
        for (k, m, _) in input.key_presses() {
            if !matches!(k, Key::J | Key::K) || m.control || m.alt || m.shift {
                continue;
            }
            used = true;
            let keys = p
                .keys
                .iter()
                .map(|(f, _)| i64::from(*f) + i64::from(l.timeline.map_or(0, |t| t.offset_frame)))
                .filter(|f| *f >= 0 && *f < total);
            let adjacent = crate::panels::key_navigation::neighbours(keys.map(|f| f as f64), at, (total - 1) as f64);
            let target = if k == Key::J { adjacent.previous } else { adjacent.next };
            if let Some(f) = target {
                at = f;
                self.selected = Some((c.clone(), f as u32));
                out.push(Action::Seek(at));
            }
        }
        used
    }
    pub(in crate::panels) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        let Some(m) = &mut self.menu else { return };
        let zh = locale == Locale::Zh;
        let labels = [
            if m.existing {
                if zh {
                    "删除关键帧"
                } else {
                    "Delete keyframe"
                }
            } else if zh {
                "添加关键帧"
            } else {
                "Add keyframe"
            },
            if zh { "线性" } else { "Linear" },
            if zh { "保持" } else { "Hold" },
            if zh { "缓入" } else { "Ease in" },
            if zh { "缓出" } else { "Ease out" },
            if zh { "缓入缓出" } else { "Ease in/out" },
        ];
        let items: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| DropdownItem {
                label,
                enabled: i == 0 || m.can_curve,
            })
            .collect();
        let first = Input {
            mouse: input.mouse,
            pressed: Some((
                MouseButton::Left,
                [
                    m.anchor.min[0] + list.styles().metric("border", 1.),
                    m.anchor.min[1],
                ],
            )),
            ..Default::default()
        };
        let choice = m.dropdown.draw_grouped(
            list,
            atlas,
            m.anchor,
            area,
            &items,
            0,
            if m.opened { &first } else { input },
            true,
            true,
            &[1],
            true,
        );
        m.opened = false;
        if let Some(i) = choice {
            if i == 0 && !m.existing {
                out.push(Action::Edit(m.parameter.set(
                    m.channel.object,
                    m.channel.item,
                    m.frame,
                    Some(true),
                )));
            } else {
                let edit = if i == 0 {
                    KeyEdit::Delete { frame: m.frame }
                } else {
                    KeyEdit::Curve {
                        frame: m.frame,
                        easing: Easing {
                            ease: [Ease::Linear, Ease::Hold, Ease::In, Ease::Out, Ease::InOut]
                                [i - 1],
                            curve: None,
                        },
                    }
                };
                out.push(command(&m.channel, edit));
            }
            self.menu = None;
        } else if !m.dropdown.open {
            self.menu = None;
        }
    }
}
