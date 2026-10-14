//! The linked graph edits the same serialized easing as the core sampler.
use super::*;
use motion_core::{Axis, Curve, CurveShape, CurveSpace, Ease, Easing};
#[derive(Clone)]
pub(crate) struct Drag {
    object: u64,
    property: Property,
    axis: Option<Axis>,
    frame: u32,
    index: usize,
    easing: Easing,
    start: f64,
    duration: f64,
    a: f64,
    b: f64,
    bounds: [f32; 2],
    mouse: [f32; 2],
    hit: [f32; 2],
    begun: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Plot {
    pub area: Rect,
    pub offset: f64,
    pub ppf: f32,
    pub low: f32,
    pub high: f32,
    pub speed: bool,
    pub fps: f64,
    pub scale: f32,
}
impl Plot {
    fn point(self, time: f64, value: f64) -> [f32; 2] {
        [
            self.area.min[0] + (time - self.offset) as f32 * self.ppf,
            self.area.max[1]
                - (value as f32 - self.low) / (self.high - self.low) * self.area.height(),
        ]
    }
}
fn cubic(easing: Easing) -> Easing {
    if matches!(
        easing.curve,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic { .. }
        })
    ) {
        return easing;
    }
    let (p, q) = match easing.ease {
        Ease::In => ([1. / 3., 0.], [2. / 3., 1. / 3.]),
        Ease::Out => ([1. / 3., 2. / 3.], [2. / 3., 1.]),
        Ease::InOut => ([1. / 3., 0.], [2. / 3., 1.]),
        _ => ([1. / 3., 1. / 3.], [2. / 3., 2. / 3.]),
    };
    Easing {
        ease: easing.ease,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1: p,
                control2: q,
                start: 0.,
                end: 1.,
            },
        }),
    }
}

