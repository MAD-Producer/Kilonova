//! Bounded glTF linear blend skin data, hierarchy validation and metre-space palettes.
use crate::{
    accessors::{Budget, Reader},
    invalid, schema, AnimationSample, Result, Scene,
};
use glam::Mat4;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod deform;

pub type SkinPalettes = Vec<Option<Vec<Mat4>>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointInfluences {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skin {
    pub joints: Vec<usize>,
    /// Column-major joint-to-bind transforms, in joint order; defaults are explicit identity.
    pub inverse_bind_matrices: Vec<[f32; 16]>,
    pub skeleton: Option<usize>,
}

pub(crate) fn decode(reader: &Reader<'_>, budget: &mut Budget) -> Result<Vec<Skin>> {
    budget.reserve(
        reader.root.skins.len(),
        std::mem::size_of::<Skin>(),
        reader.limits,
    )?;
    let mut count = 0usize;
    let mut out = Vec::with_capacity(reader.root.skins.len());
    for (id, skin) in reader.root.skins.iter().enumerate() {
        count = count
            .checked_add(skin.joints.len())
            .filter(|&n| n <= reader.limits.max_items)
            .ok_or_else(|| invalid("budget", "skin joints", "joint reference limit exceeded"))?;
        let mut unique = BTreeSet::new();
        if skin.joints.is_empty()
            || skin
                .joints
                .iter()
                .any(|&j| j >= reader.root.nodes.len() || !unique.insert(j))
            || skin.skeleton.is_some_and(|s| s >= reader.root.nodes.len())
        {
            return Err(invalid(
                "skin",
                id,
                "empty, repeated or out-of-range joint/skeleton",
            ));
        }
        budget.reserve(
            skin.joints.len(),
            std::mem::size_of::<usize>(),
            reader.limits,
        )?;
        let inverse_bind_matrices = if let Some(a) = skin.inverse_bind_matrices {
            let accessor = reader.accessor(a, "MAT4", false)?;
            if accessor.count < skin.joints.len() {
                return Err(invalid(
                    "skin",
                    id,
                    "inverse bind matrix count is less than joint count",
                ));
            }
            let mut values = reader.inverse_bind_matrices(a, budget)?;
            values.truncate(skin.joints.len());
            values
        } else {
            budget.reserve(
                skin.joints.len(),
                std::mem::size_of::<[f32; 16]>(),
                reader.limits,
            )?;
            vec![Mat4::IDENTITY.to_cols_array(); skin.joints.len()]
        };
        if inverse_bind_matrices.iter().any(|m| {
            !Mat4::from_cols_array(m).is_finite()
                || m[3] != 0.
                || m[7] != 0.
                || m[11] != 0.
                || m[15] != 1.
        }) {
            return Err(invalid(
                "skin",
                id,
                "inverse bind matrix is nonfinite or nonaffine",
            ));
        }
        out.push(Skin {
            joints: skin.joints.clone(),
            inverse_bind_matrices,
            skeleton: skin.skeleton,
        });
    }
    Ok(out)
}

fn attribute(name: &str) -> Result<Option<(bool, usize)>> {
    let (joints, suffix) = if let Some(s) = name.strip_prefix("JOINTS_") {
        (true, s)
    } else if let Some(s) = name.strip_prefix("WEIGHTS_") {
        (false, s)
    } else {
        return Ok(None);
    };
    if suffix.is_empty()
        || suffix.len() > 9
        || (suffix.len() > 1 && suffix.starts_with('0'))
        || !suffix.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(invalid(
            "skin",
            "attribute",
            "invalid joint/weight set suffix",
        ));
    }
    Ok(Some((
        joints,
        suffix
            .parse()
            .map_err(|_| invalid("skin", "attribute", "set index overflow"))?,
    )))
}

pub(crate) fn decode_influences(
    id: usize,
    primitive: &schema::Primitive,
    reader: &Reader<'_>,
    budget: &mut Budget,
    vertices: usize,
) -> Result<Vec<JointInfluences>> {
    let mut sets: BTreeMap<usize, [Option<usize>; 2]> = BTreeMap::new();
    for (name, &accessor) in &primitive.attributes {
        if let Some((joints, set)) = attribute(name)? {
            sets.entry(set).or_default()[usize::from(!joints)] = Some(accessor);
        }
    }
    budget.reserve(
        sets.len(),
        std::mem::size_of::<JointInfluences>(),
        reader.limits,
    )?;
    let mut out = Vec::with_capacity(sets.len());
    let mut quantization = Vec::with_capacity(sets.len());
    for (expected, (index, pair)) in sets.into_iter().enumerate() {
        let [Some(joints), Some(weights)] = pair else {
            return Err(invalid("skin", id, "joint and weight sets must be paired"));
        };
        if index != expected {
            return Err(invalid(
                "skin",
                id,
                "influence sets must be consecutive from zero",
            ));
        }
        for accessor in [joints, weights] {
            if reader.accessor(accessor, "VEC4", true)?.count != vertices {
                return Err(invalid("skin", id, "influence vertex count mismatch"));
            }
        }
        quantization.push(match reader.root.accessors[weights].component_type {
            5121 => Some(255u32),
            5123 => Some(65535u32),
            _ => None,
        });
        out.push(JointInfluences {
            joints: reader.joints(joints, budget)?,
            weights: reader.floats(weights, false, budget)?,
        });
    }
    if out.is_empty() {
        return Ok(out);
    }
    let quantized = quantization.iter().all(Option::is_some);
    for vertex in 0..vertices {
        let mut active = BTreeSet::new();
        let mut sum = 0f64;
        let mut integer_sum = 0u64;
        for (set, q) in out.iter().zip(&quantization) {
            for (&joint, &weight) in set.joints[vertex].iter().zip(&set.weights[vertex]) {
                if weight < 0. || (weight > 0. && !active.insert(joint)) {
                    return Err(invalid(
                        "skin",
                        format!("{id}/vertex={vertex}"),
                        "negative weight or repeated active joint",
                    ));
                }
                sum += weight as f64;
                if let Some(max) = q {
                    // Decoding U8/U16 to f32 and rounding recovers every integer exactly.
                    integer_sum +=
                        (weight as f64 * *max as f64).round() as u64 * (65535 / *max) as u64;
                }
            }
        }
        if active.is_empty()
            || (quantized && integer_sum != 65535)
            || (!quantized && (sum - 1.).abs() > 2e-7 * active.len() as f64)
        {
            return Err(invalid(
                "skin",
                format!("{id}/vertex={vertex}"),
                "joint weights must sum to one within the import profile",
            ));
        }
    }
    Ok(out)
}

