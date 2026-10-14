use super::*;
use motion_core::Axis;
#[derive(Clone)]
pub(crate) struct Drag {
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
    original: u32,
    current: u32,
    seed: f32,
    last_value: f32,
    mouse: [f32; 2],
    bounds: [f32; 2],
    vector: [f32; 3],
    copy: bool,
    copied: bool,
    begun: bool,
}
pub(super) fn begin(
    panels: &mut Panels,
    _state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
    source: &Value,
    frame: u32,
    point: [f32; 2],
    value: f32,
    bounds: [f32; 2],
    input: &Input,
    _out: &mut Vec<Action>,
) {
    let key = source["keys"].as_array().and_then(|keys| {
        keys.iter()
            .find(|k| k["frame"].as_u64() == Some(frame as u64))
    });
    let vector =
        std::array::from_fn(|i| key.map_or(0., |k| k["value"][i].as_f64().unwrap_or(0.) as f32));
    panels.timeline.key_drag = Some(Drag {
        object,
        property,
        axis,
        component,
        original: frame,
        current: frame,
        seed: value,
        last_value: if property == Property::Opacity {
            value / 100.
        } else {
            value
        },
        mouse: input.pressed.map_or(point, |(_, p)| p),
        bounds,
        vector,
        copy: input.modifiers.control,
        copied: false,
        begun: false,
    });
}
pub(super) fn update(
    panels: &mut Panels,
    state: &Value,
    plot: Rect,
    ppf: f32,
    scale: f32,
    source: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(mut drag) = panels.timeline.key_drag.clone() else {
        return;
    };
    if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
        panels.timeline.key_drag = None;
        if drag.begun {
            out.push(Action::History(4));
        }
        return;
    }
    if !drag.begun {
        if !input.dragging() && input.released.is_none() {
            return;
        }
        let delta = (input.mouse[0] - drag.mouse[0])
            .abs()
            .max((input.mouse[1] - drag.mouse[1]).abs());
        let sheet = panels
            .styles
            .clone()
            .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
        if !motion_ui::widgets::numeric_behavior::started(
            delta,
            input.pointer_travel,
            sheet.metric("motion.drag_threshold", 3.),
        ) {
            if input.released.is_some() {
                panels.timeline.key_drag = None;
            }
            return;
        }
        drag.begun = true;
        out.push(Action::History(2));
    }
    if input.dragging() || input.released.is_some() {
        let delta = input.mouse[0] - drag.mouse[0];
        let next = (drag.original as f64 + delta as f64 / ppf.max(0.001) as f64)
            .round()
            .clamp(0., last_frame(state)) as u32;
        let occupied = source["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|k| k["frame"].as_u64() == Some(next as u64) && next != drag.current);
        if next != drag.current && !occupied {
            let command = if drag.copy && !drag.copied {
                Command::CopyKey {
                    object: drag.object,
                    property: drag.property,
                    axis: drag.axis,
                    from: drag.original,
                    to: next,
                }
            } else {
                Command::MoveKey {
                    object: drag.object,
                    property: drag.property,
                    axis: drag.axis,
                    from: drag.current,
                    to: next,
                }
            };
            out.push(Action::Edit(command));
            drag.current = next;
            drag.copied = true;
        }
        if !panels.timeline.speed_graph && !input.modifiers.alt && (!drag.copy || drag.copied) {
            let dy = (drag.mouse[1] - input.mouse[1]) / plot.height().max(1.)
                * (drag.bounds[1] - drag.bounds[0]);
            let mut value = (drag.seed + dy) / scale.max(0.0001);
            if drag.property == Property::Opacity {
                value = value.clamp(0., 1.);
            }
            if value.is_finite() && (value - drag.last_value).abs() > 1e-6 {
                let command = if let Some(axis) = drag.axis {
                    Command::SetComponent {
                        object: drag.object,
                        property: drag.property,
                        axis,
                        frame: drag.current,
                        value,
                    }
                } else if drag.property == Property::Opacity {
                    Command::SetScalar {
                        object: drag.object,
                        property: drag.property,
                        frame: drag.current,
                        value,
                    }
                } else {
                    let mut vector = drag.vector;
                    vector[drag.component] = value;
                    Command::SetVector {
                        object: drag.object,
                        property: drag.property,
                        frame: drag.current,
                        value: vector,
                    }
                };
                out.push(Action::Edit(command));
                drag.last_value = value;
            }
        }
        panels.animation_controls.select_key(
            drag.object,
            drag.property,
            drag.axis,
            drag.current as i64,
        );
        panels.timeline.key_drag = Some(drag);
    }
    if input.released.is_some() {
        panels.timeline.key_drag = None;
        out.push(Action::History(3));
    }
}

impl Drag {
    pub(crate) fn begun(&self) -> bool {
        self.begun
    }
    pub(super) fn bounds(&self) -> [f32; 2] {
        self.bounds
    }
}
