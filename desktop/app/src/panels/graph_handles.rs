//! The linked graph edits the same serialized easing as the core sampler.
use super::*;
use motion_core::{Axis, Curve, CurveShape, CurveSpace, Ease, Easing};

pub(super) fn handle_command(
    target: motion_core::KeyframeTarget,
    frame: u32,
    incoming: bool,
    easing: Easing,
) -> motion_core::Result<Command> {
    let Some(Curve {
        space: CurveSpace::Progress,
        shape: CurveShape::Cubic {
            control1, control2, ..
        },
    }) = easing.curve else {
        return Err(motion_core::Error::Invalid(
            "graph handle requires a progress-space cubic".into(),
        ));
    };
    Ok(Command::KeyframeHandle {
        target,
        frame,
        incoming,
        control: if incoming { control2 } else { control1 },
    })
}
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
// Preserve the existing easing fallback while exposing malformed present fields.
pub(super) fn observed_easing(
    value: &Value,
    object: u64,
    property: Property,
    frame: f64,
) -> Easing {
    let ease = if value.get("ease").is_none() {
        Ease::Linear // Legacy omitted default is valid.
    } else {
        observe!(
            "panel.easing.ease",
            serde_json::from_value(value["ease"].clone()),
            "object={object} property={property:?} frame={frame}"
        )
        .unwrap_or(Ease::Linear)
    };
    let curve = if value["curve"].is_null() {
        None // Absent/None is normal for linear and preset easing.
    } else {
        observe!(
            "panel.easing.curve",
            serde_json::from_value(value["curve"].clone()),
            "object={object} property={property:?} frame={frame}"
        )
        .ok()
    };
    Easing { ease, curve }
}

