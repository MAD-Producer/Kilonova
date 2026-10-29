//! Normalized easing authoring, with explicit mapping to an outgoing interval.
use super::*;
use crate::panels::{
    curve_comparison::data::{self, Data, Unavailable},
    graph_adaptive::{self, Failure},
    graph_ranges::{self, Bounds, Identity, Options, Surface},
    graph_view::Range,
    key_selection::{self, Set, Status},
    Panel,
};
use motion_core::KeyframeTarget;
use std::sync::Arc;

pub(in crate::panels) struct Context<'a> {
    pub ranges: &'a mut graph_ranges::Controls,
    pub selection: &'a Set,
    pub effect_key: Option<key_selection::Key>,
    pub focused: Option<Panel>,
}

struct Shape {
    easing: Easing,
    size: [f32; 2],
    tolerance: f64,
    automatic: Option<Bounds>,
}
struct Tessellation {
    bounds: Bounds,
    points: Option<Vec<(f64, f64)>>,
}
struct Readout {
    source: crate::layer_clipboard::KeyframeSource,
    target: KeyframeTarget,
    locale: Locale,
    data: Result<Arc<Data>, Arc<Unavailable>>,
}
#[derive(Default)]
pub(super) struct View {
    shape: Option<Shape>,
    tessellation: Option<Tessellation>,
    readout: Option<Readout>,
    graph: Option<Rect>,
    #[cfg(test)]
    pub(in crate::panels) geometry: Option<(Rect, Rect, Bounds)>,
    #[cfg(test)]
    pub(in crate::panels) last_readout: Option<(String, Vec<String>)>,
    #[cfg(test)]
    shape_builds: u32,
    #[cfg(test)]
    tessellation_builds: u32,
}

pub(super) fn normalized(easing: Easing, time: f64) -> Result<f64, motion_core::Error> {
    let sample = easing.sample(time);
    let value = if let Some(curve) = easing.curve.filter(|c| c.space == CurveSpace::Velocity) {
        sample.velocity / curve.definition_scale()
    } else {
        sample.progress
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(motion_core::Error::Invalid(
            "normalized easing sample is not finite".into(),
        ))
    }
}

fn knots(easing: Easing, start: f64, end: f64) -> impl Iterator<Item = f64> {
    let baseline = (0..=80).map(move |i| start + (end - start) * f64::from(i) / 80.);
    let boundaries = [0., 1f64.next_down(), 1.]
        .into_iter()
        .filter(move |t| *t >= start && *t <= end);
    // Retain the established 80-interval baseline, but measure final flatness
    // in screen pixels. Elastic periods cannot alias all the probe locations.
    let elastic = (0..=32).map(|i| f64::from(i) / 32.).filter(move |t| {
        easing
            .curve
            .is_some_and(|c| matches!(c.shape, CurveShape::Elastic { .. }))
            && *t >= start
            && *t <= end
    });
    graph_adaptive::merge_knots(baseline, graph_adaptive::merge_knots(boundaries, elastic))
}

fn controls(easing: Easing) -> Vec<[f64; 2]> {
    match easing.curve.map(|curve| curve.shape) {
        Some(CurveShape::Cubic {
            control1, control2, ..
        }) => vec![control1, control2],
        Some(CurveShape::Quadratic { control, .. }) => vec![control],
        _ => vec![],
    }
}

fn sample(
    easing: Easing,
    bounds: Bounds,
    size: [f32; 2],
    tolerance: f64,
) -> Result<Vec<(f64, f64)>, Failure<motion_core::Error>> {
    let start = bounds.x.low.max(0.);
    let end = bounds.x.high.min(1.);
    if end <= start {
        return Ok(vec![]);
    }
    graph_adaptive::sample(
        knots(easing, start, end),
        [
            f64::from(size[0]) / bounds.x.span(),
            f64::from(size[1]) / bounds.y.span(),
        ],
        tolerance,
        |time| normalized(easing, time),
    )
}

