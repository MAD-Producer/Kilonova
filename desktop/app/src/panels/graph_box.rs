//! Group edits of a selected value curve use the existing atomic key commands.
use super::graph_handles::Plot;
use super::*;
use motion_core::Axis;

#[derive(Clone, PartialEq)]
struct Context {
    root: String,
    composition: String,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
}
#[derive(Clone, Default)]
pub(crate) struct Controls {
    context: Option<Context>,
    selected: bool,
    drag: Option<Drag>,
}
#[derive(Clone)]
struct Drag {
    plot: Plot,
    mouse: [f32; 2],
    mode: [i8; 2],
    bounds: [f64; 4],
    seeds: Vec<Value>,
    current: Vec<u32>,
    values: Vec<Value>,
    expected: Vec<Value>,
    begun: bool,
}
impl Controls {
    pub fn has_selection(&self) -> bool {
        self.selected
    }
    pub fn editing(&self) -> bool {
        self.drag.is_some()
    }
    pub fn cancel(&mut self) -> bool {
        self.selected = false;
        self.drag.take().is_some_and(|drag| drag.begun)
    }
    pub fn bounds(&self) -> Option<[f32; 2]> {
        self.drag
            .as_ref()
            .map(|drag| [drag.plot.low, drag.plot.high])
    }
}
fn context(
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
) -> Context {
    Context {
        root: state["root"].as_str().unwrap_or("").into(),
        composition: state["composition"].as_str().unwrap_or("").into(),
        object,
        property,
        axis,
        component,
    }
}
fn keys(source: &Value) -> Vec<Value> {
    source["keys"].as_array().cloned().unwrap_or_default()
}
fn value(key: &Value, context: &Context) -> f64 {
    if context.axis.is_some() || context.property == Property::Opacity {
        key["value"].as_f64().unwrap_or(0.)
    } else {
        key["value"][context.component].as_f64().unwrap_or(0.)
    }
}
fn bounds(source: &Value, context: &Context, scale: f32, total: f64) -> Option<[f64; 4]> {
    let keys: Vec<_> = source["keys"]
        .as_array()?
        .iter()
        .filter(|key| {
            key["frame"]
                .as_f64()
                .is_some_and(|frame| frame >= 0. && frame < total)
        })
        .collect();
    if keys.len() < 2 {
        return None;
    }
    let first = keys.first()?["frame"].as_f64()?;
    let last = keys.last()?["frame"].as_f64()?;
    let low = keys
        .iter()
        .map(|key| value(key, context) * f64::from(scale))
        .fold(f64::INFINITY, f64::min);
    let high = keys
        .iter()
        .map(|key| value(key, context) * f64::from(scale))
        .fold(f64::NEG_INFINITY, f64::max);
    Some([first, last, low, high])
}
fn point(plot: Plot, frame: f64, value: f64) -> [f32; 2] {
    [
        plot.area.min[0] + (frame - plot.offset) as f32 * plot.ppf,
        plot.area.max[1] - (value as f32 - plot.low) / (plot.high - plot.low) * plot.area.height(),
    ]
}
fn rectangle(plot: Plot, bounds: [f64; 4], styles: &motion_ui::style::StyleSheet) -> Rect {
    let pad = styles.metric("padding.panel", 8.);
    let outset = styles.metric("graph.selection.outset", 3.);
    let minimum = styles.metric("border", 1.);
    let a = point(plot, bounds[0], bounds[3]);
    let b = point(plot, bounds[1], bounds[2]);
    Rect::new(
        a[0],
        if plot.speed {
            plot.area.min[1] + pad
        } else {
            a[1] - outset
        },
        (b[0] - a[0]).max(minimum),
        if plot.speed {
            (plot.area.height() - pad * 2.).max(minimum)
        } else {
            (b[1] - a[1] + outset * 2.).max(outset * 2.)
        },
    )
}
fn handles(rect: Rect, plot: Plot, bounds: [f64; 4]) -> Vec<([f32; 2], [i8; 2])> {
    let cx = (rect.min[0] + rect.max[0]) * 0.5;
    let cy = (rect.min[1] + rect.max[1]) * 0.5;
    let mut result = vec![([rect.min[0], cy], [-1, 0]), ([rect.max[0], cy], [1, 0])];
    if !plot.speed && bounds[3] - bounds[2] > 0.000001 {
        for (x, hx) in [(rect.min[0], -1), (cx, 0), (rect.max[0], 1)] {
            result.push(([x, rect.min[1]], [hx, 1]));
            result.push(([x, rect.max[1]], [hx, -1]));
        }
    }
    result
}
fn near_curve(plot: Plot, curve: &[(f64, f32)], mouse: [f32; 2], tolerance: f32) -> bool {
    curve.windows(2).any(|pair| {
        let a = point(plot, pair[0].0, f64::from(pair[0].1));
        let b = point(plot, pair[1].0, f64::from(pair[1].1));
        let v = [b[0] - a[0], b[1] - a[1]];
        let length = v[0] * v[0] + v[1] * v[1];
        let t = if length > 0. {
            ((mouse[0] - a[0]) * v[0] + (mouse[1] - a[1]) * v[1]) / length
        } else {
            0.
        }
        .clamp(0., 1.);
        (mouse[0] - a[0] - v[0] * t).hypot(mouse[1] - a[1] - v[1] * t) <= tolerance
    })
}
pub(super) fn interact(
    styles: &motion_ui::style::StyleSheet,
    panels: &mut Panels,
    state: &Value,
    source: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
    plot: Plot,
    _curve: &[(f64, f32)],
    input: &Input,
    locked: bool,
    out: &mut Vec<Action>,
) -> bool {
    let ctx = context(state, object, property, axis, component);
    let mut controls = std::mem::take(&mut panels.timeline.graph_box);
    let changed = controls.context.as_ref() != Some(&ctx);
    let stale = controls.drag.as_ref().is_some_and(|drag| {
        drag.expected != keys(source)
            || drag.plot.area != plot.area
            || drag.plot.ppf != plot.ppf
            || drag.plot.offset != plot.offset
    });
    if changed
        || stale
        || locked
        || state["observing"] == true
        || input.focus_lost
        || input.keys.iter().any(|(key, _)| *key == Key::Escape)
    {
        let owned = controls.editing();
        if controls.cancel() {
            out.push(Action::History(4));
        }
        controls.context = Some(ctx);
        panels.timeline.graph_box = controls;
        return owned;
    }
    controls.context = Some(ctx.clone());
    let total = last_frame(state) + 1.;
    let current_bounds = bounds(source, &ctx, plot.scale, total);
    if let (true, Some(bounds), Some((MouseButton::Left, mouse))) =
        (controls.selected, current_bounds, input.pressed)
    {
        if plot.area.contains(mouse)
            && controls.drag.is_none()
            && panels.timeline.key_drag.is_none()
            && panels.timeline.graph_drag.is_none()
        {
            let tolerance = styles.metric("graph.selection.hit_radius", 6.);
            let rect = rectangle(plot, bounds, styles);
            let mode = handles(rect, plot, bounds)
                .iter()
                .find_map(|(point, mode)| {
                    ((mouse[0] - point[0]).abs() <= tolerance
                        && (mouse[1] - point[1]).abs() <= tolerance)
                        .then_some(*mode)
                })
                .or_else(|| rect.contains(mouse).then_some([0, 0]));
            if let Some(mode) = mode {
                let expected = keys(source);
                let seeds: Vec<_> = expected
                    .iter()
                    .filter(|key| {
                        key["frame"]
                            .as_f64()
                            .is_some_and(|frame| frame >= 0. && frame < total)
                    })
                    .cloned()
                    .collect();
                controls.drag = Some(Drag {
                    plot,
                    mouse,
                    mode,
                    bounds,
                    current: seeds
                        .iter()
                        .map(|key| key["frame"].as_u64().unwrap() as u32)
                        .collect(),
                    values: seeds.iter().map(|key| key["value"].clone()).collect(),
                    seeds,
                    expected,
                    begun: false,
                });
            }
        }
    }
    let owned = controls.drag.is_some();
    if let Some(drag) = &mut controls.drag {
        panels.cursor = Some(match drag.mode {
            [0, 0] => motion_ui::cursor::Cursor::Grabbing,
            [0, _] => motion_ui::cursor::Cursor::ResizeNS,
            [_, 0] => motion_ui::cursor::Cursor::ResizeEW,
            [x, y] if x == y => motion_ui::cursor::Cursor::ResizeNESW,
            _ => motion_ui::cursor::Cursor::ResizeNWSE,
        });
        let delta = [
            input.mouse[0] - drag.mouse[0],
            input.mouse[1] - drag.mouse[1],
        ];
        let threshold = panels
            .styles
            .clone()
            .unwrap_or_else(motion_ui::style::default_stylesheet_arc)
            .metric("motion.drag_threshold", 3.);
        if (input.dragging() || input.released.is_some())
            && (drag.begun || delta[0].abs().max(delta[1].abs()) >= threshold)
        {
            let edits = transform(&ctx, drag, input.mouse, total);
            if let Some((edits, current, values, expected)) = edits {
                if !edits.is_empty() {
                    if !drag.begun {
                        drag.begun = true;
                        out.push(Action::History(2));
                    }
                    out.push(Action::EditBatch(edits));
                    drag.current = current;
                    drag.values = values;
                    drag.expected = expected;
                }
            }
        }
        if input.released.is_some() {
            if drag.begun {
                out.push(Action::History(3));
            }
            controls.drag = None;
        }
    }
    panels.timeline.graph_box = controls;
    owned
}
fn transform(
    ctx: &Context,
    drag: &Drag,
    mouse: [f32; 2],
    total: f64,
) -> Option<(Vec<Command>, Vec<u32>, Vec<Value>, Vec<Value>)> {
    let b = drag.bounds;
    let dx = f64::from(mouse[0] - drag.mouse[0]) / f64::from(drag.plot.ppf);
    let dy = f64::from(drag.mouse[1] - mouse[1]) / f64::from(drag.plot.area.height())
        * f64::from(drag.plot.high - drag.plot.low);
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    let mut n = b;
    let minimum = (drag.seeds.len() - 1) as f64;
    match drag.mode[0] {
        -1 => n[0] = (b[0] + dx).round().clamp(0., b[1] - minimum),
        1 => n[1] = (b[1] + dx).round().clamp(b[0] + minimum, total - 1.),
        _ => {
            let dt = dx.round().clamp(-b[0], total - 1. - b[1]);
            n[0] += dt;
            n[1] += dt;
        }
    }
    if !drag.plot.speed {
        match drag.mode[1] {
            -1 => n[2] = (b[2] + dy).min(b[3] - 0.000001),
            1 => n[3] = (b[3] + dy).max(b[2] + 0.000001),
            _ => {
                n[2] += dy;
                n[3] += dy;
            }
        }
    }
    let current: Vec<_> = drag
        .seeds
        .iter()
        .map(|key| {
            let frame = key["frame"].as_f64().unwrap();
            (n[0] + (frame - b[0]) / (b[1] - b[0]) * (n[1] - n[0])).round() as u32
        })
        .collect();
    if current.windows(2).any(|pair| pair[0] >= pair[1]) {
        return None;
    }
    let mut values = vec![];
    for key in &drag.seeds {
        let v = value(key, ctx) * f64::from(drag.plot.scale);
        let next = if drag.plot.speed {
            v
        } else if b[3] - b[2] > 0.000001 {
            n[2] + (v - b[2]) / (b[3] - b[2]) * (n[3] - n[2])
        } else {
            v + dy
        };
        let mut next = (next / f64::from(drag.plot.scale)) as f32;
        if ctx.property == Property::Opacity {
            next = next.clamp(0., 1.);
        }
        if !next.is_finite() {
            return None;
        }
        let mut v = key["value"].clone();
        if ctx.axis.is_some() || ctx.property == Property::Opacity {
            v = serde_json::json!(next);
        } else {
            v[ctx.component] = serde_json::json!(next);
        }
        values.push(v);
    }
    let mut order: Vec<_> = (0..current.len())
        .filter(|i| current[*i] != drag.current[*i])
        .collect();
    order.sort_by_key(|i| {
        if current[*i] < drag.current[*i] {
            (0, i64::from(drag.current[*i]))
        } else {
            (1, -i64::from(drag.current[*i]))
        }
    });
    let reserved: Vec<_> = drag
        .expected
        .iter()
        .filter_map(|key| key["frame"].as_u64())
        .filter(|frame| !drag.current.contains(&(*frame as u32)))
        .collect();
    if current
        .iter()
        .any(|frame| reserved.contains(&u64::from(*frame)))
    {
        return None;
    }
    let mut edits = vec![];
    for i in order {
        edits.push(Command::MoveKey {
            object: ctx.object,
            property: ctx.property,
            axis: ctx.axis,
            from: drag.current[i],
            to: current[i],
        });
    }
    for (i, next) in values.iter().enumerate() {
        if *next == drag.values[i] {
            continue;
        }
        edits.push(if let Some(axis) = ctx.axis {
            Command::SetComponent {
                object: ctx.object,
                property: ctx.property,
                axis,
                frame: current[i],
                value: next.as_f64().unwrap() as f32,
            }
        } else if ctx.property == Property::Opacity {
            Command::SetScalar {
                object: ctx.object,
                property: ctx.property,
                frame: current[i],
                value: next.as_f64().unwrap() as f32,
            }
        } else {
            Command::SetVector {
                object: ctx.object,
                property: ctx.property,
                frame: current[i],
                value: std::array::from_fn(|axis| next[axis].as_f64().unwrap() as f32),
            }
        });
    }
    let mut expected = drag.expected.clone();
    for key in &mut expected {
        if let Some(i) = drag
            .current
            .iter()
            .position(|frame| key["frame"].as_u64() == Some(u64::from(*frame)))
        {
            let old = key["frame"].as_i64().unwrap();
            if let Some(local) = key["local_frame"].as_i64() {
                key["local_frame"] = serde_json::json!(local + i64::from(current[i]) - old);
            }
            key["frame"] = serde_json::json!(current[i]);
            key["value"] = values[i].clone();
        }
    }
    expected.sort_by_key(|key| key["frame"].as_i64().unwrap_or(0));
    Some((edits, current, values, expected))
}
pub(super) fn select_curve(
    styles: &motion_ui::style::StyleSheet,
    panels: &mut Panels,
    state: &Value,
    source: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    component: usize,
    plot: Plot,
    curve: &[(f64, f32)],
    input: &Input,
) -> bool {
    if panels.timeline.graph_box.editing()
        || panels.timeline.key_drag.is_some()
        || panels.timeline.graph_drag.is_some()
        || !clicked(input, plot.area)
    {
        return false;
    }
    let ctx = context(state, object, property, axis, component);
    let tolerance = styles.metric("graph.selection.hit_radius", 6.);
    let hit = bounds(source, &ctx, plot.scale, last_frame(state) + 1.).is_some_and(|bounds| {
        rectangle(plot, bounds, styles).contains(input.mouse)
            && near_curve(plot, curve, input.mouse, tolerance)
    });
    panels.timeline.graph_box.selected = hit;
    panels.timeline.graph_box.context = Some(ctx);
    hit
}
pub(super) fn draw(
    list: &mut PaintList,
    panels: &Panels,
    state: &Value,
    source: &Value,
    plot: Plot,
) {
    let controls = &panels.timeline.graph_box;
    let Some(ctx) = controls.context.as_ref().filter(|_| controls.selected) else {
        return;
    };
    let Some(bounds) = bounds(source, ctx, plot.scale, last_frame(state) + 1.) else {
        return;
    };
    let styles = list.styles();
    let rect = rectangle(plot, bounds, &styles);
    let color = styles.colors().ACCENT;
    let handle = styles.metric("graph.selection.handle_size", 6.);
    paint::rect(
        list,
        rect,
        color.with_alpha(styles.metric("graph.selection.fill_opacity", 0.05)),
    );
    paint::stroke(list, rect, color, styles.metric("graph.handle_width", 1.));
    for (point, _) in handles(rect, plot, bounds) {
        paint::rect(
            list,
            Rect::new(
                point[0] - handle * 0.5,
                point[1] - handle * 0.5,
                handle,
                handle,
            ),
            list.styles().colors().TEXT,
        );
    }
}
pub(super) fn delete(panels: &mut Panels, state: &Value, out: &mut Vec<Action>) -> bool {
    if !matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
        || !panels.timeline.graph_mode
        || !panels.timeline.graph_box.selected
    {
        return false;
    }
    let Some(ctx) = panels.timeline.graph_box.context.clone() else {
        return false;
    };
    if state["root"].as_str().unwrap_or("") != ctx.root
        || state["composition"].as_str().unwrap_or("") != ctx.composition
        || panels.selected != Some(ctx.object)
    {
        panels.timeline.graph_box.selected = false;
        return true;
    }
    let layer = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|layer| layer["id"] == ctx.object);
    if layer.is_none_or(|layer| layer["locked"] == true) || state["observing"] == true {
        return true;
    }
    let edits = super::animation_data::channel(state, ctx.object, ctx.property, ctx.axis)
        .into_iter()
        .flat_map(|source| source["keys"].as_array().into_iter().flatten())
        .filter_map(|key| {
            key["frame"]
                .as_u64()
                .filter(|frame| *frame < last_frame(state) as u64 + 1)
        })
        .map(|frame| Command::DeleteKey {
            object: ctx.object,
            property: ctx.property,
            axis: ctx.axis,
            frame: frame as u32,
        })
        .collect::<Vec<_>>();
    if !edits.is_empty() {
        out.push(Action::EditBatch(edits));
    } else {
        log::debug!("graph delete cleared selection object={} property={:?} axis={:?} reason=no_deletable_keys", ctx.object, ctx.property, ctx.axis);
    }
    panels.timeline.graph_box.selected = false;
    panels.animation_controls.clear_key();
    true
}

