//! Retained-mode paint command list and its wgpu renderer.
//!
//! Panels emit flat commands into a shared buffer each frame; the renderer
//! uploads one vertex buffer per pass and draws every solid shape and glyph.
//! Keeping the pass count fixed is what lets a dense timeline redraw without
//! allocating or stalling the GPU.

use crate::input::Rect;
use crate::theme::Color;
mod strokes;
pub mod allocation;
mod memory;
#[cfg(test)]
mod triangle_clip_tests;

/// One drawable shape.
#[derive(Clone, Debug)]
pub enum Shape {
    /// One analytic quad; clipping trims geometry without changing its pattern.
    Procedural {
        area: Rect,
        clip: Rect,
        kind: Pattern,
    },
    /// Convex quad in logical pixels, wound for a triangle strip.
    Quad { points: [[f32; 2]; 4] },
    /// Connected, coverage-antialiased one-pixel stroke. Keep its original
    /// centerline through clipping; the painter clips its expanded fringe.
    Line {
        points: Vec<[f32; 2]>,
        clip: Option<Rect>,
    },
    /// Connected, coverage-antialiased stroke of `width` logical pixels.
    /// Unlike a one-pixel Line, its width scales with content transforms.
    Stroke {
        points: Vec<[f32; 2]>,
        clip: Option<Rect>,
        width: f32,
    },
    /// Precompiled SVG geometry, in independent triangle-list order.
    Triangles { points: Vec<[f32; 2]> },
}
#[derive(Clone, Debug)]
pub enum Pattern {
    Diamond,
    Box {
        border: Color,
        width: f32,
        radius: f32,
    },
    Checker {
        dark: Color,
        size: f32,
    },
    Shadow {
        radius: f32,
        blur: f32,
        bounds: Rect,
    },
    Grid {
        major: Color,
        origin: [f32; 2],
        minor: f32,
        major_step: f32,
    },
}

