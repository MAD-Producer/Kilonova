//! Direct image variant: native analytic operations and the same explicit table
//! interpolation, without per-frame image buffers or CPU readback.
use super::*;
use crate::color_config::{ColorSpaceIdentity, OutputSelection};

/// RGB code values at a storage boundary. Alpha always remains in [0,1].
/// Private, validated f32 bits make exact contract comparisons possible.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct CodeValueRange {
    black: u32,
    white: u32,
}
impl CodeValueRange {
    pub const FULL: Self = Self {
        black: 0,
        white: 1.0_f32.to_bits(),
    };
    pub fn new(black: f64, white: f64) -> Result<Self> {
        if !black.is_finite() || !white.is_finite() || black < 0.0 || white > 1.0 || black >= white
        {
            return Err(invalid(
                "code range",
                "requires finite 0 <= black < white <= 1",
            ));
        }
        let black = if black == 0.0 { 0.0 } else { black as f32 };
        let white = white as f32;
        let width = white - black;
        if width < f32::MIN_POSITIVE {
            return Err(invalid(
                "code range",
                "black/white interval is not a normal positive Float32 value",
            ));
        }
        Ok(Self {
            black: black.to_bits(),
            white: white.to_bits(),
        })
    }
    pub fn black(self) -> f32 {
        f32::from_bits(self.black)
    }
    pub fn white(self) -> f32 {
        f32::from_bits(self.white)
    }
    pub fn is_full(self) -> bool {
        self == Self::FULL
    }
}
impl Default for CodeValueRange {
    fn default() -> Self {
        Self::FULL
    }
}
impl std::fmt::Debug for CodeValueRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeValueRange")
            .field("black", &self.black())
            .field("white", &self.white())
            .finish()
    }
}

/// Immutable image program. Image contracts still describe storage and alpha;
/// complete color-space identity belongs to this explicitly selected processor.
pub struct CompiledColorImageProcessor {
    core: CompiledColorProcessor,
    wgsl: String,
    key: [u8; 32],
    input_alpha: AlphaMode,
    output_alpha: AlphaMode,
    input_space: ColorSpaceIdentity,
    output_space: ColorSpaceIdentity,
    input_range: CodeValueRange,
    output_range: CodeValueRange,
    opaque_display: bool,
}
impl CompiledColorImageProcessor {
    pub fn is_opaque_display(&self) -> bool {
        self.opaque_display
    }
    pub fn input_range(&self) -> CodeValueRange {
        self.input_range
    }
    pub fn output_range(&self) -> CodeValueRange {
        self.output_range
    }
    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    pub fn wgsl(&self) -> &str {
        &self.wgsl
    }
    pub fn tables(&self) -> &[Vec<[f32; 4]>] {
        self.core.tables()
    }
    pub fn phase(&self) -> ColorProcessorPhase {
        self.core.phase()
    }
    pub fn input_alpha(&self) -> AlphaMode {
        self.input_alpha
    }
    pub fn output_alpha(&self) -> AlphaMode {
        self.output_alpha
    }
    pub fn input_space(&self) -> ColorSpaceIdentity {
        self.input_space
    }
    pub fn output_space(&self) -> ColorSpaceIdentity {
        self.output_space
    }
    /// Logical retained shader/table payload, excluding allocator/GPU overhead.
    pub fn payload_bytes(&self) -> usize {
        self.wgsl.len()
            + self.core.wgsl().len()
            + self
                .core
                .tables()
                .iter()
                .map(|table| table.len() * std::mem::size_of::<[f32; 4]>())
                .sum::<usize>()
    }
}
impl ColorProcessorSource {
    /// Prepare away from Present. Validate the same configuration and numerical
    /// program as the compute variant, then independently validate the image ABI.
    pub fn compile_image(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
    ) -> Result<CompiledColorImageProcessor> {
        self.compile_image_with_ranges(
            config,
            selection,
            CodeValueRange::FULL,
            CodeValueRange::FULL,
        )
    }
    /// Decode RGB code values after unpremultiplication; encode them before
    /// output premultiplication. Do not clamp negative values or highlights.
    pub fn compile_image_with_ranges(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
        input_range: CodeValueRange,
        output_range: CodeValueRange,
    ) -> Result<CompiledColorImageProcessor> {
        self.compile_image_mode(config, selection, input_range, output_range, false)
    }

