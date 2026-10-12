//! Retained-mode paint command list and its wgpu renderer.
//!
//! Panels emit flat commands into a shared buffer each frame; the renderer
//! uploads one vertex buffer per pass and draws every solid shape and glyph.
//! Keeping the pass count fixed is what lets a dense timeline redraw without
//! allocating or stalling the GPU.

use crate::input::Rect;
use crate::theme::Color;
use wgpu::util::DeviceExt;

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
    /// Polyline expanded to a strip on the CPU.
    Line { points: Vec<[f32; 2]> },
    /// Precompiled SVG geometry, in independent triangle-list order.
    Triangles { points: Vec<[f32; 2]> },
}
#[derive(Clone, Debug)]
pub enum Pattern {
    Box {
        border: Color,
        width: f32,
        radius: f32,
    },
    Checker {
        dark: Color,
        size: f32,
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
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.colors.clear();
        self.glyphs.clear();
        self.order.clear();
    }
    fn legacy_order(&mut self) {
        if self.order.len() != self.shapes.len() + self.glyphs.len() {
            self.order.clear();
            self.order.extend((0..self.shapes.len()).map(PaintOp::Shape));
            self.order.extend((0..self.glyphs.len()).map(PaintOp::Glyph));
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
        let mut mapping = vec![None; self.glyphs.len()];
        let mut old = 0;
        let mut next = 0;
        self.glyphs.retain(|glyph| {
            let index = old;
            old += 1;
            if keep(glyph) {
                mapping[index] = Some(next);
                next += 1;
                true
            } else {
                false
            }
        });
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
    }
    /// Bound flat UI geometry and adjust cropped glyph UVs to a panel rectangle.
    pub fn clip_to(&mut self, area: Rect) {
        let mut shape_map = vec![None; self.shapes.len()];
        let mut glyph_map = vec![None; self.glyphs.len()];
        let mut shapes = Vec::new();
        let mut colors = Vec::new();
        for (old, (mut shape, color)) in std::mem::take(&mut self.shapes)
            .into_iter()
            .zip(std::mem::take(&mut self.colors))
            .enumerate()
        {
            if let Shape::Procedural { clip, .. } = &mut shape {
                clip.min = [clip.min[0].max(area.min[0]), clip.min[1].max(area.min[1])];
                clip.max = [clip.max[0].min(area.max[0]), clip.max[1].min(area.max[1])];
                if clip.width() > 0. && clip.height() > 0. {
                    shape_map[old] = Some(shapes.len());
                    shapes.push(shape);
                    colors.push(color);
                }
                continue;
            }
            if let Shape::Triangles { points } = &mut shape {
                *points = clip_triangles(points, area);
                if !points.is_empty() {
                    shape_map[old] = Some(shapes.len());
                    shapes.push(shape);
                    colors.push(color);
                }
                continue;
            }
            let inside = |p: &[f32; 2]| {
                area.contains(*p)
                    || p[0] == area.max[0] && p[1] >= area.min[1] && p[1] <= area.max[1]
                    || p[1] == area.max[1] && p[0] >= area.min[0] && p[0] <= area.max[0]
            };
            let all_inside = match &shape {
                Shape::Quad { points } => points.iter().all(inside),
                Shape::Line { points } => points.iter().all(inside),
                _ => false,
            };
            if !all_inside {
                let triangles = match &shape {
                    Shape::Quad { points } => expand_quad(*points).to_vec(),
                    Shape::Line { points } => expand_line(points),
                    _ => unreachable!(),
                };
                let points = clip_triangles(&triangles, area);
                if points.is_empty() {
                    continue;
                }
                shape = Shape::Triangles { points };
            }
            shape_map[old] = Some(shapes.len());
            shapes.push(shape);
            colors.push(color);
        }
        self.shapes = shapes;
        self.colors = colors;
        let mut old_glyph = 0;
        let mut new_glyph = 0;
        self.glyphs.retain_mut(|glyph| {
            let old = old_glyph;
            old_glyph += 1;
            let left = glyph.position[0].max(area.min[0]);
            let top = glyph.position[1].max(area.min[1]);
            let right = (glyph.position[0] + glyph.size[0]).min(area.max[0]);
            let bottom = (glyph.position[1] + glyph.size[1]).min(area.max[1]);
            if right <= left || bottom <= top {
                return false;
            }
            let [u0, v0, u1, v1] = glyph.uv;
            glyph.uv = [
                u0 + (u1 - u0) * (left - glyph.position[0]) / glyph.size[0],
                v0 + (v1 - v0) * (top - glyph.position[1]) / glyph.size[1],
                u0 + (u1 - u0) * (right - glyph.position[0]) / glyph.size[0],
                v0 + (v1 - v0) * (bottom - glyph.position[1]) / glyph.size[1],
            ];
            glyph.position = [left, top];
            glyph.size = [right - left, bottom - top];
            glyph_map[old] = Some(new_glyph);
            new_glyph += 1;
            true
        });
        self.order.retain_mut(|op| {
            let (index, mapping) = match op {
                PaintOp::Shape(index) => (index, &shape_map),
                PaintOp::Glyph(index) => (index, &glyph_map),
            };
            if let Some(Some(new)) = mapping.get(*index) {
                *index = *new;
                true
            } else { false }
        });
    }
    pub fn append(&mut self, mut other: Self) {
        self.legacy_order();
        other.legacy_order();
        let shapes = self.shapes.len();
        let glyphs = self.glyphs.len();
        self.order.extend(other.order.into_iter().map(|op| match op {
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
        if triangle.iter().all(|p| area.contains(*p)) {
            result.extend_from_slice(triangle);
            continue;
        }
        let mut polygon = triangle.to_vec();
        for (axis, bound, keep_greater) in [
            (0, area.min[0], true),
            (0, area.max[0], false),
            (1, area.min[1], true),
            (1, area.max[1], false),
        ] {
            let mut clipped = Vec::with_capacity(polygon.len() + 1);
            let Some(mut previous) = polygon.last().copied() else {
                break;
            };
            let inside = |p: [f32; 2]| {
                if keep_greater {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            for current in polygon {
                if inside(previous) != inside(current) {
                    let t = (bound - previous[axis]) / (current[axis] - previous[axis]);
                    let mut crossing = [
                        previous[0] + (current[0] - previous[0]) * t,
                        previous[1] + (current[1] - previous[1]) * t,
                    ];
                    crossing[axis] = bound;
                    clipped.push(crossing);
                }
                if inside(current) {
                    clipped.push(current);
                }
                previous = current;
            }
            polygon = clipped;
        }
        for i in 1..polygon.len().saturating_sub(1) {
            result.extend_from_slice(&[polygon[0], polygon[i], polygon[i + 1]]);
        }
    }
    result
}

/// One positioned glyph.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub uv: [f32; 4],
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: Color,
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
        list.push_shape(Shape::Procedural {
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
        }, fill);
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
        list.push_shape(Shape::Procedural {
            area,
            clip: area,
            kind: Pattern::Grid {
                major: major_color,
                origin,
                minor,
                major_step: major,
            },
        }, minor_color);
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
    list.push_shape(Shape::Quad {
        points: [min, [max[0], min[1]], max, [min[0], max[1]]],
    }, color);
}

/// Stroke a rectangle outline of `thickness` logical pixels.
pub fn diamond(list: &mut PaintList, center: [f32; 2], radius: f32, color: Color) {
    let [x, y] = center;
    list.push_shape(Shape::Quad {
        points: [
            [x, y - radius],
            [x + radius, y],
            [x, y + radius],
            [x - radius, y],
        ],
    }, color);
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

pub fn line(list: &mut PaintList, points: Vec<[f32; 2]>, color: Color) {
    if points.len() < 2 || color.0[3] <= 0.0 {
        return;
    }
    list.push_shape(Shape::Line { points }, color);
}

/// Alpha checkerboard for transparent pixels.
pub fn checkerboard(list: &mut PaintList, area: Rect, light: Color, dark: Color, size: f32) {
    if area.width() > 0. && area.height() > 0. && size.is_finite() && size > 0. {
        list.push_shape(Shape::Procedural {
            area,
            clip: area,
            kind: Pattern::Checker { dark, size },
        }, light);
    }
}

fn expand_quad(points: [[f32; 2]; 4]) -> [[f32; 2]; 6] {
    [
        points[0], points[1], points[2], points[0], points[2], points[3],
    ]
}

fn expand_line(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(points.len().saturating_sub(1) * 6);
    for pair in points.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy);
        if length <= f32::EPSILON {
            continue;
        }
        let nx = -dy / length * 0.5;
        let ny = dx / length * 0.5;
        out.extend_from_slice(&expand_quad([
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ]));
    }
    out
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
    if vertices.is_empty() { return; }
    if let Some(previous) = batches.last_mut() {
        if previous.glyph == glyph && previous.vertices.end == vertices.start {
            previous.vertices.end = vertices.end;
            return;
        }
    }
    batches.push(DrawBatch { glyph, vertices });
}

/// Two vertex buffers and one render pass; only adjacent compatible draws merge.
pub struct Painter {
    solid_cpu: Vec<SolidVertex>,
    glyph_cpu: Vec<GlyphVertex>,
    shape_ranges: Vec<std::ops::Range<u32>>,
    draw_batches: Vec<DrawBatch>,
    srgb_target: bool,
    solid_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    solid_layout: wgpu::BindGroupLayout,
    glyph_layout: wgpu::BindGroupLayout,
    solid_uniform: wgpu::Buffer,
    glyph_uniform: wgpu::Buffer,
    solid_buffer: wgpu::Buffer,
    glyph_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    atlas: Option<wgpu::Texture>,
    atlas_view: Option<wgpu::TextureView>,
    atlas_size: [u32; 2],
    /// Solid vertices submitted for the last frame.
    pub last_solid_vertices: usize,
    /// Glyph vertices submitted for the last frame.
    pub last_glyph_vertices: usize,
    pub last_draw_calls: usize,
}

impl Painter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
                bind_group_layouts: &[bind],
                push_constant_ranges: &[],
            })
        };
        let solid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panel solids"),
            layout: Some(&layout_for("solid pipeline", &solid_layout)),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("solid_vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SolidVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4, 2=>Float32x4, 3=>Float32x4, 4=>Float32x4, 5=>Float32x4],
                }],
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
            multiview: None,
            cache: None,
        });
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panel glyphs"),
            layout: Some(&layout_for("glyph pipeline", &glyph_layout)),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("glyph_vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                    ],
                }],
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
            multiview: None,
            cache: None,
        });
        let uniform_bytes = bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]);
        Self {
            srgb_target: format.is_srgb(),
            solid_cpu: Vec::new(),
            glyph_cpu: Vec::new(),
            shape_ranges: Vec::new(),
            draw_batches: Vec::new(),
            solid_pipeline,
            glyph_pipeline,
            solid_layout,
            glyph_layout,
            solid_uniform: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("solid uniforms"),
                contents: uniform_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            glyph_uniform: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glyph uniforms"),
                contents: uniform_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            solid_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solid vertices"),
                size: (SOLID_CAPACITY * std::mem::size_of::<SolidVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            glyph_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph vertices"),
                size: (GLYPH_CAPACITY * std::mem::size_of::<GlyphVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("glyph sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            atlas: None,
            atlas_view: None,
            atlas_size: [0, 0],
            last_solid_vertices: 0,
            last_glyph_vertices: 0,
            last_draw_calls: 0,
        }
    }

    /// Upload a glyph atlas bitmap. Only needed when new glyphs were rasterised.
    pub fn upload_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        if self.atlas_size != [width, height] || self.atlas.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
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
            });
            self.atlas_view = Some(texture.create_view(&Default::default()));
            self.atlas = Some(texture);
            self.atlas_size = [width, height];
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.atlas.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
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
    ) {
        self.render_scaled(device, queue, encoder, target, list, viewport, [1., 1.]);
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
    ) {
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
                Shape::Line { points } => {
                    for vertex in expand_line(points) {
                        solids.push(SolidVertex::plain(vertex, rgba));
                    }
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
            let rgba = upload_color(glyph.color.0, self.srgb_target);
            let min = glyph.position;
            let max = [
                glyph.position[0] + glyph.size[0],
                glyph.position[1] + glyph.size[1],
            ];
            [
                (min, [glyph.uv[0], glyph.uv[1]]),
                ([max[0], min[1]], [glyph.uv[2], glyph.uv[1]]),
                (max, [glyph.uv[2], glyph.uv[3]]),
                (min, [glyph.uv[0], glyph.uv[1]]),
                (max, [glyph.uv[2], glyph.uv[3]]),
                ([min[0], max[1]], [glyph.uv[0], glyph.uv[3]]),
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
                        }
                    }
                    PaintOp::Glyph(index) if index < list.glyphs.len() => {
                        let start = (index * 6) as u32;
                        push_batch(&mut self.draw_batches, true, start..start + 6);
                    }
                    _ => {}
                }
            }
        }
        if !solids.is_empty() {
            queue.write_buffer(&self.solid_buffer, 0, bytemuck::cast_slice(&solids));
        }
        if !glyphs.is_empty() {
            queue.write_buffer(&self.glyph_buffer, 0, bytemuck::cast_slice(&glyphs));
        }
        // Logical pixels to clip space: x to the right, y downward.
        queue.write_buffer(
            &self.solid_uniform,
            0,
            bytemuck::bytes_of(&[2.0 / viewport[0], 0.0, 0.0, -2.0 / viewport[1]]),
        );
        queue.write_buffer(
            &self.glyph_uniform,
            0,
            bytemuck::bytes_of(&[2.0 / viewport[0], 0.0, 0.0, -2.0 / viewport[1]]),
        );
        let solid_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("solid bind group"),
            layout: &self.solid_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.solid_uniform.as_entire_binding(),
            }],
        });
        let glyph_bind = self.atlas_view.as_ref().map(|view| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph bind group"),
                layout: &self.glyph_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.glyph_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("panels"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
            return;
        }
        for batch in &self.draw_batches {
            if batch.glyph {
                let Some(bind) = &glyph_bind else { continue };
                pass.set_pipeline(&self.glyph_pipeline);
                pass.set_bind_group(0, bind, &[]);
                pass.set_vertex_buffer(0, self.glyph_buffer.slice(..));
            } else {
                pass.set_pipeline(&self.solid_pipeline);
                pass.set_bind_group(0, &solid_bind, &[]);
                pass.set_vertex_buffer(0, self.solid_buffer.slice(..));
            }
            pass.draw(batch.vertices.clone(), 0..1);
            self.last_draw_calls += 1;
        }
        drop(pass);
        self.solid_cpu = solids;
        self.glyph_cpu = glyphs;
    }
}

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
        let line = expand_line(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(line.len(), 12);
        assert!(line.iter().any(|p| (p[1] - 10.0).abs() < 0.01));
        assert!(expand_line(&[[1.0, 1.0], [1.0, 1.0]]).is_empty());
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
