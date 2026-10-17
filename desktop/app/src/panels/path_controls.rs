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
    theme::Color,
    ui::{self, Align},
};
use serde_json::Value;
pub(in crate::panels) mod groups;
mod mask_inspector;
mod mask_keys;
mod mask_path;
pub(in crate::panels) mod timeline;

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
    parameter_context: Option<(String, String, u32)>,
    inspector_context: Option<(String, String, u64, motion_ui::dock::Panel)>,
    inspector_cache: Option<InspectorLayer>,
    inspector_add: Option<(motion_ui::dock::Panel, Rect)>,
    add_mask_menu: motion_ui::widgets::Dropdown,
    mode_dropdowns: std::collections::HashMap<(u64, u64), motion_ui::widgets::Dropdown>,
    collapsed_mask_cards: std::collections::HashSet<(u64, u64)>,
    pub masks_open: bool,
    keys: mask_keys::Controls,
    pub(super) groups: groups::Controls,
}
struct InspectorLayer {
    context: (String, String, u64, u64),
    layer: std::sync::Arc<Layer>,
}
struct Parameter {
    host: motion_ui::dock::Panel,
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
    observe!("path.layer", serde_json::from_value(
        state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|v| v["id"].as_u64() == Some(id))?
            .clone(),
    ), "object={id} fallback=no_layer")
    .ok()
}
fn icons_pen_button(
    list: &mut PaintList,
    rect: Rect,
    input: &Input,
    enabled: bool,
    active: bool,
) -> bool {
    let styles = list.styles();
    let mut states = motion_ui::style::States::default();
    if !enabled {
        states = states.with(motion_ui::style::State::Disabled);
    }
    if active {
        states = states.with(motion_ui::style::State::Selected);
    }
    motion_ui::icons::button(
        list,
        rect,
        motion_ui::icons::Icon::Pen,
        input,
        styles.as_ref(),
        states,
    )
    .clicked
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
    pub(super) fn mask_key_shortcuts(
        &mut self,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        self.groups.keys.shortcuts(state, selected, input, out)
            || self.keys.delete_selected(state, selected, input, out)
            || self.keys.navigate(state, selected, input, out)
    }
    pub fn editing(&self) -> bool {
        self.draft.is_some()
            || self.drag.is_some()
            || self.parameter.is_some()
            || self.keys.editing()
            || self.groups.editing()
            || self.add_mask_menu.open
            || self.mode_dropdowns.values().any(|d| d.open)
    }
    pub(crate) fn gesture_active(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.begun)
            || self.parameter.as_ref().is_some_and(|p| p.begun)
    }
    pub fn wants_keyboard(&self) -> bool {
        self.editing() || (self.tool != Tool::Select && self.focus)
    }
    pub fn cancel(&mut self) -> bool {
        let drag = self.drag.take().is_some_and(|d| d.begun);
        let param = self.parameter.take().is_some_and(|p| p.begun);
        self.draft = None;
        self.parameter_context = None;
        self.focus = false;
        self.add_mask_menu.open = false;
        for dropdown in self.mode_dropdowns.values_mut() {
            dropdown.open = false;
        }
        let keys = self.keys.cancel();
        let group = self.groups.cancel();
        drag || param || keys || group
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
            let selected = observe!(
                "path.begin_selection",
                selected.ok_or("no selected layer"),
                "tool=mask"
            )
            .ok()?;
            let l = observe!(
                "path.begin_layer",
                layer(state, selected).ok_or("selected layer missing or malformed"),
                "object={selected} tool=mask"
            )
            .ok()?;
            if l.locked
                || matches!(
                    l.content,
                    Content::Null | Content::Audio { .. } | Content::Adjustment
                )
            {
                log::debug!(
                    "path begin refused object={} reason=locked_or_nonvisual_layer",
                    l.id
                );
                return None;
            }
            let mask = l
                .masks
                .iter()
                .map(|m| m.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .or_else(|| {
                    log::error!("path begin refused reason=id_overflow selected={selected:?}");
                    None
                })?;
            target = Target::Mask { object: l.id, mask };
            offset = l.size.map(|v| v * 0.5);
            observe!(
                "path.begin_projection",
                layer_projection(state, l.id, view).ok_or("layer projection unavailable"),
                "object={} view={view:?}",
                l.id
            )
            .ok()?
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
                    .checked_add(1)
                    .or_else(|| {
                        log::error!("path begin refused reason=id_overflow selected={selected:?}");
                        None
                    })?;
                target = Target::Vector { object: l.id };
                base_paths = paths;
                observe!(
                    "path.begin_projection",
                    layer_projection(state, l.id, view).ok_or("layer projection unavailable"),
                    "object={} view={view:?}",
                    l.id
                )
                .ok()?
            } else {
                observe!(
                    "path.begin_identity",
                    identity_projection(state, view).ok_or("composition projection unavailable"),
                    "selected={selected:?} view={view:?}"
                )
                .ok()?
            }
        } else {
            observe!(
                "path.begin_identity",
                identity_projection(state, view).ok_or("composition projection unavailable"),
                "selected={selected:?} view={view:?}"
            )
            .ok()?
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
        let styles = list.styles();
        let colors = styles.colors();

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
                        if self.begin(state, selected, view).is_none() {
                            log::debug!("path begin produced no draft selected={selected:?} tool={:?} revision={}", self.tool, state["revision"]);
                        }
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
            if drag.begun {
                let _ = observe!(
                    "path.drag_release",
                    (input.dragging()
                        || input.released.is_some_and(|(b, _)| b == MouseButton::Left))
                    .then_some(())
                    .ok_or("path gesture retained without held button or release"),
                    "path={} node={} frame={} phase=dragging",
                    drag.path,
                    drag.node,
                    drag.frame
                );
            }
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
                    if g != drag.last_geometry
                        && g.iter().all(|v| v.is_finite() && v.abs() <= 32768.)
                    {
                        let edit = match drag.target {
                            Target::Vector { object } => Some(Action::Edit(Command::Vector {
                                object,
                                action: VectorAction::SetNode {
                                    path: drag.path,
                                    node: drag.node,
                                    frame: drag.frame,
                                    value: g,
                                    animated: drag.animated,
                                },
                            })),
                            Target::Mask { object, mask } => target.as_ref().and_then(|layer| {
                                let source = layer.masks.iter().find(|m| m.id == mask)?;
                                mask_path::node_command(
                                    object,
                                    source,
                                    layer.timeline.map_or(0, |t| t.offset_frame),
                                    drag.node,
                                    drag.frame,
                                    g,
                                )
                            }),
                            Target::New => unreachable!(),
                        };
                        if let Some(edit) = edit {
                            drag.last_geometry = g;
                            out.push(edit);
                        }
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
                    colors.ACCENT,
                );
            }
        }
        if let Some(d) = &self.draft {
            let draft_color=list.styles().color("path.draft");
            let nodes: Vec<_> = d.path.nodes.iter().map(|n| n.geometry.value).collect();
            paint_path(
                list,
                &d.projection,
                &nodes,
                false,
                d.offset,
                None,
                draft_color,
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
            colors.TEXT_MUTED,
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
    pub fn height(&self, l: &Layer, styles: &motion_ui::style::StyleSheet) -> f32 {
        let row = styles.metric("row.height", 22.);
        let gap = styles.metric("padding", 5.);
        if !self.masks_open {
            row
        } else {
            row + l
                .masks
                .iter()
                .map(|m| {
                    if self.collapsed_mask_cards.contains(&(l.id, m.id)) {
                        row + gap
                    } else {
                        row * 6. + gap
                    }
                })
                .sum::<f32>()
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
        let styles = list.styles();
        let colors = styles.colors();

        if vector_only {
            self.groups.render(
                list,
                atlas,
                area,
                y,
                l,
                state,
                input,
                locale,
                motion_ui::dock::Panel::Vector,
                out,
            );
            if let Content::Vector { vector } = &l.content {
                let row = styles.metric("row.height", 22.);
                let pad = styles.metric("padding.panel", 8.);
                let slot = styles.metric("icon.slot", 20.);
                if !matches!(vector.source, VectorSource::Group { .. }) {
                    let neutral = Input::default();
                    let action_input = if self.groups.menu_in(motion_ui::dock::Panel::Vector) {
                        &neutral
                    } else {
                        input
                    };
                    let fields = mask_inspector::property_row(
                        styles.as_ref(),
                        area,
                        *y,
                        0,
                        1,
                        motion_ui::dock::Panel::Properties,
                    );
                    label(
                        list,
                        atlas,
                        fields.caption,
                        if locale == Locale::Zh {
                            "路径编辑"
                        } else {
                            "Path editing"
                        },
                        colors.TEXT_MUTED,
                    );
                    let pen = Rect::new(fields.number.min[0], *y, slot, row);
                    if icons_pen_button(list, pen, action_input, !l.locked, self.tool == Tool::Pen)
                    {
                        self.choose(Tool::Pen, out);
                    }
                    if matches!(vector.source, VectorSource::Shape { .. }) {
                        let text = if locale == Locale::Zh {
                            "转换为路径"
                        } else {
                            "Convert to path"
                        };
                        let x = pen.max[0] + pad;
                        let rect = Rect::new(
                            x,
                            *y,
                            (atlas.measure(text) + pad * 2.).min((area.max[0] - pad - x).max(0.)),
                            row,
                        );
                        if ui::button_styled(
                            list,
                            atlas,
                            rect,
                            text,
                            action_input,
                            !l.locked,
                            &styles,
                            "button.icon",
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
                    }
                    *y += row;
                }
                self.groups.finish_paint(
                    list,
                    atlas,
                    area,
                    state,
                    input,
                    locale,
                    motion_ui::dock::Panel::Vector,
                    out,
                );
            }
        }
        if vector_only {
            return;
        }
        self.compact_masks(
            list,
            atlas,
            area,
            y,
            l,
            state,
            input,
            locale,
            motion_ui::dock::Panel::Masks,
            out,
        );
    }
    pub(super) fn inspector_layer(
        &mut self,
        state: &Value,
        value: &Value,
    ) -> Option<std::sync::Arc<Layer>> {
        let revision = state["revision"].as_u64();
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            value["id"].as_u64()?,
            revision.unwrap_or(0),
        );
        if revision.is_some() {
            if let Some(cached) = &self.inspector_cache {
                if cached.context == context {
                    return Some(cached.layer.clone());
                }
            }
        }
        let layer = std::sync::Arc::new(observe!("path.inspector_layer", serde_json::from_value::<Layer>(value.clone()), "object={} revision={:?} fallback=no_layer", context.2, revision).ok()?);
        self.inspector_cache = Some(InspectorLayer {
            context,
            layer: layer.clone(),
        });
        Some(layer)
    }
    pub(super) fn masks_in_properties(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        self.compact_masks(
            list,
            atlas,
            area,
            y,
            layer,
            state,
            input,
            locale,
            motion_ui::dock::Panel::Properties,
            out,
        );
    }
    pub(super) fn inspector_menu_in(&self, host: motion_ui::dock::Panel) -> bool {
        self.inspector_context
            .as_ref()
            .is_some_and(|context| context.3 == host)
            && (self.add_mask_menu.open || self.mode_dropdowns.values().any(|d| d.open))
            || self.groups.menu_in(host)
    }
    pub(crate) fn mask_add_rect(&self, host: motion_ui::dock::Panel) -> Option<Rect> {
        self.inspector_add
            .filter(|(owner, _)| *owner == host)
            .map(|(_, rect)| rect)
    }
    pub(crate) fn vector_add_rect(&self, host: motion_ui::dock::Panel) -> Option<Rect> {
        self.groups
            .add_rect
            .filter(|(owner, _)| *owner == host)
            .map(|(_, r)| r)
    }
    pub(crate) fn reveal_vector_item(&mut self, state: &Value, object: u64, item: u64) {
        if let Some(l) = layer(state, object) {
            if let Content::Vector { vector } = &l.content {
                let root = if let VectorSource::Group { group } = &vector.source {
                    group.id
                } else {
                    1
                };
                self.groups.reveal_item(object, root, item);
            }
        }
    }
    pub(super) fn clear_mask_selection(&mut self) {
        self.keys.clear_selection();
        self.groups.keys.clear_selection();
    }
    pub(super) fn numeric_owner(&self) -> Option<motion_ui::dock::Panel> {
        self.parameter.as_ref().map(|p| p.host)
    }
    pub(super) fn mask_menu(&self) -> bool {
        self.keys.modal()
            || self.groups.menu_in(motion_ui::dock::Panel::Timeline)
            || self.groups.keys.modal()
    }
    pub(super) fn mask_overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        self.keys.overlay(list, atlas, area, input, locale, out);
        self.groups.append_overlays(list);
        self.groups
            .keys
            .overlay(list, atlas, area, input, locale, out);
        self.groups.overlay(
            list,
            atlas,
            area,
            state,
            input,
            locale,
            motion_ui::dock::Panel::Timeline,
            out,
        );
    }
    pub fn numeric_cursor(&self) -> Option<(Rect, bool)> {
        self.parameter.as_ref().map(|p| (p.rect, p.begun))
    }
    pub fn parameter_input(
        &mut self,
        state: &Value,
        input: &Input,
        out: &mut Vec<Action>,
        drag_threshold: f32,
    ) {
        let Some(mut p) = self.parameter.take() else {
            return;
        };
        let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
        let context = self.parameter_context.take();
        let stale = context.as_ref().is_some_and(|(root, composition, at)| {
            root != state["root"].as_str().unwrap_or("")
                || composition != state["composition"].as_str().unwrap_or("")
                || *at != frame
        }) || state["project"]["layers"].as_array().is_some_and(|layers| {
            !layers.iter().any(|l| {
                l["id"].as_u64() == Some(p.object)
                    && l["locked"] != true
                    && l["masks"]
                        .as_array()
                        .is_some_and(|masks| masks.iter().any(|m| m["id"].as_u64() == Some(p.mask)))
            })
        });
        if stale {
            if p.begun {
                out.push(Action::History(4));
            }
            return;
        }
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
        if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            if p.begun {
                out.push(Action::History(4));
            }
            return;
        }
        if p.start.is_none()
            && !input.composing
            && input
                .pressed
                .is_some_and(|(b, point)| b == MouseButton::Left && p.rect.contains(point))
        {
            if let Ok(value) = p.text.parse::<f32>() {
                if value.is_finite() {
                    p.values[p.axis] = value.clamp(p.range[0], p.range[1]);
                }
            }
            p.start = Some(input.pressed.unwrap().1[0]);
            p.last_value = p.values[p.axis];
        }
        let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
        let outside = input
            .pressed
            .is_some_and(|(_, point)| !p.rect.contains(point));
        if let Some(start) = p.start {
            let delta = input.mouse[0] - start;
            if (input.dragging() || release)
                && (motion_ui::widgets::numeric_behavior::started(
                    delta,
                    input.pointer_travel,
                    drag_threshold,
                ) || p.begun)
            {
                if !p.begun {
                    out.push(Action::History(2));
                    p.begun = true;
                }
                let step = motion_ui::widgets::numeric_behavior::step(input) as f32;
                let value = (p.values[p.axis] + delta * step).clamp(p.range[0], p.range[1]);
                p.text = format!("{value:.1}");
                if value != p.last_value {
                    p.last_value = value;
                    out.push(command(&p, value));
                }
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
        self.parameter_context = context;
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
        paint::box_rect(
            list,
            Rect::new(p[0] - 3., p[1] - 3., 6., 6.),
            color,
            Color([0.; 4]),
            0.,
            0.,
        );
        if selected == Some(i as u64 + 1) || selected.is_none() {
            for axis in [2, 4] {
                if g[axis].abs() + g[axis + 1].abs() < 0.01 {
                    continue;
                }
                if let Some(h) =
                    projection.screen([g[0] + g[axis] - offset[0], g[1] + g[axis + 1] - offset[1]])
                {
                    let connector=color.with_alpha(list.styles().metric("path.handle.opacity",0.7));
                    paint::line(list, vec![p, h], connector);
                    paint::diamond(list, h, 3., color);
                }
            }
        }
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    #[test]
    fn mask_scrub_obeys_shared_threshold_and_commit_or_cancel_restores_engine_bytes() {
        for threshold in [0., 12.] {
            for canceled in [false, true] {
                let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
                let mut layer = motion_core::Layer::solid(
                    7,
                    "Mask numeric",
                    [100., 100.],
                    [0., 0., 0.],
                    [1.; 4],
                );
                let path = VectorPath {
                    id: 1,
                    closed: true,
                    nodes: [[0., 0.], [100., 0.], [0., 100.]]
                        .into_iter()
                        .enumerate()
                        .map(|(i, p)| PathNode {
                            id: i as u64 + 1,
                            geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                        })
                        .collect(),
                };
                let mut mask = LayerMask::new(1, path);
                mask.opacity = Track::constant(50.);
                layer.masks.push(mask);
                project.layers.push(layer);
                let mut engine = motion_core::Engine::new(project).unwrap();
                let before = serde_json::to_vec(engine.project()).unwrap();
                let mut controls = Controls::default();
                controls.parameter = Some(Parameter {
                    host: motion_ui::dock::Panel::Masks,
                    object: 7,
                    mask: 1,
                    property: MaskProperty::Opacity,
                    axis: 0,
                    values: vec![50.],
                    text: "50".into(),
                    replace: true,
                    rect: Rect::new(90., 0., 120., 30.),
                    start: Some(100.),
                    begun: false,
                    last_value: 50.,
                    range: [0., 100.],
                });
                let state = serde_json::json!({"frame":0});
                let mut actions = vec![];
                controls.parameter_input(
                    &state,
                    &Input {
                        mouse: [105., 15.],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                    threshold,
                );
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, Action::History(2)))
                        .count(),
                    usize::from(threshold == 0.)
                );
                controls.parameter_input(
                    &state,
                    &Input {
                        mouse: [120., 15.],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    &mut actions,
                    threshold,
                );
                assert!(controls.numeric_cursor().unwrap().1);
                controls.parameter_input(
                    &state,
                    &Input {
                        mouse: [100., 15.],
                        held: Some(MouseButton::Left),
                        pointer_travel: 20.,
                        ..Default::default()
                    },
                    &mut actions,
                    threshold,
                );
                let end = if canceled {
                    Input {
                        keys: vec![(Key::Escape, Default::default())],
                        ..Default::default()
                    }
                } else {
                    Input {
                        mouse: [120., 15.],
                        released: Some((MouseButton::Left, [120., 15.])),
                        ..Default::default()
                    }
                };
                controls.parameter_input(&state, &end, &mut actions, threshold);
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, Action::History(2)))
                        .count(),
                    1
                );
                for action in actions {
                    match action {
                        Action::History(2) => engine.begin_gesture().unwrap(),
                        Action::History(3) => engine.end_gesture(true).unwrap(),
                        Action::History(4) => engine.end_gesture(false).unwrap(),
                        Action::Edit(command) => {
                            engine.apply(command).unwrap();
                        }
                        _ => panic!("unexpected mask action"),
                    }
                }
                if !canceled {
                    assert_ne!(serde_json::to_vec(engine.project()).unwrap(), before);
                    engine.undo().unwrap();
                }
                assert_eq!(serde_json::to_vec(engine.project()).unwrap(), before);
                assert!(controls.numeric_cursor().is_none());
            }
        }
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    #[test]
    fn malformed_layers_keep_path_fallback_and_emit_bounded_evidence() {
        const CHILD: &str = "MOTION_PATH_LAYER_FALLBACK_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let value = serde_json::json!({"id":7,"content":17});
            let state = serde_json::json!({"revision":1,"project":{"layers":[value.clone()]}});
            let mut controls = Controls::default();
            for _ in 0..120 {
                assert!(layer(&state,7).is_none());
                assert!(controls.inspector_layer(&state,&value).is_none());
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            for site in ["path.layer","path.inspector_layer"] {
                let records = text.lines().filter(|l| l.contains(&format!("site={site} "))).collect::<Vec<_>>();
                assert_eq!(records.len(),2,"{site}: {text}");
                assert!(records[1].contains("ERROR") && records[1].contains("object=7"));
            }
            assert!(controls.inspector_cache.is_none());
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "panels::path_controls::fallback_tests::malformed_layers_keep_path_fallback_and_emit_bounded_evidence"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("site=path.layer"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("panel degradation"));
    }
}
