//! Source path editing. Rendering supplies the matrix; commands own persisted data.
use crate::{
    i18n::{Locale, Text as T},
    panels::Action,
};
use glam::{DMat4, DVec4};
use motion_core::masks::{LayerMask, MaskAction, MaskMode, MaskProperty};
use motion_core::vector::{
    FillRule, LineCap, LineJoin, PathNode, Stroke, VectorAction, VectorContent, VectorPath,
    VectorSource,
};
use motion_core::{Command, Content, Layer, Track};
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::{palette, Color},
    ui::{self, Align},
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Pen,
    Mask,
}
impl Tool {
    pub fn title(self) -> T {
        match self {
            Self::Select => T::SelectTool,
            Self::Pen => T::Pen,
            Self::Mask => T::Mask,
        }
    }
}

#[derive(Clone)]
pub struct Projection {
    matrix: DMat4,
    inverse: DMat4,
    pub image: Rect,
}
impl Projection {
    pub fn new(matrix: [f64; 16], view: Rect, size: [f32; 2]) -> Option<Self> {
        if size.iter().any(|v| !v.is_finite() || *v <= 0.)
            || view.width() <= 0.
            || view.height() <= 0.
        {
            return None;
        }
        let scale = (view.width() / size[0]).min(view.height() / size[1]);
        let image = Rect::new(
            view.min[0] + (view.width() - size[0] * scale) * 0.5,
            view.min[1] + (view.height() - size[1] * scale) * 0.5,
            size[0] * scale,
            size[1] * scale,
        );
        let matrix = DMat4::from_cols_array(&matrix);
        if !matrix.is_finite() || matrix.determinant().abs() < 1e-15 {
            return None;
        }
        Some(Self {
            inverse: matrix.inverse(),
            matrix,
            image,
        })
    }
    pub fn screen(&self, p: [f32; 2]) -> Option<[f32; 2]> {
        let v = self.matrix * DVec4::new(p[0] as f64, -(p[1] as f64), 0., 1.);
        if !v.is_finite() || v.w <= 1e-8 {
            return None;
        }
        Some([
            self.image.min[0] + ((v.x / v.w + 1.) * 0.5) as f32 * self.image.width(),
            self.image.min[1] + ((1. - v.y / v.w) * 0.5) as f32 * self.image.height(),
        ])
    }
    pub fn local(&self, p: [f32; 2]) -> Option<[f32; 2]> {
        let x = 2. * ((p[0] - self.image.min[0]) / self.image.width()) as f64 - 1.;
        let y = 1. - 2. * ((p[1] - self.image.min[1]) / self.image.height()) as f64;
        let ray = |z| {
            let p = self.inverse * DVec4::new(x, y, z, 1.);
            (p.is_finite() && p.w.abs() > 1e-10).then(|| p.truncate() / p.w)
        };
        let a = ray(0.)?;
        let b = ray(1.)?;
        let d = b - a;
        if d.z.abs() < 1e-10 {
            return None;
        }
        let p = a + d * (-a.z / d.z);
        let local = [p.x as f32, -p.y as f32];
        if local.iter().any(|v| !v.is_finite() || v.abs() > 32768.) || self.screen(local).is_none()
        {
            None
        } else {
            Some(local)
        }
    }
}
fn composition_size(state: &Value) -> [f32; 2] {
    [
        state["project"]["width"].as_f64().unwrap_or(1920.) as f32,
        state["project"]["height"].as_f64().unwrap_or(1080.) as f32,
    ]
}
pub fn layer_projection(state: &Value, object: u64, view: Rect) -> Option<Projection> {
    let entry = state["source_transforms"]
        .as_array()?
        .iter()
        .find(|v| v["id"].as_u64() == Some(object))?;
    let m = entry["mvp"].as_array()?;
    if m.len() != 16 {
        return None;
    }
    let mut matrix = [0.; 16];
    for (a, b) in matrix.iter_mut().zip(m) {
        *a = b.as_f64()?;
    }
    Projection::new(matrix, view, composition_size(state))
}
fn identity_projection(state: &Value, view: Rect) -> Option<Projection> {
    let [w, h] = composition_size(state);
    let m = DMat4::from_cols_array(&[
        2. / w as f64,
        0.,
        0.,
        0.,
        0.,
        2. / h as f64,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        0.,
        0.,
        1.,
    ]);
    Projection::new(m.to_cols_array(), view, [w, h])
}
pub fn new_vector(path: VectorPath) -> VectorContent {
    VectorContent {
        source: VectorSource::Paths {
            paths: vec![path.clone()],
        },
        fill: path.closed.then(|| Track::constant([0.16, 0.55, 0.94, 1.])),
        fill_rule: FillRule::NonZero,
        trim: None,
        stroke: Some(Stroke {
            color: Track::constant([0.16, 0.55, 0.94, 1.]),
            width: Track::constant(3.),
            cap: LineCap::Round,
            join: LineJoin::Round,
            miter_limit: 4.,
            dashes: None,
        }),
    }
}
#[derive(Clone, Debug)]
enum Target {
    New,
    Vector { object: u64 },
    Mask { object: u64, mask: u64 },
}
struct Draft {
    target: Target,
    projection: Projection,
    offset: [f32; 2],
    path: VectorPath,
    base_paths: Vec<VectorPath>,
    revision: u64,
    frame: f64,
    selection: Option<u64>,
    root: String,
}
struct Drag {
    target: Target,
    path: u64,
    node: u64,
    axis: usize,
    value: [f32; 6],
    last_geometry: [f32; 6],
    start: [f32; 2],
    projection: Projection,
    offset: [f32; 2],
    animated: bool,
    frame: u32,
    begun: bool,
}
#[derive(Default)]
pub struct Controls {
    pub tool: Tool,
    pub mask: Option<u64>,
    draft: Option<Draft>,
    drag: Option<Drag>,
    selected: Option<(u64, u64)>,
    focus: bool,
    pub notice: Option<String>,
    parameter: Option<Parameter>,
    pub masks_open: bool,
}
struct Parameter {
    object: u64,
    mask: u64,
    property: MaskProperty,
    axis: usize,
    values: Vec<f32>,
    text: String,
    replace: bool,
    rect: Rect,
    start: Option<f32>,
    begun: bool,
    last_value: f32,
    range: [f32; 2],
}
fn label(list: &mut PaintList, atlas: &mut TextAtlas, r: Rect, text: &str, color: Color) {
    ui::text(list, atlas, r, text, color, Align::Left);
}
fn click(input: &Input, r: Rect) -> bool {
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && r.contains(p))
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
fn layer(state: &Value, id: u64) -> Option<Layer> {
    serde_json::from_value(
        state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|v| v["id"].as_u64() == Some(id))?
            .clone(),
    )
    .ok()
}
fn geometry(node: &Value) -> Option<[f32; 6]> {
    let a = node["geometry"].as_array()?;
    if a.len() != 6 {
        return None;
    }
    let mut g = [0.; 6];
    for (x, v) in g.iter_mut().zip(a) {
        *x = v.as_f64()? as f32;
    }
    Some(g)
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.draft.is_some() || self.drag.is_some() || self.parameter.is_some()
    }
    pub fn wants_keyboard(&self) -> bool {
        self.editing() || (self.tool != Tool::Select && self.focus)
    }
    pub fn cancel(&mut self) -> bool {
        let drag = self.drag.take().is_some_and(|d| d.begun);
        let param = self.parameter.take().is_some_and(|p| p.begun);
        self.draft = None;
        self.focus = false;
        drag || param
    }
    pub fn choose(&mut self, tool: Tool, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(4));
        }
        self.tool = tool;
        self.selected = None;
        self.mask = None;
        self.notice = None;
        if tool == Tool::Mask {
            self.masks_open = true;
        }
    }
    fn finish(&mut self, closed: bool, out: &mut Vec<Action>) {
        let Some(mut draft) = self.draft.take() else {
            return;
        };
        if draft.path.nodes.len() < 2
            || (matches!(draft.target, Target::Mask { .. }) && draft.path.nodes.len() < 3)
        {
            self.draft = Some(draft);
            return;
        }
        draft.path.closed = closed || matches!(draft.target, Target::Mask { .. });
        match draft.target {
            Target::New => out.push(Action::AddPath(draft.path)),
            Target::Vector { object } => {
                draft.base_paths.push(draft.path);
                out.push(Action::Edit(Command::Vector {
                    object,
                    action: VectorAction::SetPaths {
                        paths: draft.base_paths,
                    },
                }));
            }
            Target::Mask { object, mask } => {
                let mut m = LayerMask::new(mask, draft.path);
                m.name = format!("Mask {mask}");
                out.push(Action::Edit(Command::Mask {
                    object,
                    action: MaskAction::Add { mask: m },
                }));
                self.mask = Some(mask);
            }
        }
    }
    fn begin(&mut self, state: &Value, selected: Option<u64>, view: Rect) -> Option<()> {
        let mut target = Target::New;
        let mut offset = [0., 0.];
        let mut base_paths = vec![];
        let mut id = 1;
        let projection = if self.tool == Tool::Mask {
            let l = layer(state, selected?)?;
            if l.locked
                || matches!(
                    l.content,
                    Content::Null | Content::Audio { .. } | Content::Adjustment
                )
            {
                return None;
            }
            let mask = l
                .masks
                .iter()
                .map(|m| m.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)?;
            target = Target::Mask { object: l.id, mask };
            offset = l.size.map(|v| v * 0.5);
            layer_projection(state, l.id, view)?
        } else if let Some(l) = selected
            .and_then(|id| layer(state, id))
            .filter(|l| !l.locked)
        {
            if let Content::Vector {
                vector:
                    VectorContent {
                        source: VectorSource::Paths { paths },
                        ..
                    },
            } = l.content
            {
                id = paths
                    .iter()
                    .map(|p| p.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)?;
                target = Target::Vector { object: l.id };
                base_paths = paths;
                layer_projection(state, l.id, view)?
            } else {
                identity_projection(state, view)?
            }
        } else {
            identity_projection(state, view)?
        };
        self.draft = Some(Draft {
            target,
            projection,
            offset,
            path: VectorPath {
                id,
                closed: false,
                nodes: vec![],
            },
            base_paths,
            revision: state["revision"].as_u64().unwrap_or(0),
            frame: state["frame"].as_f64().unwrap_or(0.),
            selection: selected,
            root: state["root"].as_str().unwrap_or("").into(),
        });
        Some(())
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        view: Rect,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) -> bool {
        if self.tool == Tool::Select {
            return false;
        }
        if input.pressed.is_some_and(|(_, p)| view.contains(p)) {
            self.focus = true;
        } else if input.pressed.is_some() && !self.editing() {
            self.focus = false;
        }
        if self.draft.as_ref().is_some_and(|d| {
            d.revision != state["revision"].as_u64().unwrap_or(0)
                || d.frame != state["frame"].as_f64().unwrap_or(0.)
                || d.selection != selected
                || d.root != state["root"].as_str().unwrap_or("")
        }) {
            self.draft = None;
            self.notice = Some(
                if locale == Locale::Zh {
                    "工程或时间已变化，绘制已取消"
                } else {
                    "Project or time changed; drawing cancelled"
                }
                .into(),
            );
        }
        let target = selected.and_then(|id| layer(state, id));
        let projection = selected.and_then(|id| layer_projection(state, id, view));
        let offset = if self.tool == Tool::Mask {
            target
                .as_ref()
                .map_or([0., 0.], |l| l.size.map(|v| v * 0.5))
        } else {
            [0., 0.]
        };
        let sampled = if self.tool == Tool::Mask {
            state["mask_layers"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v["id"].as_u64() == selected))
                .and_then(|v| v["masks"].as_array())
                .and_then(|v| v.iter().find(|v| v["id"].as_u64() == self.mask))
                .map(|v| vec![v["path"].clone()])
                .unwrap_or_default()
        } else {
            state["vector_layers"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v["id"].as_u64() == selected))
                .and_then(|v| v["paths"].as_array())
                .cloned()
                .unwrap_or_default()
        };
        if self.focus && self.parameter.is_none() {
            for (key, mods) in &input.keys {
                if mods.control && *key == Key::S {
                    self.finish(
                        self.draft.as_ref().is_some_and(|d| d.path.nodes.len() >= 3),
                        out,
                    );
                    if self.draft.is_none() {
                        out.push(Action::Save);
                    }
                }
                if mods.control && matches!(*key, Key::Z | Key::Y) {
                    if let Some(d) = self.draft.as_mut() {
                        d.path.nodes.pop();
                    } else if self.drag.is_none() {
                        out.push(Action::History(if *key == Key::Y || mods.shift {
                            1
                        } else {
                            0
                        }));
                    }
                }
            }
            if input.keys.iter().any(|(k, _)| *k == Key::Enter) {
                self.finish(
                    self.draft.as_ref().is_some_and(|d| d.path.nodes.len() >= 3),
                    out,
                );
            }
            if input
                .keys
                .iter()
                .any(|(k, _)| *k == Key::Backspace || *k == Key::Delete)
            {
                if let Some(d) = self.draft.as_mut() {
                    d.path.nodes.pop();
                } else if let (Some(l), Some((path, node))) = (target.as_ref(), self.selected) {
                    self.remove_node(l, path, node, out);
                    self.selected = None;
                }
            }
        }
        let mut hits = vec![];
        if let Some(p) = &projection {
            for path in &sampled {
                for n in path["nodes"].as_array().into_iter().flatten() {
                    if let Some(g) = geometry(n) {
                        let pid = path["id"].as_u64().unwrap_or(1);
                        let nid = n["id"].as_u64().unwrap_or(1);
                        for axis in [0, 2, 4] {
                            if axis != 0 && self.selected != Some((pid, nid)) {
                                continue;
                            }
                            let local = if axis == 0 {
                                [g[0] - offset[0], g[1] - offset[1]]
                            } else {
                                [g[0] + g[axis] - offset[0], g[1] + g[axis + 1] - offset[1]]
                            };
                            if let Some(screen) = p.screen(local) {
                                hits.push((distance(input.mouse, screen), pid, nid, axis, g));
                            }
                        }
                    }
                }
            }
        }
        if let Some((MouseButton::Left, point)) = input.pressed.filter(|(_, p)| view.contains(*p)) {
            let hit = hits
                .into_iter()
                .filter(|h| h.0 <= 7.)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let (Some((_, path, node, axis, value)), Some(l), Some(p)) = (
                hit,
                target.as_ref().filter(|l| !l.locked),
                projection.as_ref(),
            ) {
                if let Some(start) = p.local(point) {
                    let t = if self.tool == Tool::Mask {
                        Target::Mask {
                            object: l.id,
                            mask: self.mask.unwrap(),
                        }
                    } else {
                        Target::Vector { object: l.id }
                    };
                    let animated = if let Content::Vector {
                        vector:
                            VectorContent {
                                source: VectorSource::Paths { paths },
                                ..
                            },
                    } = &l.content
                    {
                        paths
                            .iter()
                            .find(|p| p.id == path)
                            .and_then(|p| p.nodes.iter().find(|n| n.id == node))
                            .is_some_and(|n| n.geometry.is_animated())
                    } else {
                        false
                    };
                    self.drag = Some(Drag {
                        target: t,
                        path,
                        node,
                        axis,
                        value,
                        last_geometry: value,
                        start,
                        projection: p.clone(),
                        offset,
                        animated,
                        frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        begun: false,
                    });
                    self.selected = Some((path, node));
                }
            } else {
                if target.as_ref().is_some_and(|l| {
                    matches!(
                        &l.content,
                        Content::Vector {
                            vector: VectorContent {
                                source: VectorSource::Shape { .. },
                                ..
                            }
                        }
                    )
                }) && self.tool == Tool::Pen
                {
                    out.push(Action::Edit(Command::Vector {
                        object: target.as_ref().unwrap().id,
                        action: VectorAction::ConvertToPath {
                            frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        },
                    }));
                } else {
                    if self.draft.is_none() {
                        let _ = self.begin(state, selected, view);
                        if self.draft.is_some() {
                            out.push(Action::Seek(state["frame"].as_f64().unwrap_or(0.)));
                        }
                    }
                    if let Some(d) = self.draft.as_mut() {
                        if let Some(local) = d.projection.local(point) {
                            let closes = d.path.nodes.len() >= 3
                                && d.path
                                    .nodes
                                    .first()
                                    .and_then(|n| {
                                        d.projection.screen([
                                            n.geometry.value[0] - d.offset[0],
                                            n.geometry.value[1] - d.offset[1],
                                        ])
                                    })
                                    .is_some_and(|p| distance(p, point) < 8.);
                            if closes {
                                self.finish(true, out);
                            } else if d.path.nodes.len() < 2048 {
                                let id = d.path.nodes.last().map_or(1, |n| n.id + 1);
                                d.path.nodes.push(PathNode {
                                    id,
                                    geometry: Track::constant([
                                        local[0] + d.offset[0],
                                        local[1] + d.offset[1],
                                        0.,
                                        0.,
                                        0.,
                                        0.,
                                    ]),
                                });
                            }
                        }
                    } else {
                        self.notice=Some(if locale==Locale::Zh {"请选中可编辑的可视图层；调整图层暂不支持源蒙版"}else{"Select an editable visual layer; source masks exclude adjustment layers"}.into());
                    }
                }
            }
        }
        if let Some(mut drag) = self.drag.take() {
            let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
            if let Some(p) = drag.projection.local(input.mouse) {
                if (input.dragging() || release) && (distance(p, drag.start) > 0.1 || drag.begun) {
                    if !drag.begun {
                        out.push(Action::History(2));
                        drag.begun = true;
                    }
                    let mut g = drag.value;
                    if drag.axis == 0 {
                        g[0] += p[0] - drag.start[0];
                        g[1] += p[1] - drag.start[1];
                    } else {
                        g[drag.axis] = p[0] + drag.offset[0] - g[0];
                        g[drag.axis + 1] = p[1] + drag.offset[1] - g[1];
                        if !input.modifiers.alt {
                            let other = if drag.axis == 2 { 4 } else { 2 };
                            g[other] = -g[drag.axis];
                            g[other + 1] = -g[drag.axis + 1];
                        }
                    }
                    if g!=drag.last_geometry && g.iter().all(|v| v.is_finite() && v.abs() <= 32768.) {
                        drag.last_geometry=g;
                        out.push(Action::Edit(match drag.target {
                            Target::Vector { object } => Command::Vector {
                                object,
                                action: VectorAction::SetNode {
                                    path: drag.path,
                                    node: drag.node,
                                    frame: drag.frame,
                                    value: g,
                                    animated: drag.animated,
                                },
                            },
                            Target::Mask { object, mask } => Command::Mask {
                                object,
                                action: MaskAction::Set {
                                    mask,
                                    property: MaskProperty::Node(drag.node),
                                    frame: drag.frame,
                                    value: g.to_vec(),
                                    animated: None,
                                },
                            },
                            Target::New => unreachable!(),
                        }));
                    }
                }
            }
            if release {
                if drag.begun {
                    out.push(Action::History(3));
                }
            } else {
                self.drag = Some(drag);
            }
        }
        if let Some(d) = self.draft.as_mut() {
            if input.dragging() || input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
                if let (Some(local), Some(n)) =
                    (d.projection.local(input.mouse), d.path.nodes.last_mut())
                {
                    let g = &mut n.geometry.value;
                    let dx = local[0] + d.offset[0] - g[0];
                    let dy = local[1] + d.offset[1] - g[1];
                    if dx.abs() <= 32768. && dy.abs() <= 32768. {
                        g[2] = -dx;
                        g[3] = -dy;
                        g[4] = dx;
                        g[5] = dy;
                    }
                }
            }
        }
        if let Some(p) = projection {
            for path in &sampled {
                let nodes: Vec<_> = path["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(geometry)
                    .collect();
                paint_path(
                    list,
                    &p,
                    &nodes,
                    path["closed"].as_bool().unwrap_or(false),
                    offset,
                    self.selected.map(|v| v.1),
                    palette::ACCENT,
                );
            }
        }
        if let Some(d) = &self.draft {
            let nodes: Vec<_> = d.path.nodes.iter().map(|n| n.geometry.value).collect();
            paint_path(
                list,
                &d.projection,
                &nodes,
                false,
                d.offset,
                None,
                Color::rgb(255, 188, 92),
            );
        }
        label(
            list,
            atlas,
            Rect::new(
                view.min[0] + 10.,
                view.min[1] + 8.,
                (view.width() - 20.).max(1.),
                22.,
            ),
            self.notice.as_deref().unwrap_or(locale.t(T::DrawingHint)),
            palette::TEXT_MUTED,
        );
        view.contains(input.mouse) || self.editing()
    }
    fn remove_node(&self, l: &Layer, path: u64, node: u64, out: &mut Vec<Action>) {
        if l.locked {
            return;
        }
        if self.tool == Tool::Mask {
            if let Some(m) = l.masks.iter().find(|m| Some(m.id) == self.mask) {
                let mut p = m.path.clone();
                p.nodes.retain(|n| n.id != node);
                p.closed &= p.nodes.len() >= 3;
                out.push(Action::Edit(Command::Mask {
                    object: l.id,
                    action: MaskAction::Path {
                        mask: m.id,
                        path: p,
                    },
                }));
            }
        } else if let Content::Vector {
            vector:
                VectorContent {
                    source: VectorSource::Paths { paths },
                    ..
                },
        } = &l.content
        {
            let mut paths = paths.clone();
            if let Some(p) = paths.iter_mut().find(|p| p.id == path) {
                p.nodes.retain(|n| n.id != node);
                p.closed &= p.nodes.len() >= 3;
                out.push(Action::Edit(Command::Vector {
                    object: l.id,
                    action: VectorAction::SetPaths { paths },
                }));
            }
        }
    }
}
impl Controls {
    pub fn height(&self, l: &Layer) -> f32 {
        if self.masks_open {
            76. + l.masks.len() as f32 * 186.
        } else {
            30.
        }
    }
    pub fn inspector_section(
        &mut self,
        vector_only: bool,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        l: &Layer,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if vector_only {
            if let Content::Vector { vector } = &l.content {
                let title = if locale == Locale::Zh {
                    "矢量路径"
                } else {
                    "VECTOR PATH"
                };
                label(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 12., *y, area.width() - 24., 28.),
                    title,
                    palette::TEXT,
                );
                *y += 30.;
                if ui::button(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 12., *y, area.width() - 24., 26.),
                    if locale == Locale::Zh {
                        "钢笔编辑路径"
                    } else {
                        "Edit path with Pen"
                    },
                    input,
                    !l.locked,
                )
                .clicked
                {
                    self.choose(Tool::Pen, out);
                }
                *y += 30.;
                if matches!(vector.source, VectorSource::Shape { .. }) {
                    if ui::button(
                        list,
                        atlas,
                        Rect::new(area.min[0] + 12., *y, area.width() - 24., 26.),
                        if locale == Locale::Zh {
                            "转换为可编辑路径"
                        } else {
                            "Convert to editable path"
                        },
                        input,
                        !l.locked,
                    )
                    .clicked
                    {
                        out.push(Action::Edit(Command::Vector {
                            object: l.id,
                            action: VectorAction::ConvertToPath {
                                frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                            },
                        }));
                        self.choose(Tool::Pen, out);
                    }
                    *y += 30.;
                }
            }
        }
        if vector_only {
            return;
        }
        let title = Rect::new(area.min[0] + 12., *y, area.width() - 24., 28.);
        paint::rect(list, title, palette::CHROME);
        label(
            list,
            atlas,
            title.shrink(6.),
            &format!(
                "{} {} ({})",
                if self.masks_open { "⌄" } else { "›" },
                locale.t(T::Mask),
                l.masks.len()
            ),
            palette::TEXT,
        );
        if click(input, title) {
            self.masks_open = !self.masks_open;
        }
        *y += 34.;
        if !self.masks_open {
            return;
        }
        let allowed = !l.locked
            && !matches!(
                l.content,
                Content::Adjustment | Content::Null | Content::Audio { .. }
            );
        let half = (area.width() - 30.) * 0.5;
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + 12., *y, half, 26.),
            if locale == Locale::Zh {
                "钢笔蒙版"
            } else {
                "Pen Mask"
            },
            input,
            allowed,
        )
        .clicked
        {
            self.choose(Tool::Mask, out);
        }
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + 18. + half, *y, half, 26.),
            if locale == Locale::Zh {
                "矩形蒙版"
            } else {
                "Rect Mask"
            },
            input,
            allowed,
        )
        .clicked
        {
            let id = l.masks.iter().map(|m| m.id).max().unwrap_or(0) + 1;
            let [w, h] = l.size;
            let points = [
                [w * 0.1, h * 0.1],
                [w * 0.9, h * 0.1],
                [w * 0.9, h * 0.9],
                [w * 0.1, h * 0.9],
            ];
            let path = VectorPath {
                id: 1,
                closed: true,
                nodes: points
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| PathNode {
                        id: i as u64 + 1,
                        geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                    })
                    .collect(),
            };
            let mut mask = LayerMask::new(id, path);
            mask.name = format!("{} {id}", locale.t(T::Mask));
            out.push(Action::Edit(Command::Mask {
                object: l.id,
                action: MaskAction::Add { mask },
            }));
            self.tool = Tool::Mask;
            self.mask = Some(id);
        }
        *y += 36.;
        let local = state["frame"].as_f64().unwrap_or(0.)
            - f64::from(
                l.clip(state["project"]["frames"].as_u64().unwrap_or(1) as u32)
                    .offset_frame,
            );
        for mask in &l.masks {
            let heading = Rect::new(area.min[0] + 12., *y, area.width() - 24., 26.);
            paint::rect(list, heading, palette::SUNKEN);
            label(
                list,
                atlas,
                Rect::new(heading.min[0] + 5., *y, heading.width() - 65., 26.),
                &mask.name,
                palette::TEXT,
            );
            if ui::button(
                list,
                atlas,
                Rect::new(heading.max[0] - 56., *y, 26., 24.),
                "✎",
                input,
                allowed,
            )
            .clicked
            {
                if self.cancel() {
                    out.push(Action::History(4));
                }
                self.tool = Tool::Mask;
                self.mask = Some(mask.id);
            }
            if ui::button(
                list,
                atlas,
                Rect::new(heading.max[0] - 27., *y, 25., 24.),
                "×",
                input,
                allowed,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Mask {
                    object: l.id,
                    action: MaskAction::Remove { mask: mask.id },
                }));
                if self.mask == Some(mask.id) {
                    self.mask = None;
                }
            }
            *y += 30.;
            let modes = [
                MaskMode::None,
                MaskMode::Add,
                MaskMode::Subtract,
                MaskMode::Intersect,
                MaskMode::Lighten,
                MaskMode::Darken,
                MaskMode::Difference,
            ];
            let index = modes.iter().position(|m| *m == mask.mode).unwrap_or(1);
            let names = if locale == Locale::Zh {
                ["无", "相加", "相减", "交集", "变亮", "变暗", "差值"]
            } else {
                [
                    "None",
                    "Add",
                    "Subtract",
                    "Intersect",
                    "Lighten",
                    "Darken",
                    "Difference",
                ]
            };
            let rect = Rect::new(area.min[0] + 12., *y, half, 24.);
            if ui::button(list, atlas, rect, names[index], input, allowed).clicked {
                out.push(Action::Edit(Command::Mask {
                    object: l.id,
                    action: MaskAction::Options {
                        mask: mask.id,
                        mode: Some(modes[(index + 1) % modes.len()]),
                        inverted: None,
                        enabled: None,
                    },
                }));
            }
            if ui::button(
                list,
                atlas,
                Rect::new(rect.max[0] + 6., *y, half, 24.),
                if mask.inverted {
                    if locale == Locale::Zh {
                        "反相 ✓"
                    } else {
                        "Invert ✓"
                    }
                } else {
                    if locale == Locale::Zh {
                        "反相"
                    } else {
                        "Invert"
                    }
                },
                input,
                allowed,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Mask {
                    object: l.id,
                    action: MaskAction::Options {
                        mask: mask.id,
                        mode: None,
                        inverted: Some(!mask.inverted),
                        enabled: None,
                    },
                }));
            }
            *y += 28.;
            for (name, property, values, range) in [
                (
                    if locale == Locale::Zh {
                        "不透明度"
                    } else {
                        "Opacity"
                    },
                    MaskProperty::Opacity,
                    vec![mask.opacity.sample(local)],
                    [0., 100.],
                ),
                (
                    if locale == Locale::Zh {
                        "羽化 X / Y"
                    } else {
                        "Feather X / Y"
                    },
                    MaskProperty::Feather,
                    mask.feather.sample(local).to_vec(),
                    [0., 32000.],
                ),
                (
                    if locale == Locale::Zh {
                        "扩展"
                    } else {
                        "Expansion"
                    },
                    MaskProperty::Expansion,
                    vec![mask.expansion.sample(local)],
                    [-32000., 32000.],
                ),
            ] {
                label(
                    list,
                    atlas,
                    Rect::new(area.min[0] + 18., *y, area.width() * 0.42, 24.),
                    name,
                    palette::TEXT_MUTED,
                );
                let width = (area.width() * 0.48 - 12.) / values.len() as f32;
                for (axis, value) in values.iter().enumerate() {
                    let r = Rect::new(
                        area.min[0] + area.width() * 0.5 + axis as f32 * width,
                        *y,
                        width - 4.,
                        24.,
                    );
                    let active = self.parameter.as_ref().is_some_and(|p| {
                        p.mask == mask.id
                            && p.axis == axis
                            && std::mem::discriminant(&p.property)
                                == std::mem::discriminant(&property)
                    });
                    paint::rect(list, r, palette::SUNKEN);
                    let text = if active {
                        self.parameter.as_ref().unwrap().text.clone()
                    } else {
                        format!("{value:.1}")
                    };
                    label(list, atlas, r.shrink(4.), &text, palette::ACCENT);
                    if click(input, r) && allowed && !active {
                        self.parameter = Some(Parameter {
                            object: l.id,
                            mask: mask.id,
                            property: property.clone(),
                            axis,
                            values: values.clone(),
                            text: format!("{value:.1}"),
                            replace: true,
                            rect: r,
                            start: Some(input.pressed.unwrap().1[0]),
                            begun: false,
                            last_value: *value,
                            range,
                        });
                        out.push(Action::Seek(state["frame"].as_f64().unwrap_or(0.)));
                    }
                }
                *y += 28.;
            }
            if ui::button(
                list,
                atlas,
                Rect::new(area.min[0] + 12., *y, area.width() - 24., 22.),
                if mask.enabled {
                    if locale == Locale::Zh {
                        "启用蒙版 ✓"
                    } else {
                        "Mask enabled ✓"
                    }
                } else {
                    if locale == Locale::Zh {
                        "蒙版已禁用"
                    } else {
                        "Mask disabled"
                    }
                },
                input,
                allowed,
            )
            .clicked
            {
                out.push(Action::Edit(Command::Mask {
                    object: l.id,
                    action: MaskAction::Options {
                        mask: mask.id,
                        mode: None,
                        inverted: None,
                        enabled: Some(!mask.enabled),
                    },
                }));
            }
            *y += 32.;
        }
    }
    pub fn parameter_input(&mut self, state: &Value, input: &Input, out: &mut Vec<Action>) {
        let Some(mut p) = self.parameter.take() else {
            return;
        };
        let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
        let command = |p: &Parameter, value: f32| {
            let mut v = p.values.clone();
            v[p.axis] = value;
            Action::Edit(Command::Mask {
                object: p.object,
                action: MaskAction::Set {
                    mask: p.mask,
                    property: p.property.clone(),
                    frame,
                    value: v,
                    animated: None,
                },
            })
        };
        if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            if p.begun {
                out.push(Action::History(4));
            }
            return;
        }
        let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
        let outside = input
            .pressed
            .is_some_and(|(_, point)| !p.rect.contains(point));
        if let Some(start) = p.start {
            let delta = input.mouse[0] - start;
            if (input.dragging() || release) && (delta.abs().max(input.pointer_travel) >= 3. || p.begun) {
                if !p.begun {
                    out.push(Action::History(2));
                    p.begun = true;
                }
                let step = if input.modifiers.shift { 0.1 } else { 1. };
                let value = (p.values[p.axis] + delta * step).clamp(p.range[0], p.range[1]);
                p.text = format!("{value:.1}");
                if value!=p.last_value {p.last_value=value;out.push(command(&p, value));}
            }
            if release || outside {
                if p.begun {
                    out.push(Action::History(3));
                    return;
                }
                p.start = None;
            }
        }
        if p.start.is_none() {
            if !input.text.is_empty() {
                if p.replace {
                    p.text.clear();
                    p.replace = false;
                }
                p.text.push_str(&input.text);
            }
            if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
                if p.replace {
                    p.text.clear();
                    p.replace = false;
                } else {
                    p.text.pop();
                }
            }
            if outside || input.keys.iter().any(|(k, _)| *k == Key::Enter) {
                if p.text.trim().is_empty() {
                    return;
                }
                if let Ok(v) = p.text.parse::<f32>() {
                    if v.is_finite() {
                        out.push(command(&p, v.clamp(p.range[0], p.range[1])));
                        return;
                    }
                }
            }
        }
        self.parameter = Some(p);
    }
}
fn paint_path(
    list: &mut PaintList,
    projection: &Projection,
    nodes: &[[f32; 6]],
    closed: bool,
    offset: [f32; 2],
    selected: Option<u64>,
    color: Color,
) {
    let edges = nodes.len().saturating_sub(1) + usize::from(closed && nodes.len() > 2);
    for i in 0..edges {
        let a = nodes[i];
        let b = nodes[(i + 1) % nodes.len()];
        let mut points = vec![];
        for step in 0..=20 {
            let t = step as f32 / 20.;
            let u = 1. - t;
            let xy = std::array::from_fn(|axis| {
                u * u * u * a[axis]
                    + 3. * u * u * t * (a[axis] + a[axis + 4])
                    + 3. * u * t * t * (b[axis] + b[axis + 2])
                    + t * t * t * b[axis]
                    - offset[axis]
            });
            if let Some(p) = projection.screen(xy) {
                points.push(p);
            }
        }
        paint::line(list, points, color);
    }
    for (i, g) in nodes.iter().enumerate() {
        let Some(p) = projection.screen([g[0] - offset[0], g[1] - offset[1]]) else {
            continue;
        };
        paint::rect(list, Rect::new(p[0] - 3., p[1] - 3., 6., 6.), color);
        if selected == Some(i as u64 + 1) || selected.is_none() {
            for axis in [2, 4] {
                if g[axis].abs() + g[axis + 1].abs() < 0.01 {
                    continue;
                }
                if let Some(h) =
                    projection.screen([g[0] + g[axis] - offset[0], g[1] + g[axis + 1] - offset[1]])
                {
                    paint::line(list, vec![p, h], color.with_alpha(0.7));
                    paint::diamond(list, h, 3., color);
                }
            }
        }
    }
}
