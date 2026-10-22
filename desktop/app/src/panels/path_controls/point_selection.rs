//! Ephemeral authored-point selection. Persisted edits use existing atomic batches.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Point {
    pub mask: Option<u64>,
    pub item: Option<u64>,
    pub path: u64,
    pub node: u64,
}
impl Point {
    pub fn new(path: &Value, node: u64) -> Self {
        Self {
            mask: path["mask"].as_u64(),
            item: path["item"].as_u64(),
            path: path["id"].as_u64().unwrap_or(1),
            node,
        }
    }
}
struct Seed {
    point: Point,
    geometry: [f32; 6],
    projection: Projection,
    start: [f32; 2],
    animated: bool,
}
struct Move {
    seeds: Vec<Seed>,
    frame: u32,
    began: bool,
    last: Option<Vec<[f32; 6]>>,
}
struct BoxSelect {
    start: [f32; 2],
    end: [f32; 2],
    before: BTreeSet<Point>,
    add: bool,
    toggle: bool,
    frame: u32,
}
#[derive(Default)]
pub(super) struct Selection {
    pub points: BTreeSet<Point>,
    scope: Option<(String, String, u64, bool)>,
    moving: Option<Move>,
    marquee: Option<BoxSelect>,
    rejected: motion_render::observability::FailureCounter,
}
impl Selection {
    pub fn editing(&self) -> bool {
        self.moving.is_some() || self.marquee.is_some()
    }
    pub fn begun(&self) -> bool {
        self.moving.as_ref().is_some_and(|m| m.began)
    }
    pub fn cancel(&mut self) -> bool {
        if let Some(b) = self.marquee.take() {
            self.points = b.before;
        }
        self.moving.take().is_some_and(|m| m.began)
    }
    pub fn clear(&mut self) {
        self.points.clear();
    }
    /// Layer/project changes cancel the previous transaction before accepting input.
    pub fn sync(
        &mut self,
        state: &Value,
        object: Option<u64>,
        mask: bool,
        paths: &[Value],
        out: &mut Vec<Action>,
    ) -> bool {
        let scope = object.map(|id| {
            (
                state["root"].as_str().unwrap_or("").into(),
                state["composition"].as_str().unwrap_or("").into(),
                id,
                mask,
            )
        });
        let changed = self.scope.is_some() && self.scope != scope;
        if self.scope != scope {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.clear();
            self.scope = scope;
        }
        if !self.points.is_empty() {
            let available: std::collections::HashSet<_> = paths
                .iter()
                .flat_map(|path| {
                    path["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(move |n| n["id"].as_u64().map(|id| Point::new(path, id)))
                })
                .collect();
            self.points.retain(|point| available.contains(point));
        }
        changed
    }
    /// Shift adds/toggles a point. Plain press preserves an existing group for dragging.
    pub fn click(&mut self, point: Point, mods: motion_ui::input::Modifiers) -> bool {
        if mods.shift || mods.control {
            if !self.points.insert(point) {
                self.points.remove(&point);
                return false;
            }
        } else if !self.points.contains(&point) {
            self.clear();
            self.points.insert(point);
        }
        true
    }
    pub fn start_move(
        &mut self,
        paths: &[Value],
        base: &Projection,
        _offset: [f32; 2],
        press: [f32; 2],
        frame: u32,
    ) -> motion_core::Result<()> {
        let mut seeds = vec![];
        for path in paths {
            let nodes: Vec<_> = path["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| n["id"].as_u64().map(|id| (Point::new(path, id), n)))
                .filter(|(p, _)| self.points.contains(p))
                .collect();
            if nodes.is_empty() {
                continue;
            }
            let projection = source_geometry::projection(base, path).ok_or_else(|| {
                motion_core::Error::Invalid("selected path projection unavailable".into())
            })?;
            let start = projection.local(press).ok_or_else(|| {
                motion_core::Error::Invalid("selected path cannot unproject pointer".into())
            })?;
            for (point, node) in nodes {
                let geometry = geometry(node).ok_or_else(|| {
                    motion_core::Error::Invalid("selected node geometry unavailable".into())
                })?;
                seeds.push(Seed {
                    point,
                    geometry,
                    projection: projection.clone(),
                    start,
                    animated: node["animated"].as_bool().unwrap_or(false),
                });
            }
        }
        if seeds.len() != self.points.len() {
            return Err(motion_core::Error::Invalid(
                "selected path node disappeared".into(),
            ));
        }
        self.rejected.success();
        self.moving = Some(Move {
            seeds,
            frame,
            began: false,
            last: None,
        });
        Ok(())
    }
    pub fn report(&self, object: u64, phase: &str, error: &dyn std::fmt::Display) {
        if self.rejected.failure(
            "path.point_selection",
            format_args!(
                "object={object} phase={phase} selected={} reason={error}",
                self.points.len()
            ),
        ) == 1
        {
            log::warn!(
                "path point selection rejected: object={object} phase={phase} reason={error}"
            );
        }
    }
    /// Node/handle hits have priority. Only a blank modified press starts a box.
    pub fn blank_press(
        &mut self,
        paths: &[Value],
        projection: &Projection,
        offset: [f32; 2],
        press: [f32; 2],
        input: &Input,
        frame: u32,
        object: u64,
    ) -> bool {
        if let Some((a, b)) = segment(paths, projection, offset, press) {
            if !input.modifiers.shift && !input.modifiers.control {
                self.clear();
            }
            self.points.extend([a, b]);
            if let Err(error) = self.start_move(paths, projection, offset, press, frame) {
                self.report(object, "segment_press", &error);
            }
            return true;
        }
        if input.modifiers.control || input.modifiers.shift {
            self.marquee = Some(BoxSelect {
                start: press,
                end: press,
                before: self.points.clone(),
                add: input.modifiers.shift,
                toggle: input.modifiers.shift && input.modifiers.control,
                frame,
            });
            true
        } else {
            self.clear();
            false
        }
    }
    pub fn drive(
        &mut self,
        paths: &[Value],
        projection: Option<&Projection>,
        offset: [f32; 2],
        target: Option<&Layer>,
        state: &Value,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        let owned = self.editing();
        let release = input.released.filter(|(b, _)| *b == MouseButton::Left);
        let ended = release.is_some()
            || (!input.dragging() && !input.pressed.is_some_and(|(b, _)| b == MouseButton::Left));
        let mut cancelled = input.focus_lost
            || input.keys.iter().any(|(k, _)| *k == Key::Escape)
            || target.is_none_or(|l| l.locked)
            || state["observing"] == true;
        if owned && !cancelled && projection.is_none() {
            self.report(
                target.map_or(0, |l| l.id),
                "projection",
                &"active path projection unavailable",
            );
            cancelled = true;
        }
        let pointer = release.map_or(input.mouse, |(_, p)| p);
        if let Some(mut b) = self.marquee.take() {
            if cancelled || b.frame != frame(state) {
                self.points = b.before;
            } else if let Some(projection) = projection {
                b.end = pointer;
                let area = box_rect(b.start, b.end);
                self.points = if b.add {
                    b.before.clone()
                } else {
                    BTreeSet::new()
                };
                for path in paths.iter().filter(|p| !p["bounds_size"].is_array()) {
                    if let Some(p) = source_geometry::projection(projection, path) {
                        for node in path["nodes"].as_array().into_iter().flatten() {
                            if geometry(node)
                                .and_then(|g| p.screen([g[0] - offset[0], g[1] - offset[1]]))
                                .is_some_and(|p| area.contains(p))
                            {
                                let key = Point::new(path, node["id"].as_u64().unwrap_or(1));
                                if !b.toggle || !b.before.contains(&key) {
                                    self.points.insert(key);
                                } else {
                                    self.points.remove(&key);
                                }
                            }
                        }
                    }
                }
                if !ended {
                    self.marquee = Some(b);
                }
            }
        }
        if let Some(mut moving) = self.moving.take() {
            let cancelled = cancelled
                || moving.frame != frame(state)
                || moving.seeds.iter().any(|s| !self.points.contains(&s.point));
            if !cancelled && (input.dragging() || release.is_some()) {
                let result = translated(&moving.seeds, pointer).and_then(|values| {
                    if moving.last.as_ref() == Some(&values)
                        || (!moving.began
                            && moving.seeds.iter().zip(&values).all(|(s, v)| {
                                distance([s.geometry[0], s.geometry[1]], [v[0], v[1]]) <= 0.1
                            }))
                    {
                        return Ok(None);
                    }
                    let layer = target
                        .ok_or_else(|| motion_core::Error::Invalid("path layer missing".into()))?;
                    let commands = move_commands(layer, &moving.seeds, &values, moving.frame)?;
                    Ok(Some((values, commands)))
                });
                match result {
                    Ok(Some((values, commands))) => {
                        self.rejected.success();
                        if !moving.began {
                            out.push(Action::History(2));
                            moving.began = true;
                        }
                        out.push(Action::EditBatch(commands));
                        moving.last = Some(values);
                    }
                    Ok(None) => {
                        self.rejected.success();
                    }
                    Err(error) => {
                        self.report(target.map_or(0, |l| l.id), "move", &error);
                    }
                }
            }
            if cancelled || ended {
                if moving.began {
                    out.push(Action::History(if cancelled { 4 } else { 3 }));
                }
            } else {
                self.moving = Some(moving);
            }
        }
        owned
    }
    pub fn hovered(
        &self,
        paths: &[Value],
        base: &Projection,
        offset: [f32; 2],
        mouse: [f32; 2],
        primary: Option<Point>,
    ) -> Option<(Point, usize, Option<usize>)> {
        let mut nearest = HIT_RADIUS;
        let mut found = None;
        for path in paths {
            let Some(p) = source_geometry::projection(base, path) else {
                continue;
            };
            for (index, node) in path["nodes"].as_array().into_iter().flatten().enumerate() {
                let Some(g) = geometry(node) else { continue };
                let key = Point::new(path, node["id"].as_u64().unwrap_or(1));
                for axis in [0, 2, 4] {
                    if axis != 0
                        && (path["bounds_size"].is_array()
                            || (!self.points.contains(&key)
                                && primary != Some(key)
                                && (!self.points.is_empty() || primary.is_some()))
                            || g[axis].abs() + g[axis + 1].abs() < 0.01)
                    {
                        continue;
                    }
                    let local = if axis == 0 {
                        [g[0] - offset[0], g[1] - offset[1]]
                    } else {
                        [g[0] + g[axis] - offset[0], g[1] + g[axis + 1] - offset[1]]
                    };
                    if let Some(screen) = p.screen(local) {
                        let d = distance(screen, mouse);
                        if d <= nearest {
                            nearest = d;
                            found = Some((key, index, (axis != 0).then_some(axis)));
                        }
                    }
                }
            }
        }
        found
    }
    pub fn paint(
        &self,
        list: &mut PaintList,
        paths: &[Value],
        base: &Projection,
        offset: [f32; 2],
        hovered: Option<(Point, usize, Option<usize>)>,
    ) {
        let styles = list.styles();
        let radius = styles.metric("path.selection.node_size", 8.) * 0.5;
        let border = styles.metric("border", 1.);
        for path in paths {
            if let Some(projection) = source_geometry::projection(base, path) {
                for node in path["nodes"].as_array().into_iter().flatten() {
                    if self
                        .points
                        .contains(&Point::new(path, node["id"].as_u64().unwrap_or(1)))
                    {
                        if let Some((g, p)) = geometry(node).and_then(|g| {
                            projection
                                .screen([g[0] - offset[0], g[1] - offset[1]])
                                .map(|p| (g, p))
                        }) {
                            for axis in [2, 4] {
                                if g[axis].abs() + g[axis + 1].abs() < 0.01 {
                                    continue;
                                }
                                if let Some(h) = projection.screen([
                                    g[0] + g[axis] - offset[0],
                                    g[1] + g[axis + 1] - offset[1],
                                ]) {
                                    paint::line(
                                        list,
                                        vec![p, h],
                                        styles
                                            .color("focus")
                                            .with_alpha(styles.metric("path.handle.opacity", 0.7)),
                                    );
                                    let hover = hovered.is_some_and(|(key, _, handle)| {
                                        key == Point::new(path, node["id"].as_u64().unwrap_or(1))
                                            && handle == Some(axis)
                                    });
                                    if hover {
                                        paint::diamond(list, h, radius, styles.color("text"));
                                    }
                                    paint::diamond(list, h, radius - border, styles.color("focus"));
                                }
                            }
                            paint::box_rect(
                                list,
                                Rect::new(p[0] - radius, p[1] - radius, radius * 2., radius * 2.),
                                styles.color("focus"),
                                styles.color("text"),
                                border,
                                0.,
                            );
                        }
                    }
                }
            }
        }
        if let Some(b) = &self.marquee {
            paint::box_rect(
                list,
                box_rect(b.start, b.end),
                styles
                    .color("focus")
                    .with_alpha(styles.metric("path.selection.fill_opacity", 0.12)),
                styles.color("focus"),
                border,
                0.,
            );
        }
    }
    pub fn delete(&mut self, layer: &Layer) -> motion_core::Result<Vec<Command>> {
        let mut commands = vec![];
        for mask in &layer.masks {
            let mut path = mask.path.clone();
            path.nodes.retain(|n| {
                !self.points.contains(&Point {
                    mask: Some(mask.id),
                    item: None,
                    path: path.id,
                    node: n.id,
                })
            });
            if path.nodes.len() != mask.path.nodes.len() {
                path.closed &= path.nodes.len() >= 3;
                commands.push(Command::Mask {
                    object: layer.id,
                    action: MaskAction::Path {
                        mask: mask.id,
                        path,
                    },
                });
            }
        }
        if let Content::Vector { vector } = &layer.content {
            let mut updated = vector.clone();
            if delete_vector(&mut updated, None, &self.points) {
                updated.validate()?;
                commands.push(Command::Vector {
                    object: layer.id,
                    action: VectorAction::Replace { vector: updated },
                });
            }
        }
        self.rejected.success();
        Ok(commands)
    }
}
fn frame(state: &Value) -> u32 {
    state["frame"].as_f64().unwrap_or(0.).floor() as u32
}
fn box_rect(a: [f32; 2], b: [f32; 2]) -> Rect {
    Rect::new(
        a[0].min(b[0]),
        a[1].min(b[1]),
        (a[0] - b[0]).abs(),
        (a[1] - b[1]).abs(),
    )
}
fn translated(seeds: &[Seed], pointer: [f32; 2]) -> motion_core::Result<Vec<[f32; 6]>> {
    seeds
        .iter()
        .map(|s| {
            let local = s.projection.local(pointer).ok_or_else(|| {
                motion_core::Error::Invalid("point move cannot unproject pointer".into())
            })?;
            let mut g = s.geometry;
            g[0] += local[0] - s.start[0];
            g[1] += local[1] - s.start[1];
            if g.iter().any(|v| !v.is_finite() || v.abs() > 32768.) {
                return Err(motion_core::Error::Invalid(
                    "point move exceeds geometry limits".into(),
                ));
            }
            Ok(g)
        })
        .collect()
}
fn move_commands(
    layer: &Layer,
    seeds: &[Seed],
    values: &[[f32; 6]],
    frame: u32,
) -> motion_core::Result<Vec<Command>> {
    let mut commands = vec![];
    let mut masks = std::collections::BTreeMap::<u64, Vec<(u64, [f32; 6])>>::new();
    for (seed, value) in seeds.iter().zip(values) {
        let p = seed.point;
        if let Some(mask) = p.mask {
            masks.entry(mask).or_default().push((p.node, *value));
        } else {
            let action = if let Some(item) = p.item {
                VectorAction::SetGroupParameter {
                    item,
                    parameter: format!("node:{}:{}", p.path, p.node),
                    frame,
                    value: motion_core::vector::groups::ParameterValue::Geometry(*value),
                    animated: None,
                }
            } else {
                VectorAction::SetNode {
                    path: p.path,
                    node: p.node,
                    frame,
                    value: *value,
                    animated: seed.animated,
                }
            };
            commands.push(Command::Vector {
                object: layer.id,
                action,
            });
        }
    }
    for (mask, edits) in masks {
        let source = layer
            .masks
            .iter()
            .find(|m| m.id == mask)
            .ok_or_else(|| motion_core::Error::Invalid("selected mask disappeared".into()))?;
        let offset = layer.timeline.map_or(0, |t| t.offset_frame);
        let mut pose = mask_path::values(source, f64::from(frame) - f64::from(offset))?;
        let indices: std::collections::BTreeMap<_, _> = source
            .path
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
        for (node, value) in edits {
            let index = indices.get(&node).copied().ok_or_else(|| {
                motion_core::Error::Invalid("selected mask node disappeared".into())
            })?;
            pose[index * 6..(index + 1) * 6].copy_from_slice(&value);
        }
        let animated = mask_keys::animated(source, mask_path::CHANNEL);
        let updated = mask_path::edit(
            source,
            offset,
            MaskAction::Set {
                mask,
                property: MaskProperty::Node(0),
                frame,
                value: pose,
                animated: Some(animated),
            },
        )?;
        commands.push(Command::Mask {
            object: layer.id,
            action: MaskAction::Path {
                mask,
                path: updated.path,
            },
        });
    }
    Ok(commands)
}
fn delete_vector(vector: &mut VectorContent, item: Option<u64>, points: &BTreeSet<Point>) -> bool {
    use motion_core::vector::groups::{GroupItem, VectorGroup};
    fn visit(group: &mut VectorGroup, points: &BTreeSet<Point>) -> bool {
        let mut changed = false;
        for entry in &mut group.items {
            changed |= match entry {
                GroupItem::Group { group } => visit(group, points),
                GroupItem::Geometry { id, vector, .. } => delete_vector(vector, Some(*id), points),
                _ => false,
            };
        }
        changed
    }
    match &mut vector.source {
        VectorSource::Paths { paths } => {
            let mut changed = false;
            for path in paths {
                let before = path.nodes.len();
                path.nodes.retain(|n| {
                    !points.contains(&Point {
                        mask: None,
                        item,
                        path: path.id,
                        node: n.id,
                    })
                });
                if before != path.nodes.len() {
                    path.closed &= path.nodes.len() >= 3;
                    changed = true;
                }
            }
            changed
        }
        VectorSource::Group { group } => visit(group, points),
        VectorSource::Shape { .. } => false,
    }
}
fn segment(
    paths: &[Value],
    base: &Projection,
    offset: [f32; 2],
    press: [f32; 2],
) -> Option<(Point, Point)> {
    // Geometry approximation, deliberately independent of visual theme overrides.
    const SAMPLES: usize = 32;
    let mut nearest = HIT_RADIUS;
    let mut found = None;
    for path in paths.iter().filter(|p| !p["bounds_size"].is_array()) {
        let Some(p) = source_geometry::projection(base, path) else {
            continue;
        };
        let Some(nodes) = path["nodes"].as_array() else {
            continue;
        };
        let edges =
            nodes.len().saturating_sub(1) + usize::from(path["closed"] == true && nodes.len() > 2);
        for i in 0..edges {
            let j = (i + 1) % nodes.len();
            let (Some(a), Some(b)) = (geometry(&nodes[i]), geometry(&nodes[j])) else {
                continue;
            };
            let mut previous = None;
            for step in 0..=SAMPLES {
                let t = step as f32 / SAMPLES as f32;
                let u = 1. - t;
                let local = std::array::from_fn(|axis| {
                    u * u * u * a[axis]
                        + 3. * u * u * t * (a[axis] + a[axis + 4])
                        + 3. * u * t * t * (b[axis] + b[axis + 2])
                        + t * t * t * b[axis]
                        - offset[axis]
                });
                let current = p.screen(local);
                if let (Some(a), Some(b)) = (previous, current) {
                    let d = line_distance(press, a, b);
                    if d < nearest {
                        nearest = d;
                        found = Some((
                            Point::new(path, nodes[i]["id"].as_u64()?),
                            Point::new(path, nodes[j]["id"].as_u64()?),
                        ));
                    }
                }
                previous = current;
            }
        }
    }
    found
}
fn line_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > f32::EPSILON {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.
    };
    let t = t.clamp(0., 1.);
    distance(p, [a[0] + t * d[0], a[1] + t * d[1]])
}

#[cfg(test)]
mod tests;
