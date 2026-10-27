use crate::{
    accessors::{Budget, Reader},
    invalid, schema, unsupported, AlphaMode, ImportLimits, Material, Node, Primitive, Result,
    TextureSlot,
};
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeSet;

pub(crate) fn material(id: usize, m: &schema::Material, textures: usize) -> Result<Material> {
    let slot = |s: &schema::Slot| -> Result<TextureSlot> {
        if s.index >= textures {
            return Err(invalid("material", id, "texture index out of range"));
        }
        if s.tex_coord > 1 {
            return Err(unsupported(
                "material",
                id,
                "texture coordinate set greater than one",
            ));
        }
        Ok(TextureSlot {
            texture: s.index,
            texcoord: s.tex_coord,
        })
    };
    let mut out = Material::default();
    if let Some(p) = &m.pbr_metallic_roughness {
        out.base_color = p.base_color_factor;
        out.metallic = p.metallic_factor;
        out.roughness = p.roughness_factor;
        out.base_color_texture = p.base_color_texture.as_ref().map(&slot).transpose()?;
        out.metallic_roughness_texture = p
            .metallic_roughness_texture
            .as_ref()
            .map(&slot)
            .transpose()?;
    }
    out.normal_texture = m
        .normal_texture
        .as_ref()
        .map(|n| slot(&n.slot))
        .transpose()?;
    out.normal_scale = m.normal_texture.as_ref().map_or(1., |n| n.scale);
    out.occlusion_texture = m
        .occlusion_texture
        .as_ref()
        .map(|n| slot(&n.slot))
        .transpose()?;
    out.occlusion_strength = m.occlusion_texture.as_ref().map_or(1., |n| n.strength);
    out.emissive_texture = m.emissive_texture.as_ref().map(slot).transpose()?;
    out.emissive = m.emissive_factor;
    out.double_sided = m.double_sided;
    out.alpha_cutoff = m.alpha_cutoff;
    out.alpha = match m.alpha_mode.as_deref().unwrap_or("OPAQUE") {
        "OPAQUE" => AlphaMode::Opaque,
        "MASK" => AlphaMode::Mask,
        "BLEND" => AlphaMode::Blend,
        _ => return Err(invalid("material", id, "invalid alpha mode")),
    };
    if out
        .base_color
        .iter()
        .chain(&out.emissive)
        .chain([&out.metallic, &out.roughness, &out.occlusion_strength])
        .any(|n| !n.is_finite() || !(0. ..=1.).contains(n))
        || !out.alpha_cutoff.is_finite()
        || out.alpha_cutoff < 0.
        || !out.normal_scale.is_finite()
    {
        return Err(invalid("material", id, "invalid material factors"));
    }
    Ok(out)
}

