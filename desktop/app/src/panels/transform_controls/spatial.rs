//! Three-dimensional constraints in the renderer's evaluated source basis.
//! Position axes follow the parent coordinate frame; scale axes follow the
//! layer frame. Both retain the same parent bind, camera, viewport and signs.
use super::*;
use glam::{DMat4, DVec3};
use motion_ui::{style::StyleSheet, text::TextAtlas};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Constraint {
    Axis(usize),
    /// The omitted axis is the plane normal: 0=YZ, 1=XZ, 2=XY.
    Plane(usize),
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Problem {
    Direction,
    Transform,
    Locked,
    Bounds,
    Changed,
}
impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Direction => "view ray is parallel to the transform constraint",
            Self::Transform => "transform projection is not invertible",
            Self::Locked => "layer is locked",
            Self::Bounds => "transform result is non-finite or outside the model bounds",
            Self::Changed => "transform context changed during gesture",
        })
    }
}

fn axis(index: usize) -> DVec3 {
    [DVec3::X, DVec3::Y, DVec3::Z][index]
}

/// A float GPU matrix can have a small nonzero determinant after multiplying
/// a genuinely collapsed parent. Check authored zero scales before trusting
/// that rounded matrix's inverse. This does not clamp or change saved scales.
pub(super) fn invertible(state: &Value, object: u64) -> bool {
    let (Some(layers), Some(samples)) = (
        state["project"]["layers"].as_array(),
        state["sampledLayers"].as_array(),
    ) else {
        return false;
    };
    let mut visited = std::collections::BTreeSet::new();
    let mut next = Some(object);
    while let Some(id) = next {
        if !visited.insert(id) {
            return false;
        }
        if id == 0 {
            next = state["project"]["camera"]["parent"]["object"].as_u64();
            continue;
        }
        let Some(layer) = layers.iter().find(|layer| layer["id"] == id) else {
            return false;
        };
        if layer["content"]["kind"] != "camera" {
            let Some(sample) = samples.iter().find(|sample| sample["id"] == id) else {
                return false;
            };
            let Some(scale) = array::<3>(&sample["scale"]) else {
                return false;
            };
            let dimensions = if layer["three_d"] == true { 3 } else { 2 };
            if scale[..dimensions].contains(&0.) {
                return false;
            }
        }
        next = layer["parent"]["object"].as_u64();
    }
    true
}
fn color(index: usize, styles: &StyleSheet) -> motion_ui::theme::Color {
    styles.color(
        [
            "transform.gizmo.x",
            "transform.gizmo.y",
            "transform.gizmo.z",
        ][index],
    )
}
fn segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    if length <= f32::EPSILON {
        return distance(point, a);
    }
    let t = (((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / length).clamp(0., 1.);
    distance(point, [a[0] + t * d[0], a[1] + t * d[1]])
}
fn inside(point: [f32; 2], polygon: [[f32; 2]; 4]) -> bool {
    let mut sign = 0.;
    for i in 0..4 {
        let a = polygon[i];
        let b = polygon[(i + 1) % 4];
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        if cross.abs() <= f32::EPSILON {
            continue;
        }
        if sign == 0. {
            sign = cross.signum();
        } else if sign * cross < 0. {
            return false;
        }
    }
    sign != 0.
}

pub(super) struct Frame {
    projection: Projection,
    origin: [f32; 2],
    lengths: [Option<f64>; 3],
    tips: [Option<[f32; 2]>; 3],
    planes: [Option<[[f32; 2]; 4]>; 3],
}
impl Frame {
    pub(super) fn new(pose: &Pose, tool: Tool, styles: &StyleSheet) -> Option<Self> {
        if !pose.three_d || !matches!(tool, Tool::Move | Tool::Scale) {
            return None;
        }
        let local = pose.pivot_local();
        let pivot = pose
            .projection
            .with_basis(DMat4::from_translation(DVec3::new(
                f64::from(local[0]),
                -f64::from(local[1]),
                0.,
            )))?;
        let projection = if tool == Tool::Scale {
            pivot
        } else {
            // Remove only the layer's own evaluated rotation and scale. The
            // render matrix already contains the actual parent / bind / camera.
            let rs = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::from_array(pose.scale) / 100.,
                motion_core::rotation_quaternion(pose.rotation, pose.rotation_order),
                glam::Vec3::ZERO,
            );
            let rs = DMat4::from_cols_array(&rs.to_cols_array().map(f64::from));
            if !rs.is_finite() || rs.determinant().abs() < 1e-15 {
                return None;
            }
            pivot.with_basis(rs.inverse())?
        };
        let origin = projection.screen3(DVec3::ZERO)?;
        let length = styles.metric("transform.gizmo.length", 80.);
        let gap = styles.metric("transform.gizmo.center_gap", 12.);
        let mut lengths = [None; 3];
        let mut tips = [None; 3];
        for i in 0..3 {
            let Some(unit) = projection.screen3(axis(i)) else {
                continue;
            };
            let projected = distance(unit, origin);
            if projected <= 1e-4 {
                continue;
            }
            let extent = f64::from(length / projected);
            let Some(tip) = projection.screen3(axis(i) * extent) else {
                continue;
            };
            if distance(tip, origin) <= gap {
                continue;
            }
            lengths[i] = Some(extent);
            tips[i] = Some(tip);
        }
        let inner = f64::from(styles.metric("transform.gizmo.plane.inner", 0.22));
        let outer = f64::from(styles.metric("transform.gizmo.plane.outer", 0.42));
        let min_area = styles.metric("transform.gizmo.plane.min_area", 16.);
        let planes = std::array::from_fn(|normal| {
            let a = (normal + 1) % 3;
            let b = (normal + 2) % 3;
            let (la, lb) = (lengths[a]?, lengths[b]?);
            let mut points = [[0.; 2]; 4];
            for (point, (u, v)) in points.iter_mut().zip([
                (inner, inner),
                (outer, inner),
                (outer, outer),
                (inner, outer),
            ]) {
                *point = projection.screen3(axis(a) * la * u + axis(b) * lb * v)?;
            }
            let area = (0..4)
                .map(|i| {
                    points[i][0] * points[(i + 1) % 4][1] - points[i][1] * points[(i + 1) % 4][0]
                })
                .sum::<f32>()
                .abs()
                * 0.5;
            (area >= min_area).then_some(points)
        });
        Some(Self {
            projection,
            origin,
            lengths,
            tips,
            planes,
        })
    }
    pub(super) fn hit(&self, point: [f32; 2], styles: &StyleSheet) -> Option<(f32, Constraint)> {
        let radius = styles.metric("transform.gizmo.hit_radius", 8.);
        let gap = styles.metric("transform.gizmo.center_gap", 12.);
        let mut closest = None;
        for i in 0..3 {
            let Some(tip) = self.tips[i] else {
                continue;
            };
            let length = distance(tip, self.origin);
            let start =
                std::array::from_fn(|k| self.origin[k] + (tip[k] - self.origin[k]) * gap / length);
            let score = segment(point, start, tip);
            if score < radius && closest.is_none_or(|(best, _)| score < best) {
                closest = Some((score, Constraint::Axis(i)));
            }
        }
        for i in 0..3 {
            if self.planes[i].is_some_and(|polygon| inside(point, polygon)) {
                return Some((0., Constraint::Plane(i)));
            }
        }
        closest
    }
    pub(super) fn paint(
        &self,
        list: &mut PaintList,
        hovered: Option<Handle>,
        scale: bool,
        locked: bool,
    ) -> [Option<(usize, [f32; 2])>; 3] {
        let styles = list.styles();
        let colors = styles.colors();
        let width = styles.metric("transform.gizmo.line_width", 2.);
        let radius = styles.metric("transform.gizmo.tip_radius", 4.);
        let gap = styles.metric("transform.gizmo.center_gap", 12.);
        for i in 0..3 {
            let Some(points) = self.planes[i] else {
                continue;
            };
            let selected = !locked && hovered == Some(Handle::Spatial(Constraint::Plane(i), scale));
            list.push_shape(
                paint::Shape::Quad { points },
                if selected {
                    color(i, &styles)
                } else {
                    colors.PANEL
                },
            );
            let mut outline = points.to_vec();
            outline.push(points[0]);
            paint::line(
                list,
                outline,
                if locked {
                    colors.TEXT_MUTED
                } else if selected {
                    colors.TEXT
                } else {
                    color(i, &styles)
                },
            );
        }
        std::array::from_fn(|i| {
            let tip = self.tips[i]?;
            let selected = !locked && hovered == Some(Handle::Spatial(Constraint::Axis(i), scale));
            let color = if locked {
                colors.TEXT_MUTED
            } else if selected {
                colors.TEXT
            } else {
                color(i, &styles)
            };
            let length = distance(tip, self.origin);
            let dir: [f32; 2] = std::array::from_fn(|k| (tip[k] - self.origin[k]) / length);
            let start = std::array::from_fn(|k| self.origin[k] + dir[k] * gap);
            paint::line_with_width(list, vec![start, tip], color, width);
            if scale {
                let area = Rect::new(tip[0] - radius, tip[1] - radius, radius * 2., radius * 2.);
                paint::rect(list, area, colors.SUNKEN);
                paint::stroke(list, area, color, width);
            } else {
                let base = [tip[0] - dir[0] * radius * 2., tip[1] - dir[1] * radius * 2.];
                paint::line_with_width(
                    list,
                    vec![
                        [base[0] - dir[1] * radius, base[1] + dir[0] * radius],
                        tip,
                        [base[0] + dir[1] * radius, base[1] - dir[0] * radius],
                    ],
                    color,
                    width,
                );
            }
            Some((i, tip))
        })
    }
    fn point(&self, point: [f32; 2], constraint: Constraint) -> Result<DVec3, Problem> {
        let (origin, direction) = self.projection.ray3(point).ok_or(Problem::Transform)?;
        let result = match constraint {
            Constraint::Axis(i) => {
                if self.lengths[i].is_none() {
                    return Err(Problem::Direction);
                }
                let b = direction[i];
                let divisor = 1. - b * b;
                if divisor <= 1e-5 {
                    return Err(Problem::Direction);
                }
                let t = (b * origin[i] - direction.dot(origin)) / divisor;
                if t < 0. {
                    return Err(Problem::Direction);
                }
                axis(i) * (origin[i] + b * t)
            }
            Constraint::Plane(i) => {
                if self.planes[i].is_none() || direction[i].abs() <= 1e-5 {
                    return Err(Problem::Direction);
                }
                let t = -origin[i] / direction[i];
                if t < 0. {
                    return Err(Problem::Direction);
                }
                origin + direction * t
            }
        };
        result.is_finite().then_some(result).ok_or(Problem::Bounds)
    }
}

