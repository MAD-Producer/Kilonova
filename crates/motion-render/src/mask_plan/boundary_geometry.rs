//! Source-space measurement sections of the actual mask triangle raster.
//! These are geometric sections before feather, inversion and compositing;
//! they are not an opacity contour or a bounding-box approximation.
use super::{VectorVertex, shape_vector, vector_mesh};
use crate::RenderError;

pub struct BoundaryGeometry {
    vertices: Vec<VectorVertex>,
    size: [f32; 2],
    center: [f32; 2],
}

impl BoundaryGeometry {
    pub fn new(
        nodes: Vec<[f32; 6]>,
        source_rect: [f32; 4],
        size: [f32; 2],
        expansion: f32,
        density: f32,
    ) -> Result<Self, RenderError> {
        let center = [
            source_rect[0] + source_rect[2] * 0.5,
            source_rect[1] + source_rect[3] * 0.5,
        ];
        if nodes.len() > motion_model::masks::MAX_MASK_NODES
            || nodes.iter().flatten().any(|v| !v.is_finite())
            || source_rect.iter().any(|v| !v.is_finite())
            || source_rect[2..].iter().any(|v| *v <= 0.)
            || center.iter().any(|v| !v.is_finite())
            || size.iter().any(|v| !v.is_finite() || *v <= 0.)
            || !expansion.is_finite()
            || expansion.abs() > 32000.
            || !density.is_finite()
            || density <= 0.
        {
            return Err("invalid mask boundary geometry inputs".into());
        }
        let vector = shape_vector(nodes, source_rect, expansion);
        let vertices = vector_mesh::tessellate_with_limit(
            &vector,
            size,
            density,
            vector_mesh::MAX_TRIANGLE_VERTICES,
        )?;
        if vertices
            .iter()
            .flat_map(|vertex| vertex.position)
            .any(|v| !v.is_finite())
        {
            return Err("non-finite normalized mask boundary geometry".into());
        }
        Ok(Self {
            vertices,
            size,
            center,
        })
    }

    /// Intervals along `axis`, intersecting the perpendicular coordinate `at`.
    /// Positive expansion unions the white fill/stroke. Negative expansion
    /// subtracts the black stroke, exactly as their raster draw order does.
    /// Empty output is a normal empty section, including an eroded-away mask.
    pub fn section(&self, axis: usize, at: f32) -> Result<Vec<[f32; 2]>, RenderError> {
        if axis > 1 || !at.is_finite() {
            return Err("invalid mask boundary section axis or coordinate".into());
        }
        let cross = 1 - axis;
        let (mut white, mut black) = (Vec::new(), Vec::new());
        for triangle in self.vertices.chunks_exact(3) {
            let points: [[f64; 2]; 3] = std::array::from_fn(|i| {
                [
                    f64::from(triangle[i].position[0]) * f64::from(self.size[0]) * 0.5
                        + f64::from(self.center[0]),
                    -f64::from(triangle[i].position[1]) * f64::from(self.size[1]) * 0.5
                        + f64::from(self.center[1]),
                ]
            });
            let area = (points[1][0] - points[0][0]) * (points[2][1] - points[0][1])
                - (points[1][1] - points[0][1]) * (points[2][0] - points[0][0]);
            if area == 0. {
                continue;
            }
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            let at = f64::from(at);
            for edge in 0..3 {
                let a = points[edge];
                let b = points[(edge + 1) % 3];
                if a[cross] == at && b[cross] == at {
                    lo = lo.min(a[axis]).min(b[axis]);
                    hi = hi.max(a[axis]).max(b[axis]);
                } else if (a[cross] <= at && at < b[cross]) || (b[cross] <= at && at < a[cross]) {
                    let value =
                        a[axis] + (b[axis] - a[axis]) * (at - a[cross]) / (b[cross] - a[cross]);
                    lo = lo.min(value);
                    hi = hi.max(value);
                }
            }
            if lo < hi {
                let intervals = if triangle[0].color[0] > 0.5 {
                    &mut white
                } else {
                    &mut black
                };
                intervals.push([lo as f32, hi as f32]);
            }
        }
        let white = union(white);
        let black = union(black);
        let (mut result, mut first) = (Vec::new(), 0);
        for [lo, hi] in white {
            while first < black.len() && black[first][1] <= lo {
                first += 1;
            }
            let mut start = lo;
            for &[cut_lo, cut_hi] in &black[first..] {
                if cut_lo >= hi {
                    break;
                }
                if cut_lo > start {
                    result.push([start, cut_lo.min(hi)]);
                }
                start = start.max(cut_hi);
                if start >= hi {
                    break;
                }
            }
            if start < hi {
                result.push([start, hi]);
            }
        }
        Ok(result)
    }
}

fn union(mut intervals: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    intervals.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1])));
    let mut result: Vec<[f32; 2]> = Vec::with_capacity(intervals.len());
    for next in intervals {
        if let Some(last) = result.last_mut().filter(|last| next[0] <= last[1]) {
            last[1] = last[1].max(next[1]);
        } else {
            result.push(next);
        }
    }
    result
}

/// Farthest sample offset of the same discrete kernel used by GAUSSIAN.
/// The result is in raster pixels and excludes the sampler's filter footprint.
/// It describes sampling support, not a hard alpha contour.
pub fn feather_support(feather: f32) -> Result<f32, RenderError> {
    if !feather.is_finite() || feather < 0. {
        return Err("invalid mask feather support".into());
    }
    let uniform = super::gaussian_uniform(1, 1, 0, feather);
    let sigma = uniform.params[0][2];
    let step = uniform.params[0][3];
    Ok((0..=16)
        .map(|i| i as f32 * step)
        .filter(|distance| *distance <= 3. * sigma + 0.0001)
        .fold(0., f32::max))
}