fn linkable(easing: &Easing) -> bool {
    if easing.ease == Ease::Hold {
        return false;
    }
    match easing.curve {
        None => true,
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic { .. },
        }) => true,
        Some(_) => false,
    }
}
pub(super) fn draw(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    panels: &mut Panels,
    source: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
    plot: Plot,
    input: &Input,
    locked: bool,
    out: &mut Vec<Action>,
) {
    let sheet = list.styles();
    let Some(selected) = panels.animation_controls.user_key() else {
        return;
    };
    let keys = source["keys"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let key_value = |key: &Value| -> f64 {
        if property == Property::Opacity || axis.is_some() {
            key["value"].as_f64().unwrap_or(0.) * plot.scale as f64
        } else {
            key["value"][component].as_f64().unwrap_or(0.) * plot.scale as f64
        }
    };
    for pair in keys.windows(2) {
        let (Some(start), Some(end)) = (pair[0]["frame"].as_f64(), pair[1]["frame"].as_f64())
        else {
            continue;
        };
        if end <= start
            || (selected.frame as f64 != start && selected.frame as f64 != end)
            || selected.object != object
            || selected.property != property
            || selected.axis != axis
        {
            continue;
        }
        let easing = Easing {
            ease: serde_json::from_value(pair[0]["ease"].clone()).unwrap_or(Ease::Linear),
            curve: serde_json::from_value(pair[0]["curve"].clone()).ok(),
        };
        if easing.ease == Ease::Hold
            || easing.curve.is_some_and(|c| {
                !matches!(
                    c,
                    (Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Cubic { .. }
                    })
                )
            })
        {
            continue;
        }
        let current = cubic(easing);
        let Some(Curve {
            shape: CurveShape::Cubic {
                control1, control2, ..
            },
            ..
        }) = current.curve
        else {
            continue;
        };
        let (a, b) = (key_value(&pair[0]), key_value(&pair[1]));
        let duration = end - start;
        for (index, p) in [control1, control2].into_iter().enumerate() {
            let owner = if index == 0 { start } else { end };
            let hvalue = if plot.speed {
                let slope = if index == 0 {
                    p[1] / p[0].max(0.001)
                } else {
                    (1. - p[1]) / (1. - p[0]).max(0.001)
                };
                slope * (b - a) * plot.fps / duration
            } else {
                a + p[1] * (b - a)
            };
            let point = plot.point(start + p[0] * duration, hvalue);
            let anchor = plot.point(
                owner,
                if plot.speed {
                    hvalue
                } else {
                    if index == 0 {
                        a
                    } else {
                        b
                    }
                },
            );
            paint::line_with_width(
                list,
                vec![anchor, point],
                sheet.color("keyframe"),
                sheet.metric("graph.handle_width", 1.),
            );
            let radius = sheet.metric("graph.handle_radius", 4.);
            paint::box_rect(
                list,
                Rect::new(
                    point[0] - radius,
                    point[1] - radius,
                    radius * 2.,
                    radius * 2.,
                ),
                sheet.color("keyframe"),
                motion_ui::Color([0.; 4]),
                0.,
                radius,
            );
            if !locked && clicked(input, Rect::new(point[0] - 9., point[1] - 9., 18., 18.)) {
                panels.timeline.graph_drag = Some(Drag {
                    object,
                    property,
                    axis,
                    frame: start as u32,
                    index,
                    easing: current,
                    start,
                    duration,
                    a,
                    b,
                    bounds: [plot.low, plot.high],
                    mouse: input.mouse,
                    hit: [input.mouse[0] - point[0], input.mouse[1] - point[1]],
                    begun: false,
                });
            }
        }
    }
    if let Some(mut drag) = panels.timeline.graph_drag.clone() {
        if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            panels.timeline.graph_drag = None;
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
            if !motion_ui::widgets::numeric_behavior::started(
                delta,
                input.pointer_travel,
                sheet.metric("motion.drag_threshold", 3.),
            ) {
                if input.released.is_some() {
                    panels.timeline.graph_drag = None;
                }
                return;
            }
            drag.begun = true;
            out.push(Action::History(2));
            panels.timeline.graph_drag = Some(drag.clone());
        }
        if input.dragging() || input.released.is_some() {
            let time = plot.offset
                + (input.mouse[0] - drag.hit[0] - plot.area.min[0]) as f64
                    / plot.ppf.max(0.001) as f64;
            let mut p = [((time - drag.start) / drag.duration).clamp(0., 1.), 0.];
            let value = drag.bounds[0] as f64
                + (plot.area.max[1] - input.mouse[1] + drag.hit[1]) as f64
                    / plot.area.height().max(1.) as f64
                    * (drag.bounds[1] - drag.bounds[0]) as f64;
            let delta = drag.b - drag.a;
            if delta.abs() > 1e-9 {
                let previous = drag.easing;
                p[1] = if plot.speed {
                    let slope = value * drag.duration / (plot.fps * delta);
                    if drag.index == 0 {
                        slope * p[0]
                    } else {
                        1. - slope * (1. - p[0])
                    }
                } else {
                    (value - drag.a) / delta
                };
                if let Some(Curve {
                    shape:
                        CurveShape::Cubic {
                            control1, control2, ..
                        },
                    ..
                }) = &mut drag.easing.curve
                {
                    let original = if drag.index == 0 {
                        *control1
                    } else {
                        *control2
                    };
                    if input.modifiers.control {
                        p[0] = original[0];
                    }
                    if input.modifiers.shift {
                        let anchor = if drag.index == 0 { [0., 0.] } else { [1., 1.] };
                        let dx = p[0] - anchor[0];
                        let dy = p[1] - anchor[1];
                        let angle = (dy.atan2(dx) / (std::f64::consts::PI / 12.)).round()
                            * std::f64::consts::PI
                            / 12.;
                        let length = dx.hypot(dy);
                        p = [
                            (anchor[0] + length * angle.cos()).clamp(0., 1.),
                            anchor[1] + length * angle.sin(),
                        ];
                    }
                    p[1] = p[1].clamp(-8., 8.);
                    if drag.index == 0 {
                        *control1 = p;
                    } else {
                        *control2 = p;
                    }
                }
                if drag.easing.validate().is_ok() && drag.easing != previous {
                    if !input.modifiers.alt {
                        let at = keys
                            .iter()
                            .position(|k| k["frame"].as_u64() == Some(drag.frame as u64));
                        if let Some(at) = at {
                            let other = if drag.index == 0 {
                                at.checked_sub(1)
                            } else {
                                (at + 2 < keys.len()).then_some(at + 1)
                            };
                            if let Some(other) = other {
                                let oa = &keys[other];
                                let ob = &keys[other + 1];
                                let dt = ob["frame"].as_f64().unwrap_or(0.)
                                    - oa["frame"].as_f64().unwrap_or(0.);
                                let dv = key_value(ob) - key_value(oa);
                                let old = Easing {
                                    ease: serde_json::from_value(oa["ease"].clone())
                                        .unwrap_or(Ease::Linear),
                                    curve: serde_json::from_value(oa["curve"].clone()).ok(),
                                };
                                if dt > 0.
                                    && dv.abs() > 1e-9
                                    && linkable(&old)
                                {
                                    let mut linked = cubic(old);
                                    let slope = if drag.index == 0 {
                                        p[1] / p[0].max(0.001)
                                    } else {
                                        (1. - p[1]) / (1. - p[0]).max(0.001)
                                    } * (drag.b - drag.a)
                                        / drag.duration;
                                    if let Some(Curve {
                                        shape:
                                            CurveShape::Cubic {
                                                control1, control2, ..
                                            },
                                        ..
                                    }) = &mut linked.curve
                                    {
                                        if drag.index == 0 {
                                            control2[1] = (1.
                                                - slope * dt / dv * (1. - control2[0]))
                                                .clamp(-8., 8.);
                                        } else {
                                            control1[1] =
                                                (slope * dt / dv * control1[0]).clamp(-8., 8.);
                                        }
                                    }
                                    if linked.validate().is_ok() {
                                        out.push(Action::Edit(Command::Curve {
                                            object: drag.object,
                                            property: drag.property,
                                            axis: drag.axis,
                                            frame: oa["frame"].as_u64().unwrap_or(0) as u32,
                                            easing: linked,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    out.push(Action::Edit(Command::Curve {
                        object: drag.object,
                        property: drag.property,
                        axis: drag.axis,
                        frame: drag.frame,
                        easing: drag.easing,
                    }));
                    panels.timeline.graph_drag = Some(drag.clone());
                }
                let hud = Rect::new(
                    (input.mouse[0] + 16.).min(plot.area.max[0] - 168.),
                    (input.mouse[1] - 56.).max(plot.area.min[1]),
                    168.,
                    50.,
                );
                motion_ui::style::box_style(
                    list,
                    hud,
                    sheet.resolve("popup", Default::default()).unwrap(),
                );
                label(
                    list,
                    atlas,
                    Rect::new(hud.min[0] + 8., hud.min[1] + 4., 152., 20.),
                    if panels.locale == Locale::Zh {
                        "贝塞尔手柄"
                    } else {
                        "Bezier handle"
                    },
                    true,
                );
                label(
                    list,
                    atlas,
                    Rect::new(hud.min[0] + 8., hud.min[1] + 24., 152., 20.),
                    &format!(
                        "{:.1}% · {:.2}",
                        if drag.index == 0 {
                            p[0] * 100.
                        } else {
                            (1. - p[0]) * 100.
                        },
                        p[1]
                    ),
                    false,
                );
            }
        }
        if input.released.is_some() {
            panels.timeline.graph_drag = None;
            out.push(Action::History(3));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_handles_do_not_rewrite_hold_or_non_cubic_segments() {
        assert!(!linkable(&Easing {
            ease: Ease::Hold,
            curve: None,
        }));
        assert!(!linkable(&Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Quadratic {
                    control: [0.25, 0.75],
                    start: 0.,
                    end: 1.,
                },
            }),
        }));
        assert!(!linkable(&Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Velocity,
                shape: CurveShape::Cubic {
                    control1: [0.25, 0.],
                    control2: [0.75, 1.],
                    start: 0.,
                    end: 1.,
                },
            }),
        }));
        assert!(linkable(&Easing::default()));
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
