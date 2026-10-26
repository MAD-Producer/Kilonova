//! Per-instance numeric payload, independent of package/code identity. The
//! model and sampled scene share immutable Arcs; no JSON/PNG decoding on Present.
use super::*;
use serde::de::{Deserializer, SeqAccess, Visitor};
use std::{fmt, marker::PhantomData};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShaderDataValues {
    pub version: u32,
    #[serde(deserialize_with = "uniform_cells")]
    pub uniforms: Vec<[f32; 4]>,
    #[serde(deserialize_with = "texture_scalars")]
    pub texture: Vec<f32>,
}

fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    struct Bounded<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Bounded<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "at most {N} shader data elements")
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            // Do not reserve an untrusted size_hint before checking the bound.
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == N {
                    return Err(serde::de::Error::custom(
                        "shader data exceeds element limit",
                    ));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Bounded::<T, LIMIT>(PhantomData))
}
fn uniform_cells<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<[f32; 4]>, D::Error> {
    bounded::<D, _, { crate::MAX_PARAMS }>(d)
}
fn texture_scalars<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<f32>, D::Error> {
    bounded::<D, _, MAX_SHADER_DATA_SCALARS>(d)
}

impl PartialEq for ShaderDataValues {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.uniforms.len() == other.uniforms.len()
            && self.texture.len() == other.texture.len()
            && self
                .uniforms
                .iter()
                .flatten()
                .zip(other.uniforms.iter().flatten())
                .all(|(a, b)| a.to_bits() == b.to_bits())
            && self
                .texture
                .iter()
                .zip(&other.texture)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}
impl ShaderDataValues {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.version == SHADER_DATA_VERSION,
            "unsupported instance shader data version",
        )?;
        ensure(
            self.uniforms.len() <= crate::MAX_PARAMS
                && self.uniforms.len() * 4 + self.texture.len() <= MAX_SHADER_DATA_SCALARS,
            "instance shader data exceeds scalar budget",
        )?;
        ensure(
            self.uniforms
                .iter()
                .flatten()
                .chain(&self.texture)
                .all(|v| v.is_finite() && v.abs() <= 1.0e12),
            "instance shader data must be finite and bounded",
        )
    }
    pub fn validate_layout(&self, declaration: &ShaderData) -> Result<()> {
        self.validate()?;
        ensure(
            declaration.version == self.version
                && self.uniforms.len() == declaration.uniforms.len()
                && self.texture.len()
                    == declaration
                        .overflow
                        .as_ref()
                        .map_or(0, |t| t.scalars as usize),
            "instance shader data layout differs from the code package",
        )
    }
    pub fn write_uniforms(
        &self,
        declaration: &ShaderData,
        public_cells: usize,
        out: &mut [[f32; 4]; crate::MAX_PARAMS],
    ) -> Result<()> {
        self.validate_layout(declaration)?;
        let end = public_cells
            .checked_add(self.uniforms.len())
            .filter(|end| *end <= out.len())
            .ok_or_else(|| {
                crate::Error::Invalid("instance shader data exceeds uniform tail capacity".into())
            })?;
        out[public_cells..end].copy_from_slice(&self.uniforms);
        Ok(())
    }
    pub fn texture_extent(&self) -> [u32; 2] {
        [
            SHADER_DATA_TEXTURE_WIDTH,
            (self.texture.len() as u32).div_ceil(SHADER_DATA_TEXTURE_WIDTH),
        ]
    }
    pub fn texture_bytes(&self) -> u64 {
        let [w, h] = self.texture_extent();
        u64::from(w) * u64::from(h) * 4
    }
    pub fn texture_pixels(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = vec![0; self.texture_bytes() as usize];
        for (slot, value) in bytes.chunks_exact_mut(4).zip(&self.texture) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        Ok(bytes)
    }
    /// Logical retained payload only, excluding Arc/Vec overhead and sharing.
    pub fn payload_bytes(&self) -> u64 {
        (self.uniforms.len() * 16 + self.texture.len() * 4) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_payload_preserves_numeric_bits_and_rejects_untrusted_shapes() {
        let value = ShaderDataValues {
            version: SHADER_DATA_VERSION,
            uniforms: vec![[1., -0., 0.25, 1.]],
            texture: vec![0.1, -0., 1.],
        };
        let roundtrip: ShaderDataValues =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(value, roundtrip);
        let mut changed = value.clone();
        changed.uniforms[0][1] = 0.;
        assert_ne!(
            changed, value,
            "signed zero is observable through shader bitcasts"
        );
        let pixels = value.texture_pixels().unwrap();
        assert_eq!(pixels.len(), 1024);
        assert_eq!(
            &pixels[..12],
            &[0.1f32, -0., 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        let mut invalid = value.clone();
        invalid.texture[0] = f32::NAN;
        assert!(invalid.texture_pixels().is_err());
        invalid = value.clone();
        invalid.version += 1;
        assert!(invalid.validate().is_err());
        invalid = value.clone();
        invalid.texture = vec![0.; MAX_SHADER_DATA_SCALARS];
        assert!(
            invalid.validate().is_err(),
            "uniform and overflow budgets are combined"
        );
        let oversized = format!(
            r#"{{"version":1,"uniforms":[{}],"texture":[]}}"#,
            vec!["[0,0,0,0]"; 33].join(",")
        );
        assert!(serde_json::from_str::<ShaderDataValues>(&oversized).is_err());
        let oversized = format!(
            r#"{{"version":1,"uniforms":[],"texture":[{}]}}"#,
            vec!["0"; MAX_SHADER_DATA_SCALARS + 1].join(",")
        );
        assert!(serde_json::from_str::<ShaderDataValues>(&oversized).is_err());
    }
    #[test]
    fn instance_layout_cannot_write_outside_its_declared_uniform_tail() {
        let data = ShaderData {
            version: SHADER_DATA_VERSION,
            uniforms: vec![[0.; 4]],
            overflow: None,
        };
        let values = ShaderDataValues {
            version: SHADER_DATA_VERSION,
            uniforms: vec![[0.2, 0.4, 0.6, 1.]],
            texture: vec![],
        };
        let mut output = [[42.; 4]; crate::MAX_PARAMS];
        values.write_uniforms(&data, 31, &mut output).unwrap();
        assert_eq!(output[30], [42.; 4]);
        assert_eq!(output[31], values.uniforms[0]);
        assert!(values.write_uniforms(&data, 32, &mut output).is_err());
        let mut invalid = values.clone();
        invalid.texture.push(0.);
        assert!(invalid.validate_layout(&data).is_err());
        assert_eq!(output[31], values.uniforms[0]);
    }
}