/// Solid fills collected for one frame.
#[derive(Default)]
pub struct PaintList {
    /// Immutable per-window drawing context. Child buffers inherit it explicitly.
    pub stylesheet: Option<std::sync::Arc<crate::style::StyleSheet>>,
    pub shapes: Vec<Shape>,
    pub colors: Vec<Color>,
    pub glyphs: Vec<Glyph>,
    /// Creation order across buffers. Use push_shape/push_glyph to record it.
    pub order: Vec<PaintOp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintOp {
    Shape(usize),
    Glyph(usize),
}

impl PaintList {
    pub fn with_styles(styles: std::sync::Arc<crate::style::StyleSheet>) -> Self {
        Self {
            stylesheet: Some(styles),
            ..Default::default()
        }
    }
    pub fn styles(&self) -> std::sync::Arc<crate::style::StyleSheet> {
        self.stylesheet
            .clone()
            .unwrap_or_else(crate::style::default_stylesheet_arc)
    }
    pub fn child(&self) -> Self {
        Self {
            stylesheet: self.stylesheet.clone(),
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.colors.clear();
        self.glyphs.clear();
        self.order.clear();
    }
    fn legacy_order(&mut self) {
        if self.order.len() != self.shapes.len() + self.glyphs.len() {
            self.order.clear();
            self.order
                .extend((0..self.shapes.len()).map(PaintOp::Shape));
            self.order
                .extend((0..self.glyphs.len()).map(PaintOp::Glyph));
        }
    }
    pub fn push_shape(&mut self, shape: Shape, color: Color) {
        self.legacy_order();
        self.order.push(PaintOp::Shape(self.shapes.len()));
        self.shapes.push(shape);
        self.colors.push(color);
    }
    pub fn push_glyph(&mut self, glyph: Glyph) {
        self.legacy_order();
        self.order.push(PaintOp::Glyph(self.glyphs.len()));
        self.glyphs.push(glyph);
    }
    /// Remove obscured text without losing creation order or shifting stale indices.
    pub fn retain_glyphs(&mut self, mut keep: impl FnMut(&Glyph) -> bool) {
        self.legacy_order();
        let len = self.glyphs.len();
        let mut mapping: Option<Vec<Option<usize>>> = None;
        let mut old = 0;
        let mut next = 0;
        self.glyphs.retain(|glyph| {
            let index = old;
            old += 1;
            if keep(glyph) {
                if let Some(mapping) = &mut mapping {
                    mapping[index] = Some(next);
                }
                next += 1;
                true
            } else {
                if mapping.is_none() {
                    // Until the first removal, all indices still map to themselves.
                    let mut entries = vec![None; len];
                    for (index, entry) in entries.iter_mut().take(index).enumerate() {
                        *entry = Some(index);
                    }
                    mapping = Some(entries);
                }
                false
            }
        });
        if let Some(mapping) = mapping {
            self.order.retain_mut(|op| match op {
                PaintOp::Shape(_) => true,
                PaintOp::Glyph(index) => {
                    if let Some(new) = mapping[*index] {
                        *index = new;
                        true
                    } else {
                        false
                    }
                }
            });
        } else {
            // Public buffers can contain stale indices; preserve the bounds check
            // even when every glyph survives and the order needs no remapping.
            for op in &self.order {
                if let PaintOp::Glyph(index) = op {
                    let _ = &self.glyphs[*index];
                }
            }
        }
    }
    /// Bound flat UI geometry and adjust cropped glyph UVs to a panel rectangle.
    pub fn clip_to(&mut self, area: Rect) {
        let shape_len = self.shapes.len();
        let glyph_len = self.glyphs.len();
        let mut shape_map: Option<Vec<Option<usize>>> = None;
        let mut glyph_map: Option<Vec<Option<usize>>> = None;
        // Keep the old zip truncation for callers that fill the public buffers
        // directly, while retaining their allocations through compaction.
        self.shapes.truncate(self.colors.len());
        self.colors.truncate(self.shapes.len());
        let mut old_shape = 0;
        let mut new_shape = 0;
        self.shapes.retain_mut(|shape| {
            let old = old_shape;
            old_shape += 1;
            let keep = if let Shape::Line { clip, .. } | Shape::Stroke { clip, .. } = shape {
                let bounds = clip.get_or_insert(area);
                bounds.min = [
                    bounds.min[0].max(area.min[0]),
                    bounds.min[1].max(area.min[1]),
                ];
                bounds.max = [
                    bounds.max[0].min(area.max[0]),
                    bounds.max[1].min(area.max[1]),
                ];
                bounds.width() > 0. && bounds.height() > 0.
            } else if let Shape::Procedural { clip, .. } = shape {
                clip.min = [clip.min[0].max(area.min[0]), clip.min[1].max(area.min[1])];
                clip.max = [clip.max[0].min(area.max[0]), clip.max[1].min(area.max[1])];
                clip.width() > 0. && clip.height() > 0.
            } else if let Shape::Triangles { points } = shape {
                // A complete triangle list inside the clip needs no new output.
                // Keep incomplete lists on the existing chunks_exact path.
                if points.len() % 3 != 0 || !points.iter().all(|p| area.contains(*p)) {
                    *points = clip_triangles(points, area);
                }
                !points.is_empty()
            } else {
                let inside = |p: &[f32; 2]| {
                    area.contains(*p)
                        || p[0] == area.max[0] && p[1] >= area.min[1] && p[1] <= area.max[1]
                        || p[1] == area.max[1] && p[0] >= area.min[0] && p[0] <= area.max[0]
                };
                let all_inside = match &*shape {
                    Shape::Quad { points } => points.iter().all(inside),
                    _ => false,
                };
                if !all_inside {
                    let triangles = match &*shape {
                        Shape::Quad { points } => expand_quad(*points),
                        _ => unreachable!(),
                    };
                    let points = clip_triangles(&triangles, area);
                    if points.is_empty() {
                        return false;
                    }
                    *shape = Shape::Triangles { points };
                }
                true
            };
            if keep {
                // A trailing removal leaves surviving indices unchanged. Build
                // the identity prefix only when a surviving shape actually moves.
                if old != new_shape {
                    let mapping = shape_map.get_or_insert_with(|| {
                        (0..shape_len).map(|i| (i < new_shape).then_some(i)).collect()
                    });
                    mapping[old] = Some(new_shape);
                }
                new_shape += 1;
            }
            keep
        });
        if let Some(mapping) = &shape_map {
            let mut old_color = 0;
            self.colors.retain(|_| {
                let keep = mapping[old_color].is_some();
                old_color += 1;
                keep
            });
        } else {
            self.colors.truncate(self.shapes.len());
        }
        let mut old_glyph = 0;
        let mut new_glyph = 0;
        self.glyphs.retain_mut(|glyph| {
            let old = old_glyph;
            old_glyph += 1;
            let Some(clipped) = glyph.clipped(area) else {
                return false;
            };
            *glyph = clipped;
            if old != new_glyph {
                let mapping = glyph_map.get_or_insert_with(|| {
                    (0..glyph_len).map(|i| (i < new_glyph).then_some(i)).collect()
                });
                mapping[old] = Some(new_glyph);
            }
            new_glyph += 1;
            true
        });
        let shapes = self.shapes.len();
        let glyphs = self.glyphs.len();
        self.order.retain_mut(|op| {
            let (index, mapping, len) = match op {
                PaintOp::Shape(index) => (index, &shape_map, shapes),
                PaintOp::Glyph(index) => (index, &glyph_map, glyphs),
            };
            if let Some(mapping) = mapping {
                if let Some(Some(new)) = mapping.get(*index) {
                    *index = *new;
                    true
                } else {
                    false
                }
            } else {
                *index < len
            }
        });
    }
    pub fn append(&mut self, mut other: Self) {
        self.legacy_order();
        other.legacy_order();
        let shapes = self.shapes.len();
        let glyphs = self.glyphs.len();
        self.order
            .extend(other.order.into_iter().map(|op| match op {
                PaintOp::Shape(index) => PaintOp::Shape(index + shapes),
                PaintOp::Glyph(index) => PaintOp::Glyph(index + glyphs),
            }));
        self.shapes.append(&mut other.shapes);
        self.colors.append(&mut other.colors);
        self.glyphs.append(&mut other.glyphs);
    }
}

/// Sutherland-Hodgman clipping preserves an icon's silhouette at panel edges.
fn clip_triangles(points: &[[f32; 2]], area: Rect) -> Vec<[f32; 2]> {
    let mut result = Vec::with_capacity(points.len());
    if area.width() <= 0. || area.height() <= 0. {
        return result;
    }
    for triangle in points.chunks_exact(3) {
        clip_triangle([triangle[0], triangle[1], triangle[2]], area, |triangle| {
            result.extend_from_slice(&triangle);
        });
    }
    result
}

/// Clip one triangle without allocating polygon buffers. Each of four planes
/// can emit at most two vertices per input vertex: 3 * 2^4 is a conservative
/// bound even for degenerate or nonfinite input. Finite convex input needs at
/// most seven vertices, but this public drawing path also accepts raw geometry.
pub(crate) fn clip_triangle(
    triangle: [[f32; 2]; 3],
    area: Rect,
    mut emit: impl FnMut([[f32; 2]; 3]),
) {
    if area.width() <= 0. || area.height() <= 0. {
        return;
    }
    if triangle.iter().all(|p| area.contains(*p)) {
        emit(triangle);
        return;
    }
    let mut buffers = [[[0.; 2]; 3 << 4]; 2];
    buffers[0][..3].copy_from_slice(&triangle);
    let mut len = 3;
    for (pass, (axis, bound, keep_greater)) in [
        (0, area.min[0], true),
        (0, area.max[0], false),
        (1, area.min[1], true),
        (1, area.max[1], false),
    ].into_iter().enumerate() {
        if len == 0 {
            return;
        }
        let (left, right) = buffers.split_at_mut(1);
        let (polygon, clipped) = if pass % 2 == 0 {
            (&left[0], &mut right[0])
        } else {
            (&right[0], &mut left[0])
        };
        let mut previous = polygon[len - 1];
        let inside = |p: [f32; 2]| {
            if keep_greater { p[axis] >= bound } else { p[axis] <= bound }
        };
        let mut out = 0;
        for &current in &polygon[..len] {
            if inside(previous) != inside(current) {
                let t = (bound - previous[axis]) / (current[axis] - previous[axis]);
                let mut crossing = [
                    previous[0] + (current[0] - previous[0]) * t,
                    previous[1] + (current[1] - previous[1]) * t,
                ];
                crossing[axis] = bound;
                clipped[out] = crossing;
                out += 1;
            }
            if inside(current) {
                clipped[out] = current;
                out += 1;
            }
            previous = current;
        }
        len = out;
    }
    for i in 1..len.saturating_sub(1) {
        emit([buffers[0][0], buffers[0][i], buffers[0][i + 1]]);
    }
}

/// One positioned glyph.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub uv: [f32; 4],
    /// Dimensions used to normalize `uv`. None uses the currently uploaded
    /// texture. Retained text supplies a size so atlas growth preserves its UVs.
    pub atlas_size: Option<[u32; 2]>,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: Color,
}

impl Glyph {
    /// Crop one glyph before insertion, preserving its texture coordinates.
    pub(crate) fn clipped(mut self, area: Rect) -> Option<Self> {
        let left = self.position[0].max(area.min[0]);
        let top = self.position[1].max(area.min[1]);
        let right = (self.position[0] + self.size[0]).min(area.max[0]);
        let bottom = (self.position[1] + self.size[1]).min(area.max[1]);
        if right <= left || bottom <= top {
            return None;
        }
        let [u0, v0, u1, v1] = self.uv;
        self.uv = [
            u0 + (u1 - u0) * (left - self.position[0]) / self.size[0],
            v0 + (v1 - v0) * (top - self.position[1]) / self.size[1],
            u0 + (u1 - u0) * (right - self.position[0]) / self.size[0],
            v0 + (v1 - v0) * (bottom - self.position[1]) / self.size[1],
        ];
        self.position = [left, top];
        self.size = [right - left, bottom - top];
        Some(self)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SolidVertex {
    position: [f32; 2],
    color: [f32; 4],
    geometry: [f32; 4],
    params: [f32; 4],
    extra: [f32; 4],
    secondary: [f32; 4],
}
impl SolidVertex {
    fn plain(position: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            color,
            geometry: [0.; 4],
            params: [0.; 4],
            extra: [0.; 4],
            secondary: [0.; 4],
        }
    }
}
/// One bounded analytic shadow quad; clipping preserves the blur domain.
pub fn shadow(
    list: &mut PaintList,
    area: Rect,
    color: Color,
    radius: f32,
    blur: f32,
    offset_y: f32,
) {
    let bounds = Rect::new(
        area.min[0],
        area.min[1] + offset_y,
        area.width(),
        area.height(),
    );
    let pad = blur.max(0.);
    let outer = Rect::new(
        bounds.min[0] - pad,
        bounds.min[1] - pad,
        bounds.width() + 2. * pad,
        bounds.height() + 2. * pad,
    );
    list.push_shape(
        Shape::Procedural {
            area: outer,
            clip: outer,
            kind: Pattern::Shadow {
                radius,
                blur,
                bounds,
            },
        },
        color,
    );
}
pub fn box_rect(
    list: &mut PaintList,
    area: Rect,
    fill: Color,
    border: Color,
    width: f32,
    radius: f32,
) {
    if area.width() > 0. && area.height() > 0. && (fill.0[3] > 0. || border.0[3] > 0. && width > 0.)
    {
        list.push_shape(
            Shape::Procedural {
                area,
                clip: Rect::new(
                    area.min[0] - 1.,
                    area.min[1] - 1.,
                    area.width() + 2.,
                    area.height() + 2.,
                ),
                kind: Pattern::Box {
                    border,
                    width,
                    radius,
                },
            },
            fill,
        );
    }
}
pub fn grid(
    list: &mut PaintList,
    area: Rect,
    minor_color: Color,
    major_color: Color,
    origin: [f32; 2],
    minor: f32,
    major: f32,
) {
    if area.width() > 0.
        && area.height() > 0.
        && minor.is_finite()
        && major.is_finite()
        && major > 0.
    {
        list.push_shape(
            Shape::Procedural {
                area,
                clip: area,
                kind: Pattern::Grid {
                    major: major_color,
                    origin,
                    minor,
                    major_step: major,
                },
            },
            minor_color,
        );
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const SOLID_CAPACITY: usize = 1 << 16;
const GLYPH_CAPACITY: usize = 1 << 17;

/// Device vertex/draw limits or a native admission failure. Native causes retain
/// their identity; the legacy capacity fields remain available to callers.
#[derive(Clone, Debug)]
pub struct PaintError {
    pub kind: &'static str,
    pub vertices: usize,
    pub required_bytes: u64,
    pub limit_bytes: u64,
    native_cause: Option<std::sync::Arc<dyn std::error::Error + Send + Sync>>,
    capacity_refusal: Option<allocation::CapacityRefusal>,
}
impl PaintError {
    /// Preserve the host's native error identity. Only a typed capacity refusal
    /// may be supplied as retryable; shader, device and validation faults are not.
    pub fn from_source(kind: &'static str, cause: impl std::error::Error + Send + Sync + 'static,
        capacity_refusal: Option<allocation::CapacityRefusal>) -> Self {
        let (required_bytes, limit_bytes) = capacity_refusal
            .map_or((0, 0), |refusal| (refusal.required_bytes, refusal.budget_bytes));
        Self { kind, vertices: 0, required_bytes, limit_bytes,
            native_cause: Some(std::sync::Arc::new(cause)), capacity_refusal }
    }
    pub fn capacity_refusal(&self) -> Option<allocation::CapacityRefusal> {
        self.capacity_refusal
    }
    /// The toolkit supplies the UI operation; the host's original source and
    /// typed capacity refusal remain intact.
    fn with_operation(mut self, kind: &'static str) -> Self {
        self.kind = kind;
        self
    }
}
impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(cause) = &self.native_cause { return write!(f, "UI {}: {cause}", self.kind); }
        write!(f, "UI vertex buffer capacity exceeded: kind={} vertices={} required_bytes={} limit_bytes={}",
            self.kind, self.vertices, self.required_bytes, self.limit_bytes)
    }
}
impl std::error::Error for PaintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.native_cause.as_deref().map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

fn vertex_buffer_size<T>(kind: &'static str, vertices: usize, limit: u64) -> Result<u64, PaintError> {
    let required = (vertices as u64).saturating_mul(std::mem::size_of::<T>() as u64);
    if required > limit || vertices > u32::MAX as usize {
        return Err(PaintError { kind, vertices, required_bytes: required, limit_bytes: limit, native_cause: None, capacity_refusal: None });
    }
    Ok(required)
}

/// Draw a filled rectangle.
pub fn rect(list: &mut PaintList, area: Rect, color: Color) {
    if area.width() <= 0.0 || area.height() <= 0.0 || color.0[3] <= 0.0 {
        return;
    }
    if area.width() <= 1. || area.height() <= 1. {
        box_rect(list, area, color, Color([0.; 4]), 0., 0.);
        return;
    }
    let min = area.min;
    let max = area.max;
    // Counter-clockwise in screen space so both pipelines can use one winding.
    list.push_shape(
        Shape::Quad {
            points: [min, [max[0], min[1]], max, [min[0], max[1]]],
        },
        color,
    );
}

/// Filled analytic diamond; its diagonal edges retain fractional coverage.
/// Invalid inputs and bounds that overflow or collapse emit no paint command.
pub fn diamond(list: &mut PaintList, center: [f32; 2], radius: f32, color: Color) {
    if !radius.is_finite()
        || radius <= 0.
        || color.0[3] <= 0.
        || !center.into_iter().all(f32::is_finite)
        || !color.0.into_iter().all(f32::is_finite)
    {
        return;
    }
    let [x, y] = center;
    let area = Rect::new(x - radius, y - radius, 2. * radius, 2. * radius);
    let clip = Rect::new(
        x - radius - 1.,
        y - radius - 1.,
        2. * radius + 2.,
        2. * radius + 2.,
    );
    // Finite operands can still overflow, or lose the whole width to rounding.
    let valid_bounds = |bounds: Rect| {
        bounds.min.into_iter().chain(bounds.max).all(f32::is_finite)
            && bounds.width().is_finite()
            && bounds.height().is_finite()
            && bounds.width() > 0.
            && bounds.height() > 0.
    };
    if !valid_bounds(area) || !valid_bounds(clip) {
        return;
    }
    list.push_shape(
        Shape::Procedural {
            area,
            clip,
            kind: Pattern::Diamond,
        },
        color,
    );
}

/// Stroke a rectangle outline of `thickness` logical pixels.
pub fn stroke(list: &mut PaintList, area: Rect, color: Color, thickness: f32) {
    if !thickness.is_finite()
        || thickness <= 0.
        || area.width() <= 0.
        || area.height() <= 0.
        || color.0[3] <= 0.
    {
        return;
    }
    if thickness <= 1. {
        box_rect(list, area, Color([0.; 4]), color, thickness, 0.);
        return;
    }
    let t = thickness.min(area.width() * 0.5).min(area.height() * 0.5);
    let strips = [
        Rect::new(area.min[0], area.min[1], area.width(), t),
        Rect::new(area.min[0], area.max[1] - t, area.width(), t),
        Rect::new(
            area.min[0],
            area.min[1] + t,
            t,
            (area.height() - 2. * t).max(0.),
        ),
        Rect::new(
            area.max[0] - t,
            area.min[1] + t,
            t,
            (area.height() - 2. * t).max(0.),
        ),
    ];
    let points = strips
        .into_iter()
        .filter(|r| r.width() > 0. && r.height() > 0.)
        .flat_map(|r| expand_quad([r.min, [r.max[0], r.min[1]], r.max, [r.min[0], r.max[1]]]))
        .collect();
    list.push_shape(Shape::Triangles { points }, color);
}

pub fn line(list: &mut PaintList, mut points: Vec<[f32; 2]>, color: Color) {
    if points.len() < 2 || color.0[3] <= 0.0 || points.iter().flatten().any(|v| !v.is_finite()) {
        return;
    }
    points.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) <= f32::EPSILON);
    if points.len() > 1 {
        list.push_shape(Shape::Line { points, clip: None }, color);
    }
}

