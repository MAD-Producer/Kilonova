//! Unit groups have independent ordinate ranges and one composition-time axis.
use super::super::{graph_adaptive::Failure, graph_ranges, graph_view::Range};
use super::*;
use std::collections::BTreeMap;

fn identity(unit: &data::Unit, speed: bool) -> graph_ranges::Identity {
    graph_ranges::Identity {
        surface: graph_ranges::Surface::Comparison,
        target: None,
        component: None,
        speed,
        unit: format!("comparison:{unit:?}"),
    }
}
fn intervals(
    controls: &Controls,
    indices: &[usize],
    panels: &Panels,
    state: &Value,
) -> ([f64; 2], Option<[f64; 2]>) {
    let mut all: Option<[f64; 2]> = None;
    let mut selected: Option<[f64; 2]> = None;
    let status = panels.selected_keys.status(state);
    let property = panels
        .animation_controls
        .user_key_in(state, panels.selected);
    let effect = panels.effect_timeline.range_key();
    for index in indices {
        let entry = &controls.entries[*index];
        let Ok(data) = &entry.data else {
            continue;
        };
        for frame in data.frames.iter().copied() {
            let time = f64::from(frame);
            all = Some(all.map_or([time, time], |[low, high]| [low.min(time), high.max(time)]));
            let key = key_selection::Key {
                target: entry.channel.target.clone(),
                frame,
            };
            let chosen = match status {
                key_selection::Status::Current => panels.selected_keys.contains(&key),
                key_selection::Status::Unowned => {
                    effect.as_ref() == Some(&key)
                        || property.is_some_and(|p| {
                            p.frame == frame
                                && entry.channel.target
                                    == (KeyframeTarget::Property {
                                        object: p.object,
                                        property: p.property,
                                        axis: p.axis,
                                    })
                        })
                }
                _ => false,
            };
            if chosen {
                selected = Some(
                    selected.map_or([time, time], |[low, high]| [low.min(time), high.max(time)]),
                );
            }
        }
    }
    (all.unwrap_or([0., last_frame(state) + 1.]), selected)
}
fn fitted(
    controls: &Controls,
    indices: &[usize],
    interval: [f64; 2],
    fps: f64,
    speed: bool,
    size: [f32; 2],
    tolerance: f64,
) -> Result<graph_ranges::Bounds, Failure<motion_core::Error>> {
    let x = Range::fitted(interval[0], interval[1], 0.05, 1.).ok_or(Failure::InvalidGeometry)?;
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for index in indices {
        let entry = &controls.entries[*index];
        let data = entry.data.as_ref().map_err(|cause|Failure::Sample {frame:interval[0],
            cause:motion_core::Error::Invalid(format!("comparison fit channel unavailable: {cause}"))})?;
        let bounds = data.fitted(
            interval,
            entry.channel.component,
            fps,
            speed,
            size,
            tolerance,
        )?;
        low = low.min(bounds.y.low);
        high = high.max(bounds.y.high);
    }
    Ok(graph_ranges::Bounds {
        x,
        y: Range::new(low, high).ok_or(Failure::InvalidGeometry)?,
    })
}

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
    let generation = state["document_generation"].as_u64();
    let revision = state["revision"].as_u64();
    let row = styles.metric("row.height", 22.);
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("padding", 5.);
    let slot = styles.metric("icon.slot", 20.);
    let mut controls = std::mem::take(&mut panels.timeline.comparison);
    #[cfg(test)]
    {
        controls.marks.clear();
        controls.legend_hits.clear();
        controls.range_hits.clear();
        controls.range_debug.clear();
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
                Ok(_)
                    if entry
                        .tessellation
                        .as_ref()
                        .is_some_and(|cache| cache.points.is_none()) =>
                {
                    format!(
                        "{} · {}",
                        entry.label,
                        words(
                            panels.locale,
                            "绘制不可用，请检查日志",
                            "Cannot draw; check log"
                        )
                    )
                }
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
    panels.timeline.graph_ranges.comparison_groups(
        groups
            .keys()
            .map(|unit| identity(unit, panels.timeline.speed_graph)),
    );
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
            let heading = row;
            let plot = Rect::new(
                lane.min[0],
                top + heading + row * 0.5,
                lane.width(),
                (lane.height() - heading - row - pad).max(0.),
            );
            if plot.height() <= 0. {
                continue;
            }
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
            let id = identity(&unit, speed);
            let automatic = Range::new(offset, offset + f64::from(plot.width()) / f64::from(ppf))
                .zip(Range::new(low, high))
                .map(|(x, y)| graph_ranges::Bounds { x, y });
            let Ok(automatic) = observe!(
                "curve.comparison.view_bounds",
                automatic.ok_or("unrepresentable comparison range"),
                "phase=display unit={unit:?} generation={generation:?} revision={revision:?} curves={}",
                indices.len()
            ) else {
                continue;
            };
            let before = panels
                .timeline
                .graph_ranges
                .displayed(state, &id, automatic);
            if plot.contains(input.mouse)
                && scroll.viewport.contains(input.mouse)
                && input.modifiers.control
                && input.modifiers.alt
                && input.wheel[1].is_finite()
                && input.wheel[1] != 0.
                && !panels.key_gestures.active()
                && !panels.timeline.scrubbing
                && panels.timeline.horizontal_drag.is_none()
            {
                let factor = (-f64::from(input.wheel[1]) / 240.).exp().clamp(0.25, 64.);
                let anchor = before
                    .y
                    .at(f64::from((plot.max[1] - input.mouse[1]) / plot.height()));
                panels
                    .timeline
                    .graph_ranges
                    .navigate_y(&id, before.y, factor, anchor);
            }
            let displayed = panels
                .timeline
                .graph_ranges
                .displayed(state, &id, automatic);
            low = displayed.y.low;
            high = displayed.y.high;
            let y_of =
                |v: f32| plot.max[1] - ((f64::from(v) - low) / (high - low)) as f32 * plot.height();
            let mut graph = list.child();
            {
                let caption = Rect::new(
                    lane.min[0] + pad,
                    top,
                    (lane.width() - pad * 2. - styles.metric("graph.control.height", 24.) - gap)
                        .max(0.),
                    row,
                );
                let mut text = list.child();
                label(&mut text, atlas, caption, &unit.title(speed), true);
                text.clip_to(caption);
                graph.append(text);
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
            let tolerance = f64::from(styles.metric("graph.sample.error", 0.35));
            let tessellation_key = [
                offset,
                end,
                fps,
                if speed { 1. } else { 0. },
                f64::from(ppf),
                f64::from(plot.width()),
                f64::from(plot.height()),
                low,
                high,
                tolerance,
            ];
            let mut hovered = None;
            for index in indices.iter().copied() {
                let entry = &mut controls.entries[index];
                let Ok(data) = &entry.data else {
                    continue;
                };
                if entry.points.is_none() {
                    continue;
                }
                if entry
                    .tessellation
                    .as_ref()
                    .is_none_or(|cache| cache.key != tessellation_key)
                {
                    #[cfg(test)]
                    {entry.tessellation_attempts=entry.tessellation_attempts.saturating_add(1);}
                    let points=observe!("curve.comparison.adaptive",data.adaptive([offset,end],entry.channel.component,fps,speed,
                        [f64::from(ppf),f64::from(plot.height())/(high-low)],tolerance),
                        "object={} component={} unit={unit:?} frame_start={offset} frame_end={end} generation={generation:?} revision={revision:?} phase=screen_subdivision",entry.channel.target.object(),entry.channel.component)
                        .ok().map(Arc::new);
                    entry.tessellation = Some(Tessellation {
                        key: tessellation_key,
                        points,
                    });
                }
                let Some(points) = entry
                    .tessellation
                    .as_ref()
                    .and_then(|cache| cache.points.as_ref())
                else {
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
                if plot.contains(input.mouse) && scroll.viewport.contains(input.mouse) {
                    let frame = offset + f64::from(input.mouse[0] - plot.min[0]) / f64::from(ppf);
                    if let Ok(value) = observe!(
                        "curve.comparison.readout",
                        data.sample(frame, entry.channel.component, fps, speed),
                        "object={} component={} frame={frame} generation={generation:?} revision={revision:?} phase=readout",
                        entry.channel.target.object(),
                        entry.channel.component
                    ) {
                        let distance = (y_of(value) - input.mouse[1]).abs();
                        if hovered
                            .as_ref()
                            .is_none_or(|(before, _, _, _)| distance < *before)
                        {
                            hovered =
                                Some((distance, frame, f64::from(value), entry.label.clone()));
                        }
                    }
                }
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
            if let Some((_, frame, value, title)) = hovered {
                graph_ranges::readout(
                    &mut graph,
                    atlas,
                    plot,
                    input.mouse,
                    &title,
                    frame,
                    value,
                    &unit.title(speed),
                );
            }
            let busy = panels.key_gestures.active()
                || panels.timeline.horizontal_drag.is_some()
                || panels.timeline.scrubbing;
            let menu_row = Rect::new(lane.min[0], top, lane.width(), row);
            #[cfg(test)]
            controls.range_hits.push((unit.title(false),menu_row));
            // The caption is flexible; the dedicated range slot never shrinks
            // the existing key/handle targets or overlaps a scroll scrollbar.
            let neutral_menu = Input::default();
            let menu_input = if scroll.viewport.contains(input.mouse) {
                input
            } else {
                &neutral_menu
            };
            #[cfg(test)]
            panels.timeline.graph_ranges.record_probe(id.clone(),menu_row,plot,displayed);
            #[cfg(test)]
            controls.range_debug.push(format!("unit={} header={menu_row:?} viewport={:?} press={:?} mouse={:?} ready={} busy={} scrub={} keygesture={} hdrag={}",
                unit.title(false),scroll.viewport,menu_input.pressed,menu_input.mouse,panels.timeline.graph_ranges.ready(),busy,
                panels.timeline.scrubbing,panels.key_gestures.active(),panels.timeline.horizontal_drag.is_some()));
            if let Some(point) = graph_ranges::launch(
                &mut graph,
                atlas,
                menu_row,
                menu_row.min[0],
                menu_input,
                panels.timeline.graph_ranges.ready() && !busy,
            ) {
                // Group headers live inside the comparison selection viewport.
                // The range launcher owns this press, including its opening
                // frame; it must not begin a background marquee afterward.
                super::super::key_selection::ui::block_background(panels);
                let (all_interval, selection_interval) =
                    intervals(&controls, &indices, panels, state);
                let began = std::time::Instant::now();
                let all=observe!("curve.comparison.fit_all",fitted(&controls,&indices,all_interval,fps,speed,[plot.width(),plot.height()],tolerance),
                        "unit={unit:?} curves={} frame_start={} frame_end={} generation={generation:?} revision={revision:?} elapsed_ms={} phase=fit_all",indices.len(),all_interval[0],all_interval[1],began.elapsed().as_millis()).ok();
                let selection=selection_interval.and_then(|interval| {
                        let began=std::time::Instant::now();
                        observe!("curve.comparison.fit_selection",fitted(&controls,&indices,interval,fps,speed,[plot.width(),plot.height()],tolerance),
                            "unit={unit:?} curves={} frame_start={} frame_end={} generation={generation:?} revision={revision:?} elapsed_ms={} phase=fit_selection",indices.len(),interval[0],interval[1],began.elapsed().as_millis()).ok()
                    });
                panels.timeline.graph_ranges.open(
                    point,
                    id,
                    graph_ranges::Options {
                        displayed,
                        all,
                        selection,
                    },
                    state,
                    &panels.selected_keys,
                    panels.animation_controls.user_key(),
                    panels.effect_timeline.range_key(),
                    Some(controls.range_context()),
                    panels.selected,
                    panels.focused,
                );
            }
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