pub(super) fn select_all(panels: &mut Panels, state: &Value) -> bool {
    if !panels.timeline.graph_mode
        || !matches!(panels.focused, Some(Panel::Timeline | Panel::Curves))
    {
        return false;
    }
    let Some(object) = panels.selected else {
        return false;
    };
    let property = panels
        .animation_controls
        .property
        .unwrap_or(Property::Position);
    let axis = panels.animation_controls.axis;
    let Some(source) = super::animation_data::channel(state, object, property, axis) else {
        return false;
    };
    let ctx = context(
        state,
        object,
        property,
        axis,
        panels.timeline.graph_component,
    );
    if bounds(source, &ctx, 1., last_frame(state) + 1.).is_none() {
        return false;
    }
    panels.timeline.graph_box.context = Some(ctx);
    panels.timeline.graph_box.selected = true;
    panels.audio_controls.clear_selection();
    panels.animation_controls.clear_key();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_geometry_uses_theme_sizes_and_preserves_the_six_pixel_hit_radius() {
        let mut data: Value =
            serde_json::from_str(include_str!("../../../ui/styles/dark.json")).unwrap();
        let base =
            motion_ui::style::StyleSheet::from_json(&serde_json::to_vec(&data).unwrap()).unwrap();
        let plot = Plot {
            area: Rect::new(0., 0., 100., 100.),
            offset: 0.,
            ppf: 10.,
            low: 0.,
            high: 1.,
            speed: false,
            fps: 30.,
            scale: 1.,
        };
        let bounds = [1., 4., 0.2, 0.8];
        let original = rectangle(plot, bounds, &base);
        let curve = [(0., 0.5), (10., 0.5)];
        assert!(near_curve(
            plot,
            &curve,
            [25., 56.],
            base.metric("graph.selection.hit_radius", 6.)
        ));
        assert!(!near_curve(
            plot,
            &curve,
            [25., 56.1],
            base.metric("graph.selection.hit_radius", 6.)
        ));
        data["tokens"]["sizes"]["graph.selection.outset"] = 9.into();
        data["tokens"]["sizes"]["graph.selection.hit_radius"] = 14.into();
        let custom =
            motion_ui::style::StyleSheet::from_json(&serde_json::to_vec(&data).unwrap()).unwrap();
        let changed = rectangle(plot, bounds, &custom);
        assert_eq!(original.min[0], changed.min[0]);
        assert!((original.min[1] - changed.min[1] - 6.).abs() < 0.001);
        assert!((changed.height() - original.height() - 12.).abs() < 0.001);
        assert!(near_curve(
            plot,
            &curve,
            [25., 64.],
            custom.metric("graph.selection.hit_radius", 6.)
        ));
    }
}
