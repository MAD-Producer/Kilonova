//! Portable image meaning and storage requirements, independent of GPU APIs.
//! A contract describes one logical value, independent of physical pool slots.
use crate::{AlphaMode, WorkingSpace};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const IMAGE_CONTRACT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataChannels {
    Scalar,
    Vector2,
    Vector3,
    Vector4,
}

/// Color space describes shader-visible values after texture sampling, not
/// whether the backing allocation performs an automatic sRGB transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageMeaning {
    Color {
        space: WorkingSpace,
        alpha: AlphaMode,
    },
    /// A scalar in [0, 1]; producers enforce that range. No color/alpha transform.
    Coverage,
    /// Independent numeric channels. Extra storage channels have no meaning.
    Data { channels: DataChannels },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelPrecision {
    #[default]
    Unorm8,
    /// Uniform 16-bit fixed-point steps in [0,1], distinct from half-float HDR.
    Unorm16,
    Float16,
    Float32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageContract {
    version: u32,
    pub meaning: ImageMeaning,
    pub precision: ChannelPrecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageStorage {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba8UnormSrgb,
    R16Unorm,
    Rg16Unorm,
    Rgba16Unorm,
    R16Float,
    Rg16Float,
    Rgba16Float,
    R32Float,
    Rg32Float,
    Rgba32Float,
}
impl ImageStorage {
    pub const fn bytes_per_pixel(self) -> u64 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm | Self::R16Unorm | Self::R16Float => 2,
            Self::Rgba8Unorm
            | Self::Rgba8UnormSrgb
            | Self::Rg16Unorm
            | Self::Rg16Float
            | Self::R32Float => 4,
            Self::Rgba16Unorm | Self::Rgba16Float | Self::Rg32Float => 8,
            Self::Rgba32Float => 16,
        }
    }
    /// One mip/sample/layer; callers must account for additional subresources.
    pub fn capacity(self, width: u32, height: u32) -> Option<u64> {
        if width == 0 || height == 0 {
            return None;
        }
        u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(self.bytes_per_pixel())
    }
}

/// Coverage remains a bounded scalar at the requested working precision.
pub const fn coverage_storage(precision: ChannelPrecision) -> ImageStorage {
    match precision {
        ChannelPrecision::Unorm8 => ImageStorage::R8Unorm,
        ChannelPrecision::Unorm16 => ImageStorage::R16Unorm,
        ChannelPrecision::Float16 => ImageStorage::R16Float,
        ChannelPrecision::Float32 => ImageStorage::R32Float,
    }
}

