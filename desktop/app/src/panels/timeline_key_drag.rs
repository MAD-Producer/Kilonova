//! Timing-only edits for property diamonds and collapsed layer summaries.
//! A summary owns exactly the authored keys present at the pressed time.
use super::*;
use motion_core::{
    masks::{MaskAction, MaskProperty},
    vector::{groups::KeyEdit, VectorAction},
    Axis,
};
use std::{
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
};

#[derive(Clone, PartialEq, Eq)]
enum Target {
    Transform(Property, Option<Axis>),
    Mask(u64, u8),
    MaskNode(u64, u64),
    Shape(u64, String),
}
impl Hash for Target {
    fn hash<H: Hasher>(&self, h: &mut H) {
        std::mem::discriminant(self).hash(h);
        match self {
            Self::Transform(property, axis) => {
                (*property as u8).hash(h);
                axis.map(|a| a as u8).hash(h);
            }
            Self::Mask(mask, property) => {
                mask.hash(h);
                property.hash(h);
            }
            Self::MaskNode(mask, node) => {
                mask.hash(h);
                node.hash(h);
            }
            Self::Shape(item, parameter) => {
                item.hash(h);
                parameter.hash(h);
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct Drag {
    object: u64,
    targets: Vec<Target>,
    occupied: HashMap<Target, BTreeSet<i64>>,
    expected_revision: u64,
    root: String,
    composition: String,
    layer_offset: i32,
    origin: u32,
    current: u32,
    press: f32,
    ppf: f32,
    offset: f32,
    copy: bool,
    begun: bool,
}
impl Drag {
    pub(crate) fn begun(&self) -> bool {
        self.begun
    }
}

fn channels(state: &Value, row: &timeline::Row) -> HashMap<Target, BTreeSet<i64>> {
    let mut out = HashMap::new();
    let mut record = |target, frames: &mut dyn Iterator<Item = i64>| {
        out.insert(target, frames.collect());
    };
    for property in [
        Property::Anchor,
        Property::Position,
        Property::Rotation,
        Property::Scale,
        Property::Opacity,
        Property::Target,
        Property::Roll,
        Property::Fov,
        Property::Radius,
        Property::Azimuth,
        Property::Elevation,
    ] {
        for axis in [None, Some(Axis::X), Some(Axis::Y), Some(Axis::Z)] {
            if let Some(source) =
                animation_data::channel(state, row.id, property, axis).filter(|s| !s.is_null())
            {
                record(
                    Target::Transform(property, axis),
                    &mut source["keys"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|key| key["frame"].as_i64()),
                );
            }
        }
    }
    let offset = i64::from(row.mask_offset);
    for mask in &row.masks {
        record(
            Target::Mask(mask.id, 0),
            &mut mask
                .opacity
                .keys
                .iter()
                .map(|k| i64::from(k.frame) + offset),
        );
        record(
            Target::Mask(mask.id, 1),
            &mut mask
                .feather
                .keys
                .iter()
                .map(|k| i64::from(k.frame) + offset),
        );
        record(
            Target::Mask(mask.id, 2),
            &mut mask
                .expansion
                .keys
                .iter()
                .map(|k| i64::from(k.frame) + offset),
        );
        for node in &mask.path.nodes {
            record(
                Target::MaskNode(mask.id, node.id),
                &mut node
                    .geometry
                    .keys
                    .iter()
                    .map(|k| i64::from(k.frame) + offset),
            );
        }
    }
    fn shape(
        node: path_controls::groups::parameters::Node,
        offset: i64,
        record: &mut impl FnMut(Target, &mut dyn Iterator<Item = i64>),
    ) {
        for p in node.parameters {
            record(
                Target::Shape(node.id, p.id),
                &mut p.keys.iter().map(|(f, _)| i64::from(*f) + offset),
            );
        }
        for child in node.children {
            shape(child, offset, record);
        }
    }
    if let Some(vector) = &row.vector {
        shape(
            path_controls::groups::parameters::tree(vector, row.vector_size, 0., Locale::En),
            offset,
            &mut record,
        );
    }
    out
}

pub(super) fn begin(
    panels: &mut Panels,
    state: &Value,
    row: &timeline::Row,
    frame: u32,
    property: Option<(Property, Option<Axis>)>,
    ppf: f32,
    offset: f32,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if !ppf.is_finite() || ppf <= 0. {
        return;
    }
    let Some((MouseButton::Left, point)) = input.pressed else {
        return;
    };
    let all = channels(state, row);
    let mut targets: Vec<_> = all
        .iter()
        .filter_map(|(target, frames)| {
            (frames.contains(&i64::from(frame))
                && property.is_none_or(|(p, a)| *target == Target::Transform(p, a)))
            .then(|| target.clone())
        })
        .collect();
    // Hash-map order must not affect command order or serialized undo evidence.
    targets.sort_by_key(|t| match t {
        Target::Transform(p, a) => (0, *p as u64, a.map_or(0, |a| a as u64 + 1), String::new()),
        Target::Mask(m, p) => (1, *m, *p as u64, String::new()),
        Target::MaskNode(m, n) => (2, *m, *n, String::new()),
        Target::Shape(i, p) => (3, *i, 0, p.clone()),
    });
    static MISSING: motion_render::observability::FailureCounter =
        motion_render::observability::FailureCounter::new();
    if targets.is_empty() {
        MISSING.failure(
            "timeline.key_drag_source",
            format_args!(
                "object={} frame={frame} phase=begin reason=painted key has no authored channel",
                row.id
            ),
        );
        return;
    }
    MISSING.success();
    out.extend(cancel_interaction(panels));
    panels.select_only(Some(row.id));
    if let [Target::Transform(property, axis)] = targets.as_slice() {
        panels
            .animation_controls
            .select_key_in(state, row.id, *property, *axis, i64::from(frame));
    } else {
        panels.animation_controls.yield_track();
    }
    out.push(Action::Seek(f64::from(frame)));
    if row.locked || state["observing"] == true {
        return;
    }
    let occupied = all
        .into_iter()
        .filter(|(target, _)| targets.contains(target))
        .collect();
    panels.timeline.timing_drag = Some(Drag {
        object: row.id,
        targets,
        occupied,
        expected_revision: state["revision"].as_u64().unwrap_or(0),
        root: state["root"].as_str().unwrap_or("").into(),
        composition: state["composition"].as_str().unwrap_or("").into(),
        layer_offset: row.mask_offset,
        origin: frame,
        current: frame,
        press: point[0],
        ppf,
        offset,
        copy: input.modifiers.control,
        begun: false,
    });
}

fn command(target: &Target, object: u64, from: u32, to: u32, copy: bool) -> Command {
    match target {
        Target::Transform(property, axis) => {
            if copy {
                Command::CopyKey {
                    object,
                    property: *property,
                    axis: *axis,
                    from,
                    to,
                }
            } else {
                Command::MoveKey {
                    object,
                    property: *property,
                    axis: *axis,
                    from,
                    to,
                }
            }
        }
        Target::Mask(mask, index) => {
            let property = match index {
                0 => MaskProperty::Opacity,
                1 => MaskProperty::Feather,
                _ => MaskProperty::Expansion,
            };
            Command::Mask {
                object,
                action: if copy {
                    MaskAction::CopyKey {
                        mask: *mask,
                        property,
                        from,
                        to,
                    }
                } else {
                    MaskAction::MoveKey {
                        mask: *mask,
                        property,
                        from,
                        to,
                    }
                },
            }
        }
        Target::MaskNode(mask, node) => Command::Mask {
            object,
            action: if copy {
                MaskAction::CopyKey {
                    mask: *mask,
                    property: MaskProperty::Node(*node),
                    from,
                    to,
                }
            } else {
                MaskAction::MoveKey {
                    mask: *mask,
                    property: MaskProperty::Node(*node),
                    from,
                    to,
                }
            },
        },
        Target::Shape(item, parameter) => Command::Vector {
            object,
            action: VectorAction::EditGroupKey {
                item: *item,
                parameter: parameter.clone(),
                edit: if copy {
                    KeyEdit::Copy { from, to }
                } else {
                    KeyEdit::Move { from, to }
                },
            },
        },
    }
}

pub(super) fn update(
    panels: &mut Panels,
    state: &Value,
    rows: &[timeline::Row],
    ppf: f32,
    offset: f32,
    input: &Input,
    out: &mut Vec<Action>,
    threshold: f32,
) {
    if panels.timeline.navigation_paint_only {
        return;
    }
    let Some(mut drag) = panels.timeline.timing_drag.take() else {
        return;
    };
    let row = rows.iter().find(|row| row.id == drag.object);
    let invalid = input.focus_lost
        || input.keys.iter().any(|(key, _)| *key == Key::Escape)
        || panels.timeline.graph_mode
        || panels.selected != Some(drag.object)
        || drag.root != state["root"].as_str().unwrap_or("")
        || drag.composition != state["composition"].as_str().unwrap_or("")
        || state["observing"] == true
        || drag.ppf != ppf
        || drag.offset != offset
        || row.is_none_or(|row| row.locked || row.mask_offset != drag.layer_offset);
    if invalid {
        if drag.begun {
            out.push(Action::History(4));
        }
        return;
    }
    let row = match row {
        Some(row) => row,
        None => return,
    };
    let release = input
        .released
        .filter(|(button, _)| *button == MouseButton::Left)
        .map(|(_, point)| point);
    if input.dragging() || release.is_some() {
        let point = release.unwrap_or(input.mouse);
        let delta = point[0] - drag.press;
        let next = (f64::from(drag.origin) + f64::from(delta) / f64::from(ppf))
            .round()
            .clamp(0., last_frame(state)) as u32;
        // Input can arrive before the serial engine acknowledges our last batch.
        // Until then, use the bounded set of authored times maintained below.
        if state["revision"].as_u64().unwrap_or(0) >= drag.expected_revision {
            let snapshot = channels(state, row);
            if drag.targets.iter().any(|target| {
                !snapshot
                    .get(target)
                    .is_some_and(|frames| frames.contains(&i64::from(drag.current)))
            }) {
                if drag.begun {
                    out.push(Action::History(4));
                }
                return;
            }
            drag.occupied = snapshot
                .into_iter()
                .filter(|(target, _)| drag.targets.contains(target))
                .collect();
        }
        let occupied = next != drag.current
            && drag.targets.iter().any(|target| {
                drag.occupied
                    .get(target)
                    .is_some_and(|frames| frames.contains(&i64::from(next)))
            });
        if next != drag.current
            && !occupied
            && (drag.begun
                || motion_ui::widgets::numeric_behavior::started(
                    delta.abs(),
                    input.pointer_travel,
                    threshold,
                ))
        {
            let copy = drag.copy && !drag.begun;
            if !drag.begun {
                out.push(Action::History(2));
                drag.begun = true;
            }
            out.push(Action::EditBatch(
                drag.targets
                    .iter()
                    .map(|t| command(t, drag.object, drag.current, next, copy))
                    .collect(),
            ));
            for frames in drag.occupied.values_mut() {
                if !copy {
                    frames.remove(&i64::from(drag.current));
                }
                frames.insert(i64::from(next));
            }
            drag.expected_revision = drag.expected_revision.saturating_add(1);
            drag.current = next;
            if let [Target::Transform(property, axis)] = drag.targets.as_slice() {
                panels.animation_controls.select_key_in(
                    state,
                    drag.object,
                    *property,
                    *axis,
                    i64::from(next),
                );
            }
        }
    }
    if release.is_some() {
        if drag.begun {
            out.push(Action::History(3));
        }
    } else {
        panels.timeline.timing_drag = Some(drag);
    }
}
