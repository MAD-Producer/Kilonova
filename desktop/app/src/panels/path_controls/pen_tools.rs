//! Pen topology edits address authored paths, including grouped geometry and masks.
use super::*;
use point_selection::Point;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    #[default]
    Draw,
    Add,
    Delete,
    Smooth,
    Corner,
}
impl Mode {
    pub const ALL: [Self; 5] = [
        Self::Draw,
        Self::Add,
        Self::Delete,
        Self::Smooth,
        Self::Corner,
    ];
    pub fn title(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Draw, Locale::Zh) => "绘制与选择",
            (Self::Add, Locale::Zh) => "添加顶点",
            (Self::Delete, Locale::Zh) => "删除顶点",
            (Self::Smooth, Locale::Zh) => "转换为平滑点",
            (Self::Corner, Locale::Zh) => "转换为角点",
            (Self::Draw, _) => "Draw and select",
            (Self::Add, _) => "Add vertex",
            (Self::Delete, _) => "Delete vertex",
            (Self::Smooth, _) => "Convert to smooth",
            (Self::Corner, _) => "Convert to corner",
        }
    }
    pub fn hint(self, locale: Locale) -> Option<&'static str> {
        match (self, locale) {
            (Self::Draw, _) => None,
            (Self::Add, Locale::Zh) => Some("点击路径段添加顶点 · 保持曲线形状 · Esc 取消"),
            (Self::Delete, Locale::Zh) => Some("点击顶点删除 · Ctrl+Z 撤销 · Esc 取消"),
            (Self::Smooth, Locale::Zh) => Some("点击顶点对齐手柄 · 保留锚点和手柄长度 · Esc 取消"),
            (Self::Corner, Locale::Zh) => Some("点击顶点收起手柄 · 保留锚点 · Esc 取消"),
            (Self::Add, _) => {
                Some("Click a segment to add a vertex · Preserve the curve · Esc cancels")
            }
            (Self::Delete, _) => Some("Click a vertex to delete · Ctrl+Z undoes · Esc cancels"),
            (Self::Smooth, _) => Some(
                "Click a vertex to align handles · Keep anchor and handle lengths · Esc cancels",
            ),
            (Self::Corner, _) => {
                Some("Click a vertex to retract handles · Keep anchor · Esc cancels")
            }
        }
    }
}
struct Pending {
    command: Command,
    object: u64,
    revision: u64,
    frame: f64,
    root: String,
    composition: String,
    press: [f32; 2],
}
#[derive(Default)]
pub(super) struct Controls {
    pub mode: Mode,
    menu: motion_ui::widgets::Dropdown,
    pending: Option<Pending>,
    rejected: motion_render::observability::FailureCounter,
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.menu.open || self.pending.is_some()
    }
    pub fn cancel(&mut self) {
        self.menu.open = false;
        self.pending = None;
    }
}
impl super::Controls {
    /// Paint the tool menu last; capture its opening, closing and selection clicks.
    pub(super) fn draw_pen_tools(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        clip: Rect,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) -> bool {
        let styles = list.styles();
        let pad = styles.metric("padding.panel", 8.);
        let area = Rect::new(
            clip.min[0] + pad,
            clip.min[1] + pad,
            styles
                .metric("path.tools.width", 176.)
                .min((clip.width() - pad * 2.).max(0.)),
            styles.metric("field.height", 18.),
        );
        let was_open = self.pen_tools.menu.open;
        let owned = was_open
            || input
                .pressed
                .is_some_and(|(button, p)| button == MouseButton::Left && area.contains(p));
        let items = Mode::ALL.map(|mode| motion_ui::widgets::DropdownItem {
            label: mode.title(locale),
            enabled: true,
        });
        let mode = Mode::ALL
            .iter()
            .position(|m| *m == self.pen_tools.mode)
            .unwrap_or(0);
        let chosen = self
            .pen_tools
            .menu
            .draw(list, atlas, area, clip, &items, mode, input, true, false);
        if (!was_open && self.pen_tools.menu.open) || chosen.is_some() {
            let open = self.pen_tools.menu.open;
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.pen_tools.menu.open = open;
            if let Some(index) = chosen {
                self.pen_tools.mode = Mode::ALL[index];
                self.notice = None;
                self.focus = true;
            }
        }
        owned || self.pen_tools.menu.open
    }
    pub(super) fn pen_tool_input(
        &mut self,
        clip: Rect,
        paths: &[Value],
        projection: Option<&Projection>,
        offset: [f32; 2],
        target: Option<&Layer>,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) -> bool {
        let mode = self.pen_tools.mode;
        if mode == Mode::Draw {
            return false;
        }
        let frame = state["frame"].as_f64().unwrap_or(0.);
        if let Some(pending) = self.pen_tools.pending.take() {
            let cancelled = input.focus_lost
                || input.keys.iter().any(|(k, _)| *k == Key::Escape)
                || selected != Some(pending.object)
                || state["observing"] == true
                || target.is_none_or(|l| l.locked)
                || state["revision"].as_u64().unwrap_or(0) != pending.revision
                || frame != pending.frame
                || state["root"].as_str().unwrap_or("") != pending.root
                || state["composition"].as_str().unwrap_or("") != pending.composition;
            if !cancelled {
                if let Some((MouseButton::Left, point)) = input.released {
                    if distance(point, pending.press) <= HIT_RADIUS {
                        out.push(Action::Edit(pending.command));
                        self.point_selection.clear();
                        self.selected = None;
                        self.selected_item = None;
                    }
                } else if input.dragging() {
                    self.pen_tools.pending = Some(pending);
                }
            }
            return true;
        }
        let Some((MouseButton::Left, press)) = input.pressed else {
            return true;
        };
        if input.focus_lost || input.composing || state["observing"] == true {
            return true;
        }
        let (Some(layer), Some(projection)) = (target.filter(|l| !l.locked), projection) else {
            return true;
        };
        if !clip.contains(press) {
            return true;
        }
        let hit = if mode == Mode::Add {
            segment(paths, projection, offset, press)
                .map(|(point, next, t)| (point, Edit::Add { next, t }))
        } else {
            // Only anchors, never handles or the resize corners of parametric shapes.
            let mut best = HIT_RADIUS;
            let mut found = None;
            for path in paths.iter().filter(|p| !p["bounds_size"].is_array()) {
                let Some(p) = source_geometry::projection(projection, path) else {
                    continue;
                };
                for node in path["nodes"].as_array().into_iter().flatten() {
                    let (Some(g), Some(id)) = (geometry(node), node["id"].as_u64()) else {
                        continue;
                    };
                    let Some(p) = p.screen([g[0] - offset[0], g[1] - offset[1]]) else {
                        continue;
                    };
                    let d = distance(p, press);
                    if d < best {
                        best = d;
                        found = Some(Point::new(path, id));
                    }
                }
            }
            found.map(|point| {
                (
                    point,
                    match mode {
                        Mode::Delete => Edit::Delete,
                        Mode::Smooth => Edit::Smooth,
                        _ => Edit::Corner,
                    },
                )
            })
        };
        let Some((point, edit)) = hit else {
            self.pen_tools.rejected.success();
            if paths.iter().any(|p| p["bounds_size"].is_array()) {
                self.notice = Some(
                    if locale == Locale::Zh {
                        "请先在属性中将形状转换为路径，再编辑顶点"
                    } else {
                        "Use Convert to path in Properties before editing vertices"
                    }
                    .into(),
                );
            }
            return true;
        };
        match command(layer, point, edit, frame.floor() as u32) {
            Ok(command) => {
                self.pen_tools.rejected.success();
                self.notice = None;
                self.pen_tools.pending = Some(Pending {
                    command,
                    object: layer.id,
                    revision: state["revision"].as_u64().unwrap_or(0),
                    frame,
                    root: state["root"].as_str().unwrap_or("").into(),
                    composition: state["composition"].as_str().unwrap_or("").into(),
                    press,
                });
            }
            Err(error) => {
                let count=self.pen_tools.rejected.failure("path.pen_edit",format_args!(
                    "object={} mask={:?} item={:?} path={} node={} mode={mode:?} frame={frame} phase=prepare reason={error}",
                    layer.id,point.mask,point.item,point.path,point.node));
                if count == 1 {
                    log::warn!("pen edit rejected: object={} path={} node={} mode={mode:?} frame={frame} reason={error}",
                    layer.id,point.path,point.node);
                }
                self.notice = Some(format!(
                    "{}: {error}",
                    if locale == Locale::Zh {
                        "路径编辑未完成"
                    } else {
                        "Path edit rejected"
                    }
                ));
            }
        }
        true
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Edit {
    Add { next: u64, t: f32 },
    Delete,
    Smooth,
    Corner,
}
fn invalid(message: &str) -> motion_core::Error {
    motion_core::Error::Invalid(message.into())
}
fn mix(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub(super) fn cubic(a: [f32; 6], b: [f32; 6], t: f32) -> [f32; 2] {
    split(a, b, t).1[..2]
        .try_into()
        .expect("anchor has two coordinates")
}
/// De Casteljau subdivision in source space, without flattening the curve.
fn split(mut a: [f32; 6], mut b: [f32; 6], t: f32) -> ([f32; 6], [f32; 6], [f32; 6]) {
    let p0 = [a[0], a[1]];
    let p1 = [a[0] + a[4], a[1] + a[5]];
    let p2 = [b[0] + b[2], b[1] + b[3]];
    let p3 = [b[0], b[1]];
    let x = mix(p0, p1, t);
    let y = mix(p1, p2, t);
    let z = mix(p2, p3, t);
    let left = mix(x, y, t);
    let right = mix(y, z, t);
    let p = mix(left, right, t);
    a[4] = x[0] - p0[0];
    a[5] = x[1] - p0[1];
    b[2] = z[0] - p3[0];
    b[3] = z[1] - p3[1];
    (
        a,
        [
            p[0],
            p[1],
            left[0] - p[0],
            left[1] - p[1],
            right[0] - p[0],
            right[1] - p[1],
        ],
        b,
    )
}

/// Matching temporal interpolation preserves every subframe. With independent
/// clocks, preserve each authored path pose and the edit frame, then interpolate
/// the new vertex with the existing linear-key semantics. Endpoint clocks stay intact.
fn split_tracks(
    a: &Track<[f32; 6]>,
    b: &Track<[f32; 6]>,
    t: f32,
    path: &VectorPath,
    frame: i32,
) -> motion_core::Result<[Track<[f32; 6]>; 3]> {
    a.validate_local()?;
    b.validate_local()?;
    if a.axes.is_some() || b.axes.is_some() {
        return Err(invalid("pen subdivision requires coupled geometry tracks"));
    }
    let template = if a.keys.is_empty() { b } else { a };
    let independent = !a.keys.is_empty()
        && !b.keys.is_empty()
        && (a.keys.len() != b.keys.len()
            || a.keys
                .iter()
                .zip(&b.keys)
                .any(|(a, b)| a.frame != b.frame || a.ease != b.ease || a.curve != b.curve));
    let (av, nv, bv) = split(a.value, b.value, t);
    let mut left = a.clone();
    left.value = av;
    for key in &mut left.keys {
        key.value = split(key.value, [0.; 6], t).0;
    }
    let mut right = b.clone();
    right.value = bv;
    for key in &mut right.keys {
        key.value = split([0.; 6], key.value, t).2;
    }
    let mut middle = Track::constant(nv);
    if independent {
        let mut frames = std::collections::BTreeSet::from([frame]);
        for key in path.nodes.iter().flat_map(|n| &n.geometry.keys) {
            frames.insert(key.frame);
            if frames.len() > motion_core::MAX_FRAMES as usize {
                return Err(invalid(
                    "pen subdivision exceeds the new vertex keyframe budget",
                ));
            }
        }
        for frame in frames {
            middle.keys.push(motion_core::Keyframe {
                frame,
                value: split(a.sample(f64::from(frame))?, b.sample(f64::from(frame))?, t).1,
                ease: motion_core::Ease::Linear,
                curve: None,
                spatial: None,
            });
        }
        return Ok([left, middle, right]);
    }
    for key in &template.keys {
        let (_, nv, _) = split(
            a.sample(f64::from(key.frame))?,
            b.sample(f64::from(key.frame))?,
            t,
        );
        let mut next = key.clone();
        next.value = nv;
        middle.keys.push(next);
    }
    Ok([left, middle, right])
}

fn edit_path(path: &mut VectorPath, node: u64, edit: Edit, frame: i32) -> motion_core::Result<()> {
    let index = path
        .nodes
        .iter()
        .position(|n| n.id == node)
        .ok_or_else(|| invalid("pen target node disappeared"))?;
    match edit {
        Edit::Add { next, t } => {
            if !t.is_finite() || !(0. ..1.).contains(&t) || t == 0. {
                return Err(invalid(
                    "pen subdivision parameter must lie inside the segment",
                ));
            }
            let j = (index + 1) % path.nodes.len();
            if (j == 0 && (!path.closed || path.nodes.len() < 3)) || path.nodes[j].id != next {
                return Err(invalid("pen target segment disappeared or is open"));
            }
            let id = path
                .nodes
                .iter()
                .map(|n| n.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .filter(|id| *id != 0)
                .ok_or_else(|| invalid("pen node ID overflow"))?;
            let [left, middle, right] = split_tracks(
                &path.nodes[index].geometry,
                &path.nodes[j].geometry,
                t,
                path,
                frame,
            )?;
            path.nodes[index].geometry = left;
            path.nodes[j].geometry = right;
            path.nodes.insert(
                index + 1,
                PathNode {
                    id,
                    geometry: middle,
                },
            );
        }
        Edit::Delete => {
            path.nodes.remove(index);
            path.closed &= path.nodes.len() >= 3;
        }
        Edit::Corner | Edit::Smooth => {
            let mut g = path.nodes[index].geometry.sample(f64::from(frame))?;
            if matches!(edit, Edit::Corner) {
                g[2..].fill(0.);
            } else {
                let previous = if index > 0 {
                    Some(index - 1)
                } else {
                    path.closed.then_some(path.nodes.len() - 1)
                };
                let next = if index + 1 < path.nodes.len() {
                    Some(index + 1)
                } else {
                    path.closed.then_some(0)
                };
                let anchor = [g[0], g[1]];
                let sample = |i: Option<usize>| {
                    i.map(|i| -> motion_core::Result<_> {
                        let p = path.nodes[i].geometry.sample(f64::from(frame))?;
                        Ok([p[0], p[1]])
                    }).transpose()
                };
                let before = sample(previous)?;
                let after = sample(next)?;
                let incoming = g[2].hypot(g[3]);
                let outgoing = g[4].hypot(g[5]);
                let direction = if outgoing > f32::EPSILON {
                    [g[4] / outgoing, g[5] / outgoing]
                } else if incoming > f32::EPSILON {
                    [-g[2] / incoming, -g[3] / incoming]
                } else {
                    let a = before.unwrap_or(anchor);
                    let b = after.unwrap_or(anchor);
                    let length = distance(a, b);
                    if length <= f32::EPSILON {
                        return Err(invalid(
                            "pen smoothing requires distinct neighbouring anchors",
                        ));
                    }
                    [(b[0] - a[0]) / length, (b[1] - a[1]) / length]
                };
                let incoming = if incoming > f32::EPSILON {
                    incoming
                } else {
                    before.map_or(0., |p| distance(anchor, p) / 3.)
                };
                let outgoing = if outgoing > f32::EPSILON {
                    outgoing
                } else {
                    after.map_or(0., |p| distance(anchor, p) / 3.)
                };
                for axis in 0..2 {
                    g[axis + 2] = -direction[axis] * incoming;
                    g[axis + 4] = direction[axis] * outgoing;
                }
            }
            path.nodes[index].geometry.set_at(frame, g)?;
        }
    }
    Ok(())
}

fn vector_path(
    vector: &mut VectorContent,
    item: Option<u64>,
    path: u64,
) -> Option<&mut VectorPath> {
    use motion_core::vector::groups::GroupItem;
    match &mut vector.source {
        VectorSource::Paths { paths } if item.is_none() => paths.iter_mut().find(|p| p.id == path),
        VectorSource::Group { group } => {
            for entry in &mut group.items {
                let found = match entry {
                    GroupItem::Geometry { id, vector, .. } if Some(*id) == item => {
                        vector_path(vector, None, path)
                    }
                    GroupItem::Group { group } => group_path(group, item, path),
                    _ => None,
                };
                if found.is_some() {
                    return found;
                }
            }
            None
        }
        _ => None,
    }
}
fn group_path(
    group: &mut motion_core::vector::groups::VectorGroup,
    item: Option<u64>,
    path: u64,
) -> Option<&mut VectorPath> {
    use motion_core::vector::groups::GroupItem;
    for entry in &mut group.items {
        let found = match entry {
            GroupItem::Geometry { id, vector, .. } if Some(*id) == item => {
                vector_path(vector, None, path)
            }
            GroupItem::Group { group } => group_path(group, item, path),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}
pub(super) fn command(
    layer: &Layer,
    point: Point,
    edit: Edit,
    frame: u32,
) -> motion_core::Result<Command> {
    if layer.locked {
        return Err(invalid("pen target layer is locked"));
    }
    let frame =
        i32::try_from(i64::from(frame) - i64::from(layer.timeline.map_or(0, |t| t.offset_frame)))
            .map_err(|_| invalid("pen local frame overflow"))?;
    if let Some(id) = point.mask {
        let mut masks = layer.masks.clone();
        let mask = masks
            .iter_mut()
            .find(|m| m.id == id && m.path.id == point.path)
            .ok_or_else(|| invalid("pen target mask disappeared"))?;
        edit_path(&mut mask.path, point.node, edit, frame)?;
        let path = mask.path.clone();
        motion_core::masks::validate(&masks)?;
        Ok(Command::Mask {
            object: layer.id,
            action: MaskAction::Path { mask: id, path },
        })
    } else if let Content::Vector { vector } = &layer.content {
        let mut updated = vector.clone();
        let path = vector_path(&mut updated, point.item, point.path).ok_or_else(|| {
            invalid("pen target path disappeared; convert the shape to a path first")
        })?;
        edit_path(path, point.node, edit, frame)?;
        updated.validate()?;
        Ok(Command::Vector {
            object: layer.id,
            action: VectorAction::Replace { vector: updated },
        })
    } else {
        Err(invalid("pen target is not editable vector geometry"))
    }
}

/// Nearest point on the projected cubic. A coarse bracket followed by local
/// refinement keeps hit tolerance in screen pixels at every viewport zoom.
pub(super) fn segment(
    paths: &[Value],
    base: &Projection,
    offset: [f32; 2],
    mouse: [f32; 2],
) -> Option<(Point, u64, f32)> {
    const SAMPLES: usize = 64;
    let mut best = HIT_RADIUS;
    let mut found = None;
    for path in paths.iter().filter(|p| !p["bounds_size"].is_array()) {
        let Some(projection) = source_geometry::projection(base, path) else {
            continue;
        };
        let Some(nodes) = path["nodes"].as_array() else {
            continue;
        };
        let count =
            nodes.len().saturating_sub(1) + usize::from(path["closed"] == true && nodes.len() > 2);
        for i in 0..count {
            let j = (i + 1) % nodes.len();
            let (Some(a), Some(b)) = (geometry(&nodes[i]), geometry(&nodes[j])) else {
                continue;
            };
            let cost = |t| {
                let p = cubic(a, b, t);
                projection
                    .screen([p[0] - offset[0], p[1] - offset[1]])
                    .map_or(f32::INFINITY, |p| distance(p, mouse))
            };
            let step = (0..=SAMPLES).min_by(|a, b| {
                cost(*a as f32 / SAMPLES as f32).total_cmp(&cost(*b as f32 / SAMPLES as f32))
            })?;
            let mut lo = step.saturating_sub(1) as f32 / SAMPLES as f32;
            let mut hi = (step + 1).min(SAMPLES) as f32 / SAMPLES as f32;
            for _ in 0..20 {
                let x = lo + (hi - lo) / 3.;
                let y = hi - (hi - lo) / 3.;
                if cost(x) < cost(y) {
                    hi = y
                } else {
                    lo = x
                }
            }
            let t = (lo + hi) * 0.5;
            let d = cost(t);
            if d < best {
                best = d;
                found = Some((
                    Point::new(path, nodes[i]["id"].as_u64()?),
                    nodes[j]["id"].as_u64()?,
                    t,
                ));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests;
