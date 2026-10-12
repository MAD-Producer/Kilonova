//! Property visibility and edits on explicitly selected composition-time keys.
use super::*;
use super::{
    animation_data as data,
    types::{property_bit, KeySelection, PropertyFilter},
};
use crate::shortcuts::Command as Shortcut;
use motion_core::{Axis, Curve, CurveShape, CurveSpace, Ease, Easing};

fn axis(index: usize) -> Axis {
    [Axis::X, Axis::Y, Axis::Z][index]
}
fn visual_layer<'a>(state: &'a Value, object: u64) -> Option<&'a Value> {
    state["project"]["layers"].as_array()?.iter().find(|l| {
        l["id"].as_u64() == Some(object) && l["content"]["kind"].as_str() != Some("audio")
    })
}
fn keys(source: &Value) -> &[Value] {
    source["keys"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn selection(panels: &mut Panels, state: &Value) -> Option<KeySelection> {
    let key = panels.animation_controls.user_key()?;
    if panels.selected != Some(key.object)
        || !data::channel(state, key.object, key.property, key.axis).is_some_and(|s| {
            keys(s)
                .iter()
                .any(|k| k["frame"].as_u64() == Some(key.frame as u64))
        })
    {
        panels.animation_controls.clear_key();
        return None;
    }
    Some(key)
}
fn reveal(
    panels: &mut Panels,
    state: &Value,
    object: u64,
    property: Option<Property>,
    append: bool,
) {
    let old = panels
        .timeline
        .filters
        .get(&object)
        .copied()
        .unwrap_or_default();
    let expanded = if panels.focused == Some(Panel::Properties) {
        !panels.properties.transform_collapsed
    } else {
        panels.timeline.expanded.contains(&object)
    };
    let next = if let Some(property) = property {
        let bit = property_bit(property);
        let mask = if append {
            if expanded {
                (match old {
                    PropertyFilter::All => 15,
                    PropertyFilter::Properties(mask) => mask,
                    PropertyFilter::Animated => timeline::filtered_properties(state, object, old)
                        .iter()
                        .fold(0, |bits, r| bits | property_bit(r.property)),
                }) ^ bit
            } else {
                bit
            }
        } else if expanded && old == PropertyFilter::Properties(bit) {
            0
        } else {
            bit
        };
        panels
            .animation_controls
            .choose_track(object, property, None);
        PropertyFilter::Properties(mask)
    } else if expanded && old == PropertyFilter::Animated {
        PropertyFilter::Properties(0)
    } else {
        PropertyFilter::Animated
    };
    panels.timeline.filters.insert(object, next);
    let hidden = next == PropertyFilter::Properties(0);
    if hidden {
        panels.timeline.expanded.remove(&object);
        panels.animation_controls.clear_key();
    } else {
        panels.timeline.expanded.insert(object);
    }
    panels.properties.transform_collapsed = hidden;
    panels.properties.properties_scroll = 0.;
    if next == PropertyFilter::Animated {
        if let Some(first) = timeline::filtered_properties(state, object, next).first() {
            panels
                .animation_controls
                .choose_track(object, first.property, first.axis.map(axis));
        }
    }
}

fn cubic_ease(source: &Value, incoming: bool, outgoing: bool) -> Easing {
    let easing = Easing {
        ease: serde_json::from_value(source["ease"].clone()).unwrap_or_default(),
        curve: serde_json::from_value(source["curve"].clone()).unwrap_or(None),
    };
    let (mut a, mut b, start, end) = match easing.curve {
        Some(Curve {
            space: CurveSpace::Progress,
            shape:
                CurveShape::Cubic {
                    control1,
                    control2,
                    start,
                    end,
                },
        }) => (control1, control2, start, end),
        Some(Curve {
            space: CurveSpace::Progress,
            shape:
                CurveShape::Quadratic {
                    control,
                    start,
                    end,
                },
        }) => (
            [control[0] * 2. / 3., start / 3. + control[1] * 2. / 3.],
            [
                1. / 3. + control[0] * 2. / 3.,
                end / 3. + control[1] * 2. / 3.,
            ],
            start,
            end,
        ),
        _ => (
            [1. / 3., easing.sample(0.).velocity.clamp(-12., 12.) / 3.],
            [
                2. / 3.,
                1. - easing.sample(1.).velocity.clamp(-12., 12.) / 3.,
            ],
            0.,
            1.,
        ),
    };
    if outgoing {
        a[1] = start;
        if a[0] < 1e-6 {
            a[0] = 1. / 3.;
        }
    }
    if incoming {
        b[1] = end;
        if b[0] > 1. - 1e-6 {
            b[0] = 2. / 3.;
        }
    }
    Easing {
        ease: Ease::Linear,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1: a,
                control2: b,
                start,
                end,
            },
        }),
    }
}