pub(super) struct Gesture {
    frame: Frame,
    start: DVec3,
}
impl Drag {
    pub(super) fn spatial_invalid(&self, state: &Value) -> Option<Problem> {
        let before = self.spatial_context.as_ref()?;
        let layers = state["project"]["layers"].as_array()?;
        if layers
            .iter()
            .any(|layer| layer["id"] == self.pose.object && layer["locked"] == true)
        {
            return Some(Problem::Locked);
        }
        (context(state, self.pose.object, self.handle).as_ref() != Some(before))
            .then_some(Problem::Changed)
    }
    pub(super) fn spatial_commands(
        &mut self,
        state: &Value,
        input: &Input,
        styles: &StyleSheet,
    ) -> Vec<Action> {
        let Handle::Spatial(constraint, scale) = self.handle else {
            return vec![];
        };
        let result = (|| {
            if self.spatial.is_none() {
                let frame = Frame::new(
                    &self.pose,
                    if scale { Tool::Scale } else { Tool::Move },
                    styles,
                )
                .ok_or(Problem::Transform)?;
                let start = frame.point(self.seed.point, constraint)?;
                self.spatial = Some(Box::new(Gesture { frame, start }));
            }
            let gesture = self.spatial.as_ref().ok_or(Problem::Transform)?;
            let delta = gesture.frame.point(input.mouse, constraint)? - gesture.start;
            let mut value = if scale {
                self.pose.scale
            } else {
                self.pose.position
            };
            for i in 0..3 {
                let active = match constraint {
                    Constraint::Axis(a) => i == a,
                    Constraint::Plane(normal) => i != normal,
                };
                if !active {
                    continue;
                }
                value[i] = if scale {
                    (f64::from(value[i])
                        * (1. + delta[i] / gesture.frame.lengths[i].ok_or(Problem::Direction)?))
                        as f32
                } else {
                    (f64::from(value[i]) + delta[i]) as f32
                };
            }
            if value.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
                return Err(Problem::Bounds);
            }
            Ok(value)
        })();
        // Geometrically unavailable directions and model-bound refusals are
        // ordinary input limitations; expose a notice without fault escalation.
        if let Err(problem @ (Problem::Direction | Problem::Bounds)) = result {
            self.notice = Some(problem);
            return vec![];
        }
        let value = match observe!(
            "transform.spatial_drag",
            result,
            "object={} phase=constraint handle={:?}",
            self.pose.object,
            constraint
        ) {
            Ok(value) => value,
            Err(problem) => {
                self.notice = Some(problem);
                return vec![];
            }
        };
        let edits = self.vector(
            if scale {
                Property::Scale
            } else {
                Property::Position
            },
            value,
            state,
        );
        if edits.is_empty() {
            return edits;
        }
        let mut out = vec![];
        if !self.begun {
            self.begun = true;
            out.push(Action::History(2));
        }
        out.extend(edits);
        out
    }
}