pub(crate) fn validate(scene: &Scene) -> Result<()> {
    if scene.skins.is_empty() {
        return Ok(());
    }
    let mut parents = vec![None; scene.nodes.len()];
    for (id, node) in scene.nodes.iter().enumerate() {
        for &child in &node.children {
            parents[child] = Some(id);
        }
    }
    // Node validation has already bounded depth and excluded cycles.
    let roots: Vec<_> = (0..scene.nodes.len())
        .map(|mut id| {
            while let Some(parent) = parents[id] {
                id = parent;
            }
            id
        })
        .collect();
    let mut joint_roots = Vec::with_capacity(scene.skins.len());
    for (id, skin) in scene.skins.iter().enumerate() {
        let root = roots[skin.joints[0]];
        if skin.joints.iter().any(|&j| roots[j] != root) {
            return Err(invalid("skin", id, "joints have no common ancestor"));
        }
        if let Some(skeleton) = skin.skeleton {
            for &joint in &skin.joints {
                let mut at = joint;
                while at != skeleton {
                    at = parents[at]
                        .ok_or_else(|| invalid("skin", id, "skeleton is not a common ancestor"))?;
                }
            }
        }
        joint_roots.push(root);
    }
    let mut required: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); scene.nodes.len()];
    // Scan geometry once. Binding a large mesh to many skins must not repeatedly
    // scan every vertex, including unused components which are also range-checked.
    let maxima: Vec<_> = scene
        .meshes
        .iter()
        .map(|mesh| {
            mesh.primitives
                .iter()
                .enumerate()
                .filter_map(|(p, primitive)| {
                    primitive
                        .joint_influences
                        .iter()
                        .flat_map(|s| s.joints.iter().flatten())
                        .max()
                        .map(|&joint| (joint as usize, p))
                })
                .max()
        })
        .collect();
    for (id, node) in scene.nodes.iter().enumerate() {
        if let (Some(mesh), Some(skin)) = (node.mesh, node.skin) {
            required[roots[id]].insert(joint_roots[skin]);
            if let Some((maximum, p)) = maxima[mesh] {
                if maximum >= scene.skins[skin].joints.len() {
                    return Err(invalid(
                        "skin",
                        format!("node={id}/mesh={mesh}/primitive={p}"),
                        "joint index exceeds bound skin",
                    ));
                }
            }
        }
    }
    for (id, scene_roots) in scene.scenes.iter().enumerate() {
        let included: BTreeSet<_> = scene_roots.iter().copied().collect();
        if scene_roots
            .iter()
            .any(|&r| !required[r].is_subset(&included))
        {
            return Err(invalid(
                "skin",
                format!("scene={id}"),
                "joint common root is absent from referencing scene",
            ));
        }
    }
    Ok(())
}

impl Scene {
    /// One palette per used skin, once per sampled hierarchy. Matrices map bind-space
    /// geometry directly to world metres; the instantiating mesh node is not multiplied.
    /// Unused skins have no palette; no mesh-instance copies of joint arrays are made.
    pub fn skin_matrices_at(
        &self,
        scene: usize,
        sample: Option<AnimationSample>,
    ) -> Result<SkinPalettes> {
        let mut worlds = vec![None; self.nodes.len()];
        let mut used = vec![false; self.skins.len()];
        self.visit_node_instances_at(scene, 1., sample, |node, world| {
            worlds[node] = Some(world);
            if let Some(skin) = self.nodes[node].skin {
                used[skin] = true;
            }
        })?;
        self.skin_matrices_from_worlds(&worlds, &used)
    }
    pub(crate) fn skin_matrices_from_worlds(
        &self,
        worlds: &[Option<Mat4>],
        used: &[bool],
    ) -> Result<SkinPalettes> {
        let mut out = Vec::with_capacity(self.skins.len());
        for (id, skin) in self.skins.iter().enumerate() {
            if !used[id] {
                out.push(None);
                continue;
            }
            let mut palette = Vec::with_capacity(skin.joints.len());
            for (&joint, bind) in skin.joints.iter().zip(&skin.inverse_bind_matrices) {
                let world = worlds[joint].ok_or_else(|| {
                    invalid("skin_sample", id, "joint is absent from selected scene")
                })?;
                let matrix = world * Mat4::from_cols_array(bind);
                if !matrix.is_finite() {
                    return Err(invalid(
                        "skin_sample",
                        format!("{id}/joint={joint}"),
                        "joint matrix overflow",
                    ));
                }
                palette.push(matrix);
            }
            out.push(Some(palette));
        }
        Ok(out)
    }
}