fn quick_ease(panels: &mut Panels, state: &Value, command: Shortcut, out: &mut Vec<Action>) {
    let Some(key) = selection(panels, state) else {
        return;
    };
    if visual_layer(state, key.object).is_none_or(|l| l["locked"].as_bool() == Some(true)) {
        return;
    }
    let source = data::channel(state, key.object, key.property, key.axis).unwrap();
    let entries = keys(source);
    let selected = entries
        .iter()
        .position(|k| k["frame"].as_u64() == Some(key.frame as u64))
        .unwrap();
    let mut edits = vec![];
    for (index, incoming, outgoing) in [
        (
            selected.checked_sub(1),
            command != Shortcut::EaseOutgoing,
            false,
        ),
        (
            (selected + 1 < entries.len()).then_some(selected),
            false,
            command != Shortcut::EaseIncoming,
        ),
    ] {
        let Some(index) = index.filter(|_| incoming || outgoing) else {
            continue;
        };
        let Some(frame) = entries[index]["frame"]
            .as_u64()
            .and_then(|f| u32::try_from(f).ok())
        else {
            panels.message = Some(
                if panels.locale == Locale::Zh {
                    "区间起点在合成之前，无法编辑该段"
                } else {
                    "Interval starts before the composition; cannot edit this segment"
                }
                .into(),
            );
            return;
        };
        let easing = cubic_ease(&entries[index], incoming, outgoing);
        if easing.validate().is_err() {
            return;
        }
        edits.push(Command::Curve {
            object: key.object,
            property: key.property,
            axis: key.axis,
            frame,
            easing,
        });
    }
    if !edits.is_empty() {
        out.push(Action::EditBatch(edits));
    }
}

fn toggle_key(
    panels: &mut Panels,
    state: &Value,
    object: u64,
    property: Property,
    frame: u32,
    out: &mut Vec<Action>,
) {
    if visual_layer(state, object).is_none_or(|l| l["locked"].as_bool() == Some(true)) {
        return;
    }
    let Some(timeline) = data::timeline(state, object) else {
        return;
    };
    let root = &timeline["properties"][data::name(property)];
    let axes: Vec<_> = if root["separated"].as_bool() == Some(true) {
        if panels.animation_controls.property == Some(property)
            && panels.animation_controls.axis.is_some()
        {
            vec![panels.animation_controls.axis]
        } else {
            timeline::filtered_properties(
                state,
                object,
                PropertyFilter::Properties(property_bit(property)),
            )
            .iter()
            .map(|row| row.axis.map(axis))
            .collect()
        }
    } else {
        vec![None]
    };
    let all_present = !axes.is_empty()
        && axes.iter().all(|axis| {
            data::channel(state, object, property, *axis).is_some_and(|s| {
                keys(s)
                    .iter()
                    .any(|k| k["frame"].as_u64() == Some(frame as u64))
            })
        });
    let mut edits = vec![];
    for axis in axes {
        let Some(source) = data::channel(state, object, property, axis) else {
            continue;
        };
        let present = keys(source)
            .iter()
            .any(|k| k["frame"].as_u64() == Some(frame as u64));
        if all_present {
            edits.push(Command::DeleteKey {
                object,
                property,
                axis,
                frame,
            });
        } else if !present {
            let Some(value) = data::sample(state, object, property, axis, frame as f64) else {
                return;
            };
            edits.push(Command::Animate {
                object,
                property,
                axis,
                frame,
                enabled: true,
            });
            edits.push(if property == Property::Opacity {
                Command::SetScalar {
                    object,
                    property,
                    frame,
                    value: value.as_f64().unwrap() as f32,
                }
            } else if let Some(axis) = axis {
                Command::SetComponent {
                    object,
                    property,
                    axis,
                    frame,
                    value: value.as_f64().unwrap() as f32,
                }
            } else {
                Command::SetVector {
                    object,
                    property,
                    frame,
                    value: std::array::from_fn(|i| value[i].as_f64().unwrap() as f32),
                }
            });
        }
    }
    if !edits.is_empty() {
        out.push(Action::EditBatch(edits));
        if all_present {
            panels.animation_controls.clear_key();
        }
    }
}

