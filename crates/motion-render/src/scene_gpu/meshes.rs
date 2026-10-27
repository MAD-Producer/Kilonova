use super::{charge, invalid, MeshBuffers, Result, SceneGpuLimits, Vertex};
use crate::AllocationContext;
use glam::Vec3;

/// One immutable vertex buffer for this node and sample. Pending encodes never
/// observe a later frame's displacement through shared queue writes.
pub(super) fn deformed(
    context: &AllocationContext,
    source: &motion_scene::Primitive,
    weights: &[f32],
    palette: Option<&[glam::Mat4]>,
) -> Result<(crate::AdmittedBuffer, Vec3)> {
    let mut attributes = if let Some(palette) = palette {
        source.skin_attributes(weights, palette)?
    } else {
        source.morph_attributes(weights)?
    };
    // Normalize directions before interpolation, with f64 magnitude evaluation
    // so large but finite deltas cannot overflow a native dot product.
    let unit = |v: [f32; 3], vertex| -> Result<[f32; 3]> {
        let length = v.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>().sqrt();
        if length == 0. || !length.is_finite() {
            return Err(invalid(
                "morph_basis",
                vertex,
                "zero or nonfinite displaced shading direction",
            ));
        }
        Ok(v.map(|v| (f64::from(v) / length) as f32))
    };
    if let Some(normals) = &mut attributes.normals {
        for (i, n) in normals.iter_mut().enumerate() {
            *n = unit(*n, i)?;
        }
        if let Some(tangents) = &mut attributes.tangents {
            for (i, t) in tangents.iter_mut().enumerate() {
                let [x, y, z, w] = *t;
                let [x, y, z] = unit([x, y, z], i)?;
                *t = [x, y, z, w];
            }
        }
    }
    let mut vertices = Vec::new();
    vertices
        .try_reserve_exact(attributes.positions.len())
        .map_err(|_| invalid("morph", 0, "vertex staging allocation failed"))?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for (i, &position) in attributes.positions.iter().enumerate() {
        for c in 0..3 {
            min[c] = min[c].min(f64::from(position[c]));
            max[c] = max[c].max(f64::from(position[c]));
        }
        vertices.push(Vertex {
            position,
            normal: attributes.normals.as_ref().map_or([0.; 3], |a| a[i]),
            tangent: attributes.tangents.as_ref().map_or([0.; 4], |a| a[i]),
            uv0: source.texcoords[0].as_ref().map_or([0.; 2], |a| a[i]),
            uv1: source.texcoords[1].as_ref().map_or([0.; 2], |a| a[i]),
            color: source.colors.as_ref().map_or([1.; 4], |a| a[i]),
        });
    }
    let center = Vec3::from_array(std::array::from_fn(|c| ((min[c] + max[c]) * 0.5) as f32));
    let buffer = context.buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(if palette.is_some() {
            "immutable scene skin vertices"
        } else {
            "immutable scene morph vertices"
        }),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })?;
    Ok((buffer, center))
}

pub(super) fn prepare(
    context: &AllocationContext,
    source: &motion_scene::Scene,
    limits: SceneGpuLimits,
    bytes: &mut u64,
) -> Result<Vec<Vec<MeshBuffers>>> {
    let mut meshes = Vec::new();
    for (mesh_id, mesh) in source.meshes().iter().enumerate() {
        let mut primitives = Vec::new();
        for p in &mesh.primitives {
            if p.material.is_some_and(|id|matches!(&source.materials()[id].model,motion_scene::MaterialModel::Wavefront(m) if !m.maps.is_empty())) && p.texcoords[0].is_none(){return Err(super::unsupported(mesh_id,"textured OBJ mesh requires UV coordinates"));}
            let cost = (p.positions.len() as u64)
                .checked_mul(std::mem::size_of::<Vertex>() as u64)
                .and_then(|v| v.checked_add(p.indices.len() as u64 * 4))
                .ok_or_else(|| invalid("mesh", mesh_id, "mesh byte count overflow"))?;
            charge(bytes, cost, limits)?;
            if (p.positions.len() as u64) * std::mem::size_of::<Vertex>() as u64
                > context.device().limits().max_buffer_size
                || p.indices.len() as u64 * 4 > context.device().limits().max_buffer_size
            {
                return Err(super::unsupported(
                    mesh_id,
                    "mesh buffer exceeds native capacity",
                ));
            }
            let mut vertices = Vec::new();
            vertices
                .try_reserve_exact(p.positions.len())
                .map_err(|_| invalid("mesh", mesh_id, "vertex staging allocation failed"))?;
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            for (i, &position) in p.positions.iter().enumerate() {
                for a in 0..3 {
                    min[a] = min[a].min(position[a] as f64);
                    max[a] = max[a].max(position[a] as f64);
                }
                vertices.push(Vertex {
                    position,
                    normal: p.normals.as_ref().map_or([0.; 3], |v| v[i]),
                    tangent: p.tangents.as_ref().map_or([0.; 4], |v| v[i]),
                    uv0: p.texcoords[0].as_ref().map_or([0.; 2], |v| v[i]),
                    uv1: p.texcoords[1].as_ref().map_or([0.; 2], |v| v[i]),
                    color: p.colors.as_ref().map_or([1.; 4], |v| v[i]),
                });
            }
            let vertices = context.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene mesh vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })?;
            let indices = context.buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene triangle indices"),
                contents: bytemuck::cast_slice(&p.indices),
                usage: wgpu::BufferUsages::INDEX,
            })?;
            primitives.push(MeshBuffers {
                vertices,
                indices,
                index_count: u32::try_from(p.indices.len())
                    .map_err(|_| invalid("mesh", mesh_id, "index count exceeds u32"))?,
                material: p.material.unwrap_or(source.materials().len()),
                normal: p.normals.is_some(),
                tangent: p.normals.is_some() && p.tangents.is_some(),
                center: Vec3::from_array(std::array::from_fn(|i| ((min[i] + max[i]) * 0.5) as f32)),
            });
        }
        meshes.push(primitives);
    }
    Ok(meshes)
}
