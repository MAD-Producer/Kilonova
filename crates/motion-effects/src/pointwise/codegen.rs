//! Preserve the three storage boundaries of each supported SDK image effect.
use super::*;
use std::{fmt::Write, time::Instant};

const MAX_POINTWISE_SOURCE_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct PortableFusionMetrics {
    pub stages: usize,
    pub uniform_cells: usize,
    pub generated_source_bytes: usize,
    pub compiled_wgsl_bytes: usize,
    pub compiled_glsl_bytes: usize,
    /// CPU WGSL validation and GLSL generation, not native pipeline creation.
    pub portable_compile_nanos: u64,
    /// Existing portable compiler metric, not a GPU instruction count.
    pub static_loop_work: u64,
    /// Bound derived from the actual compiled fragment IR, including the SDK.
    pub shader_texture_samples_upper_bound: u64,
}

#[derive(Clone, Debug)]
pub struct FusedShader {
    pub compiled: CompiledShader,
    pub metrics: PortableFusionMetrics,
}

const OPERATIONS: &str = r#"
fn pointwise_invert(c:vec4<f32>,base:u32)->vec4<f32>{
    let channel=fx.params[base].x;var rgb=c.rgb;var alpha=c.a;
    if channel==1.0{rgb=1.0-rgb;}else if channel==2.0{rgb.r=1.0-rgb.r;}
    else if channel==3.0{rgb.g=1.0-rgb.g;}else if channel==4.0{rgb.b=1.0-rgb.b;}
    else{alpha=1.0-alpha;}
    return mix(vec4(rgb,alpha),c,fx.params[base+1u].x/100.0);
}
fn pointwise_tint(c:vec4<f32>,base:u32)->vec4<f32>{
    let gray=dot(c.rgb,vec3(0.299,0.587,0.114));
    let rgb=mix(fx.params[base+1u].rgb,fx.params[base+2u].rgb,gray);
    return vec4(mix(c.rgb,rgb,fx.params[base+3u].x/100.0),c.a);
}
"#;

fn quantizers(precision: ChannelPrecision) -> &'static str {
    match precision {
        ChannelPrecision::Unorm8 => {
            r#"
fn pointwise_work_store(v:vec4<f32>)->vec4<f32>{return round(clamp(v,vec4(0.0),vec4(1.0))*255.0)/255.0;}
fn pointwise_canonical_store(v:vec4<f32>)->vec4<f32>{
    let encoded=vec4(srgb_encode(v.rgb),v.a);
    let q=round(clamp(encoded,vec4(0.0),vec4(1.0))*255.0)/255.0;
    return vec4(srgb_decode(q.rgb),q.a);
}
"#
        }
        ChannelPrecision::Unorm16 => {
            r#"
fn pointwise_work_store(v:vec4<f32>)->vec4<f32>{return round(clamp(v,vec4(0.0),vec4(1.0))*65535.0)/65535.0;}
fn pointwise_canonical_store(v:vec4<f32>)->vec4<f32>{return pointwise_work_store(v);}
"#
        }
        ChannelPrecision::Float16 => {
            r#"
// Round binary32 to binary16 precision without f16 types or packing builtins.
// Normal values retain ten fraction bits with round-to-nearest, ties-to-even.
// Subnormal binary16 values are integral multiples of 2^-24.
fn pointwise_half(v:f32)->f32{
    let bits=bitcast<u32>(v);let sign=bits&0x80000000u;let magnitude=bits&0x7fffffffu;
    if magnitude>=0x7f800000u{return v;}
    if magnitude>=0x47800000u{return bitcast<f32>(sign|0x7f800000u);}
    if magnitude<0x38800000u{
        let q=round(abs(v)*16777216.0)/16777216.0;
        return bitcast<f32>(bitcast<u32>(q)|sign);
    }
    let rounded=(magnitude+0xfffu+((magnitude>>13u)&1u))&0x7fffe000u;
    if rounded>=0x47800000u{return bitcast<f32>(sign|0x7f800000u);}
    return bitcast<f32>(sign|rounded);
}
fn pointwise_work_store(v:vec4<f32>)->vec4<f32>{
    return vec4(pointwise_half(v.x),pointwise_half(v.y),pointwise_half(v.z),pointwise_half(v.w));
}
fn pointwise_canonical_store(v:vec4<f32>)->vec4<f32>{return pointwise_work_store(v);}
"#
        }
        ChannelPrecision::Float32 => {
            r#"
fn pointwise_work_store(v:vec4<f32>)->vec4<f32>{return v;}
fn pointwise_canonical_store(v:vec4<f32>)->vec4<f32>{return v;}
"#
        }
    }
}