/// Stroke a connected polyline at a finite, positive logical-pixel width.
/// Uses the same antialiasing, bounded miter joins and butt caps as `line`.
/// Invalid inputs and paths containing only repeated points emit no geometry.
pub fn line_with_width(list: &mut PaintList, mut points: Vec<[f32; 2]>, color: Color, width: f32) {
    if points.len() < 2
        || !width.is_finite()
        || width <= 0.
        || color.0[3] <= 0.
        || color.0.iter().any(|v| !v.is_finite())
        || points.iter().flatten().any(|v| !v.is_finite())
    {
        return;
    }
    points.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) <= f32::EPSILON);
    if points.len() > 1 {
        list.push_shape(
            Shape::Stroke { points, clip: None, width },
            color,
        );
    }
}

/// Alpha checkerboard for transparent pixels.
pub fn checkerboard(list: &mut PaintList, area: Rect, light: Color, dark: Color, size: f32) {
    if area.width() > 0. && area.height() > 0. && size.is_finite() && size > 0. {
        list.push_shape(
            Shape::Procedural {
                area,
                clip: area,
                kind: Pattern::Checker { dark, size },
            },
            light,
        );
    }
}

fn expand_quad(points: [[f32; 2]; 4]) -> [[f32; 2]; 6] {
    [
        points[0], points[1], points[2], points[0], points[2], points[3],
    ]
}

