//! Private numeric data for editable image programs. Public controls remain
//! separate. Overflow uses an unfiltered RGBA8 byte container, not UNORM values.
use crate::{EffectDefinition, Result, ensure};
use serde::{Deserialize, Serialize};
mod values;
pub use values::ShaderDataValues;

pub const SHADER_DATA_VERSION: u32 = 1;
pub const SHADER_DATA_CAPABILITY: &str = "shader_data_v1";
pub const MAX_SHADER_DATA_SCALARS: usize = 16_384;
pub const SHADER_DATA_TEXTURE_WIDTH: u32 = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShaderData {
    pub version: u32,
    /// Tail cells beginning immediately after the public parameter cells.
    pub uniforms: Vec<[f32; 4]>,
    pub overflow: Option<ShaderDataTexture>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShaderDataTexture {
    /// Must be resource zero, uploaded as linear Rgba8Unorm. One little-endian
    /// f32 bit pattern per pixel; textureLoad reconstructs the original bits.
    pub path: String,
    pub scalars: u32,
}

impl ShaderData {
    pub(crate) fn validate(&self, effect: &EffectDefinition) -> Result<()> {
        ensure(
            self.version == SHADER_DATA_VERSION,
            "unsupported shader data version",
        )?;
        ensure(
            effect.renderer == crate::RendererKind::Image,
            "shader data requires an image program",
        )?;
        ensure(
            !effect
                .required_capabilities
                .iter()
                .any(|c| c == "image_input" || c == "param_lut"),
            "shader data cannot share reserved parameter/resource bindings",
        )?;
        ensure(
            effect.params.len() + self.uniforms.len() <= crate::MAX_PARAMS,
            "shader data exceeds uniform tail capacity",
        )?;
        ensure(
            self.uniforms
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1.0e12),
            "shader data uniforms must be finite and bounded",
        )?;
        if let Some(texture) = &self.overflow {
            ensure(
                texture.scalars > 0
                    && texture.scalars as usize + self.uniforms.len() * 4
                        <= MAX_SHADER_DATA_SCALARS,
                "shader data texture exceeds scalar budget",
            )?;
            ensure(
                effect.resources.first() == Some(&texture.path),
                "shader data texture must occupy resource zero",
            )?;
        }
        Ok(())
    }

    pub fn write_uniforms(
        &self,
        public_cells: usize,
        output: &mut [[f32; 4]; crate::MAX_PARAMS],
    ) -> Result<()> {
        let end = public_cells
            .checked_add(self.uniforms.len())
            .filter(|&end| end <= crate::MAX_PARAMS)
            .ok_or_else(|| crate::Error::Invalid("shader data uniform range is invalid".into()))?;
        output[public_cells..end].copy_from_slice(&self.uniforms);
        Ok(())
    }

    /// Validate private runtime bytes before a consumer decodes or uploads them.
    /// Package callers also apply ordinary decoded-resource admission.
    pub fn validate_texture(&self, bytes: &[u8]) -> Result<()> {
        let Some(texture) = &self.overflow else {
            return Ok(());
        };
        let height = texture.scalars.div_ceil(SHADER_DATA_TEXTURE_WIDTH);
        let dimensions =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png)
                .into_dimensions()
                .map_err(|cause| crate::Error::ImageResource {
                    phase: "shader_data_dimensions",
                    path: texture.path.clone(),
                    cause: Box::new(cause),
                })?;
        ensure(
            dimensions == (SHADER_DATA_TEXTURE_WIDTH, height),
            "shader data byte-container dimensions do not match the declaration",
        )?;
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).map_err(
            |cause| crate::Error::ImageResource {
                phase: "shader_data_decode",
                path: texture.path.clone(),
                cause: Box::new(cause),
            },
        )?;
        ensure(
            image.width() == SHADER_DATA_TEXTURE_WIDTH
                && image.height() == height
                && image.color() == image::ColorType::Rgba8,
            "shader data requires exact RGBA8 byte-container dimensions",
        )?;
        let rgba = image.into_rgba8();
        for bytes in rgba.as_raw().chunks_exact(4).take(texture.scalars as usize) {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            ensure(
                value.is_finite() && value.abs() <= 1.0e12,
                "shader data texture contains an invalid numeric value",
            )?;
        }
        Ok(())
    }
    /// Decode package defaults or a newly compiled payload on the preparation
    /// path. Rendering consumes the resulting typed values directly.
    pub fn values(&self, bytes: Option<&[u8]>) -> Result<ShaderDataValues> {
        let texture = if let Some(declaration) = &self.overflow {
            let bytes = bytes
                .ok_or_else(|| crate::Error::Invalid("shader data resource is missing".into()))?;
            self.validate_texture(bytes)?;
            let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
                .map_err(|cause| crate::Error::ImageResource {
                    phase: "instance_shader_data_decode",
                    path: declaration.path.clone(),
                    cause: Box::new(cause),
                })?
                .into_rgba8();
            image
                .as_raw()
                .chunks_exact(4)
                .take(declaration.scalars as usize)
                .map(|p| f32::from_le_bytes([p[0], p[1], p[2], p[3]]))
                .collect()
        } else {
            vec![]
        };
        let values = ShaderDataValues {
            version: self.version,
            uniforms: self.uniforms.clone(),
            texture,
        };
        values.validate_layout(self)?;
        Ok(values)
    }
}
