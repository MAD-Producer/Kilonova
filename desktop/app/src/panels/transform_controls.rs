//! Viewport gestures use evaluated source geometry and the shared edit commands.
use super::{
    path_controls::{layer_projection, Projection},
    Action,
};
use motion_core::{Axis, Command, Property};
use motion_ui::{
    paint::{self, PaintList},
    Input, Key, MouseButton, Rect,
};
use serde_json::Value;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Move,
    Rotate,
    Scale,
    Anchor,
}
#[derive(Clone, Copy)]
enum Handle {
    Move,
    Rotate,
    Scale(bool, bool),
    Anchor,
}
#[derive(Clone)]
struct Pose {
    object: u64,
    projection: Projection,
    size: [f32; 2],
    anchor: [f32; 2],
    scale: [f32; 3],
    rotation: [f32; 3],
    three_d: bool,
    locked: bool,
    corners: [[f32; 2]; 4],
    pivot: [f32; 2],
    rotate: [f32; 2],
}
fn array<const N: usize>(v: &Value) -> Option<[f32; N]> {
    let a = v.as_array()?;
    if a.len() != N {
        return None;
    }
    let mut out = [0.; N];
    for (x, v) in out.iter_mut().zip(a) {
        *x = v.as_f64()? as f32;
        if !x.is_finite() {
            return None;
        }
    }
    Some(out)
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn mid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
impl Pose {
    fn new(state: &Value, object: u64, view: Rect) -> Option<Self> {
        let l = state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|l| l["id"].as_u64() == Some(object))?;
        if l["visible"] == false || l["content"]["kind"] == "audio" {
            return None;
        }
        let s = state["sampledLayers"]
            .as_array()?
            .iter()
            .find(|l| l["id"].as_u64() == Some(object))?;
        if s["active"] == false {
            return None;
        }
        let size = array::<2>(&l["size"])?;
        if size.iter().any(|s| *s <= 0.) {
            return None;
        }
        let anchor = array::<2>(&l["transform"]["anchor"])?;
        let projection = layer_projection(state, object, view)?;
        let w = size[0] * 0.5;
        let h = size[1] * 0.5;
        let mut corners = [[0.; 2]; 4];
        for (p, local) in corners.iter_mut().zip([[-w, -h], [w, -h], [w, h], [-w, h]]) {
            *p = projection.screen(local)?;
        }
        let pivot =
            projection.screen([(anchor[0] - 0.5) * size[0], (anchor[1] - 0.5) * size[1]])?;
        let top = mid(corners[0], corners[1]);
        let len = distance(top, pivot).max(1.);
        let rotate = [
            top[0] + (top[0] - pivot[0]) * 24. / len,
            top[1] + (top[1] - pivot[1]) * 24. / len,
        ];
        Some(Self {
            object,
            projection,
            size,
            anchor,
            corners,
            pivot,
            rotate,
            scale: array(&s["scale"])?,
            rotation: array(&s["rotation"])?,
            three_d: l["three_d"] == true,
            locked: l["locked"] == true,
        })
    }
    fn pivot_local(&self) -> [f32; 2] {
        [
            (self.anchor[0] - 0.5) * self.size[0],
            (self.anchor[1] - 0.5) * self.size[1],
        ]
    }
    fn handle(&self, p: [f32; 2], tool: Tool, alt: bool) -> Option<Handle> {
        if self.locked {
            return None;
        }
        if (tool == Tool::Anchor || alt) && distance(p, self.pivot) < 12. {
            return Some(Handle::Anchor);
        }
        if distance(p, self.rotate) < 9. {
            return Some(Handle::Rotate);
        }
        for point in self.corners {
            if distance(p, point) < 8. {
                return Some(Handle::Scale(true, true));
            }
        }
        for i in 0..4 {
            if distance(p, mid(self.corners[i], self.corners[(i + 1) % 4])) < 7. {
                return Some(Handle::Scale(i % 2 == 1, i % 2 == 0));
            }
        }
        None
    }
    fn paint(&self, list: &mut PaintList) {
        let styles = list.styles();
        let colors = styles.colors();

        let color = if self.locked {
            colors.TEXT_MUTED
        } else {
            colors.ACCENT
        };
        let mut outline = self.corners.to_vec();
        outline.push(self.corners[0]);
        paint::line(list, outline, color);
        for i in 0..4 {
            for p in [
                self.corners[i],
                mid(self.corners[i], self.corners[(i + 1) % 4]),
            ] {
                let r = Rect::new(p[0] - 3., p[1] - 3., 6., 6.);
                paint::rect(list, r, colors.SUNKEN);
                paint::stroke(list, r, color, 1.);
            }
        }
        let top = mid(self.corners[0], self.corners[1]);
        paint::line(list, vec![top, self.rotate], color);
        paint::diamond(list, self.rotate, 4., color);
        let [x, y] = self.pivot;
        paint::line(list, vec![[x - 7., y], [x + 7., y]], color);
        paint::line(list, vec![[x, y - 7.], [x, y + 7.]], color);
        paint::stroke(list, Rect::new(x - 3., y - 3., 6., 6.), color, 1.);
    }
}
struct Seed {
    point: [f32; 2],
    frame: u64,
    root: String,
    composition: String,
    view: Rect,
}
impl Seed {
    fn new(point: [f32; 2], state: &Value, view: Rect) -> Self {
        Self {
            point,
            frame: state["frame"].as_f64().unwrap_or(0.).to_bits(),
            root: state["root"].as_str().unwrap_or("").into(),
            composition: state["composition"].as_str().unwrap_or("").into(),
            view,
        }
    }
    fn valid(&self, state: &Value, view: Rect) -> bool {
        self.frame == state["frame"].as_f64().unwrap_or(0.).to_bits()
            && self.root == state["root"].as_str().unwrap_or("")
            && self.composition == state["composition"].as_str().unwrap_or("")
            && self.view == view
    }
}
struct Drag {
    seed: Seed,
    pose: Pose,
    handle: Handle,
    begun: bool,
    last_pointer: [f32; 2],
    last_value: [f32; 3],
    last_anchor: [f32; 2],
    local_start: [f32; 2],
    last_angle: f32,
    angle: f32,
}
impl Drag {
    fn new(seed: Seed, pose: Pose, handle: Handle) -> Option<Self> {
        let local_start = pose.projection.local(seed.point)?;
        let pivot = pose.pivot_local();
        let angle = ((local_start[1] - pivot[1]) * pose.scale[1])
            .atan2((local_start[0] - pivot[0]) * pose.scale[0]);
        let last_value = if matches!(handle, Handle::Rotate) {
            pose.rotation
        } else {
            pose.scale
        };
        Some(Self {
            last_pointer: seed.point,
            last_anchor: pose.anchor,
            seed,
            pose,
            handle,
            begun: false,
            local_start,
            last_value,
            last_angle: angle,
            angle: 0.,
        })
    }
    fn commands(&mut self, state: &Value, input: &Input) -> Vec<Action> {
        let release = input.released.is_some_and(|(b, _)| b == MouseButton::Left);
        if !input.dragging() && !release {
            return vec![];
        }
        let movement = distance(input.mouse, self.seed.point).max(input.pointer_travel);
        if movement < 3. && !self.begun {
            return vec![];
        }
        let mut out = vec![];
        if !self.begun {
            self.begun = true;
            out.push(Action::History(2));
        }
        match self.handle {
            Handle::Move => {
                let delta = [
                    input.mouse[0] - self.last_pointer[0],
                    input.mouse[1] - self.last_pointer[1],
                ];
                if delta[0].abs() + delta[1].abs() > 0.01 {
                    out.push(Action::PreviewDrag {
                        object: self.pose.object,
                        delta,
                        size: [
                            self.seed.view.width().round().max(1.) as i32,
                            self.seed.view.height().round().max(1.) as i32,
                        ],
                    });
                    self.last_pointer = input.mouse;
                }
            }
            Handle::Rotate | Handle::Scale(..) | Handle::Anchor => {
                let Some(local) = observe!(
                    "transform.drag_projection",
                    self.pose
                        .projection
                        .local(input.mouse)
                        .ok_or("projection unavailable after gesture began"),
                    "object={} phase=dragging mouse={:?}",
                    self.pose.object,
                    input.mouse
                )
                .ok() else {
                    return out;
                };
                let pivot = self.pose.pivot_local();
                match self.handle {
                    Handle::Rotate => {
                        let a = ((local[1] - pivot[1]) * self.pose.scale[1])
                            .atan2((local[0] - pivot[0]) * self.pose.scale[0]);
                        let delta = (a - self.last_angle + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                        self.angle += delta.to_degrees();
                        self.last_angle = a;
                        let mut value = self.pose.rotation;
                        value[2] += self.angle;
                        if input.modifiers.shift {
                            value[2] = (value[2] / 15.).round() * 15.;
                        }
                        out.extend(self.vector(Property::Rotation, value, state));
                    }
                    Handle::Scale(x, y) => {
                        let start = [
                            self.local_start[0] - pivot[0],
                            self.local_start[1] - pivot[1],
                        ];
                        let now = [local[0] - pivot[0], local[1] - pivot[1]];
                        let mut value = self.pose.scale;
                        if x && y && !input.modifiers.control {
                            let divisor = start[0] * start[0] + start[1] * start[1];
                            if divisor > 1e-6 {
                                let ratio = (now[0] * start[0] + now[1] * start[1]) / divisor;
                                for (i, v) in value.iter_mut().enumerate() {
                                    if i < 2 || self.pose.three_d {
                                        *v *= ratio;
                                    }
                                }
                            }
                        } else {
                            for (axis, enabled) in [(0, x), (1, y)] {
                                if enabled && start[axis].abs() > 1e-6 {
                                    value[axis] *= now[axis] / start[axis];
                                }
                            }
                        }
                        out.extend(self.vector(Property::Scale, value, state));
                    }
                    Handle::Anchor => {
                        let value = std::array::from_fn(|i| {
                            self.pose.anchor[i]
                                + (local[i] - self.local_start[i]) / self.pose.size[i]
                        });
                        if value != self.last_anchor
                            && value.iter().all(|v| v.is_finite() && v.abs() <= 1000.)
                        {
                            self.last_anchor = value;
                            out.push(Action::Edit(Command::Anchor {
                                object: self.pose.object,
                                anchor: value,
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }
    fn vector(&mut self, property: Property, value: [f32; 3], state: &Value) -> Vec<Action> {
        if value == self.last_value || value.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return vec![];
        }
        let key = if property == Property::Scale {
            "scale"
        } else {
            "rotation"
        };
        let separated = state["timeline_layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|l| {
                l["object"].as_u64() == Some(self.pose.object)
                    && l["properties"][key]["separated"] == true
            });
        let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
        let commands = if separated {
            (0..3)
                .filter(|i| value[*i] != self.last_value[*i])
                .map(|i| Command::SetComponent {
                    object: self.pose.object,
                    property,
                    axis: [Axis::X, Axis::Y, Axis::Z][i],
                    frame,
                    value: value[i],
                })
                .collect()
        } else {
            vec![Command::SetVector {
                object: self.pose.object,
                property,
                frame,
                value,
            }]
        };
        self.last_value = value;
        vec![Action::EditBatch(commands)]
    }
}
#[derive(Default)]
pub struct Controls {
    pub tool: Tool,
    drag: Option<Drag>,
    pick: Option<Seed>,
    pub cursor: Option<motion_ui::cursor::Cursor>,
}
impl Controls {
    pub fn cancel(&mut self) -> bool {
        self.pick = None;
        self.drag.take().is_some_and(|d| d.begun)
    }
    pub fn choose(&mut self, tool: Tool, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(4));
        }
        self.tool = tool;
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        view: Rect,
        state: &Value,
        selected: Option<u64>,
        input: &Input,
        out: &mut Vec<Action>,
    ) -> bool {
        if input.focus_lost || input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return true;
        }
        if self
            .drag
            .as_ref()
            .is_some_and(|d| !d.seed.valid(state, view) || selected != Some(d.pose.object))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        let pose = selected.and_then(|id| Pose::new(state, id, view));
        self.cursor = self
            .drag
            .as_ref()
            .map(|d| d.handle)
            .or_else(|| {
                pose.as_ref()
                    .and_then(|p| p.handle(input.mouse, self.tool, input.modifiers.alt))
            })
            .map(|h| match h {
                Handle::Move => motion_ui::cursor::Cursor::Grabbing,
                Handle::Scale(..) => {
                    let pivot = pose.as_ref().map(|p| p.pivot).unwrap_or(input.mouse);
                    motion_ui::cursor::resize_for_direction([
                        input.mouse[0] - pivot[0],
                        input.mouse[1] - pivot[1],
                    ])
                }
                Handle::Rotate | Handle::Anchor => motion_ui::cursor::Cursor::Crosshair,
            });
        if let Some(seed) = self.pick.take() {
            if seed.valid(state, view) {
                if let Some(p) = pose.clone().filter(|p| !p.locked) {
                    let kind = match self.tool {
                        Tool::Move => Handle::Move,
                        Tool::Rotate => Handle::Rotate,
                        Tool::Scale => Handle::Scale(true, true),
                        Tool::Anchor => Handle::Anchor,
                    };
                    self.drag = Drag::new(seed, p, kind);
                }
            }
        }
        if let Some(p) = &pose {
            p.paint(list);
        }
        let mut consumed = self.drag.is_some();
        if self.drag.is_none() {
            if let Some((MouseButton::Left, point)) =
                input.pressed.filter(|(_, p)| view.contains(*p))
            {
                let seed = Seed::new(point, state, view);
                if let Some((p, handle)) = pose.clone().and_then(|p| {
                    p.handle(point, self.tool, input.modifiers.alt)
                        .map(|h| (p, h))
                }) {
                    self.drag = Drag::new(seed, p, handle);
                    consumed = true;
                } else {
                    // Picking stays with the backend, including overlap/depth ordering.
                    self.pick = Some(seed);
                }
            }
        }
        if let Some(mut d) = self.drag.take() {
            out.extend(d.commands(state, input));
            if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
                if d.begun {
                    out.push(Action::History(3));
                }
            } else {
                self.drag = Some(d);
            }
        }
        consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    fn fixture(three_d: bool) -> (tempfile::TempDir, Engine, Value, Rect) {
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let mut layer =
            motion_core::Layer::solid(7, "Gizmo", [200., 100.], [960., 540., 0.], [1.; 4]);
        layer.three_d = three_d;
        let state = engine
            .edit(serde_json::to_string(&Command::Add { layer }).unwrap())
            .unwrap();
        (folder, engine, state, Rect::new(0., 0., 1920., 1080.))
    }
    fn apply(engine: &Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::History(op) => {
                    engine.history(op).unwrap();
                }
                Action::Edit(c) => {
                    engine.edit(serde_json::to_string(&c).unwrap()).unwrap();
                }
                Action::EditBatch(cs) => {
                    engine.edit(serde_json::to_string(&cs).unwrap()).unwrap();
                }
                Action::PreviewDrag {
                    object,
                    delta,
                    size,
                } => {
                    engine
                        .drag(
                            object as i64,
                            delta[0] as f64,
                            delta[1] as f64,
                            size[0],
                            size[1],
                        )
                        .unwrap();
                }
                _ => panic!("unexpected gesture action"),
            }
        }
    }
    fn pointer(point: [f32; 2], release: bool) -> Input {
        Input {
            mouse: point,
            held: (!release).then_some(MouseButton::Left),
            released: release.then_some((MouseButton::Left, point)),
            ..Default::default()
        }
    }
    #[test]
    fn source_projection_matches_actual_selection_for_2d_and_3d() {
        for three_d in [false, true] {
            let (_folder, _engine, state, view) = fixture(three_d);
            let p = Pose::new(&state, 7, view).unwrap();
            let projected = state["projectedLayers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == 7)
                .unwrap();
            for corner in projected["corners"].as_array().unwrap() {
                let point = array::<2>(corner).unwrap();
                assert!(p.corners.iter().any(|p| distance(*p, point) < 0.01));
            }
        }
    }
    #[test]
    fn uniform_scale_preserves_ratio_and_one_undo_restores_tracks() {
        let (_folder, engine, state, view) = fixture(false);
        let p = Pose::new(&state, 7, view).unwrap();
        let start = p.corners[2];
        let end = [
            p.pivot[0] + 2. * (start[0] - p.pivot[0]),
            p.pivot[1] + 2. * (start[1] - p.pivot[1]),
        ];
        let mut d =
            Drag::new(Seed::new(start, &state, view), p, Handle::Scale(true, true)).unwrap();
        apply(&engine, d.commands(&state, &pointer(end, false)));
        assert!(d
            .commands(&engine.state().unwrap(), &pointer(end, false))
            .is_empty());
        engine.history(3).unwrap();
        let after = engine.state().unwrap();
        let scale = array::<3>(&after["sampledLayers"][0]["scale"]).unwrap();
        assert!((scale[0] - 200.).abs() < 0.001 && scale[0] == scale[1] && scale[2] == 100.);
        assert_eq!(engine.history(0).unwrap()["project"], state["project"]);
    }
    #[test]
    fn separate_x_scale_does_not_write_y_or_z_tracks() {
        let (_folder, engine, _state, view) = fixture(false);
        let state = engine
            .edit(
                serde_json::to_string(&Command::SeparateDimensions {
                    object: 7,
                    property: Property::Scale,
                })
                .unwrap(),
            )
            .unwrap();
        let p = Pose::new(&state, 7, view).unwrap();
        let start = mid(p.corners[1], p.corners[2]);
        let end = [start[0] + 100., start[1]];
        let mut d = Drag::new(
            Seed::new(start, &state, view),
            p,
            Handle::Scale(true, false),
        )
        .unwrap();
        let actions = d.commands(&state, &pointer(end, false));
        assert!(
            matches!(&actions[1],Action::EditBatch(cs) if matches!(cs.as_slice(),[Command::SetComponent{axis:Axis::X,..}]))
        );
        apply(&engine, actions);
        engine.history(3).unwrap();
        let after = engine.state().unwrap();
        for axis in ["y", "z"] {
            assert_eq!(
                after["project"]["layers"][0]["transform"]["scale"]["axes"][axis],
                state["project"]["layers"][0]["transform"]["scale"]["axes"][axis]
            );
        }
    }
    #[test]
    fn moving_anchor_compensates_position_without_moving_pixels() {
        let (_folder, engine, state, view) = fixture(false);
        let p = Pose::new(&state, 7, view).unwrap();
        let start = p.pivot;
        let mut d = Drag::new(Seed::new(start, &state, view), p, Handle::Anchor).unwrap();
        apply(
            &engine,
            d.commands(&state, &pointer([start[0] + 20., start[1] + 10.], false)),
        );
        engine.history(3).unwrap();
        let after = engine.state().unwrap();
        let a = Pose::new(&state, 7, view).unwrap();
        let b = Pose::new(&after, 7, view).unwrap();
        for (a, b) in a.corners.iter().zip(b.corners) {
            assert!(distance(*a, b) < 0.01);
        }
        assert!((b.pivot[0] - a.pivot[0] - 20.).abs() < 0.01);
        assert_eq!(engine.history(0).unwrap()["project"], state["project"]);
    }
    #[test]
    fn move_under_nonuniform_parent_changes_position_without_changing_scale() {
        for three_d in [false, true] {
            let (_folder, engine, _state, view) = fixture(three_d);
            let parent =
                motion_core::Layer::solid(8, "Parent", [100., 100.], [960., 540., 0.], [1.; 4]);
            let commands = vec![
                Command::Add { layer: parent },
                Command::Parent {
                    object: 7,
                    parent: Some(8),
                    frame: 0,
                },
                Command::SetVector {
                    object: 8,
                    property: Property::Scale,
                    frame: 0,
                    value: [200., 50., 100.],
                },
                Command::SetVector {
                    object: 8,
                    property: Property::Rotation,
                    frame: 0,
                    value: [0., 0., 30.],
                },
            ];
            let state = engine
                .edit(serde_json::to_string(&commands).unwrap())
                .unwrap();
            let p = Pose::new(&state, 7, view).unwrap();
            let start = p.pivot;
            let mut d = Drag::new(Seed::new(start, &state, view), p, Handle::Move).unwrap();
            apply(
                &engine,
                d.commands(&state, &pointer([start[0] + 20., start[1] + 10.], false)),
            );
            engine.history(3).unwrap();
            let after = engine.state().unwrap();
            let a = Pose::new(&state, 7, view).unwrap();
            let b = Pose::new(&after, 7, view).unwrap();
            assert!((b.pivot[0] - a.pivot[0] - 20.).abs() < 0.05);
            assert!((b.pivot[1] - a.pivot[1] - 10.).abs() < 0.05);
            assert_eq!(
                state["project"]["layers"][0]["transform"]["scale"],
                after["project"]["layers"][0]["transform"]["scale"]
            );
        }
    }
}
