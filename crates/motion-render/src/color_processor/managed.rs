//! Explicit OCIO image values, separate from the legacy Linear/sRGB scene ABI.
//! The inner Float32 texture is raw transport: no transfer function is implied.
use super::*;
use motion_effects::{color_config::ColorSpaceIdentity, WorkingSpace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorImageContract {
    pub space: ColorSpaceIdentity,
    pub alpha: AlphaMode,
    pub precision: ChannelPrecision,
}
impl ColorImageContract {
    fn transport(self) -> ImageContract {
        ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: self.alpha,
            },
            self.precision,
        )
    }
}

/// A producer declares the OCIO meaning at import/allocation, then every
/// processor checks it. There is no mutable tag or automatic relabelling.
pub struct ColorImageTarget {
    image: ImageCaptureTarget,
    contract: ColorImageContract,
}
#[derive(Debug)]
pub struct CapturedColorImage {
    pub contract: ColorImageContract,
    pub storage: motion_effects::image_contract::ImageStorage,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
impl ColorImageTarget {
    /// Explicit producer boundary for an already allocated raw texture. Values
    /// must already belong to the declared space; this does not transform them.
    /// Numeric data and automatic sRGB texture transfers cannot enter this ABI.
    pub fn from_image(
        image: ImageCaptureTarget,
        space: ColorSpaceIdentity,
    ) -> Result<Self, RenderError> {
        let contract = image.contract();
        if !contract.precision.is_float() {
            return Err(RenderError::Invalid(
                "OCIO image declaration requires floating raw transport".into(),
            ));
        }
        let ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha,
        } = contract.meaning
        else {
            return Err(RenderError::Invalid(
                "OCIO image declaration requires raw color transport, without data or an automatic sRGB transfer".into(),
            ));
        };
        Ok(Self {
            image,
            contract: ColorImageContract {
                space,
                alpha,
                precision: contract.precision,
            },
        })
    }
    pub fn contract(&self) -> ColorImageContract {
        self.contract
    }
    /// Raw producer/readback access. Use contract() for the complete meaning;
    /// the legacy inner Linear tag only requests non-transferring storage.
    pub fn image(&self) -> &ImageCaptureTarget {
        &self.image
    }
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        self.image.texture_resource_ledger()
    }
}
impl Renderer {
    /// Explicit export/qualification readback, never a preview polling path.
    /// Preserve the full endpoint identity without applying a display transfer.
    pub fn read_color_image_target(
        &self,
        target: &ColorImageTarget,
    ) -> Result<CapturedColorImage, RenderError> {
        let raw = self.read_image_target(&target.image)?;
        Ok(CapturedColorImage {
            contract: target.contract,
            storage: raw.storage,
            width: raw.width,
            height: raw.height,
            pixels: raw.pixels,
        })
    }
    pub fn color_image_target(
        &self,
        width: u32,
        height: u32,
        contract: ColorImageContract,
    ) -> Result<ColorImageTarget, RenderError> {
        // Raw transport requires floating channels; integer storage may apply
        // clipping/transfer before the selected color processor can see values.
        if !contract.precision.is_float() {
            return Err(RenderError::Invalid(
                "OCIO image allocation requires floating raw transport".into(),
            ));
        }
        ColorImageTarget::from_image(
            self.image_render_target(width, height, contract.transport())?,
            contract.space,
        )
    }

    /// Upload already interpreted Float32 RGBA without transfer, clamping or
    /// alpha conversion. The caller supplies its exact OCIO space identity.
    /// Call only during resource preparation, never as a per-frame replacement.
    /// Pending queue writes retain the allocation until submission completes.
    pub fn upload_color_image_f32(
        &mut self,
        width: u32,
        height: u32,
        contract: ColorImageContract,
        pixels: &[[f32; 4]],
    ) -> Result<ColorImageTarget, RenderError> {
        self.check_health()?;
        if contract.precision != ChannelPrecision::Float32 {
            return Err(ColorImageUploadError::Precision(contract.precision).into());
        }
        let expected = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || expected != pixels.len() as u64 {
            return Err(ColorImageUploadError::Count {
                width,
                height,
                expected,
                actual: pixels.len(),
            }
            .into());
        }
        for (pixel, rgba) in pixels.iter().enumerate() {
            for (channel, value) in rgba.iter().enumerate() {
                if !value.is_finite() || (channel == 3 && !(0.0..=1.0).contains(value)) {
                    return Err(ColorImageUploadError::Pixel { pixel, channel }.into());
                }
            }
        }
        let target = self.color_image_target(width, height, contract)?;
        self.allocation_uploads()
            .track(target.image.texture().allocation())?;
        self.queue.write_texture(
            target.image.texture().as_image_copy(),
            bytemuck::cast_slice(pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 16),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.check_health()?;
        Ok(target)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ColorImageUploadError {
    #[error("OCIO image upload requires Float32 storage, got {0:?}")]
    Precision(ChannelPrecision),
    #[error(
        "OCIO image upload size={width}x{height} expected_pixels={expected} actual_pixels={actual}"
    )]
    Count {
        width: u32,
        height: u32,
        expected: u64,
        actual: usize,
    },
    #[error("OCIO image upload invalid pixel={pixel} channel={channel}: requires finite RGB and alpha in [0,1]")]
    Pixel { pixel: usize, channel: usize },
}

#[derive(Debug, thiserror::Error)]
#[error("OCIO image phase={phase:?} boundary={boundary}: expected={expected:?} actual={actual:?}")]
pub struct ColorSpaceMismatch {
    pub phase: ColorProcessorPhase,
    pub boundary: &'static str,
    pub expected: ColorSpaceIdentity,
    pub actual: ColorSpaceIdentity,
}
fn matches(
    phase: ColorProcessorPhase,
    boundary: &'static str,
    expected: ColorSpaceIdentity,
    actual: ColorSpaceIdentity,
) -> Result<(), RenderError> {
    if expected != actual {
        return Err(ColorSpaceMismatch {
            phase,
            boundary,
            expected,
            actual,
        }
        .into());
    }
    Ok(())
}

/// Reusable, identity-checked image pass. Pipeline construction remains outside
/// Present; encode only compares fixed-size identities before the existing pass.
pub struct ColorProcessorOutput {
    raw: RawColorProcessorOutput,
    phase: ColorProcessorPhase,
    output: ColorImageContract,
}
impl ColorProcessorOutput {
    pub fn new(
        renderer: &Renderer,
        source: &ColorImageTarget,
        processor: &CompiledColorImageProcessor,
        output: ColorImageContract,
    ) -> Result<Self, RenderError> {
        matches(
            processor.phase(),
            "source",
            processor.input_space(),
            source.contract.space,
        )?;
        matches(
            processor.phase(),
            "output-declaration",
            processor.output_space(),
            output.space,
        )?;
        let raw =
            RawColorProcessorOutput::new(renderer, &source.image, processor, output.transport())?;
        Ok(Self {
            raw,
            phase: processor.phase(),
            output,
        })
    }
    pub fn key(&self) -> [u8; 32] {
        self.raw.key()
    }
    pub fn output_contract(&self) -> ColorImageContract {
        self.output
    }
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        destination: &ColorImageTarget,
    ) -> Result<(), RenderError> {
        self.raw
            .validate_destination(renderer, encoder, &destination.image)?;
        matches(
            self.phase,
            "destination",
            self.output.space,
            destination.contract.space,
        )?;
        self.raw.encode_validated(encoder, &destination.image);
        Ok(())
    }
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        self.raw.texture_resource_ledger()
    }
}