fn upload_color(mut rgba: [f32; 4], srgb: bool) -> [f32; 4] {
    if srgb {
        for component in &mut rgba[..3] {
            let value = *component;
            *component = if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
        }
    }
    rgba
}

struct DrawBatch {
    glyph: bool,
    vertices: std::ops::Range<u32>,
}
fn push_batch(batches: &mut Vec<DrawBatch>, glyph: bool, vertices: std::ops::Range<u32>) {
    if vertices.is_empty() {
        return;
    }
    if let Some(previous) = batches.last_mut() {
        if previous.glyph == glyph && previous.vertices.end == vertices.start {
            previous.vertices.end = vertices.end;
            return;
        }
    }
    batches.push(DrawBatch { glyph, vertices });
}

mod observation;
#[cfg(test)]
mod buffer_tests;

/// Two vertex buffers and one render pass; only adjacent compatible draws merge.
pub struct Painter {
    observation: observation::Observation,
    solid_cpu: Vec<SolidVertex>,
    glyph_cpu: Vec<GlyphVertex>,
    shape_ranges: Vec<std::ops::Range<u32>>,
    draw_batches: Vec<DrawBatch>,
    srgb_target: bool,
    solid_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    glyph_layout: wgpu::BindGroupLayout,
    solid_bind: wgpu::BindGroup,
    glyph_bind: Option<wgpu::BindGroup>,
    solid_uniform: memory::Buffer,
    glyph_uniform: memory::Buffer,
    solid_buffer: memory::Buffer,
    glyph_buffer: memory::Buffer,
    admission: Option<std::rc::Rc<dyn allocation::Context>>,
    sampler: wgpu::Sampler,
    atlas: Option<memory::Texture>,
    pending_atlas: Option<memory::PendingAtlas>,
    atlas_size: [u32; 2],
    /// Saturating count over this Painter's lifetime; reset only by constructing
    /// a new Painter. Includes the solid group and atlas-generation replacements.
    pub bind_group_creations: u64,
    /// Solid vertices submitted for the last frame.
    pub last_solid_vertices: usize,
    /// Glyph vertices submitted for the last frame.
    pub last_glyph_vertices: usize,
    pub last_draw_calls: usize,
}

