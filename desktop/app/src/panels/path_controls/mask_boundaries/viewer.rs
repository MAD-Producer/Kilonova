//! Measurement grips share the numeric parameter transaction and source plane.
use super::super::*;
use motion_render::mask_plan::{BoundaryGeometry, feather_support};
#[cfg(test)]
mod tests;

#[derive(PartialEq)]
struct GeometryKey {
    nodes: Vec<[f32; 6]>,
    expansion: f32,
    size: [f32; 2],
    density: f32,
}
struct Span {
    axis: usize,
    at: f32,
    base: [f32; 2],
    expanded: Vec<[f32; 2]>,
}
struct Cached {
    key: GeometryKey,
    result: Result<Vec<Span>, String>,
}
struct Capture {
    object: u64,
    mask: u64,
    axis: usize,
    sign: f32,
    factor: f32,
    projection: Projection,
    start: [f32; 2],
    screen: [f32; 2],
    frame: f64,
    nodes: Vec<[f32; 6]>,
}
#[derive(Clone)]
struct Grip {
    property: MaskProperty,
    axis: usize,
    sign: f32,
    screen: [f32; 2],
    anchor: [f32; 2],
    recovery: bool,
}
#[derive(Default)]
pub(in crate::panels::path_controls) struct Viewer {
    cache: Option<Cached>,
    capture: Option<Capture>,
    failures: motion_render::observability::FailureCounter,
    failed: bool,
    #[cfg(test)]
    grips: Vec<Grip>,
}
impl Viewer {
    pub(in crate::panels::path_controls) fn cancel(&mut self) {
        self.capture = None;
        self.cache = None;
        self.failures.success();
        self.failed = false;
    }
}
fn point(axis: usize, value: f32, at: f32) -> [f32; 2] {
    if axis == 0 { [value, at] } else { [at, value] }
}
fn widest(intervals: &[[f32; 2]]) -> Option<[f32; 2]> {
    intervals
        .iter()
        .copied()
        .max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
}
fn build(key: &GeometryKey) -> Result<Vec<Span>, motion_render::RenderError> {
    let rect = [0., 0., key.size[0], key.size[1]];
    let base = BoundaryGeometry::new(key.nodes.clone(), rect, key.size, 0., key.density)?;
    let expanded = if key.expansion == 0. {
        None
    } else {
        Some(BoundaryGeometry::new(
            key.nodes.clone(),
            rect,
            key.size,
            key.expansion,
            key.density,
        )?)
    };
    let mut spans = vec![];
    for axis in [0, 1] {
        if let Some(at) = base.reference_coordinate(axis)? {
            if let Some(interval) = widest(&base.section(axis, at)?) {
                spans.push(Span {
                    axis,
                    at,
                    base: interval,
                    expanded: expanded.as_ref().unwrap_or(&base).section(axis, at)?,
                });
            }
        }
    }
    Ok(spans)
}
impl Controls {
    fn stop_boundary(&mut self, cancelled: bool, out: &mut Vec<Action>) {
        self.boundaries.capture = None;
        if self
            .parameter
            .take()
            .is_some_and(|parameter| parameter.begun)
        {
            out.push(Action::History(if cancelled { 4 } else { 3 }));
        }
        self.parameter_context = None;
    }
    pub(in crate::panels::path_controls) fn mask_boundaries(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        clip: Rect,
        projection: Option<&Projection>,
        layer: Option<&Layer>,
        state: &Value,
        input: &Input,
        locale: Locale,
        visible: bool,
        authored: &[[f32; 2]],
        out: &mut Vec<Action>,
    ) -> bool {
        #[cfg(test)]
        self.boundaries.grips.clear();
        let styles = list.styles();
        let mask = layer.and_then(|layer| layer.masks.iter().find(|m| Some(m.id) == self.mask));
        let frame = state["frame"].as_f64().unwrap_or(0.);
        let eligible = self.tool == Tool::Mask
            && self.pen_tools.mode == pen_tools::Mode::Draw
            && visible
            && state["observing"] != true
            && self.context_menu.is_none()
            && layer.is_some_and(|l| !l.locked)
            && projection.is_some()
            && mask.is_some()
            && self.drag.is_none()
            && self.draft.is_none()
            && !self.point_selection.editing()
            && (self.parameter.is_none() || self.boundaries.capture.is_some())
            && (self.boundaries.capture.is_some()
                || !(input.modifiers.control || input.modifiers.shift || input.modifiers.alt))
            && input.held != Some(MouseButton::Middle)
            && !input
                .pressed
                .is_some_and(|(button, _)| button == MouseButton::Middle);
        let sampled = if eligible {
            mask.map(|m| m.sample(layer.unwrap().local_frame(frame)))
                .transpose()
        } else {
            Ok(None)
        };
        let (sampled, sampling_failed) = match sampled {
            Ok(Some(Some(sample))) => (Some(sample), false),
            Ok(_) => (None, false),
            Err(error) => {
                let cause: String = error.to_string().chars().take(512).collect();
                self.boundaries.failures.failure(
                    "mask.boundary_geometry",
                    format_args!(
                        "object={} mask={} frame={frame} phase=sample cause={cause}",
                        layer.map_or(0, |l| l.id),
                        self.mask.unwrap_or(0)
                    ),
                );
                self.boundaries.failed = true;
                self.notice = Some(
                    if locale == Locale::Zh {
                        "蒙版边界不可用，请查看日志"
                    } else {
                        "Mask boundary unavailable; see log"
                    }
                    .into(),
                );
                (None, true)
            }
        };
        let mut owned = false;
        if let Some(capture) = self.boundaries.capture.take() {
            owned = true;
            let valid = eligible
                && layer.is_some_and(|l| l.id == capture.object)
                && self.mask == Some(capture.mask)
                && frame == capture.frame
                && sampled.as_ref().is_some_and(|s| s.nodes == capture.nodes)
                && projection.is_some_and(|p| {
                    p.matrix == capture.projection.matrix && p.image == capture.projection.image
                })
                && self.parameter.is_some()
                && self
                    .parameter_context
                    .as_ref()
                    .is_some_and(|(root, composition, _)| {
                        root == state["root"].as_str().unwrap_or("")
                            && composition == state["composition"].as_str().unwrap_or("")
                    });
            if !valid || input.pressed.is_some_and(|(b, _)| b == MouseButton::Right) {
                self.stop_boundary(true, out);
            } else {
                let finishing = input.focus_lost
                    || input
                        .key_presses()
                        .any(|(k, _, repeat)| k == Key::Escape && !repeat)
                    || input.released.is_some_and(|(b, _)| b == MouseButton::Left)
                    || input.pressed.is_some()
                    || !input.dragging();
                let begun = self.parameter.as_ref().is_some_and(|p| p.begun);
                let actual = input
                    .released
                    .filter(|(b, _)| *b == MouseButton::Left)
                    .map_or(input.mouse, |(_, p)| p);
                let moved =
                    distance(actual, capture.screen) >= styles.metric("motion.drag_threshold", 3.);
                let release = input
                    .released
                    .is_some_and(|(button, _)| button == MouseButton::Left);
                if !begun && finishing && (!release || !moved) {
                    self.stop_boundary(true, out);
                } else if begun || moved {
                    if let Some(local) = capture.projection.local(actual) {
                        let delta = (local[capture.axis] - capture.start[capture.axis])
                            * capture.sign
                            * capture.factor;
                        let mut mapped = input.clone();
                        mapped.mouse = [delta, 0.];
                        mapped.released = input.released.map(|(button, _)| (button, [delta, 0.]));
                        self.parameter_input(state, &mapped, out, 0.);
                        if self.parameter.is_some() {
                            self.boundaries.capture = Some(capture);
                        }
                    } else {
                        self.stop_boundary(true, out);
                    }
                } else {
                    self.boundaries.capture = Some(capture);
                }
            }
        }
        let (Some(layer), Some(mask), Some(sampled), Some(projection)) =
            (layer, mask, sampled, projection)
        else {
            if !sampling_failed {
                self.boundaries.cache = None;
                self.boundaries.failures.success();
                if self.boundaries.failed {
                    self.notice = None;
                    self.boundaries.failed = false;
                }
            }
            return owned;
        };
        let offset = layer.size.map(|v| v * 0.5);
        let screen =
            |source: [f32; 2]| projection.screen([source[0] - offset[0], source[1] - offset[1]]);
        let center = screen(offset);
        let density = center
            .and_then(|a| {
                Some(
                    distance(a, screen([offset[0] + 1., offset[1]])?)
                        .max(distance(a, screen([offset[0], offset[1] + 1.])?))
                        .clamp(1., 64.),
                )
            })
            .unwrap_or(1.);
        let key = GeometryKey {
            nodes: sampled.nodes.clone(),
            expansion: sampled.expansion,
            size: layer.size,
            density,
        };
        if self
            .boundaries
            .cache
            .as_ref()
            .is_none_or(|cache| cache.key != key)
        {
            let result = build(&key).map_err(|e| e.to_string().chars().take(512).collect());
            self.boundaries.cache = Some(Cached { key, result });
        }
        let spans = match &self.boundaries.cache.as_ref().unwrap().result {
            Ok(spans) => {
                self.boundaries.failures.success();
                if self.boundaries.failed {
                    self.notice = None;
                    self.boundaries.failed = false;
                }
                spans
            }
            Err(cause) => {
                self.boundaries.failures.failure(
                    "mask.boundary_geometry",
                    format_args!(
                        "object={} mask={} frame={frame} phase=measurement cause={cause}",
                        layer.id, mask.id
                    ),
                );
                self.boundaries.failed = true;
                self.notice = Some(
                    if locale == Locale::Zh {
                        "蒙版边界不可用，请查看日志"
                    } else {
                        "Mask boundary unavailable; see log"
                    }
                    .into(),
                );
                return owned;
            }
        };
        let line_width = styles.metric("viewer.overlay.line.width", 1.);
        let radius = styles.metric("graph.handle_radius", 4.);
        let hit = styles.metric("graph.handle.hit_radius", 9.);
        let gap = styles.metric("icon.slot", 20.);
        let colors = styles.colors();
        let color = styles.color("viewer.guide");
        let corner = styles.metric("radius.control", 2.);
        let mut grips = vec![];
        for span in spans {
            for &[lo, hi] in &span.expanded {
                if let (Some(a), Some(b)) = (
                    screen(point(span.axis, lo, span.at)),
                    screen(point(span.axis, hi, span.at)),
                ) {
                    paint::line_with_width(list, vec![a, b], color, line_width);
                }
            }
            if let Some([lo, hi]) = widest(&span.expanded) {
                for (edge, sign) in [(lo, -1.), (hi, 1.)] {
                    if let Some(at) = screen(point(span.axis, edge, span.at)) {
                        grips.push(Grip {
                            property: MaskProperty::Expansion,
                            axis: span.axis,
                            sign,
                            screen: at,
                            anchor: at,
                            recovery: false,
                        });
                    }
                    // Source-space 1x kernel support, independently of UI zoom.
                    // This is a sampling-range guide, not an alpha contour.
                    let support = match feather_support(sampled.feather[span.axis]) {
                        Ok(support) => support,
                        Err(cause) => {
                            self.boundaries.failures.failure(
                                "mask.boundary_geometry",
                                format_args!(
                                    "object={} mask={} frame={frame} phase=feather cause={cause}",
                                    layer.id, mask.id
                                ),
                            );
                            continue;
                        }
                    };
                    if let Some(anchor) = screen(point(span.axis, edge + sign * support, span.at)) {
                        let display = if span.axis == 0 {
                            [anchor[0], anchor[1] - gap]
                        } else {
                            [anchor[0] + gap, anchor[1]]
                        };
                        paint::line_with_width(list, vec![anchor, display], color, line_width);
                        grips.push(Grip {
                            property: MaskProperty::Feather,
                            axis: span.axis,
                            sign,
                            screen: display,
                            anchor,
                            recovery: false,
                        });
                    }
                }
            } else if let Some(at) = screen(point(
                span.axis,
                (span.base[0] + span.base[1]) * 0.5,
                span.at,
            )) {
                grips.push(Grip {
                    property: MaskProperty::Expansion,
                    axis: span.axis,
                    sign: 1.,
                    screen: at,
                    anchor: at,
                    recovery: true,
                });
            }
        }
        for grip in &mut grips {
            for _ in 0..4 {
                if authored
                    .iter()
                    .all(|p| distance(*p, grip.screen) > hit + HIT_RADIUS)
                {
                    break;
                }
                let previous = grip.screen;
                grip.screen[1 - grip.axis] += if grip.axis == 0 { -gap } else { gap };
                paint::line_with_width(list, vec![previous, grip.screen], color, line_width);
            }
        }
        grips.retain(|grip| {
            authored
                .iter()
                .all(|p| distance(*p, grip.screen) > hit + HIT_RADIUS)
                && clip.contains_rect(Rect::new(
                    grip.screen[0] - hit,
                    grip.screen[1] - hit,
                    hit * 2.,
                    hit * 2.,
                ))
        });
        let cursor = input.pressed.map_or(input.mouse, |(_, p)| p);
        let hovered = grips
            .iter()
            .enumerate()
            .filter(|(_, g)| distance(cursor, g.screen) <= hit)
            .filter(|_| authored.iter().all(|p| distance(cursor, *p) > HIT_RADIUS))
            .min_by(|(_, a), (_, b)| {
                distance(cursor, a.screen).total_cmp(&distance(cursor, b.screen))
            })
            .map(|(i, _)| i);
        for (i, grip) in grips.iter().enumerate() {
            if grip.screen != grip.anchor {
                paint::line_with_width(
                    list,
                    vec![
                        [grip.anchor[0] - radius, grip.anchor[1]],
                        [grip.anchor[0] + radius, grip.anchor[1]],
                    ],
                    color,
                    line_width,
                );
            }
            let fill = if hovered == Some(i) {
                colors.TEXT
            } else {
                color
            };
            if matches!(grip.property, MaskProperty::Expansion) {
                // Boundary distances use rings; diamonds remain authored
                // tangent handles, so both operations retain distinct cues.
                paint::box_rect(
                    list,
                    Rect::new(
                        grip.screen[0] - radius,
                        grip.screen[1] - radius,
                        radius * 2.,
                        radius * 2.,
                    ),
                    colors.PANEL,
                    fill,
                    line_width,
                    radius,
                );
            } else {
                paint::box_rect(
                    list,
                    Rect::new(
                        grip.screen[0] - radius,
                        grip.screen[1] - radius,
                        radius * 2.,
                        radius * 2.,
                    ),
                    colors.PANEL,
                    fill,
                    line_width,
                    corner,
                );
            }
        }
        #[cfg(test)]
        {
            self.boundaries.grips = grips.clone();
        }
        if let Some(index) = hovered {
            let grip = &grips[index];
            let feather = matches!(grip.property, MaskProperty::Feather);
            let value = if feather {
                sampled.feather[grip.axis]
            } else {
                sampled.expansion
            };
            let title = match (locale == Locale::Zh, feather, grip.recovery) {
                (true, true, _) => "羽化采样范围",
                (false, true, _) => "Feather sample range",
                (true, false, true) => "截面为空 · 扩展",
                (false, false, true) => "Empty section · Expansion",
                (true, false, false) => "扩展",
                (false, false, false) => "Expansion",
            };
            let caption = if feather {
                format!(
                    "{title} {}  {value:.2} px",
                    if grip.axis == 0 { "X" } else { "Y" }
                )
            } else {
                format!("{title}  {value:.2} px")
            };
            let pad = styles.metric("padding", 5.);
            let width = (atlas.measure(&caption) + pad * 2.).min(clip.width());
            let height = styles.metric("row.height", 22.);
            let rect = Rect::new(
                (cursor[0] + pad).clamp(clip.min[0], (clip.max[0] - width).max(clip.min[0])),
                (cursor[1] + gap).clamp(clip.min[1], (clip.max[1] - height).max(clip.min[1])),
                width,
                height,
            );
            paint::box_rect(
                list,
                rect,
                colors.PANEL,
                colors.BORDER,
                line_width,
                styles.metric("radius.popup", 4.),
            );
            label(list, atlas, rect, &caption, colors.TEXT);
            if input
                .pressed
                .is_some_and(|(button, _)| button == MouseButton::Left)
                && !owned
                && self.parameter.is_none()
                && self.drag.is_none()
                && self.draft.is_none()
                && !self.point_selection.editing()
            {
                if let Some(start) = projection.local(cursor) {
                    self.parameter = Some(Parameter {
                        host: motion_ui::dock::Panel::Composition,
                        object: layer.id,
                        mask: mask.id,
                        property: grip.property.clone(),
                        axis: if feather { grip.axis } else { 0 },
                        values: if feather {
                            sampled.feather.to_vec()
                        } else {
                            vec![sampled.expansion]
                        },
                        text: format!("{value:.2}"),
                        replace: true,
                        editor: Default::default(),
                        rect: Rect::new(
                            grip.screen[0] - hit,
                            grip.screen[1] - hit,
                            hit * 2.,
                            hit * 2.,
                        ),
                        start: Some(0.),
                        begun: false,
                        last_value: value,
                        linked: feather && mask.feather_linked,
                        range: if feather {
                            [0., motion_core::masks::MAX_MASK_DISTANCE]
                        } else {
                            [
                                -motion_core::masks::MAX_MASK_DISTANCE,
                                motion_core::masks::MAX_MASK_DISTANCE,
                            ]
                        },
                    });
                    self.parameter_context = Some((
                        state["root"].as_str().unwrap_or("").into(),
                        state["composition"].as_str().unwrap_or("").into(),
                        frame.floor() as u32,
                    ));
                    self.boundaries.capture = Some(Capture {
                        object: layer.id,
                        mask: mask.id,
                        axis: grip.axis,
                        sign: grip.sign,
                        factor: if feather { 4. / 3. } else { 1. },
                        projection: projection.clone(),
                        start,
                        screen: cursor,
                        frame,
                        nodes: sampled.nodes,
                    });
                    self.focus = true;
                    out.push(Action::Seek(frame));
                    owned = true;
                }
            }
        }
        owned || hovered.is_some()
    }
}