/// Retain only bounded evaluated geometry and hierarchy identities. In
/// particular, do not clone effects, user expressions or complete tracks on the
/// pointer hot path. The edited component is excluded; all other dependencies
/// must remain unchanged for the seed coordinate system to stay meaningful.
pub(super) fn context(state: &Value, object: u64, handle: Handle) -> Option<Value> {
    let layers = state["project"]["layers"].as_array()?;
    let samples = state["sampledLayers"].as_array()?;
    let Handle::Spatial(_, scale) = handle else {
        return None;
    };
    let camera = layers.iter().rev().find(|layer| {
        layer["content"]["kind"] == "camera"
            && samples
                .iter()
                .any(|sample| sample["id"] == layer["id"] && sample["active"] == true)
    });
    let camera_parent = camera
        .map(|layer| &layer["parent"])
        .unwrap_or(&state["project"]["camera"]["parent"])["object"]
        .as_u64();
    let mut geometry = vec![];
    let mut visited = std::collections::BTreeSet::new();
    for root in [Some(object), camera_parent] {
        let mut chain = std::collections::BTreeSet::new();
        let mut next = root;
        while let Some(id) = next {
            if !chain.insert(id) {
                return None;
            }
            // Shared ancestors have already been retained.
            if !visited.insert(id) {
                break;
            }
            if id == 0 {
                geometry.push(serde_json::json!([
                    "legacy_camera",
                    state["sampledCamera"],
                    state["project"]["camera"]["parent"],
                    state["project"]["camera"]["mode"]
                ]));
                next = state["project"]["camera"]["parent"]["object"].as_u64();
                continue;
            }
            let layer = layers.iter().find(|layer| layer["id"] == id)?;
            let sample = samples.iter().find(|sample| sample["id"] == id)?;
            let mut snapshot = serde_json::json!([
                id,
                layer["parent"],
                layer["rotation_order"],
                layer["size"],
                layer["three_d"],
                layer["locked"],
                layer["visible"],
                sample["active"],
                sample["anchor"],
                sample["rotation"]
            ]);
            let fields = snapshot.as_array_mut()?;
            if id != object || scale {
                fields.push(sample["position"].clone());
            }
            if id != object || !scale {
                fields.push(sample["scale"].clone());
            }
            if id != object {
                fields.push(
                    state["source_transforms"]
                        .as_array()
                        .and_then(|transforms| {
                            transforms.iter().find(|transform| transform["id"] == id)
                        })
                        .map(|transform| transform["mvp"].clone())
                        .unwrap_or(Value::Null),
                );
            }
            geometry.push(snapshot);
            next = layer["parent"]["object"].as_u64();
        }
    }
    let camera_sample =
        camera.and_then(|layer| samples.iter().find(|sample| sample["id"] == layer["id"]));
    Some(serde_json::json!([
        geometry,
        state["viewRevision"],
        state["observing"],
        state["observationView"],
        state["observationCamera"],
        state["sampledCamera"],
        state["project"]["width"],
        state["project"]["height"],
        camera.map(|layer| &layer["id"]),
        camera_sample,
        camera.map(|layer| &layer["parent"]),
        state["project"]["camera"]["created"],
        state["project"]["camera"]["mode"]
    ]))
}