impl Painter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::with_buffers(device, format, memory::Buffers::raw(device))
    }
    /// Use this renderer's device, allocated encoders and allocated submissions
    /// for every draw. Admission and queued writes retain the same shared budget.
    pub fn new_admitted(context: std::rc::Rc<dyn allocation::Context>, format: wgpu::TextureFormat) -> Result<Self, PaintError> {
        let buffers = memory::Buffers::admitted(&context)?;
        Ok(Self::with_buffers(context.device(), format, buffers))
    }
    fn with_buffers(device: &wgpu::Device, format: wgpu::TextureFormat, buffers: memory::Buffers) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panel shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/panel.wgsl").into()),
        });
        let uniform_entry = || wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let solid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("solid layout"),
            entries: &[uniform_entry()],
        });
        let glyph_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph layout"),
            entries: &[
                uniform_entry(),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // The transform is passed through a uniform buffer rather than push
        // constants: push constants need a device feature the panel pipeline
        // should not depend on, and one uniform per pass is enough here.
        let layout_for = |label: &str, bind: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(bind)],
                immediate_size: 0,
            })
        };
        let solid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panel solids"),
            layout: Some(&layout_for("solid pipeline", &solid_layout)),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("solid_vs"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SolidVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4, 2=>Float32x4, 3=>Float32x4, 4=>Float32x4, 5=>Float32x4],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("solid_fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panel glyphs"),
            layout: Some(&layout_for("glyph pipeline", &glyph_layout)),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("glyph_vs"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                    ],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("glyph_fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let memory::Buffers { solid_uniform, glyph_uniform, solid_buffer, glyph_buffer, context: admission } = buffers;
        let solid_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("solid bind group"),
            layout: &solid_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: solid_uniform.as_entire_binding(),
            }],
        });
        Self {
            srgb_target: format.is_srgb(),
            observation: Default::default(),
            solid_cpu: Vec::new(),
            glyph_cpu: Vec::new(),
            shape_ranges: Vec::new(),
            draw_batches: Vec::new(),
            solid_pipeline,
            glyph_pipeline,
            glyph_layout,
            solid_bind,
            glyph_bind: None,
            solid_uniform,
            glyph_uniform,
            solid_buffer,
            glyph_buffer,
            admission,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("glyph sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            atlas: None,
            pending_atlas: None,
            atlas_size: [0, 0],
            bind_group_creations: 1,
            last_solid_vertices: 0,
            last_glyph_vertices: 0,
            last_draw_calls: 0,
        }
    }

    /// Move a complete bitmap from the desktop worker. Keep at most one pending
    /// upload if native admission refuses it; the CPU producer already consumed
    /// its dirty flag, so dropping this intent would leave new UVs on an old atlas.
    pub fn upload_atlas_owned(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        pixels: Vec<u8>, width: u32, height: u32) -> Result<(), PaintError>
    {
        self.pending_atlas = None;
        match self.upload_atlas(device, queue, &pixels, width, height) {
            Ok(()) => Ok(()),
            Err(failure) => {
                if failure.capacity_refusal().is_some() {
                    self.pending_atlas = Some(memory::PendingAtlas { pixels, width, height, failure: failure.clone() });
                }
                Err(failure)
            }
        }
    }
    /// No allocation attempt while the pending atlas still cannot fit. A fresh
    /// bitmap replaces the pending intent; otherwise capacity recovery permits
    /// one ordinary upload attempt. Device/codec failures are never retried here.
    pub fn retry_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), PaintError> {
        let Some(pending) = &self.pending_atlas else { return Ok(()); };
        let Some(context) = &self.admission else { return Err(pending.failure.clone()); };
        let available = context.available_bytes()
            .map_err(|failure| failure.with_operation("glyph atlas admission"))?;
        let required = u64::from(pending.width) * u64::from(pending.height) * 4;
        if required > available { return Err(pending.failure.clone()); }
        if let Some(pending) = self.pending_atlas.take() {
            self.upload_atlas_owned(device, queue, pending.pixels, pending.width, pending.height)?;
        }
        Ok(())
    }
    /// Upload a borrowed glyph atlas. Callers retain responsibility for resending
    /// a refused bitmap; desktop callers use upload_atlas_owned instead.
    pub fn upload_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PaintError> {
        self.pending_atlas = None;
        let max = device.limits().max_texture_dimension_2d;
        if width > max || height > max {
            return Err(PaintError::from_source("glyph atlas", std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("glyph atlas exceeds device dimensions width={width} height={height} max_dimension={max}")), None));
        }
        let bytes = u64::from(width).checked_mul(u64::from(height)).and_then(|bytes| bytes.checked_mul(4));
        if width == 0 || height == 0 || bytes != Some(pixels.len() as u64) {
            return Err(PaintError::from_source("glyph atlas", std::io::Error::new(
                std::io::ErrorKind::InvalidInput, "invalid glyph atlas dimensions or pixel length"), None));
        }
        if self.atlas_size != [width, height] || self.atlas.is_none() {
            let texture = memory::Texture::new(device, self.admission.as_ref(), &wgpu::TextureDescriptor {
                label: Some("glyph atlas"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })?;
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph bind group"),
                layout: &self.glyph_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.glyph_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            // Upload before replacing either binding or texture. On refusal,
            // the previous complete atlas remains paired with its old binding.
            texture.write(queue, pixels, width, height)?;
            self.glyph_bind = Some(bind);
            self.bind_group_creations = self.bind_group_creations.saturating_add(1);
            self.atlas = Some(texture);
            self.atlas_size = [width, height];
        } else if let Some(texture) = &self.atlas {
            texture.write(queue, pixels, width, height)?;
        }
        Ok(())
    }

    /// Record commands and draw them over `target`.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        list: &PaintList,
        viewport: [f32; 2],
    ) -> Result<(), PaintError> {
        self.render_scaled(device, queue, encoder, target, list, viewport, [1., 1.])
    }
    pub fn render_scaled(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        list: &PaintList,
        viewport: [f32; 2],
        scale: [f32; 2],
    ) -> Result<(), PaintError> {
        let mut solids = std::mem::take(&mut self.solid_cpu);
        solids.clear();
        self.shape_ranges.clear();
        for (shape, color) in list.shapes.iter().zip(list.colors.iter()) {
            let start = solids.len() as u32;
            let rgba = upload_color(color.0, self.srgb_target);
            match shape {
                Shape::Procedural { area, clip, kind } => {
                    let mut area = *area;
                    let mut rgba = rgba;
                    let mut border_scale = 1.;
                    let mut snapped_width = None;
                    if let Pattern::Box { width, .. } = kind {
                        let sx = scale[0].max(0.001);
                        let sy = scale[1].max(0.001);
                        if *width > 0. {
                            area.min = [
                                (area.min[0] * sx).round() / sx,
                                (area.min[1] * sy).round() / sy,
                            ];
                            area.max = [
                                (area.max[0] * sx).round() / sx,
                                (area.max[1] * sy).round() / sy,
                            ];
                            let density = sx.min(sy);
                            let px = (*width * density).ceil().max(1.);
                            snapped_width = Some(px / density);
                            border_scale = (*width * density / px).min(1.);
                        } else if area.width() <= 1. || area.height() <= 1. {
                            for (axis, density) in [(0, sx), (1, sy)] {
                                let size = area.max[axis] - area.min[axis];
                                if size <= 1. {
                                    let px = (size * density).ceil().max(1.);
                                    rgba[3] *= (size * density / px).min(1.);
                                    area.min[axis] = (area.min[axis] * density).round() / density;
                                    area.max[axis] = area.min[axis] + px / density;
                                }
                            }
                        }
                    }
                    let geometry = [area.min[0], area.min[1], area.width(), area.height()];
                    let (params, extra, secondary) = match kind {
                        Pattern::Diamond => ([6., 0., 0., 0.], [0.; 4], [0.; 4]),
                        Pattern::Box {
                            border,
                            width,
                            radius,
                        } => {
                            let mut color = upload_color(border.0, self.srgb_target);
                            color[3] *= border_scale;
                            (
                                [1., snapped_width.unwrap_or(*width), *radius, 0.],
                                [0.; 4],
                                color,
                            )
                        }
                        Pattern::Shadow {
                            radius,
                            blur,
                            bounds,
                        } => (
                            [4., *blur, *radius, 0.],
                            [
                                bounds.min[0],
                                bounds.min[1],
                                bounds.width(),
                                bounds.height(),
                            ],
                            [0.; 4],
                        ),
                        Pattern::Checker { dark, size } => (
                            [2., *size, 0., 0.],
                            [0.; 4],
                            upload_color(dark.0, self.srgb_target),
                        ),
                        Pattern::Grid {
                            major,
                            origin,
                            minor,
                            major_step,
                        } => (
                            [3., 0., 0., 0.],
                            [origin[0], origin[1], *minor, *major_step],
                            upload_color(major.0, self.srgb_target),
                        ),
                    };
                    let min = [
                        clip.min[0].max(area.min[0] - 1.),
                        clip.min[1].max(area.min[1] - 1.),
                    ];
                    let max = [
                        clip.max[0].min(area.max[0] + 1.),
                        clip.max[1].min(area.max[1] + 1.),
                    ];
                    if max[0] > min[0] && max[1] > min[1] {
                        solids.extend(
                            expand_quad([min, [max[0], min[1]], max, [min[0], max[1]]])
                                .into_iter()
                                .map(|position| SolidVertex {
                                    position,
                                    color: rgba,
                                    geometry,
                                    params,
                                    extra,
                                    secondary,
                                }),
                        );
                    }
                }
                Shape::Quad { points } => {
                    for vertex in expand_quad(*points) {
                        solids.push(SolidVertex::plain(vertex, rgba));
                    }
                }
                Shape::Line { points, clip } => {
                    strokes::append(&mut solids, points, rgba, scale, *clip, 1.);
                }
                Shape::Stroke { points, clip, width } => {
                    strokes::append(&mut solids, points, rgba, scale, *clip, *width);
                }
                Shape::Triangles { points } => {
                    solids.extend(
                        points
                            .iter()
                            .map(|position| SolidVertex::plain(*position, rgba)),
                    );
                }
            }
            self.shape_ranges.push(start..solids.len() as u32);
        }
        let mut glyphs = std::mem::take(&mut self.glyph_cpu);
        glyphs.clear();
        glyphs.extend(list.glyphs.iter().flat_map(|glyph| {
            let mut uv = glyph.uv;
            if let Some(size) = glyph.atlas_size {
                for i in 0..4 {
                    let axis = i % 2;
                    uv[i] *= size[axis] as f32 / self.atlas_size[axis].max(1) as f32;
                }
            }
            let rgba = upload_color(glyph.color.0, self.srgb_target);
            let min = glyph.position;
            let max = [
                glyph.position[0] + glyph.size[0],
                glyph.position[1] + glyph.size[1],
            ];
            [
                (min, [uv[0], uv[1]]),
                ([max[0], min[1]], [uv[2], uv[1]]),
                (max, [uv[2], uv[3]]),
                (min, [uv[0], uv[1]]),
                (max, [uv[2], uv[3]]),
                ([min[0], max[1]], [uv[0], uv[3]]),
            ]
            .into_iter()
            .map(move |(position, uv)| GlyphVertex {
                position,
                uv,
                color: rgba,
            })
        }));
        self.last_solid_vertices = solids.len();
        self.last_glyph_vertices = glyphs.len();
        self.last_draw_calls = 0;
        self.draw_batches.clear();
        let sizes = (|| {
            let limit = device.limits().max_buffer_size;
            Ok((vertex_buffer_size::<SolidVertex>("solid", solids.len(), limit)?,
                vertex_buffer_size::<GlyphVertex>("glyph", glyphs.len(), limit)?))
        })();
        let (solid_bytes, glyph_bytes) = match sizes {
            Ok(sizes) => sizes,
            Err(error) => {
                self.solid_cpu = solids;
                self.glyph_cpu = glyphs;
                return Err(error);
            }
        };
        let growth: Result<_, PaintError> = (|| Ok((
            self.solid_buffer.growth(device, solid_bytes, "UI solid vertices")?,
            self.glyph_buffer.growth(device, glyph_bytes, "UI glyph vertices")?,
        )))();
        let (solid_growth, glyph_growth) = match growth {
            Ok(growth) => growth,
            Err(error) => {
                self.solid_cpu = solids;
                self.glyph_cpu = glyphs;
                return Err(error);
            }
        };
        // Both admissions succeed before replacing either old vertex buffer.
        if let Some(buffer) = solid_growth { self.solid_buffer = buffer; }
        if let Some(buffer) = glyph_growth { self.glyph_buffer = buffer; }
        let mut invalid_ops = 0usize;
        let mut first_invalid = None;
        if list.order.len() != list.shapes.len() + list.glyphs.len() {
            // Preserve compatibility with legacy lists assembled from raw buffers.
            push_batch(&mut self.draw_batches, false, 0..solids.len() as u32);
            push_batch(&mut self.draw_batches, true, 0..glyphs.len() as u32);
        } else {
            for op in &list.order {
                match *op {
                    PaintOp::Shape(index) => {
                        if let Some(range) = self.shape_ranges.get(index) {
                            push_batch(&mut self.draw_batches, false, range.clone());
                        } else {
                            invalid_ops += 1;
                            first_invalid.get_or_insert(*op);
                        }
                    }
                    PaintOp::Glyph(index) if index < list.glyphs.len() => {
                        let start = (index * 6) as u32;
                        push_batch(&mut self.draw_batches, true, start..start + 6);
                    }
                    _ => {
                        invalid_ops += 1;
                        first_invalid.get_or_insert(*op);
                    }
                }
            }
        }
        self.observation.order(invalid_ops, first_invalid, list.shapes.len(), list.glyphs.len());
        self.observation.atlas(self.glyph_bind.is_none() && self.draw_batches.iter().any(|batch| batch.glyph), list.glyphs.len());
        let uploads: Result<(), PaintError> = (|| {
            if !solids.is_empty() { self.solid_buffer.write(queue, bytemuck::cast_slice(&solids))?; }
            if !glyphs.is_empty() { self.glyph_buffer.write(queue, bytemuck::cast_slice(&glyphs))?; }
            // Logical pixels to clip space: x to the right, y downward.
            let uniform = [2.0 / viewport[0], 0.0, 0.0, -2.0 / viewport[1]];
            self.solid_uniform.write(queue, bytemuck::bytes_of(&uniform))?;
            self.glyph_uniform.write(queue, bytemuck::bytes_of(&uniform))?;
            Ok(())
        })();
        if let Err(error) = uploads {
            self.solid_cpu = solids;
            self.glyph_cpu = glyphs;
            return Err(error);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            multiview_mask: None,
            label: Some("panels"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if solids.is_empty() && glyphs.is_empty() {
            self.solid_cpu = solids;
            self.glyph_cpu = glyphs;
            return Ok(());
        }
        for batch in &self.draw_batches {
            if batch.glyph {
                let Some(bind) = &self.glyph_bind else { continue };
                pass.set_pipeline(&self.glyph_pipeline);
                pass.set_bind_group(0, bind, &[]);
                pass.set_vertex_buffer(0, self.glyph_buffer.slice(..));
            } else {
                pass.set_pipeline(&self.solid_pipeline);
                pass.set_bind_group(0, &self.solid_bind, &[]);
                pass.set_vertex_buffer(0, self.solid_buffer.slice(..));
            }
            pass.draw(batch.vertices.clone(), 0..1);
            self.last_draw_calls += 1;
        }
        drop(pass);
        self.solid_cpu = solids;
        self.glyph_cpu = glyphs;
        Ok(())
    }
}

#[cfg(test)]
mod bindings_tests;
#[cfg(test)]
mod admission_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_clipping_crops_text_uvs_and_keeps_geometry_inside() {
        let mut list = PaintList::default();
        rect(
            &mut list,
            Rect::new(-5.0, -5.0, 20.0, 20.0),
            Color::grey(30),
        );
        list.glyphs.push(Glyph {
            atlas_size: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            position: [-5.0, 0.0],
            size: [20.0, 10.0],
            color: Color::grey(255),
        });
        list.clip_to(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(list.glyphs[0].uv, [0.25, 0.0, 0.75, 1.0]);
        assert_eq!(list.glyphs[0].size, [10.0, 10.0]);
        assert!(
            matches!(&list.shapes[0],Shape::Triangles{points} if points.iter().all(|p|p[0]>=0.0&&p[0]<=10.0&&p[1]>=0.0&&p[1]<=10.0))
        );
    }

    #[test]
    fn degenerate_rectangles_emit_nothing() {
        let mut list = PaintList::default();
        rect(
            &mut list,
            Rect::new(0.0, 0.0, 0.0, 10.0),
            Color::rgb(1, 2, 3),
        );
        rect(
            &mut list,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Color::rgba(1, 2, 3, 0.0),
        );
        assert!(list.shapes.is_empty());
    }

    #[test]
    fn a_quad_becomes_six_triangle_list_vertices() {
        let mut list = PaintList::default();
        rect(&mut list, Rect::new(0.0, 0.0, 4.0, 4.0), Color::grey(20));
        assert_eq!(list.shapes.len(), 1);
        let mut solids = Vec::new();
        for (shape, color) in list.shapes.iter().zip(list.colors.iter()) {
            if let Shape::Quad { points } = shape {
                solids.extend(expand_quad(*points));
            }
            assert_eq!(color.0[3], 1.0);
        }
        assert_eq!(solids.len(), 6);
    }

    #[test]
    fn polylines_expand_to_triangles_without_dropping_the_last_segment() {
        let mut line = vec![];
        strokes::append(
            &mut line,
            &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            [1.; 4],
            [1.; 2],
            None,
            1.,
        );
        assert_eq!(line.len(), 12);
        assert!(line.iter().any(|p| p.position[1] > 10.));
        let mut list = PaintList::default();
        super::line(&mut list, vec![[1., 1.], [1., 1.]], Color::grey(255));
        assert!(list.shapes.is_empty());
    }

    #[test]
    fn stroke_covers_four_edges() {
        let mut list = PaintList::default();
        stroke(
            &mut list,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Color::grey(80),
            1.0,
        );
        assert_eq!(list.shapes.len(), 1);
        let mut list = PaintList::default();
        stroke(
            &mut list,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Color::grey(80),
            0.0,
        );
        assert!(list.shapes.is_empty());
    }

    #[test]
    fn checkerboard_tiles_are_clipped_to_the_requested_area() {
        let mut list = PaintList::default();
        checkerboard(
            &mut list,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Color::grey(70),
            Color::grey(50),
            8.0,
        );
        assert!(
            matches!(&list.shapes[0],Shape::Procedural{area,kind:Pattern::Checker{size,..},..} if area.width()*area.height()==100.&&*size==8.)
        );
        list.clip_to(Rect::new(2., 3., 5., 4.));
        assert!(
            matches!(&list.shapes[0],Shape::Procedural{area,clip,..} if area.min==[0.,0.]&&clip.min==[2.,3.]&&clip.max==[7.,7.])
        );
    }
}