pub(super) fn handle(
    command: Shortcut,
    modifiers: motion_ui::input::Modifiers,
    panels: &mut Panels,
    state: &Value,
    frame: &mut f64,
    out: &mut Vec<Action>,
) -> bool {
    if !matches!(
        panels.focused,
        Some(Panel::Timeline | Panel::Curves | Panel::Properties | Panel::Composition)
    ) {
        return false;
    }
    if panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
        || panels.timeline.scrubbing
    {
        return true;
    }
    let object = panels
        .selected
        .filter(|id| visual_layer(state, *id).is_some());
    match command {
        Shortcut::Reveal(property) => {
            if let Some(object) = object {
                reveal(panels, state, object, Some(property), modifiers.shift);
            }
        }
        Shortcut::AnimatedProperties => {
            if let Some(object) = object {
                reveal(panels, state, object, None, false);
            }
        }
        Shortcut::ToggleGraph => {
            panels.timeline.graph_mode = !panels.timeline.graph_mode;
        }
        Shortcut::PreviousKey | Shortcut::NextKey => {
            if panels.selected.is_some() && object.is_none() {
                return true;
            }
            let mut times = vec![];
            for row in timeline::rows(state) {
                if object.is_some_and(|id| id != row.id)
                    || !row
                        .name
                        .to_lowercase()
                        .contains(&panels.timeline.timeline_query.to_lowercase())
                {
                    continue;
                }
                if panels.timeline.expanded.contains(&row.id)
                    || panels.timeline.graph_mode
                    || panels.focused == Some(Panel::Curves)
                {
                    for property in timeline::filtered_properties(
                        state,
                        row.id,
                        panels
                            .timeline
                            .filters
                            .get(&row.id)
                            .copied()
                            .unwrap_or_default(),
                    ) {
                        if (panels.timeline.graph_mode || panels.focused == Some(Panel::Curves))
                            && (property.property
                                != panels
                                    .animation_controls
                                    .property
                                    .unwrap_or(Property::Position)
                                || panels
                                    .animation_controls
                                    .axis
                                    .is_some_and(|a| property.axis != Some(a.index())))
                        {
                            continue;
                        }
                        times.extend(property.keys);
                    }
                } else {
                    times.extend(row.keys);
                }
            }
            times.retain(|t| t.is_finite() && *t >= 0. && *t <= last_frame(state));
            times.sort_by(f64::total_cmp);
            times.dedup();
            let next = if command == Shortcut::NextKey {
                times.into_iter().find(|t| *t > *frame + 1e-6)
            } else {
                times.into_iter().rev().find(|t| *t < *frame - 1e-6)
            };
            if let Some(next) = next {
                *frame = next;
                let span = (last_frame(state) + 1.) / panels.timeline.zoom.max(1.) as f64;
                if next < panels.timeline.offset as f64
                    || next > panels.timeline.offset as f64 + span
                {
                    panels.timeline.offset = (next - span * 0.5)
                        .clamp(0., (last_frame(state) + 1. - span).max(0.))
                        as f32;
                }
                out.push(Action::Seek(next));
            }
        }
        Shortcut::EaseBoth | Shortcut::EaseIncoming | Shortcut::EaseOutgoing => {
            quick_ease(panels, state, command, out)
        }
        Shortcut::ToggleKey(property) => {
            if let Some(object) = object {
                toggle_key(
                    panels,
                    state,
                    object,
                    property,
                    frame.floor().max(0.) as u32,
                    out,
                );
            }
        }
        Shortcut::Delete if matches!(panels.focused, Some(Panel::Timeline | Panel::Curves)) => {
            if let Some(key) = selection(panels, state) {
                if visual_layer(state, key.object)
                    .is_some_and(|l| l["locked"].as_bool() != Some(true))
                {
                    out.push(Action::Edit(Command::DeleteKey {
                        object: key.object,
                        property: key.property,
                        axis: key.axis,
                        frame: key.frame,
                    }));
                    panels.animation_controls.clear_key();
                }
            } else if panels.focused != Some(Panel::Curves) && !panels.timeline.graph_mode {
                return false;
            }
        }
        _ => return false,
    }
    true
}
