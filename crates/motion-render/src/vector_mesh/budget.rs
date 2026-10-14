//! Lyon output writes are bounded before growing either vertex buffer.
use super::VectorVertex;
use lyon_tessellation::{
    geometry_builder::{FillGeometryBuilder, GeometryBuilder, StrokeGeometryBuilder},
    FillVertex, GeometryBuilderError, StrokeVertex, VertexId,
};

pub(super) struct BoundedGeometry {
    unique: Vec<VectorVertex>,
    pub output: Vec<VectorVertex>,
    size: [f32; 2],
    transform: Option<(glam::Mat3, glam::Vec2)>,
    pub color: [f32; 4],
    limit: usize,
    start: usize,
    raw_triangles: usize,
    rejected: bool,
    #[cfg(test)]
    pub peak_unique: usize,
    #[cfg(test)]
    pub peak_output: usize,
}

pub(super) fn reserve<T>(values: &mut Vec<T>, needed: usize, limit: usize) {
    if needed > values.capacity() {
        let capacity = values
            .capacity()
            .saturating_mul(2)
            .max(128)
            .max(needed)
            .min(limit);
        values.reserve_exact(capacity - values.len());
    }
}

impl BoundedGeometry {
    pub fn new(
        output: Vec<VectorVertex>,
        size: [f32; 2],
        limit: usize,
        transform: Option<(glam::Mat3, glam::Vec2)>,
    ) -> Self {
        Self {
            unique: Vec::new(),
            output,
            size,
            transform,
            color: [0.; 4],
            limit,
            start: 0,
            raw_triangles: 0,
            rejected: false,
            #[cfg(test)]
            peak_unique: 0,
            #[cfg(test)]
            peak_output: 0,
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.rejected {
            Err(format!(
                "vector triangle vertex limit exceeded (262144; remaining output limit {})",
                self.limit
            ))
        } else {
            Ok(())
        }
    }
    fn vertex(
        &mut self,
        p: lyon_tessellation::math::Point,
    ) -> Result<VertexId, GeometryBuilderError> {
        if self.rejected || self.unique.len() >= super::MAX_TRIANGLE_VERTICES {
            self.rejected = true;
            return Err(GeometryBuilderError::TooManyVertices);
        }
        let mut position = [p.x * 2. / self.size[0], -p.y * 2. / self.size[1]];
        if let Some((matrix, origin)) = self.transform {
            let local = glam::Vec2::new(
                position[0] * self.size[0] * 0.5,
                -position[1] * self.size[1] * 0.5,
            );
            let point = matrix.transform_point2(local) - origin;
            position = [point.x * 2. / self.size[0], -point.y * 2. / self.size[1]];
        }
        let id = VertexId::from_usize(self.unique.len());
        reserve(
            &mut self.unique,
            id.to_usize() + 1,
            super::MAX_TRIANGLE_VERTICES,
        );
        self.unique.push(VectorVertex {
            position,
            color: self.color,
        });
        #[cfg(test)]
        {
            self.peak_unique = self.peak_unique.max(self.unique.len());
        }
        Ok(id)
    }
}
impl GeometryBuilder for BoundedGeometry {
    fn begin_geometry(&mut self) {
        self.unique.clear();
        self.start = self.output.len();
    }
    fn end_geometry(&mut self) {
        self.unique.clear();
    }
    fn abort_geometry(&mut self) {
        self.unique.clear();
        self.output.truncate(self.start);
    }
    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        // Lyon's triangle callback cannot return an error. Refuse the write now,
        // fail its next fallible vertex callback, and also check after completion.
        if self.rejected || self.raw_triangles + 3 > super::MAX_TRIANGLE_VERTICES {
            self.rejected = true;
            return;
        }
        self.raw_triangles += 3;
        let [Some(a), Some(b), Some(c)] =
            [a, b, c].map(|id| self.unique.get(id.to_usize()).copied())
        else {
            self.rejected = true;
            return;
        };
        let triangle = [a, b, c];
        if self.transform.is_some()
            && (0..2).any(|axis| {
                triangle.iter().all(|v| v.position[axis] < -1.)
                    || triangle.iter().all(|v| v.position[axis] > 1.)
            })
        {
            return;
        }
        if self.output.len() + 3 > self.limit {
            self.rejected = true;
            return;
        }
        let needed = self.output.len() + 3;
        reserve(&mut self.output, needed, self.limit);
        self.output.extend_from_slice(&triangle);
        #[cfg(test)]
        {
            self.peak_output = self.peak_output.max(self.output.len());
        }
    }
}
impl FillGeometryBuilder for BoundedGeometry {
    fn add_fill_vertex(&mut self, vertex: FillVertex) -> Result<VertexId, GeometryBuilderError> {
        self.vertex(vertex.position())
    }
}
impl StrokeGeometryBuilder for BoundedGeometry {
    fn add_stroke_vertex(
        &mut self,
        vertex: StrokeVertex,
    ) -> Result<VertexId, GeometryBuilderError> {
        self.vertex(vertex.position())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_model::vector::{FillRule, LineCap, LineJoin, SampledPath, SampledVector};

    fn shape(nodes: Vec<[f32; 6]>, closed: bool, width: f32) -> SampledVector {
        SampledVector { paths: vec![SampledPath { closed, nodes }], fill: Some([1.; 4]),
            fill_rule: FillRule::NonZero, stroke: Some(([1.; 4], width, LineCap::Round, LineJoin::Round, 4.)),
            trim: None, dashes: None, batches: None, root_opacity: 1., group_parameters: Default::default() }
    }
    fn rectangle() -> SampledVector {
        shape([[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]].into_iter().map(|p| [p[0], p[1], 0., 0., 0., 0.]).collect(), true, 0.)
    }

    #[test]
    fn actual_lyon_output_rejects_before_capacity_growth_and_triangle_callback_stays_bounded() {
        let mut builder = BoundedGeometry::new(vec![], [64.; 2], 5, None);
        assert!(crate::vector_mesh::tessellate_into(&rectangle(), 1., &mut builder).is_err());
        assert!(builder.peak_output <= 5);
        assert!(builder.output.capacity() <= 5);
        assert!(builder.peak_unique <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
        let capacity = builder.output.capacity();
        for _ in 0..10000 { builder.add_triangle(VertexId(0), VertexId(1), VertexId(2)); }
        assert_eq!(builder.output.capacity(), capacity);
        assert!(builder.vertex(lyon_tessellation::math::point(0., 0.)).is_err());
    }

    #[test]
    fn extreme_round_expansion_with_valid_node_count_has_bounded_raw_and_output_peaks() {
        let nodes = (0..motion_model::masks::MAX_MASK_NODES).map(|i| {
            [if i % 2 == 0 { -8. } else { 8. }, (i % 3) as f32 * 8., 0., 0., 0., 0.]
        }).collect();
        let vector = shape(nodes, true, motion_model::masks::MAX_MASK_DISTANCE * 2.);
        let mut builder = BoundedGeometry::new(vec![], [64.; 2], crate::vector_mesh::MAX_TRIANGLE_VERTICES, None);
        assert!(crate::vector_mesh::tessellate_into(&vector, 1., &mut builder).is_err());
        assert!(builder.peak_output <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
        assert!(builder.peak_unique <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
        assert!(builder.output.capacity() <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
        assert!(builder.unique.capacity() <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
    }

    #[test]
    fn maximum_expansion_and_collinear_nodes_remain_legal_when_actual_geometry_fits() {
        let corners = [[-8., -8.], [8., -8.], [8., 8.], [-8., 8.]];
        let nodes = (0..motion_model::masks::MAX_MASK_NODES).map(|i| {
            let side = i / 512; let t = (i % 512) as f32 / 512.;
            let a = corners[side]; let b = corners[(side + 1) % 4];
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, 0., 0., 0., 0.]
        }).collect();
        let vector = shape(nodes, true, motion_model::masks::MAX_MASK_DISTANCE * 2.);
        assert!(motion_model::masks::MAX_MASK_NODES as f32 * vector.stroke.unwrap().1 > crate::vector_mesh::MAX_TRIANGLE_VERTICES as f32);
        let vertices = crate::vector_mesh::tessellate(&vector, [64.; 2], 1.).unwrap();
        assert!(!vertices.is_empty() && vertices.len() < crate::vector_mesh::MAX_TRIANGLE_VERTICES);
    }

    #[test]
    fn culled_batches_do_not_spend_visible_output_budget_but_raw_capacity_is_still_guarded() {
        let mut vector = rectangle();
        vector.batches = Some((0..256).map(|_| motion_model::vector::groups::PaintBatch {
            vector: Box::new(rectangle()),
            transform: glam::Mat3::from_translation(glam::Vec2::splat(1000.)).to_cols_array(), scopes: vec![],
        }).chain(std::iter::once(motion_model::vector::groups::PaintBatch {
            vector: Box::new(rectangle()), transform: glam::Mat3::IDENTITY.to_cols_array(), scopes: vec![],
        })).collect());
        let geometry = crate::vector_mesh::rasterize_with_limit(&vector, [64.; 2], 1., [0.; 2], 6).unwrap();
        assert_eq!(geometry.vertices.len(), 6);
        assert!(geometry.vertices.capacity() <= 6);
        let mut nodes = Vec::new();
        for i in 0..motion_model::masks::MAX_MASK_NODES { nodes.push([if i % 2 == 0 { -8. } else { 8. }, (i % 3) as f32 * 8., 0., 0., 0., 0.]); }
        let mut builder = BoundedGeometry::new(vec![], [64.; 2], 0, Some((glam::Mat3::from_translation(glam::Vec2::splat(1e8)), glam::Vec2::ZERO)));
        assert!(crate::vector_mesh::tessellate_into(&shape(nodes, true, 64000.), 1., &mut builder).is_err());
        assert_eq!(builder.peak_output, 0);
        assert!(builder.peak_unique <= crate::vector_mesh::MAX_TRIANGLE_VERTICES);
    }
}
