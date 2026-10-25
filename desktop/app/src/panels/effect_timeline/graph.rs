//! The value/speed graph reads real effect tracks and writes EffectAction.
use super::*;
use motion_core::{Curve, CurveShape, CurveSpace};

fn dimensions(parameter: &EffectParam) -> usize {
    match parameter.kind {
        motion_effects::ParamKind::Vec2 => 2,
        motion_effects::ParamKind::Vec3 => 3,
        motion_effects::ParamKind::Color => 4,
        _ => 1,
    }
}
// Differentiate the actual bounded parameter, not its unclamped track.
pub(super) fn speed_value(parameter: &EffectParam, frame: f64, fps: f64, component: usize) -> motion_core::Result<f32> {
    if parameter.kind.discrete() {
        return Ok(0.);
    }
    let value = parameter.track.sample(frame)?[component];
    if value < parameter.min || value > parameter.max {
        return Ok(0.);
    }
    Ok(graph_sampling::velocity(&parameter.track, frame, fps, |v| v[component] as f64) as f32)
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
    let (control1, control2) = match easing.ease {
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
                control1,
                control2,
                start: 0.,
                end: 1.,
            },
        }),
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::panels) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    offset: f64,
    ppf: f32,
    out: &mut Vec<Action>,
) -> bool {
    let Some(target) = selected(panels, state) else {
        return false;
    };
    let Some(parameter) = target.parameter(state) else {
        return true;
    };
    let styles = list.styles();
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let slot = styles.metric("icon.slot", 20.);
    let header = styles.metric("graph.plot.header.height", 32.);
    let footer = styles.metric("graph.plot.footer.height", 48.);
    let plot = Rect::new(
        area.min[0],
        area.min[1] + header,
        (area.width() - pad).max(0.),
        (area.height() - header - footer).max(0.),
    );
    let mut graph = list.child();
    paint::rect(&mut graph, area, styles.color("well"));
    if parameter.curve.is_some() {
        label(
            &mut graph,
            atlas,
            area.shrink(pad),
            if panels.locale == Locale::Zh {
                "该参数是颜色映射曲线，请在效果控件中编辑"
            } else {
                "Edit this colour transfer curve in Effect Controls"
            },
            true,
        );
        graph.clip_to(area);
        list.append(graph);
        return true;
    }
    if !ppf.is_finite() || ppf <= 0. || plot.width() <= 0. || plot.height() <= 0. {
        graph.clip_to(area);
        list.append(graph);
        return true;
    }
    let count = dimensions(&parameter);
    panels.timeline.graph_component = panels.timeline.graph_component.min(count - 1);
    let component = panels.timeline.graph_component;
    let mode_width = styles.metric("graph.mode.width", 100.);
    for (i, speed) in [false, true].into_iter().enumerate() {
        let rect = Rect::new(
            area.min[0] + pad + i as f32 * mode_width,
            area.min[1],
            mode_width,
            row,
        );
        let title = match (panels.locale, speed) {
            (Locale::Zh, false) => "数值图表",
            (Locale::Zh, true) => "速度图表",
            (_, false) => "Value graph",
            (_, true) => "Speed graph",
        };
        let mut states = motion_ui::style::States::default();
        if panels.timeline.speed_graph == speed {
            states = states.with(motion_ui::style::State::Selected);
        }
        if ui::button_with_states(
            &mut graph,
            atlas,
            rect,
            title,
            input,
            !panels.effect_timeline.editing(),
            &styles,
            "button.icon",
            states,
        )
        .clicked
        {
            panels.timeline.speed_graph = speed;
            panels.timeline.graph_y_zoom = 1.;
            panels.timeline.graph_y_offset = 0.;
        }
    }
    for i in 0..count {
        let rect = Rect::new(
            area.min[0] + pad + mode_width * 2. + i as f32 * slot,
            area.min[1],
            slot,
            row,
        );
        let title = if parameter.kind == motion_effects::ParamKind::Color {
            ["R", "G", "B", "A"][i]
        } else {
            ["X", "Y", "Z", "W"][i]
        };
        let mut states = motion_ui::style::States::default();
        if i == component {
            states = states.with(motion_ui::style::State::Selected);
        }
        if count > 1
            && ui::button_with_states(
                &mut graph,
                atlas,
                rect,
                title,
                input,
                !panels.effect_timeline.editing(),
                &styles,
                "button.icon",
                states,
            )
            .clicked
        {
            panels.timeline.graph_component = i;
        }
    }
    let speed = panels.timeline.speed_graph;
    let fps = state["project"]["fps"].as_f64().unwrap_or(30.);
    let layer_offset = target.source(state).unwrap().0["timeline"]["offset_frame"]
        .as_f64()
        .unwrap_or(0.);
    let value = |frame: f64| -> motion_core::Result<f32> {
        if speed {
            speed_value(&parameter, frame - layer_offset, fps, component)
        } else {
            Ok(parameter.sample(frame - layer_offset)?[component])
        }
    };
    let end = (offset + plot.width() as f64 / ppf as f64).min(last_frame(state));
    let points = (0..=256)
        .map(|i| {
            let f = offset + (end - offset).max(0.) * i as f64 / 256.;
            value(f).map(|value| (f, value))
        })
        .collect::<motion_core::Result<Vec<_>>>();
    let Ok(points) = observe!("effect.graph_samples", points,
        "object={} effect={} param={} frame_start={offset} frame_end={end}", target.object, target.effect, target.param) else {
        graph.clip_to(area);
        list.append(graph);
        return true;
    };
    let min = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    let margin = ((max - min) * 0.1).max(1.);
    let auto = [min - margin, max + margin];
    let mut zoom = if panels.timeline.graph_y_zoom > 0. {
        panels.timeline.graph_y_zoom
    } else {
        1.
    };
    if plot.contains(input.mouse)
        && input.modifiers.control
        && input.modifiers.alt
        && input.wheel[1].is_finite()
        && input.wheel[1] != 0.
        && !panels.effect_timeline.editing()
    {
        let previous = zoom;
        zoom = (zoom * (-input.wheel[1] / 240.).exp()).clamp(0.25, 64.);
        let fraction = (plot.max[1] - input.mouse[1]) / plot.height() - 0.5;
        panels.timeline.graph_y_offset +=
            fraction * (auto[1] - auto[0]) * (1. / previous - 1. / zoom);
        panels.timeline.graph_y_zoom = zoom;
    }
    let half = (auto[1] - auto[0]) * 0.5 / zoom;
    let center = (auto[0] + auto[1]) * 0.5 + panels.timeline.graph_y_offset;
    let bounds = panels
        .effect_timeline
        .handle
        .as_ref()
        .map(|d| d.bounds)
        .or_else(|| panels.effect_timeline.drag.as_ref().and_then(|d| d.bounds))
        .unwrap_or([center - half, center + half]);
    let y_of = |v: f32| plot.max[1] - (v - bounds[0]) / (bounds[1] - bounds[0]) * plot.height();
    let x_of = |f: f64| plot.min[0] + (f - offset) as f32 * ppf;
    for i in 0..=4 {
        let v = bounds[0] + (bounds[1] - bounds[0]) * i as f32 / 4.;
        let y = y_of(v);
        paint::line_with_width(
            &mut graph,
            vec![[plot.min[0], y], [plot.max[0], y]],
            styles.color("border"),
            styles.metric("tree.separator.width", 0.5),
        );
        label(
            &mut graph,
            atlas,
            Rect::new(
                plot.min[0] + pad,
                y - row * 0.5,
                styles.metric("value.min_width", 64.),
                row,
            ),
            &format!("{v:.2}"),
            true,
        );
    }
    let mut lines = list.child();
    paint::line_with_width(
        &mut lines,
        points.iter().map(|(f, v)| [x_of(*f), y_of(*v)]).collect(),
        styles.color("curve"),
        styles.metric("graph.curve_width", 2.),
    );
    let selected_key = panels
        .effect_timeline
        .key
        .as_ref()
        .filter(|(t, _)| t == &target)
        .map(|(_, f)| *f);
    let locked = !target.editable(state) || !parameter.implemented;
    let mut hit_key = false;
    key_selection::ui::plot(panels, plot, bounds[0], bounds[1]);
    if key_selection::ui::selecting_all(panels) {
        key_selection::ui::all(panels,parameter.track.keys.iter().filter_map(|key| {
            let time=f64::from(key.frame)+layer_offset;
            (time>=0.&&time<=last_frame(state)).then(||key_selection::Key {
                target:motion_core::KeyframeTarget::Effect {object:target.object,effect:target.effect,param:target.param.clone()},frame:time as u32,
            })
        }));
    }
    for key in &parameter.track.keys {
        let time = key.frame as f64 + layer_offset;
        if time < offset || time > end {
            continue;
        }
        let Ok(sampled) = observe!("effect.graph_key_sample", value(time),
            "object={} effect={} param={} frame={time}", target.object, target.effect, target.param)
            else { continue; };
        let point = [x_of(time), y_of(sampled)];
        let selected = selected_key.is_some_and(|f| f as f64 == time);
        paint::diamond(
            &mut lines,
            point,
            styles.metric("graph.handle_radius", 4.),
            styles.color(if selected { "focus" } else { "keyframe" }),
        );
        let hit = styles.metric("timeline.key.hit_radius", 5.);
        let rect = Rect::new(point[0] - hit, point[1] - hit, hit * 2., hit * 2.);
        if let Some(frame) = composition_frame(time, state) {
            key_selection::ui::mark(&mut lines, panels, key_selection::Key {
                target: motion_core::KeyframeTarget::Effect { object: target.object, effect: target.effect, param: target.param.clone() }, frame,
            }, point, styles.metric("graph.handle_radius", 4.), rect);
        }
        #[cfg(test)]
        panels
            .effect_timeline
            .rects
            .insert((target.effect, target.param.clone(), time as i32), rect);
        if clicked(input, rect) && plot.contains(input.mouse) {
            if let Some(frame) = composition_frame(time, state) {
                begin_key(
                    panels,
                    state,
                    target.clone(),
                    frame,
                    point,
                    input,
                    ppf,
                    offset,
                    if speed || parameter.kind.discrete() {
                        0.
                    } else {
                        (bounds[1] - bounds[0]) / plot.height()
                    },
                    component,
                    out,
                );
                if let Some(drag) = &mut panels.effect_timeline.drag {
                    drag.bounds = Some(bounds);
                }
                hit_key = true;
            }
        }
        if right_clicked(input, rect) && plot.contains(input.mouse) {
            if let Some(frame) = composition_frame(time, state) {
                open_menu(panels, state, target.clone(), frame, input.mouse);
            }
        }
    }
    if !parameter.kind.discrete() {
        for pair in parameter.track.keys.windows(2) {
            let start = pair[0].frame as f64 + layer_offset;
            let end = pair[1].frame as f64 + layer_offset;
            if end <= start
                || start < 0.
                || !selected_key.is_some_and(|f| f as f64 == start || f as f64 == end)
                || pair[0].ease == Ease::Hold
            {
                continue;
            }
            let original = Easing {
                ease: pair[0].ease,
                curve: pair[0].curve,
            };
            if original.curve.is_some_and(|c| {
                !matches!(
                    c,
                    Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Cubic { .. }
                    }
                )
            }) {
                continue;
            }
            let easing = cubic(original);
            let Some(Curve {
                shape:
                    CurveShape::Cubic {
                        control1, control2, ..
                    },
                ..
            }) = easing.curve
            else {
                continue;
            };
            let a = pair[0].value[component];
            let b = pair[1].value[component];
            let duration = end - start;
            for (index, p) in [control1, control2].into_iter().enumerate() {
                let hv = if speed {
                    let slope = if index == 0 {
                        p[1] / p[0].max(0.001)
                    } else {
                        (1. - p[1]) / (1. - p[0]).max(0.001)
                    };
                    slope as f32 * (b - a) * fps as f32 / duration as f32
                } else {
                    a + p[1] as f32 * (b - a)
                };
                let point = [x_of(start + p[0] * duration), y_of(hv)];
                let anchor = [
                    x_of(if index == 0 { start } else { end }),
                    y_of(if speed {
                        hv
                    } else if index == 0 {
                        a
                    } else {
                        b
                    }),
                ];
                paint::line_with_width(
                    &mut lines,
                    vec![anchor, point],
                    styles.color("keyframe"),
                    styles.metric("graph.handle_width", 1.),
                );
                let radius = styles.metric("graph.handle_radius", 4.);
                paint::rect(
                    &mut lines,
                    Rect::new(
                        point[0] - radius,
                        point[1] - radius,
                        radius * 2.,
                        radius * 2.,
                    ),
                    styles.color("keyframe"),
                );
                let hit = styles.metric("graph.handle.hit_radius", 9.);
                #[cfg(test)]
                panels.effect_timeline.rects.insert(
                    (target.effect, target.param.clone(), -100 - index as i32),
                    Rect::new(point[0] - hit, point[1] - hit, hit * 2., hit * 2.),
                );
                if !locked
                    && !hit_key
                    && clicked(
                        input,
                        Rect::new(point[0] - hit, point[1] - hit, hit * 2., hit * 2.),
                    )
                    && plot.contains(input.mouse)
                {
                    panels.effect_timeline.handle = Some(HandleDrag {
                        target: target.clone(),
                        frame: start as u32,
                        index,
                        easing,
                        start,
                        duration,
                        a,
                        b,
                        point: input.mouse,
                        plot,
                        bounds,
                        ppf,
                        offset,
                        begun: false,
                    });
                }
            }
        }
    }
    update_handle(panels, state, input, ppf, offset, speed, fps, out);
    lines.clip_to(plot);
    graph.append(lines);
    let time = state["frame"].as_f64().unwrap_or(0.);
    let x = x_of(time);
    if x >= plot.min[0] && x <= plot.max[0] {
        paint::rect(
            &mut graph,
            Rect::new(x, plot.min[1], styles.metric("border", 1.), plot.height()),
            styles.color("focus"),
        );
    }
    graph_notice::draw(
        &mut graph,
        atlas,
        Rect::new(
            area.min[0] + pad,
            plot.max[1] + pad,
            area.width() - pad * 2.,
            row,
        ),
        panels.locale,
        parameter
            .track
            .keys
            .iter()
            .any(|k| k.frame as f64 + layer_offset < 0.),
    );
    graph.clip_to(area);
    list.append(graph);
    true
}
pub(super) fn update_handle(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    offset: f64,
    speed: bool,
    fps: f64,
    out: &mut Vec<Action>,
) {
    let Some(mut drag) = panels.effect_timeline.handle.take() else {
        return;
    };
    if !drag.target.editable(state)
        || selected(panels, state).as_ref() != Some(&drag.target)
        || drag.ppf != ppf
        || drag.offset != offset
        || input.focus_lost
        || input.keys.iter().any(|(k, _)| *k == Key::Escape)
    {
        if drag.begun {
            out.push(Action::History(4));
        }
        return;
    }
    if panels.timeline.navigation_paint_only {
        panels.effect_timeline.handle = Some(drag);
        return;
    }
    let release = input.released.filter(|(b, _)| *b == MouseButton::Left);
    if input.dragging() || release.is_some() {
        let point = release.map_or(input.mouse, |(_, p)| p);
        if !drag.begun
            && (point[0] - drag.point[0])
                .abs()
                .max((point[1] - drag.point[1]).abs())
                >= list_styles(panels).metric("motion.drag_threshold", 3.)
        {
            drag.begun = true;
            out.push(Action::History(2));
        }
        if drag.begun {
            let x = ((drag.offset + (point[0] - drag.plot.min[0]) as f64 / ppf as f64
                - drag.start)
                / drag.duration)
                .clamp(0.001, 0.999);
            let v = drag.bounds[0]
                + (drag.plot.max[1] - point[1]) / drag.plot.height()
                    * (drag.bounds[1] - drag.bounds[0]);
            let y = if (drag.b - drag.a).abs() < 1e-6 {
                if drag.index == 0 {
                    0.
                } else {
                    1.
                }
            } else if speed {
                let slope = v as f64 * drag.duration / ((drag.b - drag.a) as f64 * fps);
                if drag.index == 0 {
                    slope * x
                } else {
                    1. - slope * (1. - x)
                }
            } else {
                ((v - drag.a) / (drag.b - drag.a)) as f64
            };
            if let Some(Curve {
                shape:
                    CurveShape::Cubic {
                        control1, control2, ..
                    },
                ..
            }) = &mut drag.easing.curve
            {
                if drag.index == 0 {
                    *control1 = [x.min(control2[0]), y.clamp(-12., 12.)];
                } else {
                    *control2 = [x.max(control1[0]), y.clamp(-12., 12.)];
                }
            }
            if observe!(
                "timeline.effect_curve",
                drag.easing.validate(),
                "object={} effect={} parameter={} frame={}",
                drag.target.object,
                drag.target.effect,
                drag.target.param,
                drag.frame
            )
            .is_ok()
            {
                out.push(Action::Edit(drag.target.command(EffectAction::Curve {
                    effect: drag.target.effect,
                    param: drag.target.param.clone(),
                    frame: drag.frame,
                    easing: drag.easing,
                })));
            }
        }
    }
    if release.is_some() {
        if drag.begun {
            out.push(Action::History(3));
        }
    } else {
        panels.effect_timeline.handle = Some(drag);
    }
}
