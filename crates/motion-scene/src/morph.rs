//! Object-space glTF displacements; no clamping of authored weights or positions.
use crate::{
    accessors::{Budget, Reader},
    invalid, schema, unsupported, AnimationSample, Primitive, Result, Scene,
};
use glam::Mat4;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MorphTarget {
    pub positions: Option<Vec<[f32; 3]>>,
    pub normals: Option<Vec<[f32; 3]>>,
    /// XYZ deltas only. The base tangent's handedness remains unchanged.
    pub tangents: Option<Vec<[f32; 3]>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MorphAttributes {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
}
#[derive(Clone, Debug)]
pub struct MeshInstance<'a> {
    pub node: usize,
    pub mesh: usize,
    pub world: Mat4,
    pub weights: Cow<'a, [f32]>,
    active: bool,
}
impl MeshInstance<'_> {
    pub fn has_active_morph(&self) -> bool {
        self.active
    }
}

pub(crate) fn validate_weights(
    phase: &'static str,
    id: usize,
    weights: &[f32],
    count: usize,
) -> Result<()> {
    if count == 0 || weights.len() != count || weights.iter().any(|v| !v.is_finite()) {
        return Err(invalid(
            phase,
            id,
            "morph weight count or finite value mismatch",
        ));
    }
    Ok(())
}
pub(crate) fn decode_targets(
    id: usize,
    primitive: &schema::Primitive,
    reader: &Reader<'_>,
    budget: &mut Budget,
    count: usize,
) -> Result<Vec<MorphTarget>> {
    budget.reserve(
        primitive.targets.len(),
        std::mem::size_of::<MorphTarget>(),
        reader.limits,
    )?;
    let mut out = Vec::with_capacity(primitive.targets.len());
    for (target, attrs) in primitive.targets.iter().enumerate() {
        let object = format!("{id}/target={target}");
        if attrs.is_empty() {
            return Err(invalid("morph", object, "empty morph target"));
        }
        let mut views = std::collections::BTreeSet::new();
        for (name, &accessor) in attrs {
            if !["POSITION", "NORMAL", "TANGENT"].contains(&name.as_str()) {
                return Err(unsupported(
                    "morph",
                    &object,
                    "only POSITION/NORMAL/TANGENT deltas are implemented",
                ));
            }
            if !primitive.attributes.contains_key(name) {
                return Err(invalid("morph", &object, "delta has no base attribute"));
            }
            let a = reader.accessor(accessor, "VEC3", true)?;
            if a.count != count {
                return Err(invalid("morph", &object, "delta vertex count mismatch"));
            }
            if let Some(view) = a.buffer_view {
                let description = reader.root.buffer_views.get(view).ok_or_else(|| {
                    invalid("morph", &object, "delta buffer view index out of range")
                })?;
                if !views.insert(view) && description.byte_stride.is_none() {
                    return Err(invalid(
                        "morph",
                        &object,
                        "shared vertex delta view requires a stride",
                    ));
                }
            }
            if a.buffer_view
                .and_then(|v| reader.root.buffer_views.get(v))
                .is_some_and(|v| v.target == Some(34963))
            {
                return Err(invalid(
                    "morph",
                    &object,
                    "vertex delta cannot target an index buffer",
                ));
            }
        }
        let mut data = |name: &str| {
            attrs
                .get(name)
                .map(|&a| reader.floats::<3>(a, true, budget))
                .transpose()
        };
        let positions = data("POSITION")?;
        let normals = data("NORMAL")?;
        let tangents = data("TANGENT")?;
        if let Some(positions) = &positions {
            let a = &reader.root.accessors[attrs["POSITION"]];
            let min: [f32; 3] = std::array::from_fn(|c| {
                positions.iter().map(|p| p[c]).fold(f32::INFINITY, f32::min)
            });
            let max: [f32; 3] = std::array::from_fn(|c| {
                positions
                    .iter()
                    .map(|p| p[c])
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            if a.min.as_deref() != Some(&min) || a.max.as_deref() != Some(&max) {
                return Err(invalid("morph", &object, "POSITION delta bounds mismatch"));
            }
        }
        out.push(MorphTarget {
            positions,
            normals,
            tangents,
        });
    }
    Ok(out)
}

impl Primitive {
    /// Evaluates all targets before transforms. Negative/greater-than-one weights
    /// are meaningful extrapolation, not normalized probabilities.
    pub fn morph_attributes(&self, weights: &[f32]) -> Result<MorphAttributes> {
        if weights.len() != self.morph_targets.len() || weights.iter().any(|v| !v.is_finite()) {
            return Err(invalid(
                "morph_sample",
                "weights",
                "weight count or finite value mismatch",
            ));
        }
        let count = self.positions.len();
        if self.normals.as_ref().is_some_and(|a| a.len() != count)
            || self.tangents.as_ref().is_some_and(|a| a.len() != count)
        {
            return Err(invalid(
                "morph_sample",
                "base",
                "attribute vertex count mismatch",
            ));
        }
        for (id, target) in self.morph_targets.iter().enumerate() {
            if [&target.positions, &target.normals, &target.tangents]
                .into_iter()
                .flatten()
                .any(|v| v.len() != count)
                || (target.normals.is_some() && self.normals.is_none())
                || (target.tangents.is_some() && self.tangents.is_none())
            {
                return Err(invalid("morph_sample", id, "delta/base attribute mismatch"));
            }
        }
        let mut positions = Vec::new();
        positions
            .try_reserve_exact(count)
            .map_err(|_| invalid("morph_sample", "positions", "staging allocation failed"))?;
        let mut normals = self.normals.as_ref().map(|_| Vec::new());
        let mut tangents = self.tangents.as_ref().map(|_| Vec::new());
        if let Some(output) = &mut normals {
            output
                .try_reserve_exact(count)
                .map_err(|_| invalid("morph_sample", "normals", "staging allocation failed"))?;
        }
        if let Some(output) = &mut tangents {
            output
                .try_reserve_exact(count)
                .map_err(|_| invalid("morph_sample", "tangents", "staging allocation failed"))?;
        }
        let sample = |base: [f32; 3],
                      vertex: usize,
                      select: fn(&MorphTarget) -> &Option<Vec<[f32; 3]>>|
         -> Result<[f32; 3]> {
            let mut v = base.map(f64::from);
            for (target, &weight) in self.morph_targets.iter().zip(weights) {
                if let Some(deltas) = select(target) {
                    for c in 0..3 {
                        v[c] += f64::from(weight) * f64::from(deltas[vertex][c]);
                    }
                }
            }
            let v = v.map(|c| c as f32);
            if v.iter().any(|v| !v.is_finite()) {
                return Err(invalid(
                    "morph_sample",
                    vertex,
                    "displacement overflow or nonfinite attribute",
                ));
            }
            Ok(v)
        };
        for vertex in 0..count {
            positions.push(sample(self.positions[vertex], vertex, |t| &t.positions)?);
            if let (Some(base), Some(output)) = (&self.normals, &mut normals) {
                output.push(sample(base[vertex], vertex, |t| &t.normals)?);
            }
            if let (Some(base), Some(output)) = (&self.tangents, &mut tangents) {
                let [x, y, z, w] = base[vertex];
                let [x, y, z] = if self.normals.is_some() {
                    sample([x, y, z], vertex, |t| &t.tangents)?
                } else {
                    [x, y, z]
                };
                output.push([x, y, z, w]);
            }
        }
        Ok(MorphAttributes {
            positions,
            normals,
            tangents,
        })
    }
}
impl Scene {
    /// Keeps node identity and per-instance morph overrides for shared meshes.
    pub fn mesh_instances_at(
        &self,
        scene: usize,
        pixels_per_metre: f32,
        sample: Option<AnimationSample>,
    ) -> Result<Vec<MeshInstance<'_>>> {
        let mut animated = sample.map(|s| self.sample_morph_weights(s)).transpose()?;
        // Shared meshes must not multiply the retained default-weight allocation
        // or rescan it for every instance.
        let active_defaults: Vec<_> = self
            .meshes
            .iter()
            .map(|mesh| mesh.weights.iter().any(|&w| w != 0.))
            .collect();
        let mut out = Vec::new();
        self.visit_node_instances_at(scene, pixels_per_metre, sample, |node, world| {
            if let Some(mesh) = self.nodes[node].mesh {
                let (weights, active) =
                    if let Some(weights) = animated.as_mut().and_then(|a| a[node].take()) {
                        let active = weights.iter().any(|&w| w != 0.);
                        (Cow::Owned(weights), active)
                    } else if let Some(weights) = self.nodes[node].weights.as_ref() {
                        (
                            Cow::Borrowed(weights.as_slice()),
                            weights.iter().any(|&w| w != 0.),
                        )
                    } else {
                        (
                            Cow::Borrowed(self.meshes[mesh].weights.as_slice()),
                            active_defaults[mesh],
                        )
                    };
                out.push(MeshInstance {
                    node,
                    mesh,
                    world,
                    weights,
                    active,
                });
            }
        })?;
        Ok(out)
    }
}