pub(crate) fn primitive(
    id: usize,
    p: &schema::Primitive,
    reader: &Reader<'_>,
    materials: &[Material],
    budget: &mut Budget,
) -> Result<Primitive> {
    if p.mode.unwrap_or(4) != 4 {
        return Err(unsupported(
            "primitive",
            id,
            "only triangle lists are implemented",
        ));
    }
    if p.material.is_some_and(|n| n >= materials.len()) {
        return Err(invalid("primitive", id, "invalid material index"));
    }
    for name in p.attributes.keys() {
        if ![
            "POSITION",
            "NORMAL",
            "TANGENT",
            "TEXCOORD_0",
            "TEXCOORD_1",
            "COLOR_0",
        ]
        .contains(&name.as_str())
        {
            return Err(unsupported(
                "primitive",
                id,
                "attribute outside static import profile",
            ));
        }
    }
    let mut used_views = BTreeSet::new();
    for &accessor in p.attributes.values() {
        let a = reader
            .root
            .accessors
            .get(accessor)
            .ok_or_else(|| invalid("primitive", id, "attribute accessor out of range"))?;
        if let Some(view) = a.buffer_view {
            let v = reader
                .root
                .buffer_views
                .get(view)
                .ok_or_else(|| invalid("primitive", id, "attribute view out of range"))?;
            if v.target == Some(34963) || (!used_views.insert(view) && v.byte_stride.is_none()) {
                return Err(invalid(
                    "primitive",
                    id,
                    "shared vertex view needs stride and cannot target indices",
                ));
            }
        }
    }
    let pos_id = *p
        .attributes
        .get("POSITION")
        .ok_or_else(|| invalid("primitive", id, "missing POSITION"))?;
    let positions = reader.floats::<3>(pos_id, true, budget)?;
    budget.vertices = budget
        .vertices
        .checked_add(positions.len())
        .filter(|&n| n <= reader.limits.max_vertices)
        .ok_or_else(|| invalid("budget", "vertices", "total vertex limit exceeded"))?;
    let accessor = &reader.root.accessors[pos_id];
    let (min, max) = match (&accessor.min, &accessor.max) {
        (Some(min), Some(max)) if min.len() == 3 && max.len() == 3 => (min, max),
        _ => return Err(invalid("primitive", id, "POSITION requires min/max bounds")),
    };
    for c in 0..3 {
        let actual_min = positions.iter().map(|p| p[c]).fold(f32::INFINITY, f32::min);
        let actual_max = positions
            .iter()
            .map(|p| p[c])
            .fold(f32::NEG_INFINITY, f32::max);
        if min[c] != actual_min || max[c] != actual_max {
            return Err(invalid(
                "primitive",
                id,
                "POSITION bounds do not match decoded data",
            ));
        }
    }
    let normals = p
        .attributes
        .get("NORMAL")
        .map(|&a| reader.floats::<3>(a, true, budget))
        .transpose()?;
    let tangents = p
        .attributes
        .get("TANGENT")
        .map(|&a| reader.floats::<4>(a, true, budget))
        .transpose()?;
    let texcoords = [
        p.attributes
            .get("TEXCOORD_0")
            .map(|&a| reader.floats::<2>(a, false, budget))
            .transpose()?,
        p.attributes
            .get("TEXCOORD_1")
            .map(|&a| reader.floats::<2>(a, false, budget))
            .transpose()?,
    ];
    let colors = p
        .attributes
        .get("COLOR_0")
        .map(|&a| {
            if reader
                .root
                .accessors
                .get(a)
                .is_some_and(|a| a.shape == "VEC3")
            {
                let values = reader.floats::<3>(a, false, budget)?;
                budget.reserve(values.len(), 4, reader.limits)?;
                Ok(values.into_iter().map(|[r, g, b]| [r, g, b, 1.]).collect())
            } else {
                reader.floats::<4>(a, false, budget)
            }
        })
        .transpose()?;
    if normals.as_ref().is_some_and(|v| v.len() != positions.len())
        || tangents
            .as_ref()
            .is_some_and(|v| v.len() != positions.len())
        || texcoords
            .iter()
            .flatten()
            .any(|v| v.len() != positions.len())
        || colors.as_ref().is_some_and(|v| v.len() != positions.len())
    {
        return Err(invalid("primitive", id, "attribute counts differ"));
    }
    if normals.as_ref().is_some_and(|v| {
        v.iter()
            .any(|&n| (Vec3::from_array(n).length_squared() - 1.).abs() > 0.001)
    }) || tangents.as_ref().is_some_and(|v| {
        v.iter().any(|t| {
            ![1., -1.].contains(&t[3])
                || (Vec3::new(t[0], t[1], t[2]).length_squared() - 1.).abs() > 0.001
        })
    }) {
        return Err(invalid(
            "primitive",
            id,
            "normal/tangent is not a unit basis",
        ));
    }
    let colors = colors.map(|mut values| {
        for color in &mut values {
            for n in color {
                *n = n.clamp(0., 1.);
            }
        }
        values
    });
    let indices = if let Some(a) = p.indices {
        reader.indices(a, positions.len(), budget)?
    } else {
        if positions.len() > u32::MAX as usize {
            return Err(invalid("indices", id, "vertex count exceeds u32"));
        }
        budget.reserve(positions.len(), 4, reader.limits)?;
        (0..positions.len() as u32).collect()
    };
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(invalid(
            "primitive",
            id,
            "triangle index count must be nonzero and divisible by three",
        ));
    }
    budget.indices = budget
        .indices
        .checked_add(indices.len())
        .filter(|&n| n <= reader.limits.max_indices)
        .ok_or_else(|| invalid("budget", "indices", "total index limit exceeded"))?;
    if let Some(m) = p.material.map(|m| &materials[m]) {
        for slot in [
            &m.base_color_texture,
            &m.metallic_roughness_texture,
            &m.normal_texture,
            &m.occlusion_texture,
            &m.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            if texcoords[slot.texcoord].is_none() {
                return Err(invalid(
                    "primitive",
                    id,
                    "material references missing texture coordinates",
                ));
            }
        }
    }
    Ok(Primitive {
        morph_targets: crate::morph::decode_targets(id, p, reader, budget, positions.len())?,
        positions,
        normals,
        tangents,
        texcoords,
        colors,
        indices,
        material: p.material,
    })
}