impl PointwiseChain {
    /// Shader code contains static operations and uniform offsets only. No object
    /// name, parameter name, instance ID or numeric parameter is interpolated.
    pub fn generated_source(&self) -> Result<String> {
        let mut source = String::from(OPERATIONS);
        source.push_str(quantizers(self.precision));
        source.push_str("\nfn main_fx(p:vec2<f32>)->vec4<f32>{var current=sample_input(p);\n");
        for (index, instruction) in self.instructions.iter().enumerate() {
            // Conversion into the effect's working texture and its first store.
            writeln!(source,"let original_{index}=pointwise_work_store(convert_pixel(current,vec2(0.0,0.0),vec2(1.0,1.0)));" )
                .map_err(|_| Error::Invalid("pointwise source generation failed".into()))?;
            writeln!(
                source,
                "let filtered_{index}=pointwise_{}(original_{index},{}u);",
                instruction.op.id(),
                instruction.parameter_slots.start
            )
            .map_err(|_| Error::Invalid("pointwise source generation failed".into()))?;
            // SDK non-rectangle image shaders mix in their original working
            // representation, before the second physical working store.
            writeln!(source,"let mixed_{index}=pointwise_work_store(mix(original_{index},filtered_{index},fx.params[{}u].x/100.0));",
                instruction.opacity_slot.index())
                .map_err(|_| Error::Invalid("pointwise source generation failed".into()))?;
            writeln!(
                source,
                "current=convert_pixel(mixed_{index},vec2(1.0,1.0),vec2(0.0,0.0));"
            )
            .map_err(|_| Error::Invalid("pointwise source generation failed".into()))?;
            if index + 1 != self.instructions.len() {
                // The final canonical store is performed by the actual output
                // attachment, not a second emulated conversion in this shader.
                source.push_str("current=pointwise_canonical_store(current);\n");
            }
        }
        source.push_str("return current;}\n");
        if source.len() > MAX_POINTWISE_SOURCE_BYTES {
            return Err(Error::Invalid(
                "pointwise generated source exceeds 32 KiB".into(),
            ));
        }
        Ok(source)
    }

    pub fn compile(&self) -> Result<FusedShader> {
        let source = self.generated_source()?;
        let began = Instant::now();
        let compiled = crate::shader::compile(&source, "main_fx")?;
        let elapsed = began.elapsed();
        let metrics = PortableFusionMetrics {
            stages: self.instructions.len(),
            uniform_cells: self.bindings.len(),
            generated_source_bytes: source.len(),
            compiled_wgsl_bytes: compiled.wgsl.len(),
            compiled_glsl_bytes: compiled.glsl.vertex.len() + compiled.glsl.fragment.len(),
            portable_compile_nanos: elapsed.as_nanos().min(u64::MAX as u128) as u64,
            static_loop_work: compiled.loop_work,
            shader_texture_samples_upper_bound: compiled.samples_per_fragment.ok_or_else(|| {
                Error::Invalid("pointwise compiled fragment has no texture-read bound".into())
            })?,
        };
        Ok(FusedShader { compiled, metrics })
    }
}