/// Use the same neighbour-derived timing as rendering. The JSON snapshot keeps
/// authored metadata; it must not be mistaken for the effective automatic curve.
fn interval_easing(source: &Value, object: u64, property: Property, axis: Option<Axis>, index: usize) -> Option<Easing> {
    let key = &source["keys"][index];
    if key["auto_bezier"] != true && source["keys"][index + 1]["auto_bezier"] != true {
        return Some(observed_easing(key, object, property, key["frame"].as_f64().unwrap_or(0.)));
    }
    let easing = if property.is_scalar() || axis.is_some() {
        super::animation_data::track::<f32>(source, 0)?.effective_easing(index)
    } else if property == Property::Anchor {
        super::animation_data::track::<[f32; 2]>(source, 0)?.effective_easing(index)
    } else {
        super::animation_data::track::<[f32; 3]>(source, 0)?.effective_easing(index)
    };
    observe!("graph_handles.automatic_timing", easing,
        "object={object} property={property:?} axis={axis:?} interval={index}").ok()
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
        if property.is_scalar() || axis.is_some() {
            key["value"].as_f64().unwrap_or(0.) * plot.scale as f64
        } else {
            key["value"][component].as_f64().unwrap_or(0.) * plot.scale as f64
        }
    };
    for (interval, pair) in keys.windows(2).enumerate() {
        let (Some(start), Some(end)) = (pair[0]["frame"].as_f64(), pair[1]["frame"].as_f64())
        else {
            continue;
        };
        if start < 0.
            || end <= start
            || (selected.frame as f64 != start && selected.frame as f64 != end)
            || selected.object != object
            || selected.property != property
            || selected.axis != axis
        {
            continue;
        }
        let Some(easing) = interval_easing(source, object, property, axis, interval) else { continue; };
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
                if observe!(
                    "graph_handles.validation",
                    drag.easing.validate(),
                    "object={} property={:?} frame={} phase=dragging",
                    drag.object,
                    drag.property,
                    drag.frame
                )
                .is_ok()
                    && drag.easing != previous
                {
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
                                if let Some(old) = interval_easing(source, drag.object, drag.property, drag.axis, other)
                                    .filter(|old| dt > 0. && dv.abs() > 1e-9 && linkable(old)) {
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
                                        if let Ok(command) = observe!(
                                            "graph_handles.linked_command",
                                            handle_command(
                                                motion_core::KeyframeTarget::Property {
                                                    object: drag.object,
                                                    property: drag.property,
                                                    axis: drag.axis,
                                                },
                                                oa["frame"].as_u64().unwrap_or(0) as u32,
                                                drag.index == 0,
                                                linked,
                                            ),
                                            "object={} property={:?} phase=linked_handle",
                                            drag.object,
                                            drag.property
                                        ) {
                                            out.push(Action::Edit(command));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(command) = observe!(
                        "graph_handles.command",
                        handle_command(
                            motion_core::KeyframeTarget::Property {
                                object: drag.object,
                                property: drag.property,
                                axis: drag.axis,
                            },
                            drag.frame,
                            drag.index != 0,
                            drag.easing,
                        ),
                        "object={} property={:?} frame={} phase=handle",
                        drag.object,
                        drag.property,
                        drag.frame
                    ) {
                        out.push(Action::Edit(command));
                    }
                    panels.timeline.graph_drag = Some(drag.clone());
                }

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
    fn handle_commands_send_only_the_selected_control_for_properties_and_effects() {
        let easing = Easing {
            ease: Ease::Linear,
            curve: Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Cubic {
                    control1: [0.21, 0.13],
                    control2: [0.79, 0.87],
                    start: 0.,
                    end: 1.,
                },
            }),
        };
        for target in [
            motion_core::KeyframeTarget::Property {
                object: 7,
                property: Property::Position,
                axis: Some(Axis::X),
            },
            motion_core::KeyframeTarget::Effect {
                object: 7,
                effect: 19,
                param: "center".into(),
            },
        ] {
            for incoming in [false, true] {
                let Command::KeyframeHandle {
                    target: actual,
                    frame,
                    incoming: side,
                    control,
                } = handle_command(target.clone(), 17, incoming, easing).unwrap()
                else {
                    panic!()
                };
                assert_eq!(actual, target);
                assert_eq!(frame, 17);
                assert_eq!(side, incoming);
                assert_eq!(control, if incoming { [0.79, 0.87] } else { [0.21, 0.13] });
            }
        }
    }

    #[test]
    fn handle_command_rejects_a_missing_or_velocity_space_curve() {
        let target = motion_core::KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        };
        assert!(handle_command(target.clone(), 0, false, Easing::default()).is_err());
        let easing = Easing {
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
        };
        assert!(handle_command(target, 0, true, easing).is_err());
    }

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

#[cfg(test)]
mod observation_tests {
    use super::*;
    #[test]
    fn malformed_easing_logs_context_without_changing_fallbacks() {
        const CHILD: &str = "MOTION_PANEL_EASING_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let healthy = serde_json::to_value(Easing::default()).unwrap();
            for _ in 0..120 {
                assert_eq!(
                    observed_easing(&healthy, 7, Property::Opacity, 15.),
                    Easing::default()
                );
            }
            assert!(std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .is_empty());
            let malformed = serde_json::json!({"ease":17,"curve":42});
            for _episode in 1..=2 {
                for _ in 0..120 {
                    assert_eq!(
                        observed_easing(&malformed, 7, Property::Opacity, 15.),
                        Easing::default()
                    );
                    assert_eq!(
                        observed_easing(&healthy, 7, Property::Opacity, 15.),
                        Easing::default()
                    );
                }
                let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
                assert_eq!(
                    text.lines()
                        .filter(|line| line.contains("panel degradation site=panel.easing.")
                            && line.contains("count=1 "))
                        .count(),
                    2
                );
                assert_eq!(
                    text.lines()
                        .filter(
                            |line| line.contains("persistent panel degradation site=panel.easing.")
                        )
                        .count(),
                    2
                );
                assert!(text.contains("object=7 property=Opacity frame=15"));
                assert_eq!(
                    observed_easing(&healthy, 7, Property::Opacity, 15.),
                    Easing::default()
                );
            }
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::graph_handles::observation_tests::malformed_easing_logs_context_without_changing_fallbacks"])
            .env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("persistent panel degradation site=panel.easing.ease"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
}