pub(super) fn blocked_hit(
    pose: Option<&Pose>,
    state: &Value,
    selected: Option<u64>,
    view: Rect,
    point: [f32; 2],
    tool: Tool,
    styles: &StyleSheet,
) -> Option<Problem> {
    if !matches!(tool, Tool::Move | Tool::Scale) {
        return None;
    }
    if let Some(pose) = pose {
        if pose.locked
            && Frame::new(pose, tool, styles)
                .is_some_and(|frame| frame.hit(point, styles).is_some())
        {
            return Some(Problem::Locked);
        }
        return None;
    }
    let id = selected?;
    let stored = state["project"]["layers"]
        .as_array()?
        .iter()
        .find(|layer| layer["id"] == id)?;
    let sampled = state["sampledLayers"]
        .as_array()?
        .iter()
        .find(|layer| layer["id"] == id)?;
    if stored["three_d"] != true || sampled["active"] == false {
        return None;
    }
    let anchor = state["projectedLayers"]
        .as_array()?
        .iter()
        .find(|layer| layer["id"] == id)?["anchor"]
        .as_array()?;
    let width = state["project"]["width"].as_f64()? as f32;
    let height = state["project"]["height"].as_f64()? as f32;
    if width <= 0. || height <= 0. {
        return None;
    }
    let scale = (view.width() / width).min(view.height() / height);
    let origin = [
        view.min[0] + (view.width() - width * scale) * 0.5,
        view.min[1] + (view.height() - height * scale) * 0.5,
    ];
    let pivot = [
        origin[0] + anchor.first()?.as_f64()? as f32 * scale,
        origin[1] + anchor.get(1)?.as_f64()? as f32 * scale,
    ];
    (distance(point, pivot) < styles.metric("transform.anchor.hit_radius", 12.))
        .then_some(Problem::Transform)
}