/// Single-sample sampled/render/copy usage. Storage binding and multisampling
/// require separate contracts; they are not implied by these capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageUsage {
    pub sampled: bool,
    pub linear_filter: bool,
    pub render: bool,
    pub blend: bool,
    pub copy_src: bool,
    pub copy_dst: bool,
}
impl ImageUsage {
    pub const EFFECT_TARGET: Self = Self {
        sampled: true,
        linear_filter: true,
        render: true,
        blend: true,
        copy_src: true,
        copy_dst: true,
    };
    pub fn missing_from(self, available: Self) -> Self {
        Self {
            sampled: self.sampled && !available.sampled,
            linear_filter: self.linear_filter && !available.linear_filter,
            render: self.render && !available.render,
            blend: self.blend && !available.blend,
            copy_src: self.copy_src && !available.copy_src,
            copy_dst: self.copy_dst && !available.copy_dst,
        }
    }
    pub fn validate(self) -> Result<(), ImageFormatError> {
        if self.linear_filter && !self.sampled {
            return Err(ImageFormatError::InvalidUsage(
                "linear filtering requires sampled input",
            ));
        }
        if self.blend && !self.render {
            return Err(ImageFormatError::InvalidUsage(
                "blending requires a render attachment",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageFormatError {
    UnsupportedVersion(u32),
    InvalidUsage(&'static str),
    /// At most two attempts. No unbounded shader or user payload is retained.
    Unsupported {
        contract: ImageContract,
        missing: Vec<(ImageStorage, ImageUsage)>,
    },
}
impl fmt::Display for ImageFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "image format contract: {self:?}")
    }
}
impl std::error::Error for ImageFormatError {}

impl ImageContract {
    pub const fn new(meaning: ImageMeaning, precision: ChannelPrecision) -> Self {
        Self {
            version: IMAGE_CONTRACT_VERSION,
            meaning,
            precision,
        }
    }
    pub fn version(self) -> u32 {
        self.version
    }
    pub fn preferred_storage(self) -> Result<ImageStorage, ImageFormatError> {
        if self.version != IMAGE_CONTRACT_VERSION {
            return Err(ImageFormatError::UnsupportedVersion(self.version));
        }
        use ChannelPrecision::*;
        use DataChannels::*;
        use ImageStorage::*;
        Ok(match (self.meaning, self.precision) {
            (
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    ..
                },
                Unorm8,
            ) => Rgba8UnormSrgb,
            (
                ImageMeaning::Color {
                    space: WorkingSpace::Srgb,
                    ..
                },
                Unorm8,
            ) => Rgba8Unorm,
            (ImageMeaning::Color { .. }, Unorm16) => Rgba16Unorm,
            (ImageMeaning::Color { .. }, Float16) => Rgba16Float,
            (ImageMeaning::Color { .. }, Float32) => Rgba32Float,
            (ImageMeaning::Coverage | ImageMeaning::Data { channels: Scalar }, Unorm8) => R8Unorm,
            (ImageMeaning::Coverage | ImageMeaning::Data { channels: Scalar }, Unorm16) => R16Unorm,
            (ImageMeaning::Coverage | ImageMeaning::Data { channels: Scalar }, Float16) => R16Float,
            (ImageMeaning::Coverage | ImageMeaning::Data { channels: Scalar }, Float32) => R32Float,
            (ImageMeaning::Data { channels: Vector2 }, Unorm8) => Rg8Unorm,
            (ImageMeaning::Data { channels: Vector2 }, Unorm16) => Rg16Unorm,
            (ImageMeaning::Data { channels: Vector2 }, Float16) => Rg16Float,
            (ImageMeaning::Data { channels: Vector2 }, Float32) => Rg32Float,
            (
                ImageMeaning::Data {
                    channels: Vector3 | Vector4,
                },
                Unorm8,
            ) => Rgba8Unorm,
            (
                ImageMeaning::Data {
                    channels: Vector3 | Vector4,
                },
                Unorm16,
            ) => Rgba16Unorm,
            (
                ImageMeaning::Data {
                    channels: Vector3 | Vector4,
                },
                Float16,
            ) => Rgba16Float,
            (
                ImageMeaning::Data {
                    channels: Vector3 | Vector4,
                },
                Float32,
            ) => Rgba32Float,
        })
    }
    /// Fallback may only add ignored channels at the same precision. Never
    /// quantize float to UNORM, discard alpha, or change the color transfer.
    pub fn negotiate(
        self,
        usage: ImageUsage,
        capabilities: impl Fn(ImageStorage) -> ImageUsage,
    ) -> Result<ImageStorage, ImageFormatError> {
        usage.validate()?;
        let preferred = self.preferred_storage()?;
        let padded = match preferred {
            ImageStorage::R8Unorm | ImageStorage::Rg8Unorm => Some(ImageStorage::Rgba8Unorm),
            ImageStorage::R16Unorm | ImageStorage::Rg16Unorm => Some(ImageStorage::Rgba16Unorm),
            ImageStorage::R16Float | ImageStorage::Rg16Float => Some(ImageStorage::Rgba16Float),
            ImageStorage::R32Float | ImageStorage::Rg32Float => Some(ImageStorage::Rgba32Float),
            _ => None,
        };
        let mut missing = Vec::with_capacity(2);
        for storage in std::iter::once(preferred).chain(padded) {
            let absent = usage.missing_from(capabilities(storage));
            if absent == ImageUsage::default() {
                return Ok(storage);
            }
            missing.push((storage, absent));
        }
        Err(ImageFormatError::Unsupported {
            contract: self,
            missing,
        })
    }
}