fn automatic(
    easing: Easing,
    size: [f32; 2],
    tolerance: f64,
) -> Result<Bounds, Failure<motion_core::Error>> {
    let extent = graph_adaptive::extrema(knots(easing, 0., 1.), |t| normalized(easing, t))?
        .ok_or(Failure::InvalidGeometry)?;
    let range = |extent: [f64; 2]| {
        let [low, high] = controls(easing)
            .into_iter()
            .fold([extent[0].min(0.), extent[1].max(1.)], |[low, high], p| {
                [low.min(p[1]), high.max(p[1])]
            });
        Range::fitted(low, high, 0.1, 1.).ok_or(Failure::InvalidGeometry)
    };
    let mut bounds = Bounds {
        x: Range::new(0., 1.).ok_or(Failure::InvalidGeometry)?,
        y: range(extent)?,
    };
    let points = sample(easing, bounds, size, tolerance)?;
    bounds.y = range(
        points
            .iter()
            .fold(extent, |[low, high], p| [low.min(p.1), high.max(p.1)]),
    )?;
    Ok(bounds)
}

fn selection(
    state: &Value,
    target: &KeyframeTarget,
    start: i64,
    end: Option<i64>,
    context: &Context<'_>,
    legacy: Option<KeySelection>,
    easing: Easing,
    automatic: Bounds,
) -> Option<Bounds> {
    let end = end.filter(|end| start >= 0 && *end > start)?;
    let selected = |frame: i64| {
        let frame = u32::try_from(frame).ok()?;
        Some(match context.selection.status(state) {
            Status::Current => context.selection.contains(&key_selection::Key {
                target: target.clone(),
                frame,
            }),
            Status::Unowned => legacy.is_some_and(|key| {
                KeyframeTarget::Property {
                    object: key.object,
                    property: key.property,
                    axis: key.axis,
                } == *target
                    && key.frame == frame
            }),
            Status::Empty | Status::Stale => false,
        })
    };
    let times: Vec<_> = [(start, 0.), (end, 1.)]
        .into_iter()
        .filter_map(|(frame, time)| selected(frame).filter(|yes| *yes).map(|_| time))
        .collect();
    let low = *times.first()?;
    let high = *times.last()?;
    if low != high {
        return Some(automatic);
    }
    Some(Bounds {
        x: Range::fitted(low, high, 0.05, 0.1)?,
        y: Range::fitted(
            normalized(easing, low).ok()?,
            normalized(easing, high).ok()?,
            0.1,
            0.2,
        )?,
    })
}

pub(super) fn interval_frame(start: i64, end: Option<i64>, time: f64) -> Option<f64> {
    let end = end.filter(|end| start >= 0 && *end > start)?;
    (time.is_finite() && (0. ..=1.).contains(&time))
        .then_some(start as f64 + (end - start) as f64 * time)
}

