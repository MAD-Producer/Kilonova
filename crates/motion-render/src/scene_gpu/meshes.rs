use super::{charge, invalid, MeshBuffers, Result, SceneGpuLimits, Vertex};
use crate::AllocationContext;
use glam::Vec3;

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