    /// Composite premultiplied working RGB over working black before the display
    /// transform. This produces opaque surface pixels without an extra image pass
    /// or the incorrect alternative of matting already-encoded display values.
    pub fn compile_display_over_black(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
    ) -> Result<CompiledColorImageProcessor> {
        if self.phase != ColorProcessorPhase::WorkingToDisplay
            || self.input_alpha != AlphaMode::Premultiplied
        {
            return Err(invalid(
                "opaque display",
                "requires WorkingToDisplay with premultiplied full-range working RGB",
            ));
        }
        self.compile_image_mode(
            config,
            selection,
            CodeValueRange::FULL,
            CodeValueRange::FULL,
            true,
        )
    }

    fn compile_image_mode(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
        input_range: CodeValueRange,
        output_range: CodeValueRange,
        opaque_display: bool,
    ) -> Result<CompiledColorImageProcessor> {
        let core = self.compile(config, selection)?;
        let (input_space, output_space) = match self.phase {
            ColorProcessorPhase::InputToWorking => (
                config.space_identity(&selection.input)?,
                config.space_identity(&selection.working)?,
            ),
            ColorProcessorPhase::WorkingToDisplay => (
                config.space_identity(&selection.working)?,
                config.display_identity(&selection.display)?,
            ),
            ColorProcessorPhase::WorkingToOutput => (
                config.space_identity(&selection.working)?,
                match &selection.output {
                    OutputSelection::Space { space } => config.space_identity(space)?,
                    OutputSelection::Display { display_view } => {
                        config.display_identity(display_view)?
                    }
                },
            ),
        };
        let mut glsl = self.glsl_source()?;
        glsl.push_str("\nlayout(set=1,binding=0) uniform texture2D motion_source;\nlayout(location=0) out vec4 motion_output;\nvoid main() { vec4 original=texelFetch(motion_source,ivec2(gl_FragCoord.xy),0);\n");
        if opaque_display {
            glsl.push_str(
                "original.rgb=original.a>0.0 ? original.rgb : vec3(0.0); original.a=1.0;\n",
            );
        }
        glsl.push_str("vec4 color=original;\n");
        if self.input_alpha == AlphaMode::Premultiplied {
            if input_range.is_full() {
                glsl.push_str("color.rgb = original.a > 0.0 ? color.rgb/original.a : vec3(0.0);\n");
            } else {
                // Undefined zero-alpha RGB is canonical black in decoded
                // units, which is the black code before normalization.
                glsl.push_str(&format!(
                    "color.rgb = original.a > 0.0 ? color.rgb/original.a : vec3({:.9e});\n",
                    input_range.black()
                ));
            }
        }
        if !input_range.is_full() {
            glsl.push_str(&format!(
                "color.rgb=(color.rgb-vec3({:.9e}))/vec3({:.9e});\n",
                input_range.black(),
                input_range.white() - input_range.black()
            ));
        }
        glsl.push_str("color=motion_ocio_transform(color); color.a=original.a;\n");
        if !output_range.is_full() {
            glsl.push_str(&format!(
                "color.rgb=color.rgb*vec3({:.9e})+vec3({:.9e});\n",
                output_range.white() - output_range.black(),
                output_range.black()
            ));
        }
        if self.output_alpha == AlphaMode::Premultiplied {
            glsl.push_str("color.rgb*=original.a;\n");
        }
        glsl.push_str("motion_output=color; }\n");
        let module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
                &glsl,
            )
            .map_err(ColorProcessorError::Parse)?;
        validate_bindings(&module, self.tables.len(), true)?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(ColorProcessorError::Validation)?;
        let mut wgsl = naga::back::wgsl::write_string(
            &module,
            &info,
            naga::back::wgsl::WriterFlags::EXPLICIT_TYPES,
        )
        .map_err(ColorProcessorError::Write)?;
        wgsl.push_str("\n@vertex fn motion_image_vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> { let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0)); return vec4(p[i],0.0,1.0); }\n");
        bounded("compiled image shader bytes", wgsl.len(), 512 * 1024)?;
        // The independently authored vertex entry and generated fragment code
        // must also form one valid final module; preserve original parser causes.
        let final_module =
            naga::front::wgsl::parse_str(&wgsl).map_err(ColorProcessorError::WgslParse)?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&final_module)
        .map_err(ColorProcessorError::Validation)?;
        let mut digest = Sha256::new();
        digest.update(b"motion-color-image-v1\0");
        digest.update(core.key());
        digest.update(wgsl.as_bytes());
        Ok(CompiledColorImageProcessor {
            core,
            wgsl,
            key: digest.finalize().into(),
            input_alpha: self.input_alpha,
            output_alpha: self.output_alpha,
            input_space,
            output_space,
            input_range,
            output_range,
            opaque_display,
        })
    }
}
