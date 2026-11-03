use super::{clip_triangle, Shape};
use crate::Rect;

#[derive(Clone, Copy, Debug)]
struct Transform {
    origin: [f32; 2],
    scale: f32,
}

/// An instance of immutable process-lifetime geometry. Coordinates are evaluated
/// in their original operation order, including a node canvas's outer transform.
/// No mesh, transform cache, or heap allocation is created per instance.
#[derive(Clone, Copy, Debug)]
pub struct StaticMesh {
    triangles: &'static [[[f32; 2]; 3]],
    transforms: [Transform; 2],
    count: usize,
}

impl StaticMesh {
    pub fn new(triangles: &'static [[[f32; 2]; 3]], origin: [f32; 2], scale: f32) -> Self {
        Self {
            triangles,
            transforms: [Transform { origin, scale }; 2],
            count: 1,
        }
    }

    pub fn point_count(&self) -> usize {
        self.triangles.len() * 3
    }

    pub fn triangles(&self) -> impl Iterator<Item = [[f32; 2]; 3]> + '_ {
        self.triangles.iter().map(|triangle| {
            triangle.map(|mut point| {
                for transform in &self.transforms[..self.count] {
                    point = [
                        transform.origin[0] + point[0] * transform.scale,
                        transform.origin[1] + point[1] * transform.scale,
                    ];
                }
                point
            })
        })
    }

    pub fn points(&self) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.triangles().flatten()
    }

    /// Retain both icon placement and the ordinary node-content transform.
    /// Further nesting materializes the same coordinates rather than combining
    /// transforms (which would change f32 rounding) or growing a retained cache.
    pub fn transformed(mut self, origin: [f32; 2], scale: f32) -> Shape {
        if self.count < self.transforms.len() {
            self.transforms[self.count] = Transform { origin, scale };
            self.count += 1;
            Shape::Mesh(self)
        } else {
            Shape::Triangles {
                points: self
                    .points()
                    .map(|p| [origin[0] + p[0] * scale, origin[1] + p[1] * scale])
                    .collect(),
            }
        }
    }

    pub(super) fn clipped(self, area: Rect) -> Option<Shape> {
        if self.point_count() == 0 {
            return None;
        }
        if self.points().all(|point| area.contains(point)) {
            return Some(Shape::Mesh(self));
        }
        let mut points = Vec::new();
        for triangle in self.triangles() {
            clip_triangle(triangle, area, |triangle| points.extend(triangle));
        }
        (!points.is_empty()).then_some(Shape::Triangles { points })
    }
}
