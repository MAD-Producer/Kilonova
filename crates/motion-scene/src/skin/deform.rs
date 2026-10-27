use crate::{invalid, unsupported, MorphAttributes, Primitive, Result};
use glam::{DMat3, DMat4, DVec3, Mat4};

impl Primitive {
    /// Morph first, then linear-blend skin directly into world metres. Small
    /// representational weight drift is normalized in f64, after import validation.
    /// The initial shading profile explicitly refuses singular/reflected effective
    /// transforms; it never substitutes bind geometry for an unsupported pose.
    pub fn skin_attributes(&self, morph: &[f32], palette: &[Mat4]) -> Result<MorphAttributes> {
        if self.joint_influences.is_empty() {
            return Err(invalid(
                "skin_sample",
                "attributes",
                "primitive has no joint influences",
            ));
        }
        if self.joint_influences.iter().any(|s| {
            s.joints.len() != self.positions.len() || s.weights.len() != self.positions.len()
        }) || palette.iter().any(|m| !m.is_finite())
        {
            return Err(invalid(
                "skin_sample",
                "attributes",
                "invalid influence count or nonfinite palette",
            ));
        }
        let mut attributes = self.morph_attributes(morph)?;
        for vertex in 0..self.positions.len() {
            let mut values = [0f64; 16];
            let mut sum = 0.;
            let mut active = 0;
            for s in &self.joint_influences {
                for (&joint, &weight) in s.joints[vertex].iter().zip(&s.weights[vertex]) {
                    let matrix = palette.get(joint as usize).ok_or_else(|| {
                        invalid("skin_sample", vertex, "joint index exceeds palette")
                    })?;
                    if !weight.is_finite() || weight < 0. {
                        return Err(invalid("skin_sample", vertex, "invalid joint weight"));
                    }
                    if weight == 0. {
                        continue;
                    }
                    active += 1;
                    sum += weight as f64;
                    for (out, element) in values.iter_mut().zip(matrix.to_cols_array()) {
                        *out += weight as f64 * element as f64;
                    }
                }
            }
            if active == 0 || !sum.is_finite() || (sum - 1.).abs() > 2e-7 * active as f64 {
                return Err(invalid(
                    "skin_sample",
                    vertex,
                    "joint weights must sum to one",
                ));
            }
            let matrix = DMat4::from_cols_array(&values.map(|v| v / sum));
            let basis = DMat3::from_mat4(matrix);
            let determinant = basis.determinant();
            if !matrix.is_finite() || !determinant.is_finite() || determinant <= 0. {
                return Err(unsupported(
                    "skin_sample",
                    vertex,
                    "singular or reflected effective skin transform",
                ));
            }
            let finite = |v: DVec3| -> Result<[f32; 3]> {
                let value = v.as_vec3();
                if !value.is_finite() {
                    return Err(invalid(
                        "skin_sample",
                        vertex,
                        "deformed attribute overflow",
                    ));
                }
                Ok(value.to_array())
            };
            attributes.positions[vertex] = finite(matrix.transform_point3(DVec3::from_array(
                attributes.positions[vertex].map(f64::from),
            )))?;
            if let Some(normals) = &mut attributes.normals {
                normals[vertex] = finite(
                    basis.inverse().transpose() * DVec3::from_array(normals[vertex].map(f64::from)),
                )?;
                if let Some(tangents) = &mut attributes.tangents {
                    let [x, y, z, w] = tangents[vertex];
                    let [x, y, z] = finite(basis * DVec3::new(x as f64, y as f64, z as f64))?;
                    tangents[vertex] = [x, y, z, w];
                }
            }
        }
        Ok(attributes)
    }
}