impl View {
    pub(super) fn owns_wheel(&self, input: &Input) -> bool {
        self.graph.is_some_and(|graph| graph.contains(input.mouse))
            && (input.modifiers.control || input.modifiers.shift)
    }
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        easing: Easing,
        target: KeyframeTarget,
        start: i64,
        end: Option<i64>,
        legacy: Option<KeySelection>,
        frozen: Option<Bounds>,
        mut context: Option<Context<'_>>,
    ) -> (Rect, Bounds) {
        let styles = list.styles();
        let colors = styles.colors();
        let row = styles.metric("row.height", 22.);
        let header = Rect::new(rect.min[0], rect.min[1], rect.width(), row);
        let graph = Rect::new(rect.min[0], rect.min[1] + row, rect.width(), rect.height());
        self.graph = Some(graph);
        let size = [graph.width(), graph.height()];
        let tolerance = f64::from(styles.metric("graph.sample.error", 0.35));
        let velocity = easing
            .curve
            .is_some_and(|c| c.space == CurveSpace::Velocity);
        let name = match (locale == Locale::Zh, velocity) {
            (true, false) => "归一化进度 · t 0–1",
            (false, false) => "Normalized progress · t 0–1",
            (true, true) => "归一化速度定义 · t 0–1",
            (false, true) => "Normalized velocity definition · t 0–1",
        };
        if self.shape.as_ref().is_none_or(|shape| {
            shape.easing != easing || shape.size != size || shape.tolerance != tolerance
        }) {
            #[cfg(test)]
            {
                self.shape_builds = self.shape_builds.saturating_add(1);
            }
            let started = std::time::Instant::now();
            let automatic = observe!(
                "curve.mini.fit_all",
                automatic(easing, size, tolerance),
                "phase=fit target={target:?} start={start} end={end:?} generation={:?} revision={:?} elapsed_us={}",
                state["document_generation"].as_u64(),state["revision"].as_u64(),started.elapsed().as_micros()
            )
            .ok();
            self.shape = Some(Shape {
                easing,
                size,
                tolerance,
                automatic,
            });
            self.tessellation = None;
        }
        // A failed model has a visible unavailable state; this fallback is only
        // a finite coordinate space for existing handles, never a partial curve.
        let fallback = Bounds {
            x: Range { low: 0., high: 1. },
            y: Range {
                low: -0.1,
                high: 1.1,
            },
        };
        let automatic = self.shape.as_ref().and_then(|s| s.automatic);
        let id = Identity {
            surface: Surface::Mini(Panel::Curves),
            target: Some(target.clone()),
            component: None,
            speed: velocity,
            unit: if velocity {
                "normalized-definition"
            } else {
                "normalized-progress"
            }
            .into(),
        };
        let mut bounds = frozen.unwrap_or_else(|| {
            context
                .as_mut()
                .map_or(automatic.unwrap_or(fallback), |ctx| {
                    ctx.ranges
                        .displayed(state, &id, automatic.unwrap_or(fallback))
                })
        });
        if let Some(ctx) = context.as_mut() {
            if frozen.is_none()
                && !ctx.ranges.editing()
                && graph.contains(input.mouse)
                && !input.composing
                && !input.ime_consumed
                && !input.focus_lost
            {
                let factor = 1.15f64.powf(f64::from(input.wheel[1])).clamp(0.25, 64.);
                let fx = f64::from((input.mouse[0] - graph.min[0]) / graph.width());
                let fy = f64::from((graph.max[1] - input.mouse[1]) / graph.height());
                if input.modifiers.control {
                    let axis = usize::from(input.modifiers.alt);
                    let current = if axis == 0 { bounds.x } else { bounds.y };
                    let anchor = current.at(if axis == 0 { fx } else { fy });
                    ctx.ranges
                        .navigate_mini(&id, bounds, axis, factor, anchor, 0.);
                } else if input.modifiers.shift {
                    let distance =
                        -f64::from(input.wheel[1] + input.wheel[0]) * bounds.x.span() * 0.1;
                    ctx.ranges.navigate_mini(&id, bounds, 0, 1., 0., distance);
                }
                bounds = ctx
                    .ranges
                    .displayed(state, &id, automatic.unwrap_or(fallback));
            }
            if let Some(point) = graph_ranges::launch(
                list,
                atlas,
                header,
                header.min[0],
                input,
                frozen.is_none()
                    && ctx.ranges.ready()
                    && !input.focus_lost
                    && !input.composing
                    && !input.ime_consumed,
            ) {
                let selected = automatic.and_then(|all| {
                    selection(state, &target, start, end, ctx, legacy, easing, all)
                });
                ctx.ranges.open(
                    point,
                    id.clone(),
                    Options {
                        displayed: bounds,
                        all: automatic,
                        selection: selected,
                    },
                    state,
                    ctx.selection,
                    legacy,
                    ctx.effect_key.clone(),
                    None,
                    Some(target.object()),
                    ctx.focused,
                );
                ctx.ranges.capture_mini(target.clone(), start);
            }
        }
        #[cfg(test)]
        if let Some(ctx) = context.as_mut() {
            ctx.ranges.record_probe(id.clone(), header, graph, bounds);
        }
        let pad = styles.metric("padding.panel", 8.);
        let slot = styles.metric("graph.control.height", 24.);
        let mut text = list.child();
        ui::text(
            &mut text,
            atlas,
            Rect::new(
                header.min[0],
                header.min[1],
                (header.width() - pad - slot - styles.metric("graph.control.gap", 2.)).max(0.),
                row,
            ),
            name,
            colors.TEXT_MUTED,
            Align::Left,
        );
        text.clip_to(header);
        list.append(text);
        paint::rect(list, graph, colors.SUNKEN);
        paint::stroke(list, graph, colors.BORDER, styles.metric("border", 1.));
        let point = |time: f64, value: f64| {
            [
                graph.min[0] + bounds.x.fraction(time) as f32 * graph.width(),
                graph.max[1] - bounds.y.fraction(value) as f32 * graph.height(),
            ]
        };
        let mut paint = list.child();
        for i in 0..=4 {
            let x = graph.min[0] + graph.width() * i as f32 / 4.;
            paint::line(
                &mut paint,
                vec![[x, graph.min[1]], [x, graph.max[1]]],
                colors.BORDER,
            );
        }
        if automatic.is_some() {
            if self
                .tessellation
                .as_ref()
                .is_none_or(|cache| cache.bounds != bounds)
            {
                #[cfg(test)]
                {
                    self.tessellation_builds = self.tessellation_builds.saturating_add(1);
                }
                let started = std::time::Instant::now();
                let points = observe!(
                    "curve.mini.adaptive",
                    sample(easing, bounds, size, tolerance),
                    "phase=adaptive target={target:?} start={start} end={end:?} bounds={bounds:?} generation={:?} revision={:?} elapsed_us={}",
                    state["document_generation"].as_u64(),state["revision"].as_u64(),started.elapsed().as_micros()
                )
                .ok();
                self.tessellation = Some(Tessellation { bounds, points });
            }
            if let Some(points) = self
                .tessellation
                .as_ref()
                .and_then(|cache| cache.points.as_ref())
            {
                paint::line_with_width(
                    &mut paint,
                    points.iter().map(|&(t, v)| point(t, v)).collect(),
                    colors.ACCENT,
                    styles.metric("graph.curve_width", 2.),
                );
            }
        }
        if automatic.is_none()
            || self
                .tessellation
                .as_ref()
                .is_some_and(|cache| cache.points.is_none())
        {
            ui::text(
                &mut paint,
                atlas,
                graph.shrink(pad),
                if locale == Locale::Zh {
                    "曲线不可用，请查看日志"
                } else {
                    "Curve unavailable; check log"
                },
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        paint.clip_to(graph);
        list.append(paint);
        #[cfg(test)]
        {
            self.geometry = Some((header, graph, bounds));
            self.last_readout = None;
        }
        if frozen.is_none()
            && graph.contains(input.mouse)
            && automatic.is_some()
            && !context.as_ref().is_some_and(|ctx| ctx.ranges.editing())
        {
            let time = bounds
                .x
                .at(f64::from((input.mouse[0] - graph.min[0]) / graph.width()));
            if (0. ..=1.).contains(&time) {
                let normalized = observe!(
                    "curve.mini.normalized_readout",
                    normalized(easing, time),
                    "phase=normalized_readout target={target:?} t={time} generation={:?} revision={:?}",
                    state["document_generation"].as_u64(),state["revision"].as_u64()
                )
                .ok();
                if let Some(value) = normalized {
                    let title = format!("t={time:.4}");
                    let lines = vec![graph_ranges::value_label(
                        value,
                        match (locale == Locale::Zh, velocity) {
                            (true, true) => "归一化速度定义",
                            (true, false) => "归一化进度",
                            (false, true) => "normalized definition",
                            (false, false) => "normalized progress",
                        },
                    )];
                    let (title, lines) = self.actual_readout(
                        state, locale, &target, start, end, time, velocity, title, lines,
                    );
                    graph_ranges::readout_lines(list, atlas, graph, input.mouse, &title, &lines);
                    #[cfg(test)]
                    {
                        self.last_readout = Some((title, lines));
                    }
                }
            }
        }
        (graph, bounds)
    }

    fn actual_readout(
        &mut self,
        state: &Value,
        locale: Locale,
        target: &KeyframeTarget,
        start: i64,
        end: Option<i64>,
        time: f64,
        speed: bool,
        mut title: String,
        mut lines: Vec<String>,
    ) -> (String, Vec<String>) {
        let Some(frame) = interval_frame(start, end, time) else {
            return (title, lines);
        };
        let source = crate::layer_clipboard::KeyframeSource::of(state);
        if self
            .readout
            .as_ref()
            .is_none_or(|r| r.source != source || r.target != *target || r.locale != locale)
        {
            let started = std::time::Instant::now();
            let data = data::load(state, target, locale)
                .map(Arc::new)
                .map_err(Arc::new);
            if let Err(cause) = &data {
                if matches!(cause.as_ref(), Unavailable::Fault(_)) {
                    let _ = observe!(
                        "curve.mini.readout_load",
                        Err::<(), _>(cause),
                        "phase=load target={target:?} frame={frame} generation={:?} revision={:?} elapsed_us={}",
                        state["document_generation"].as_u64(),state["revision"].as_u64(),started.elapsed().as_micros()
                    );
                }
            }
            self.readout = Some(Readout {
                source,
                target: target.clone(),
                locale,
                data,
            });
        }
        let Some(data) = self.readout.as_ref().and_then(|r| r.data.as_ref().ok()) else {
            lines.push(
                if locale == Locale::Zh {
                    "实际读数不可用"
                } else {
                    "Actual readout unavailable"
                }
                .into(),
            );
            return (title, lines);
        };
        let fps = state["project"]["fps"]
            .as_f64()
            .filter(|fps| fps.is_finite() && *fps > 0.);
        let Some(fps) = fps else {
            let _ = observe!(
                "curve.mini.readout_fps",
                Err::<(), _>("composition fps unavailable"),
                "phase=fps target={target:?} frame={frame} generation={:?} revision={:?}",
                state["document_generation"].as_u64(),
                state["revision"].as_u64()
            );
            lines.push(
                if locale == Locale::Zh {
                    "实际读数不可用"
                } else {
                    "Actual readout unavailable"
                }
                .into(),
            );
            return (title, lines);
        };
        let started = std::time::Instant::now();
        let values = data
            .dimensions
            .iter()
            .map(|&component| {
                data.sample(frame, component, fps, speed).map(|value| {
                    format!(
                        "{}: {}",
                        data.readout_title(component),
                        graph_ranges::value_label(f64::from(value), &data.unit.title(speed))
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>();
        match observe!(
            "curve.mini.readout_sample",
            values,
            "phase=sample target={target:?} frame={frame} speed={speed} generation={:?} revision={:?} elapsed_us={}",
            state["document_generation"].as_u64(),state["revision"].as_u64(),started.elapsed().as_micros()
        ) {
            Ok(values) => {
                title.push_str(&format!(" · {frame:.2} f"));
                lines.extend(values);
            }
            Err(_) => lines.push(
                if locale == Locale::Zh {
                    "实际读数不可用"
                } else {
                    "Actual readout unavailable"
                }
                .into(),
            ),
        }
        (title, lines)
    }
}

#[cfg(test)]
mod tests;