pub(crate) fn nodes(
    root: &schema::Root,
    limits: &ImportLimits,
    budget: &mut Budget,
) -> Result<Vec<Node>> {
    let mut parents = vec![None; root.nodes.len()];
    let mut out = Vec::with_capacity(root.nodes.len());
    for (id, n) in root.nodes.iter().enumerate() {
        if n.skin.is_some() {
            return Err(unsupported("node", id, "skin"));
        }
        if n.camera.is_some_and(|c| c >= root.cameras.len()) {
            return Err(invalid("node", id, "camera index out of range"));
        }
        if n.mesh.is_some_and(|m| m >= root.meshes.len()) {
            return Err(invalid("node", id, "mesh index out of range"));
        }
        if let Some(weights) = &n.weights {
            let count = n
                .mesh
                .map(|m| root.meshes[m].primitives[0].targets.len())
                .ok_or_else(|| invalid("node", id, "morph weights require a mesh"))?;
            crate::morph::validate_weights("node", id, weights, count)?;
            budget.reserve(weights.len(), std::mem::size_of::<f32>(), limits)?;
        }
        if n.children.len() > limits.max_items {
            return Err(invalid("node", id, "child count limit exceeded"));
        }
        for &child in &n.children {
            if child >= parents.len() || parents[child].replace(id).is_some() {
                return Err(invalid("node", id, "invalid child or multiple parents"));
            }
        }
        let local = if let Some(m) = n.matrix {
            if n.translation.is_some() || n.rotation.is_some() || n.scale.is_some() {
                return Err(invalid("node", id, "matrix and TRS are mutually exclusive"));
            }
            let matrix = Mat4::from_cols_array(&m);
            if !matrix.is_finite() || m[3] != 0. || m[7] != 0. || m[11] != 0. || m[15] != 1. {
                return Err(invalid("node", id, "non-finite or non-affine transform"));
            }
            let columns = [
                matrix.x_axis.truncate(),
                matrix.y_axis.truncate(),
                matrix.z_axis.truncate(),
            ];
            for a in 0..3 {
                for b in a + 1..3 {
                    let lengths = columns[a].length() * columns[b].length();
                    if !lengths.is_finite()
                        || (lengths > 0. && columns[a].dot(columns[b]).abs() > lengths * 0.0001)
                    {
                        return Err(invalid(
                            "node",
                            id,
                            "sheared matrix is not representable by glTF TRS",
                        ));
                    }
                }
            }
            matrix
        } else {
            let translation = Vec3::from_array(n.translation.unwrap_or([0.; 3]));
            let scale = Vec3::from_array(n.scale.unwrap_or([1.; 3]));
            let rotation = Quat::from_array(n.rotation.unwrap_or([0., 0., 0., 1.]));
            if !translation.is_finite()
                || !scale.is_finite()
                || !rotation.is_finite()
                || (rotation.length_squared() - 1.).abs() > 0.001
            {
                return Err(invalid("node", id, "invalid TRS or non-unit quaternion"));
            }
            Mat4::from_scale_rotation_translation(scale, rotation, translation)
        };
        if !local.is_finite() {
            return Err(invalid("node", id, "local transform overflow"));
        }
        out.push(Node {
            local: local.to_cols_array(),
            mesh: n.mesh,
            camera: n.camera,
            weights: n.weights.clone(),
            children: n.children.clone(),
            rest: n.matrix.is_none().then(|| crate::animation::Transform {
                translation: n.translation.unwrap_or([0.; 3]),
                rotation: n.rotation.unwrap_or([0., 0., 0., 1.]),
                scale: n.scale.unwrap_or([1.; 3]),
            }),
        });
    }
    // Iterative traversal over the whole forest catches disconnected cycles as well.
    let mut visited = 0usize;
    let mut pending: Vec<_> = parents
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_none())
        .map(|(n, _)| (n, 1usize))
        .collect();
    while let Some((id, depth)) = pending.pop() {
        if depth > limits.max_node_depth {
            return Err(invalid("node", id, "hierarchy depth limit exceeded"));
        }
        visited += 1;
        pending.extend(out[id].children.iter().map(|&child| (child, depth + 1)));
    }
    if visited != out.len() {
        return Err(invalid("node", "forest", "cyclic node hierarchy"));
    }
    Ok(out)
}
