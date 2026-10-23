//! Adapt the toolbar to existing vector paint and history commands.
use motion_core::{
    vector::{
        groups::{Composite, GroupItem, VectorGroup},
        FillRule, ShapeKind, Stroke, VectorAction, VectorContent, VectorSource,
    },
    Command, Content, Layer, Track,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Fill,
    Stroke,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Shared<T> {
    None,
    Value(T),
    Mixed,
}
impl<T: PartialEq + Clone> Shared<T> {
    fn collect(values: impl IntoIterator<Item = Option<T>>) -> Self {
        let mut values = values.into_iter();
        let Some(first) = values.next() else {
            return Self::None;
        };
        if values.any(|v| v != first) {
            Self::Mixed
        } else {
            first.map_or(Self::None, Self::Value)
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Paint {
    pub fill: Option<usize>, // none / nonzero / even-odd
    pub stroke: Option<bool>,
    pub fill_color: [Option<f32>; 4],
    pub stroke_color: [Option<f32>; 4],
    pub width: Option<f32>,
}
#[derive(Clone)]
pub(super) struct Target {
    pub layer: Layer,
    pub vector: VectorContent,
}
#[derive(Clone)]
pub(super) struct Selection {
    pub targets: Vec<Target>,
    pub frame: f64,
    pub root: String,
    pub composition: String,
}
pub(super) struct Summary {
    pub fill: Shared<usize>,
    pub stroke: Shared<bool>,
    pub fill_color: Shared<[f32; 4]>,
    pub stroke_color: Shared<[f32; 4]>,
    pub width: Shared<f32>,
    pub first_fill: [f32; 4],
    pub first_stroke: [f32; 4],
    pub first_width: f32,
}
impl Selection {
    pub fn read(state: &Value, ids: &[u64]) -> Result<Self, &'static str> {
        if state["observing"] == true {
            return Err("observing");
        }
        if ids.is_empty() {
            return Err("selection");
        }
        let frame = state["frame"].as_f64().unwrap_or(0.);
        if !frame.is_finite()
            || frame.fract() != 0.
            || !(0. ..=f64::from(u32::MAX)).contains(&frame)
        {
            return Err("frame");
        }
        let entries = state["project"]["layers"].as_array().ok_or("selection")?;
        let mut targets = vec![];
        for id in ids {
            let entry = entries
                .iter()
                .find(|l| l["id"].as_u64() == Some(*id))
                .ok_or("selection")?;
            let layer: Layer = observe!(
                "toolbar.paint_layer",
                serde_json::from_value(entry.clone()),
                "object={id} phase=decode fallback=disabled"
            )
            .map_err(|_| "invalid")?;
            if layer.locked {
                return Err("locked");
            }
            let at = layer.local_frame(frame);
            if at.fract() != 0. || at < i32::MIN as f64 || at > i32::MAX as f64 {
                return Err("frame");
            }
            let Content::Vector { vector } = &layer.content else {
                return Err("vector");
            };
            targets.push(Target {
                vector: vector.clone(),
                layer,
            });
        }
        Ok(Self {
            targets,
            frame,
            root: state["root"].as_str().unwrap_or("").into(),
            composition: state["composition"].as_str().unwrap_or("").into(),
        })
    }
    pub fn same_context(&self, other: &Self) -> bool {
        self.root == other.root
            && self.composition == other.composition
            && self.frame == other.frame
            && self
                .targets
                .iter()
                .map(|t| t.layer.id)
                .eq(other.targets.iter().map(|t| t.layer.id))
    }
    pub fn summary(&self) -> motion_core::Result<Summary> {
        let mut fills = vec![];
        let mut strokes = vec![];
        for target in &self.targets {
            let at = target.layer.local_frame(self.frame);
            let f = fills.len();
            let s = strokes.len();
            collect(&target.vector, at, &mut fills, &mut strokes)?;
            if f == fills.len() {
                fills.push(None);
            }
            if s == strokes.len() {
                strokes.push(None);
            }
        }
        Ok(Summary {
            fill: Shared::collect(fills.iter().map(|v| v.as_ref().map(|(rule, _)| *rule))),
            stroke: Shared::collect(strokes.iter().map(|s| Some(s.is_some()))),
            fill_color: Shared::collect(fills.iter().map(|v| v.as_ref().map(|(_, c)| *c))),
            stroke_color: Shared::collect(strokes.iter().map(|v| v.as_ref().map(|(c, _)| *c))),
            width: Shared::collect(strokes.iter().map(|v| v.as_ref().map(|(_, w)| *w))),
            first_fill: fills.iter().flatten().next().map_or([1.; 4], |(_, c)| *c),
            first_stroke: strokes.iter().flatten().next().map_or([1.; 4], |(c, _)| *c),
            first_width: strokes.iter().flatten().next().map_or(3., |(_, w)| *w),
        })
    }
    pub fn commands(&self, paint: &Paint) -> motion_core::Result<Vec<Command>> {
        // Scrubbing never resends the geometry payload. Use the same typed
        // parameter adapter as the inspectors, preserving animation semantics.
        if paint.fill.is_none() && paint.stroke.is_none() {
            return self.parameter_commands(paint);
        }
        self.targets
            .iter()
            .map(|target| {
                let local = target.layer.local_frame(self.frame);
                if !local.is_finite()
                    || local < i32::MIN as f64
                    || local > i32::MAX as f64
                    || local.fract() != 0.
                {
                    return Err(motion_core::Error::Invalid(
                        "toolbar paint requires an integral local frame".into(),
                    ));
                }
                let mut vector = target.vector.clone();
                update(&mut vector, paint, local as i32)?;
                Ok(Command::Vector {
                    object: target.layer.id,
                    action: VectorAction::Replace { vector },
                })
            })
            .collect()
    }
    fn parameter_commands(&self, paint: &Paint) -> motion_core::Result<Vec<Command>> {
        use crate::panels::path_controls::groups::parameters::{self, Node};
        fn visit(node: &Node, object: u64, frame: u32, paint: &Paint, out: &mut Vec<Command>) {
            let is_stroke = node.parameters.iter().any(|p| p.id == "width");
            for p in &node.parameters {
                let channels = match p.id.as_str() {
                    "fill" => Some(paint.fill_color),
                    "stroke_color" => Some(paint.stroke_color),
                    "color" => Some(if is_stroke {
                        paint.stroke_color
                    } else {
                        paint.fill_color
                    }),
                    _ => None,
                };
                let mut p = p.clone();
                if let Some(channels) = channels.filter(|c| c.iter().any(Option::is_some)) {
                    for (value, channel) in p.values.iter_mut().zip(channels) {
                        if let Some(channel) = channel {
                            *value = channel;
                        }
                    }
                    out.push(p.set(object, node.id, frame, None));
                } else if matches!(p.id.as_str(), "width" | "stroke_width") {
                    if let Some(width) = paint.width {
                        p.values[0] = width;
                        out.push(p.set(object, node.id, frame, None));
                    }
                }
            }
            for child in &node.children {
                visit(child, object, frame, paint, out);
            }
        }
        let mut commands = vec![];
        for t in &self.targets {
            let tree = parameters::tree(
                &t.vector,
                t.layer.size,
                t.layer.local_frame(self.frame),
                crate::i18n::Locale::En,
            )?;
            visit(&tree, t.layer.id, self.frame as u32, paint, &mut commands);
        }
        Ok(commands)
    }
}
type Fill = Option<(usize, [f32; 4])>;
type Line = Option<([f32; 4], f32)>;
fn rule(rule: FillRule) -> usize {
    if rule == FillRule::NonZero {
        1
    } else {
        2
    }
}
fn collect(
    vector: &VectorContent,
    at: f64,
    fills: &mut Vec<Fill>,
    strokes: &mut Vec<Line>,
) -> motion_core::Result<()> {
    if let Some(fill) = &vector.fill {
        fills.push(Some((rule(vector.fill_rule), fill.sample(at)?)));
    }
    if let Some(s) = &vector.stroke {
        strokes.push(Some((s.color.sample(at)?, s.width.sample(at)?)));
    }
    if let VectorSource::Group { group } = &vector.source {
        collect_group(group, at, fills, strokes)?;
    }
    Ok(())
}
fn collect_group(
    group: &VectorGroup,
    at: f64,
    fills: &mut Vec<Fill>,
    strokes: &mut Vec<Line>,
) -> motion_core::Result<()> {
    for item in &group.items {
        match item {
            GroupItem::Group { group } => collect_group(group, at, fills, strokes)?,
            GroupItem::Geometry { vector, .. } => collect(vector, at, fills, strokes)?,
            GroupItem::Fill {
                color, fill_rule, ..
            } => fills.push(Some((rule(*fill_rule), color.sample(at)?))),
            GroupItem::Stroke { stroke: s, .. } => {
                strokes.push(Some((s.color.sample(at)?, s.width.sample(at)?)))
            }
            _ => {}
        }
    }
    Ok(())
}
fn color(
    track: &mut Track<[f32; 4]>,
    values: [Option<f32>; 4],
    frame: i32,
) -> motion_core::Result<()> {
    if values.iter().any(Option::is_some) {
        let mut sampled = track.sample(f64::from(frame))?;
        for (channel, value) in sampled.iter_mut().zip(values) {
            if let Some(v) = value {
                *channel = v;
            }
        }
        track.set_at(frame, sampled)?;
    }
    Ok(())
}
fn stroke(s: &mut Stroke, paint: &Paint, frame: i32) -> motion_core::Result<()> {
    color(&mut s.color, paint.stroke_color, frame)?;
    if let Some(width) = paint.width {
        s.width.set_at(frame, width)?;
    }
    Ok(())
}
fn update(vector: &mut VectorContent, paint: &Paint, frame: i32) -> motion_core::Result<()> {
    // Keep all unrelated geometry, keyframes, operators and paint options intact.
    let mut fills = vec![];
    let mut strokes = vec![];
    collect(vector, f64::from(frame), &mut fills, &mut strokes)?;
    let has_fill = !fills.is_empty();
    let has_stroke = !strokes.is_empty();
    update_existing(vector, paint, frame)?;
    if paint.fill.is_some_and(|m| m != 0) && !has_fill {
        let fill_rule = if paint.fill == Some(2) {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
        let mut track = Track::constant([1.; 4]);
        color(&mut track, paint.fill_color, frame)?;
        if let VectorSource::Group { group } = &mut vector.source {
            let id = next_id(group)?;
            group.items.push(GroupItem::Fill {
                id,
                name: "Fill".into(),
                color: track,
                fill_rule,
                composite: Composite::Above,
            });
        } else {
            vector.fill = Some(track);
            vector.fill_rule = fill_rule;
        }
    }
    if paint.stroke == Some(true) && !has_stroke {
        let mut s = VectorContent::shape(ShapeKind::Line)
            .stroke
            .ok_or_else(|| motion_core::Error::Invalid("line stroke constructor missing".into()))?;
        stroke(&mut s, paint, frame)?;
        if let VectorSource::Group { group } = &mut vector.source {
            let id = next_id(group)?;
            group.items.push(GroupItem::Stroke {
                id,
                name: "Stroke".into(),
                stroke: s,
                composite: Composite::Above,
            });
        } else {
            vector.stroke = Some(s);
        }
    }
    Ok(())
}
fn next_id(group: &VectorGroup) -> motion_core::Result<u64> {
    fn maximum(group: &VectorGroup) -> u64 {
        group
            .items
            .iter()
            .map(|item| match item {
                GroupItem::Group { group } => maximum(group),
                _ => item.id(),
            })
            .fold(group.id, u64::max)
    }
    maximum(group)
        .checked_add(1)
        .ok_or_else(|| motion_core::Error::Invalid("vector item ID exhausted".into()))
}
fn update_existing(
    vector: &mut VectorContent,
    paint: &Paint,
    frame: i32,
) -> motion_core::Result<()> {
    if paint.fill == Some(0) {
        vector.fill = None;
    }
    if let Some(fill) = &mut vector.fill {
        color(fill, paint.fill_color, frame)?;
        if let Some(mode) = paint.fill {
            vector.fill_rule = if mode == 2 {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            };
        }
    }
    if paint.stroke == Some(false) {
        vector.stroke = None;
    }
    if let Some(s) = &mut vector.stroke {
        stroke(s, paint, frame)?;
    }
    if let VectorSource::Group { group } = &mut vector.source {
        update_group(group, paint, frame)?;
    }
    Ok(())
}
fn update_group(group: &mut VectorGroup, paint: &Paint, frame: i32) -> motion_core::Result<()> {
    group.items.retain(|item| {
        !matches!(item, GroupItem::Fill { .. } if paint.fill == Some(0))
            && !matches!(item, GroupItem::Stroke { .. } if paint.stroke == Some(false))
    });
    for item in &mut group.items {
        match item {
            GroupItem::Group { group } => update_group(group, paint, frame)?,
            GroupItem::Geometry { vector, .. } => update_existing(vector, paint, frame)?,
            GroupItem::Fill {
                color: track,
                fill_rule,
                ..
            } => {
                color(track, paint.fill_color, frame)?;
                if let Some(mode) = paint.fill {
                    *fill_rule = if mode == 2 {
                        FillRule::EvenOdd
                    } else {
                        FillRule::NonZero
                    };
                }
            }
            GroupItem::Stroke { stroke: s, .. } => stroke(s, paint, frame)?,
            _ => {}
        }
    }
    Ok(())
}
