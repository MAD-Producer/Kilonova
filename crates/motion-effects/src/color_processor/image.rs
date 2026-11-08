//! Direct image variant: native analytic operations and the same explicit table
//! interpolation, without per-frame image buffers or CPU readback.
use super::*;
use crate::color_config::{ColorSpaceIdentity, OutputSelection};

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
}
impl CompiledColorImageProcessor {
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
}
impl ColorProcessorSource {
    /// Prepare away from Present. Validate the same configuration and numerical
    /// program as the compute variant, then independently validate the image ABI.
    pub fn compile_image(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
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
        glsl.push_str("\nlayout(set=1,binding=0) uniform texture2D motion_source;\nlayout(location=0) out vec4 motion_output;\nvoid main() { vec4 original=texelFetch(motion_source,ivec2(gl_FragCoord.xy),0); vec4 color=original;\n");
        if self.input_alpha == AlphaMode::Premultiplied {
            glsl.push_str("color.rgb = original.a > 0.0 ? color.rgb/original.a : vec3(0.0);\n");
        }
        glsl.push_str("color=motion_ocio_transform(color); color.a=original.a;\n");
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
        })
    }
}