impl Controls {
    pub(in crate::panels) fn paint_spatial_labels(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
    ) {
        let styles = list.styles();
        let Some(mut style) = styles.resolve("label.small", Default::default()) else {
            return;
        };
        let offset = styles.metric("transform.gizmo.label.offset", 6.);
        let width = styles.metric("transform.gizmo.label.width", 14.);
        let height = styles.metric("transform.gizmo.label.height", 18.);
        for (axis, point) in self.spatial_labels.iter().flatten() {
            style.foreground = if self.spatial_locked {
                styles.colors().TEXT_MUTED
            } else {
                color(*axis, &styles)
            };
            motion_ui::style::text_style(
                list,
                atlas,
                Rect::new(point[0] + offset, point[1] - height * 0.5, width, height),
                ["X", "Y", "Z"][*axis],
                style,
                motion_ui::ui::Align::Left,
            );
        }
    }
    pub(in crate::panels) fn take_notice(&mut self, locale: crate::i18n::Locale) -> Option<String> {
        let zh = locale == crate::i18n::Locale::Zh;
        Some(match (self.notice.take()?,zh) {
            (Problem::Direction,true)=>"当前轴或平面与视线接近平行，请调整视角后拖动。",
            (Problem::Direction,false)=>"This axis or plane is nearly parallel to the view; change the viewing angle.",
            (Problem::Transform,true)=>"当前变换不可求逆，请检查图层或父级的零缩放。",
            (Problem::Transform,false)=>"The transform cannot be inverted; check zero scale on this layer or its parent.",
            (Problem::Locked,true)=>"图层已锁定，不能拖动三维变换手柄。",
            (Problem::Locked,false)=>"This layer is locked; its 3D transform handles cannot be dragged.",
            (Problem::Bounds,true)=>"变换结果超出允许范围，已保留上一次有效值。",
            (Problem::Bounds,false)=>"The transform is outside the allowed range; the last valid value was retained.",
            (Problem::Changed,true)=>"视角或图层变换关系已改变，已取消三维拖动。",
            (Problem::Changed,false)=>"The view or transform hierarchy changed; the 3D drag was cancelled.",
        }.into())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod native_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod worker_tests;
