// Independent rank-five indexing admission. Not compiled or GPU-qualified yet.
fn padded(shape: &[u32]) -> [u32; 5] {
    let mut result = [1; 5];
    result[5 - shape.len()..].copy_from_slice(shape);
    result
}
fn contiguous(shape: &[u32]) -> Result<Vec<i64>> {
    let mut strides = vec![0; shape.len()];
    let mut stride = 1i64;
    for i in (0..shape.len()).rev() {
        strides[i] = stride;
        stride = stride
            .checked_mul(i64::from(shape[i]))
            .ok_or("index stride overflow")?;
    }
    ensure(
        stride <= i32::MAX as i64,
        "signed indexing element range exceeded",
    )?;
    Ok(strides)
}
fn axis(value: i32, rank: usize) -> Result<usize> {
    let resolved = if value < 0 {
        i64::from(value) + rank as i64
    } else {
        i64::from(value)
    };
    ensure(
        resolved >= 0 && resolved < rank as i64,
        "index axis outside tensor rank",
    )?;
    Ok(resolved as usize)
}
fn mapping(shape: &[u32], output: &[u32]) -> Result<Vec<i64>> {
    ensure(
        shape.len() <= output.len(),
        "broadcast cannot remove a source axis",
    )?;
    let mut result = vec![0; output.len()];
    let strides = contiguous(shape)?;
    let prefix = output.len() - shape.len();
    for (i, &d) in shape.iter().enumerate() {
        ensure(
            d == 1 || d == output[prefix + i],
            "broadcast axis extent mismatch",
        )?;
        result[prefix + i] = if d == 1 { 0 } else { strides[i] };
    }
    Ok(result)
}
fn affine(
    p: &mut [u32; 64],
    input_index: usize,
    strides: &[i64],
    base: i64,
    coordinate_shape: &[u32],
    source_elements: u32,
) -> Result<()> {
    ensure(
        strides.len() == coordinate_shape.len() && strides.len() <= 5,
        "affine rank mismatch",
    )?;
    let mut minimum = base as i128;
    let mut maximum = minimum;
    ensure(
        base >= 0 && base <= i32::MAX as i64,
        "affine base outside signed range",
    )?;
    let offset = [12, 20, 28][input_index] + 5 - strides.len();
    for (i, (&stride, &d)) in strides.iter().zip(coordinate_shape).enumerate() {
        let stride = i32::try_from(stride).map_err(|_| "affine stride exceeds i32")?;
        let term = i128::from(stride) * i128::from(d - 1);
        ensure(
            term >= i32::MIN as i128 && term <= i32::MAX as i128,
            "affine product exceeds signed range",
        )?;
        minimum += term.min(0);
        maximum += term.max(0);
        p[offset + i] = stride as u32;
    }
    ensure(
        minimum >= 0 && maximum < i128::from(source_elements) && maximum <= i32::MAX as i128,
        "affine mapping escapes source storage",
    )?;
    p[48 + input_index] = base as u32;
    Ok(())
}
impl Case {
    pub fn plan(&self, limits: &wgpu::Limits, budget: u64) -> Result<Plan> {
        ensure(
            !self.inputs.is_empty() && self.inputs.len() <= 3,
            "invalid index input count",
        )?;
        for tensor in self.inputs.iter().chain([&self.expected]) {
            tensor.bytes()?;
            contiguous(&tensor.shape)?;
        }
        let output = &self.expected.shape;
        let first = &self.inputs[0].shape;
        let mut p = [0; 64];
        let count = product(output)?;
        p[0] = count;
        p[4..9].copy_from_slice(&padded(output));
        let maximum = limits.max_compute_workgroups_per_dimension;
        ensure(
            maximum > 0
                && limits.max_compute_workgroup_size_x >= 64
                && limits.max_compute_invocations_per_workgroup >= 64
                && limits.max_storage_buffers_per_shader_stage >= 4
                && limits.max_uniform_buffers_per_shader_stage >= 1,
            "index compute limits insufficient",
        )?;
        let groups = count.div_ceil(64);
        let x = groups.min(maximum);
        let y = groups.div_ceil(x);
        ensure(y <= maximum, "index dispatch exceeds device limits")?;
        p[1] = x.checked_mul(64).ok_or("index dispatch stride overflow")?;
        let entry = match &self.operation {
            Operation::Add | Operation::Mul | Operation::Div => {
                ensure(
                    self.inputs.len() == 2,
                    "binary index operation requires two inputs",
                )?;
                let second = &self.inputs[1].shape;
                let rank = first.len().max(second.len());
                let a = padded(first);
                let b = padded(second);
                let mut shape = vec![1; rank];
                for i in 0..rank {
                    let j = 5 - rank + i;
                    ensure(
                        a[j] == b[j] || a[j] == 1 || b[j] == 1,
                        "binary broadcast mismatch",
                    )?;
                    shape[i] = a[j].max(b[j]);
                }
                ensure(
                    &shape == output,
                    "binary output is not the exact broadcast result",
                )?;
                for (i, tensor) in self.inputs.iter().enumerate() {
                    affine(
                        &mut p,
                        i,
                        &mapping(&tensor.shape, output)?,
                        0,
                        output,
                        product(&tensor.shape)?,
                    )?;
                }
                match self.operation {
                    Operation::Add => "binary_add",
                    Operation::Mul => "binary_mul",
                    _ => "binary_div",
                }
            }
            Operation::Relu => {
                ensure(
                    self.inputs.len() == 1 && first == output,
                    "Relu shape/input mismatch",
                )?;
                affine(&mut p, 0, &contiguous(first)?, 0, output, product(first)?)?;
                "relu"
            }
            Operation::Expand => {
                ensure(self.inputs.len() == 1, "Expand requires one data tensor")?;
                affine(
                    &mut p,
                    0,
                    &mapping(first, output)?,
                    0,
                    output,
                    product(first)?,
                )?;
                "index_copy"
            }
            Operation::Transpose { permutation } => {
                ensure(
                    self.inputs.len() == 1 && permutation.len() == first.len(),
                    "transpose rank/input mismatch",
                )?;
                let mut sorted = permutation.clone();
                sorted.sort_unstable();
                ensure(
                    sorted == (0..first.len() as u32).collect::<Vec<_>>(),
                    "transpose permutation is not bijective",
                )?;
                let shape: Vec<_> = permutation.iter().map(|&i| first[i as usize]).collect();
                ensure(&shape == output, "transpose output shape differs")?;
                let strides = contiguous(first)?;
                let mapped: Vec<_> = permutation.iter().map(|&i| strides[i as usize]).collect();
                affine(&mut p, 0, &mapped, 0, output, product(first)?)?;
                "index_copy"
            }
            Operation::GatherScalar {
                axis: chosen,
                index,
            } => {
                ensure(
                    self.inputs.len() == 1,
                    "scalar Gather requires one data tensor",
                )?;
                let chosen = axis(*chosen, first.len())?;
                let size = i64::from(first[chosen]);
                let at = if *index < 0 {
                    index.checked_add(size).ok_or("Gather index overflow")?
                } else {
                    *index
                };
                ensure(at >= 0 && at < size, "Gather scalar index out of bounds")?;
                let mut shape = first.clone();
                shape.remove(chosen);
                ensure(&shape == output, "Gather output shape differs")?;
                let mut strides = contiguous(first)?;
                let base = at
                    .checked_mul(strides.remove(chosen))
                    .ok_or("Gather base overflow")?;
                affine(&mut p, 0, &strides, base, output, product(first)?)?;
                "index_copy"
            }
            Operation::Slice {
                starts,
                ends,
                axes,
                steps,
            } => {
                ensure(
                    self.inputs.len() == 1
                        && starts.len() == ends.len()
                        && starts.len() == axes.len()
                        && starts.len() == steps.len()
                        && starts.len() <= first.len(),
                    "Slice control lengths differ",
                )?;
                let mut mapped = contiguous(first)?;
                let mut shape = first.clone();
                let mut base = 0i64;
                let mut used = std::collections::HashSet::new();
                for i in 0..axes.len() {
                    let chosen = axis(axes[i], first.len())?;
                    ensure(used.insert(chosen), "Slice axes repeat")?;
                    let size = i128::from(first[chosen]);
                    let step = i128::from(steps[i]);
                    ensure(step != 0, "Slice step is zero")?;
                    let normalize = |v: i64| {
                        let v = i128::from(v);
                        let v = if v < 0 { v + size } else { v };
                        if step > 0 {
                            v.clamp(0, size)
                        } else {
                            v.clamp(-1, size - 1)
                        }
                    };
                    let start = normalize(starts[i]);
                    let end = normalize(ends[i]);
                    let distance = if step > 0 { end - start } else { start - end };
                    let extent = if distance <= 0 {
                        0
                    } else {
                        (distance + step.abs() - 1) / step.abs()
                    };
                    ensure(
                        extent > 0 && extent <= u32::MAX as i128,
                        "Slice empty or oversized extent unsupported",
                    )?;
                    shape[chosen] = extent as u32;
                    let delta = start * i128::from(mapped[chosen]);
                    base = base
                        .checked_add(i64::try_from(delta).map_err(|_| "Slice base overflow")?)
                        .ok_or("Slice combined base overflow")?;
                    mapped[chosen] = if extent == 1 {
                        0
                    } else {
                        i64::try_from(i128::from(mapped[chosen]) * step)
                            .map_err(|_| "Slice stride overflow")?
                    };
                }
                ensure(&shape == output, "Slice output shape differs")?;
                affine(&mut p, 0, &mapped, base, output, product(first)?)?;
                "index_copy"
            }
            Operation::Concat { axis: chosen } => {
                ensure(
                    self.inputs.len() >= 2,
                    "Concat requires two or three data tensors",
                )?;
                let chosen = axis(*chosen, first.len())?;
                let mut shape = first.clone();
                shape[chosen] = 0;
                for tensor in &self.inputs {
                    ensure(
                        tensor.shape.len() == first.len()
                            && tensor
                                .shape
                                .iter()
                                .enumerate()
                                .all(|(i, &d)| i == chosen || d == first[i]),
                        "Concat non-axis dimensions differ",
                    )?;
                    shape[chosen] = shape[chosen]
                        .checked_add(tensor.shape[chosen])
                        .ok_or("Concat output axis overflow")?;
                }
                ensure(&shape == output, "Concat output shape differs")?;
                p[42] = (5 - first.len() + chosen) as u32;
                for (i, tensor) in self.inputs.iter().enumerate() {
                    p[43 + i] = tensor.shape[chosen];
                    affine(
                        &mut p,
                        i,
                        &contiguous(&tensor.shape)?,
                        0,
                        &tensor.shape,
                        product(&tensor.shape)?,
                    )?;
                }
                "concat"
            }
        };
        let output_bytes = self.expected.bytes()?;
        let mut bytes = output_bytes
            .checked_mul(2)
            .and_then(|v| v.checked_add(256))
            .ok_or("index output allocation overflow")?;
        for tensor in self.inputs.iter().chain([&self.expected]) {
            let size = tensor.bytes()?;
            ensure(
                size <= limits.max_storage_buffer_binding_size && size <= limits.max_buffer_size,
                "index buffer exceeds device limit",
            )?;
        }
        for tensor in &self.inputs {
            bytes = bytes
                .checked_add(tensor.bytes()?)
                .ok_or("index input allocation overflow")?;
        }
        bytes = bytes
            .checked_add((3 - self.inputs.len()) as u64 * 4)
            .ok_or("index dummy allocation overflow")?;
        ensure(bytes <= budget, "index logical buffer budget exhausted")?;
        Ok(Plan {
            entry,
            parameters: p,
            dispatch: [x, y, 1],
            output_bytes,
            admitted_buffer_bytes: bytes,
        })
    }
}
