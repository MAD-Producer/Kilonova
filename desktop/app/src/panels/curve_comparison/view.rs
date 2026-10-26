//! Unit groups have independent ordinate ranges and one composition-time axis.
use super::*;
use std::collections::BTreeMap;

pub(super) fn color(
    styles: &motion_ui::style::StyleSheet,
    index: usize,
) -> motion_ui::theme::Color {
    styles.color(
        [
            "curve",
            "focus",
            "path.draft",
            "error",
            "keyframe",
            "camera",
        ][index % 6],
    )
}
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
) {
    let neutral = (input.composing || input.ime_consumed || input.focus_lost).then(|| Input {
        keys: vec![],
        ..input.clone()
    });
    let input = neutral.as_ref().unwrap_or(input);
    let styles = list.styles();
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("padding", 5.);
    let slot = styles.metric("icon.slot", 20.);
    let mut controls = std::mem::take(&mut panels.timeline.comparison);
    #[cfg(test)]
    {
        controls.marks.clear();
        controls.legend_hits.clear();
    }
    let mut body = list.child();
    paint::rect(&mut body, area, styles.color("well"));
    let content = area;
    let legend_width = styles.metric("graph.comparison.legend.width", 200.);
    let side =
        content.width() >= legend_width + styles.metric("graph.comparison.plot.min_width", 240.);
    let (plots, legend) = if side {
        (
            plot_bounds(content, &styles),
            Rect::new(
                content.max[0] - legend_width,
                content.min[1],
                legend_width,
                content.height(),
            ),
        )
    } else {
        (content, Rect::new(content.max[0], content.min[1], 0., 0.))
    };
    if side {
        let legend_frame = controls.legend.begin_view(
            &mut body,
            legend,
            controls.entries.len() as f32 * row,
            input,
            false,
        );
        let mut legend_paint = list.child();
        for (index, entry) in controls.entries.iter_mut().enumerate() {
            let y = legend_frame.content.min[1] + index as f32 * row;
            let rect = Rect::new(
                legend_frame.content.min[0],
                y,
                legend_frame.viewport.width(),
                row,
            );
            if rect.max[1] <= legend_frame.viewport.min[1]
                || rect.min[1] >= legend_frame.viewport.max[1]
            {
                continue;
            }
            paint::rect(
                &mut legend_paint,
                Rect::new(
                    rect.min[0],
                    y + row * 0.5 - styles.metric("border", 1.),
                    slot - gap,
                    styles.metric("graph.curve_width", 2.),
                ),
                color(&styles, index),
            );
            let title = match &entry.data {
                Ok(data) => format!(
                    "{} · {}",
                    entry.label,
                    data.unit.title(panels.timeline.speed_graph)
                ),
                Err(_) => format!(
                    "{} · {}",
                    entry.label,
                    words(panels.locale, "通道不可用", "Unavailable channel")
                ),
            };
            let check = Rect::new(rect.min[0] + slot, y, (rect.width() - slot).max(0.), row);
            #[cfg(test)]
            controls.legend_hits.push((index, check));
            if Checkbox::draw(
                &mut legend_paint,
                atlas,
                check,
                &title,
                entry.visible,
                &legend_frame.input,
                true,
                controls.bar_focus == Some(index + 5),
                None,
            )
            .clicked
            {
                entry.visible = !entry.visible;
                controls.bar_focus = Some(index + 5);
                out.extend(super::super::cancel_interaction(panels));
                super::super::key_selection::ui::block_background(panels);
            }
        }
        legend_paint.clip_to(legend_frame.viewport);
        body.append(legend_paint);
        if side {
            paint::line(
                &mut body,
                vec![
                    [legend.min[0] - gap, content.min[1]],
                    [legend.min[0] - gap, content.max[1]],
                ],
                styles.color("border"),
            );
        }
    }
    let mut groups: BTreeMap<data::Unit, Vec<usize>> = BTreeMap::new();
    for (i, entry) in controls
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.visible)
    {
        if let Ok(data) = &entry.data {
            groups.entry(data.unit.clone()).or_default().push(i);
        }
    }
    if controls.single_group {
        let chosen = controls
            .unit
            .as_ref()
            .filter(|unit| groups.contains_key(*unit))
            .cloned()
            .or_else(|| groups.keys().next().cloned());
        groups.retain(|unit, _| Some(unit) == chosen.as_ref());
    }
    let min_height = if controls.single_group {
        plots.height()
    } else {
        styles.metric("graph.comparison.group.min_height", 120.)
    };
    let group_height = if groups.is_empty() {
        min_height
    } else {
        (plots.height() / groups.len() as f32).max(min_height)
    };
    let neutral_wheel = Input {
        wheel: [0.; 2],
        ..input.clone()
    };
    let scroll_input = if input.modifiers.control || input.modifiers.alt || input.modifiers.shift {
        &neutral_wheel
    } else {
        input
    };
    let scroll = controls.groups.begin_view(
        &mut body,
        plots,
        group_height * groups.len() as f32,
        scroll_input,
        false,
    );
    super::super::key_selection::ui::plot(panels, scroll.viewport, 0., 1.);
    let fps = state["project"]["fps"].as_f64().unwrap_or(30.);
    let end = (offset + scroll.viewport.width() as f64 / ppf.max(f32::MIN_POSITIVE) as f64)
        .min(last_frame(state));
    let speed = panels.timeline.speed_graph;
    let sample_key = [offset, end, fps, if speed { 1. } else { 0. }];
    if ppf.is_finite() && ppf > 0. && scroll.viewport.width() > 0. && scroll.viewport.height() > 0.
    {
        for (group, (unit, indices)) in groups.into_iter().enumerate() {
            let top = scroll.content.min[1] + group as f32 * group_height;
            let lane = Rect::new(
                scroll.viewport.min[0],
                top,
                scroll.viewport.width(),
                group_height,
            );
            if lane.max[1] <= scroll.viewport.min[1] || lane.min[1] >= scroll.viewport.max[1] {
                continue;
            }
            let heading = if controls.single_group { 0. } else { row };
            let plot = Rect::new(
                lane.min[0],
                top + heading + row * 0.5,
                lane.width(),
                (lane.height() - heading - row - pad).max(0.),
            );
            let mut low = f64::INFINITY;
            let mut high = f64::NEG_INFINITY;
            for index in &indices {
                let entry = &mut controls.entries[*index];
                let Ok(data) = entry.data.as_ref() else {
                    continue;
                };
                if entry
                    .points
                    .as_ref()
                    .is_none_or(|(key, _)| *key != sample_key)
                {
                    let points = (0..=256)
                        .map(|i| {
                            let frame = offset + (end - offset).max(0.) * i as f64 / 256.;
                            data.sample(frame, entry.channel.component, fps, speed)
                                .map(|v| (frame, v))
                        })
                        .collect::<motion_core::Result<Vec<_>>>();
                    match observe!(
                        "curve.comparison.samples",
                        points,
                        "object={} component={} frame_start={offset} frame_end={end}",
                        entry.channel.target.object(),
                        entry.channel.component
                    ) {
                        Ok(points) => entry.points = Some((sample_key, Arc::new(points))),
                        Err(_) => {
                            entry.points = None;
                            continue;
                        }
                    }
                }
                if let Some((_, points)) = &entry.points {
                    for (_, value) in points.iter() {
                        low = low.min(f64::from(*value));
                        high = high.max(f64::from(*value));
                    }
                }
            }
            if !low.is_finite() || !high.is_finite() {
                let mut notice = list.child();
                label(
                    &mut notice,
                    atlas,
                    lane.shrink(pad),
                    words(
                        panels.locale,
                        "无法绘制该组，请检查日志。",
                        "Cannot draw this group; check the log.",
                    ),
                    true,
                );
                notice.clip_to(scroll.viewport);
                body.append(notice);
                continue;
            }
            let margin = ((high - low) * 0.1).max(1.);
            low -= margin;
            high += margin;
            let y_of =
                |v: f32| plot.max[1] - ((f64::from(v) - low) / (high - low)) as f32 * plot.height();
            let mut graph = list.child();
            if !controls.single_group {
                label(
                    &mut graph,
                    atlas,
                    Rect::new(lane.min[0] + pad, top, lane.width() - pad, row),
                    &unit.title(speed),
                    true,
                );
            }
            let steps = (plot.height() / row).floor().clamp(1., 4.) as usize;
            for i in 0..=steps {
                let value = low + (high - low) * i as f64 / steps as f64;
                let y = plot.max[1] - (i as f32 / steps as f32) * plot.height();
                paint::line(
                    &mut graph,
                    vec![[plot.min[0], y], [plot.max[0], y]],
                    styles.color("border"),
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
                    &format!("{value:.1}"),
                    true,
                );
            }
            let mut curves = list.child();
            for index in indices {
                let entry = &controls.entries[index];
                let Ok(data) = &entry.data else {
                    continue;
                };
                let Some((_, points)) = &entry.points else {
                    continue;
                };
                paint::line_with_width(
                    &mut curves,
                    points
                        .iter()
                        .map(|(f, v)| [plot.min[0] + (*f - offset) as f32 * ppf, y_of(*v)])
                        .collect(),
                    color(&styles, index),
                    styles.metric("graph.curve_width", 2.),
                );
                for frame in data
                    .frames
                    .iter()
                    .copied()
                    .filter(|f| f64::from(*f) >= offset && f64::from(*f) <= end)
                {
                    let Ok(value) = observe!(
                        "curve.comparison.key_sample",
                        data.sample(f64::from(frame), entry.channel.component, fps, speed),
                        "object={} frame={frame} component={}",
                        entry.channel.target.object(),
                        entry.channel.component
                    ) else {
                        continue;
                    };
                    let point = [
                        plot.min[0] + (f64::from(frame) - offset) as f32 * ppf,
                        y_of(value),
                    ];
                    if !scroll.viewport.contains(point) {
                        continue;
                    }
                    #[cfg(test)]
                    controls.marks.push((entry.channel.clone(), frame, point));
                    let radius = styles.metric("graph.key_radius", 5.);
                    let hit = styles.metric("graph.handle.hit_radius", 9.);
                    paint::diamond(&mut curves, point, radius, color(&styles, index));
                    super::super::key_selection::ui::graph_mark(
                        &mut curves,
                        panels,
                        super::super::key_selection::Key {
                            target: entry.channel.target.clone(),
                            frame,
                        },
                        point,
                        radius,
                        Rect::new(point[0] - hit, point[1] - hit, hit * 2., hit * 2.),
                        entry.channel.component,
                        if speed {
                            0.
                        } else {
                            ((high - low)
                                / f64::from(plot.height())
                                / f64::from(data.scale(entry.channel.component)))
                                as f32
                        },
                        data.value_limits(),
                    );
                }
            }
            curves.clip_to(plot);
            graph.append(curves);
            graph.clip_to(scroll.viewport);
            body.append(graph);
        }
        let head =
            scroll.viewport.min[0] + (state["frame"].as_f64().unwrap_or(0.) - offset) as f32 * ppf;
        if head >= scroll.viewport.min[0] && head <= scroll.viewport.max[0] {
            paint::rect(
                &mut body,
                Rect::new(
                    head,
                    scroll.viewport.min[1],
                    styles.metric("border", 1.),
                    scroll.viewport.height(),
                ),
                styles.color("focus"),
            );
        }
    }
    if controls.entries.is_empty()
        || controls
            .entries
            .iter()
            .all(|e| !e.visible || e.data.is_err())
    {
        label(
            &mut body,
            atlas,
            plots.shrink(pad),
            words(
                panels.locale,
                "选择属性或关键帧，点击“加入所选”以比较。",
                "Select properties or keys, then choose Add selected.",
            ),
            true,
        );
    }
    body.clip_to(area);
    list.append(body);
    panels.timeline.comparison = controls;
}
