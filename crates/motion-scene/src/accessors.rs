use crate::{invalid, schema, ImportLimits, Resource, Result};

pub(crate) struct Budget {
    pub bytes: usize,
    pub vertices: usize,
    pub indices: usize,
}
impl Budget {
    pub fn reserve(&mut self, count: usize, element: usize, limits: &ImportLimits) -> Result<()> {
        self.bytes = count
            .checked_mul(element)
            .and_then(|n| self.bytes.checked_add(n))
            .filter(|&n| n <= limits.max_retained_bytes)
            .ok_or_else(|| invalid("budget", "payload", "retained byte limit exceeded"))?;
        Ok(())
    }
}

pub(crate) struct Reader<'a> {
    pub root: &'a schema::Root,
    pub buffers: &'a [Resource],
    pub limits: &'a ImportLimits,
}

impl Reader<'_> {
    pub fn view(&self, id: usize) -> Result<&[u8]> {
        let view = self
            .root
            .buffer_views
            .get(id)
            .ok_or_else(|| invalid("buffer_view", id, "view index out of range"))?;
        let bytes = &self
            .buffers
            .get(view.buffer)
            .ok_or_else(|| invalid("buffer_view", id, "buffer index out of range"))?
            .bytes;
        let declared = self.root.buffers[view.buffer].byte_length;
        let bytes = bytes
            .get(..declared)
            .ok_or_else(|| invalid("buffer_view", id, "buffer is shorter than declared"))?;
        let end = view
            .byte_offset
            .checked_add(view.byte_length)
            .ok_or_else(|| invalid("buffer_view", id, "range overflow"))?;
        if view.byte_length == 0 {
            return Err(invalid("buffer_view", id, "empty buffer view"));
        }
        if !matches!(view.target, None | Some(34962 | 34963)) {
            return Err(invalid("buffer_view", id, "invalid GPU target"));
        }
        bytes
            .get(view.byte_offset..end)
            .ok_or_else(|| invalid("buffer_view", id, "range exceeds buffer"))
    }

    pub fn accessor(&self, id: usize, shape: &str, vertex: bool) -> Result<&schema::Accessor> {
        let a = self
            .root
            .accessors
            .get(id)
            .ok_or_else(|| invalid("accessor", id, "accessor index out of range"))?;
        if a.count == 0
            || a.count
                > if vertex {
                    self.limits.max_vertices
                } else {
                    self.limits.max_indices
                }
            || a.shape != shape
        {
            return Err(invalid("accessor", id, "invalid shape or element count"));
        }
        Ok(a)
    }

    fn elements(
        &self,
        view: usize,
        offset: usize,
        count: usize,
        width: usize,
        size: usize,
        vertex: bool,
    ) -> Result<Elements<'_>> {
        let data = self.view(view)?;
        let description = &self.root.buffer_views[view];
        let stride = description.byte_stride.unwrap_or(size);
        if !offset.is_multiple_of(width)
            || !description.byte_offset.is_multiple_of(width)
            || stride < size
            || !stride.is_multiple_of(width)
            || (vertex
                && (!offset.is_multiple_of(4)
                    || description
                        .byte_stride
                        .is_some_and(|n| !n.is_multiple_of(4))))
            || description
                .byte_stride
                .is_some_and(|n| !(4..=252).contains(&n))
            || (!vertex && description.byte_stride.is_some())
        {
            return Err(invalid("accessor", view, "invalid alignment or stride"));
        }
        let end = count
            .checked_sub(1)
            .and_then(|n| n.checked_mul(stride))
            .and_then(|n| n.checked_add(size))
            .and_then(|n| offset.checked_add(n))
            .filter(|&n| n <= data.len())
            .ok_or_else(|| invalid("accessor", view, "accessor exceeds view"))?;
        Ok(Elements {
            bytes: &data[offset..end],
            stride,
            size,
        })
    }

    /// Float and normalized U8/U16 are the core types for texture coordinates/colors.
    pub fn floats<const N: usize>(
        &self,
        id: usize,
        float_only: bool,
        budget: &mut Budget,
    ) -> Result<Vec<[f32; N]>> {
        self.float_data(id, float_only, true, false, budget)
    }

    pub fn animation_floats<const N: usize>(
        &self,
        id: usize,
        rotation: bool,
        budget: &mut Budget,
    ) -> Result<Vec<[f32; N]>> {
        let a = self
            .root
            .accessors
            .get(id)
            .ok_or_else(|| invalid("animation", id, "accessor index out of range"))?;
        if a.count > self.limits.max_animation_keys.saturating_mul(3)
            || a.buffer_view
                .and_then(|v| self.root.buffer_views.get(v))
                .is_some_and(|v| v.target.is_some())
        {
            return Err(invalid(
                "animation",
                id,
                "key count limit or animation buffer target",
            ));
        }
        self.float_data(id, !rotation, false, rotation, budget)
    }

    pub fn animation_weight_floats(&self, id: usize, budget: &mut Budget) -> Result<Vec<[f32; 1]>> {
        let a = self.accessor(id, "SCALAR", false)?;
        if a.buffer_view
            .and_then(|v| self.root.buffer_views.get(v))
            .is_some_and(|v| v.target.is_some())
        {
            return Err(invalid(
                "animation",
                id,
                "morph animation buffer cannot have a GPU target",
            ));
        }
        // Scalar count includes every target and tangent. The accessor index
        // bound and retained-byte budget apply before allocation, unlike key count.
        self.float_data(id, false, false, true, budget)
    }

    fn float_data<const N: usize>(
        &self,
        id: usize,
        float_only: bool,
        vertex: bool,
        signed: bool,
        budget: &mut Budget,
    ) -> Result<Vec<[f32; N]>> {
        let shape = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            4 => "VEC4",
            _ => return Err(invalid("accessor", id, "unsupported shape")),
        };
        let a = self.accessor(id, shape, vertex)?;
        let width = match (a.component_type, a.normalized) {
            (5126, false) => 4,
            (5121, true) if !float_only => 1,
            (5123, true) if !float_only => 2,
            (5120, true) if signed => 1,
            (5122, true) if signed => 2,
            _ => {
                return Err(invalid(
                    "accessor",
                    id,
                    "unsupported attribute component/normalization",
                ))
            }
        };
        budget.reserve(a.count, std::mem::size_of::<[f32; N]>(), self.limits)?;
        let mut out = vec![[0.; N]; a.count];
        let decode = |bytes: &[u8]| -> Result<[f32; N]> {
            let mut value = [0.; N];
            for c in 0..N {
                let b = &bytes[c * width..(c + 1) * width];
                value[c] = match a.component_type {
                    5120 => (b[0] as i8 as f32 / 127.).max(-1.),
                    5121 => b[0] as f32 / 255.,
                    5122 => (i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.).max(-1.),
                    5123 => u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.,
                    _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                };
                if !value[c].is_finite() {
                    return Err(invalid("accessor", id, "non-finite attribute"));
                }
            }
            Ok(value)
        };
        if let Some(view) = a.buffer_view {
            let elements = self.elements(view, a.byte_offset, a.count, width, N * width, vertex)?;
            for (i, item) in out.iter_mut().enumerate() {
                *item = decode(elements.at(i))?;
            }
        } else if a.byte_offset != 0 {
            return Err(invalid("accessor", id, "offset without buffer view"));
        }
        if let Some(sparse) = &a.sparse {
            let indices = self.sparse_indices(id, a.count, sparse)?;
            let values = self.elements(
                sparse.values.buffer_view,
                sparse.values.byte_offset,
                sparse.count,
                width,
                N * width,
                false,
            )?;
            for (i, index) in indices.into_iter().enumerate() {
                out[index] = decode(values.at(i))?;
            }
        }
        Ok(out)
    }

    fn sparse_indices(
        &self,
        id: usize,
        count: usize,
        sparse: &schema::Sparse,
    ) -> Result<Vec<usize>> {
        for view in [sparse.indices.buffer_view, sparse.values.buffer_view] {
            if self
                .root
                .buffer_views
                .get(view)
                .is_some_and(|v| v.target.is_some())
            {
                return Err(invalid(
                    "sparse",
                    id,
                    "sparse views cannot have GPU targets",
                ));
            }
        }
        if sparse.count == 0 || sparse.count > count {
            return Err(invalid("sparse", id, "invalid sparse count"));
        }
        let width = unsigned_width(sparse.indices.component_type)
            .ok_or_else(|| invalid("sparse", id, "invalid index component type"))?;
        let elements = self.elements(
            sparse.indices.buffer_view,
            sparse.indices.byte_offset,
            sparse.count,
            width,
            width,
            false,
        )?;
        let mut out = Vec::with_capacity(sparse.count);
        for i in 0..sparse.count {
            let index = unsigned(elements.at(i), width) as usize;
            if index >= count || out.last().is_some_and(|&previous| previous >= index) {
                return Err(invalid(
                    "sparse",
                    id,
                    "indices must be strictly increasing and in range",
                ));
            }
            out.push(index);
        }
        Ok(out)
    }

    pub fn indices(&self, id: usize, vertices: usize, budget: &mut Budget) -> Result<Vec<u32>> {
        let a = self.accessor(id, "SCALAR", false)?;
        let width = unsigned_width(a.component_type)
            .filter(|_| !a.normalized)
            .ok_or_else(|| invalid("indices", id, "indices must be unsigned and unnormalized"))?;
        budget.reserve(a.count, 4, self.limits)?;
        let mut out = vec![0; a.count];
        if let Some(view) = a.buffer_view {
            if self
                .root
                .buffer_views
                .get(view)
                .is_some_and(|v| v.target == Some(34962))
            {
                return Err(invalid(
                    "indices",
                    id,
                    "index view cannot target vertex attributes",
                ));
            }
            let elements = self.elements(view, a.byte_offset, a.count, width, width, false)?;
            for (i, item) in out.iter_mut().enumerate() {
                *item = unsigned(elements.at(i), width);
            }
        } else if a.byte_offset != 0 {
            return Err(invalid("indices", id, "offset without buffer view"));
        }
        if let Some(sparse) = &a.sparse {
            let indices = self.sparse_indices(id, a.count, sparse)?;
            let values = self.elements(
                sparse.values.buffer_view,
                sparse.values.byte_offset,
                sparse.count,
                width,
                width,
                false,
            )?;
            for (i, index) in indices.into_iter().enumerate() {
                out[index] = unsigned(values.at(i), width);
            }
        }
        let restart = match width {
            1 => 255,
            2 => 65535,
            _ => u32::MAX,
        };
        if out.iter().any(|&n| n as usize >= vertices || n == restart) {
            return Err(invalid(
                "indices",
                id,
                "vertex index out of range or reserved restart value",
            ));
        }
        Ok(out)
    }
}

struct Elements<'a> {
    bytes: &'a [u8],
    stride: usize,
    size: usize,
}
impl Elements<'_> {
    // The constructor checked the range for every element and its stride.
    fn at(&self, index: usize) -> &[u8] {
        &self.bytes[index * self.stride..index * self.stride + self.size]
    }
}
fn unsigned_width(component: u32) -> Option<usize> {
    match component {
        5121 => Some(1),
        5123 => Some(2),
        5125 => Some(4),
        _ => None,
    }
}
fn unsigned(bytes: &[u8], width: usize) -> u32 {
    match width {
        1 => bytes[0] as u32,
        2 => u16::from_le_bytes([bytes[0], bytes[1]]) as u32,
        _ => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}
