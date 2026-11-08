//! Preparation of explicit OCIO GPU programs. Native OCIO is an external
//! preparer, not a product dependency. Tables are unfiltered numeric buffers;
//! their interpolation is explicit and does not require Float32 filtering.
use crate::color_config::{ColorConfigError, ColorSelection, ColorVersion, VerifiedColorConfig};
use crate::AlphaMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod image;
pub use image::{CodeValueRange, CompiledColorImageProcessor};

pub const COLOR_PROCESSOR_VERSION: u32 = 1;
pub const MAX_PROCESSOR_JSON_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PROCESSOR_SHADER_BYTES: usize = 256 * 1024;
pub const MAX_PROCESSOR_TABLES: usize = 4;
pub const MAX_PROCESSOR_TABLE_TEXELS: usize = 262_144;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorProcessorPhase {
    InputToWorking,
    WorkingToDisplay,
    WorkingToOutput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableInterpolation {
    Nearest,
    Linear,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorProcessorTable {
    pub texture_name: String,
    pub sampler_name: String,
    pub binding: u32,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub interpolation: TableInterpolation,
    pub values: Vec<f32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorProcessorSource {
    pub version: u32,
    pub ocio_version: ColorVersion,
    pub config_key: [u8; 32],
    pub selection_key: [u8; 32],
    pub phase: ColorProcessorPhase,
    pub input_alpha: AlphaMode,
    pub output_alpha: AlphaMode,
    /// Unmodified OCIO 2.6 Vulkan GLSL, function motion_ocio_transform,
    /// resource prefix motion_ocio_, setAllowTexture1D(false).
    pub shader: String,
    pub tables: Vec<ColorProcessorTable>,
}

#[derive(Debug, thiserror::Error)]
pub enum ColorProcessorError {
    #[error("color processor phase=json line={} column={}", .0.line(), .0.column())]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] ColorConfigError),
    #[error("color processor invalid field={field}: {reason}")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    #[error("color processor capacity field={field} requested={requested} limit={limit}")]
    Capacity {
        field: &'static str,
        requested: usize,
        limit: usize,
    },
    #[error("color processor phase=glsl-parse")]
    Parse(#[source] naga::front::glsl::ParseErrors),
    #[error("color processor phase=shader-validation")]
    Validation(#[source] naga::WithSpan<naga::valid::ValidationError>),
    #[error("color processor phase=image-wgsl-parse")]
    WgslParse(#[source] naga::front::wgsl::ParseError),
    #[error("color processor phase=wgsl-write")]
    Write(#[source] naga::back::wgsl::Error),
}
type Result<T> = std::result::Result<T, ColorProcessorError>;
fn invalid(field: &'static str, reason: &'static str) -> ColorProcessorError {
    ColorProcessorError::Invalid { field, reason }
}
fn bounded(field: &'static str, requested: usize, limit: usize) -> Result<()> {
    if requested > limit {
        Err(ColorProcessorError::Capacity {
            field,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}
fn identifier(value: &str) -> bool {
    value.len() <= 128
        && value.starts_with("motion_ocio_")
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'_')
}

/// Immutable prepared code and tables. Compilation must run in a non-Present
/// preparation operation. This verifies bindings and identities, not that an
/// arbitrary shader implements the OCIO semantics claimed by its preparer.
pub struct CompiledColorProcessor {
    key: [u8; 32],
    wgsl: String,
    tables: Vec<Vec<[f32; 4]>>,
    phase: ColorProcessorPhase,
}
impl CompiledColorProcessor {
    pub fn key(&self) -> [u8; 32] {
        self.key
    }
    pub fn wgsl(&self) -> &str {
        &self.wgsl
    }
    pub fn tables(&self) -> &[Vec<[f32; 4]>] {
        &self.tables
    }
    pub fn phase(&self) -> ColorProcessorPhase {
        self.phase
    }
}

impl ColorProcessorSource {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        bounded("json bytes", bytes.len(), MAX_PROCESSOR_JSON_BYTES)?;
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if self.version != COLOR_PROCESSOR_VERSION {
            return Err(invalid("version", "unsupported processor schema"));
        }
        if self.ocio_version
            != (ColorVersion {
                major: 2,
                minor: 6,
                patch: 0,
            })
        {
            return Err(invalid(
                "ocio_version",
                "requires the explicit OCIO 2.6.0 preparer",
            ));
        }
        bounded(
            "shader bytes",
            self.shader.len(),
            MAX_PROCESSOR_SHADER_BYTES,
        )?;
        if self.shader.is_empty() {
            return Err(invalid("shader", "empty source"));
        }
        bounded("tables", self.tables.len(), MAX_PROCESSOR_TABLES)?;
        let mut names = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for table in &self.tables {
            for name in [&table.texture_name, &table.sampler_name] {
                if !identifier(name) || !names.insert(name) {
                    return Err(invalid(
                        "table name",
                        "invalid or duplicate resource identifier",
                    ));
                }
            }
            if !bindings.insert(table.binding) {
                return Err(invalid(
                    "table binding",
                    "duplicate original resource binding",
                ));
            }
            if table.width == 0 || table.height == 0 || !matches!(table.channels, 1 | 3) {
                return Err(invalid(
                    "table shape",
                    "requires nonzero dimensions and 1 or 3 channels",
                ));
            }
            let texels = u64::from(table.width) * u64::from(table.height);
            if texels > MAX_PROCESSOR_TABLE_TEXELS as u64 {
                return Err(ColorProcessorError::Capacity {
                    field: "table texels",
                    requested: usize::try_from(texels).unwrap_or(usize::MAX),
                    limit: MAX_PROCESSOR_TABLE_TEXELS,
                });
            }
            if table.values.len() != texels as usize * table.channels as usize
                || table.values.iter().any(|v| !v.is_finite())
            {
                return Err(invalid(
                    "table values",
                    "requires the exact finite sample count",
                ));
            }
        }
        Ok(())
    }

    fn glsl_source(&self) -> Result<String> {
        let mut source = self.shader.clone();
        // OCIO emits nested RGB swizzles. Flatten only the exact equivalent
        // components; Naga 30 GLSL otherwise creates an invalid store pointer.
        for component in ['r', 'g', 'b'] {
            source = replace_expression(
                &source,
                &format!("outColor.rgb.{component}"),
                &format!("outColor.{component}"),
            );
        }
        let mut header = String::from("#version 460\n");
        for (index, table) in self.tables.iter().enumerate() {
            let declaration = format!(
                "layout(set=0, binding = {}) uniform sampler2D {};",
                table.binding, table.sampler_name
            );
            if source.matches(&declaration).count() != 1 {
                return Err(invalid(
                    "table declaration",
                    "expected one exact OCIO 2.6 Vulkan sampler2D declaration",
                ));
            }
            source = source.replacen(&declaration, "", 1);
            let call = format!("texture({},", table.sampler_name);
            if !source.contains(&call) {
                return Err(invalid(
                    "table sample",
                    "declared sampler has no recognized OCIO texture call",
                ));
            }
            source = source.replace(&call, &format!("motion_sample_{index}("));
            header.push_str(&sample_function(index, table));
        }
        header.push_str(&source);
        Ok(header)
    }

    pub fn compile(
        &self,
        config: &VerifiedColorConfig,
        selection: &ColorSelection,
    ) -> Result<CompiledColorProcessor> {
        self.validate()?;
        let resolved = config.resolve(selection)?;
        if self.config_key != config.key()
            || self.selection_key != resolved.key
            || self.ocio_version != config.manifest().ocio_version
        {
            return Err(invalid(
                "identity",
                "processor differs from selected config, policy or runtime",
            ));
        }
        let mut header = self.glsl_source()?;
        let tables = self
            .tables
            .iter()
            .map(|table| {
                table
                    .values
                    .chunks_exact(table.channels as usize)
                    .map(|v| {
                        if table.channels == 1 {
                            [v[0], 0., 0., 1.]
                        } else {
                            [v[0], v[1], v[2], 1.]
                        }
                    })
                    .collect()
            })
            .collect();
        header.push_str("\nlayout(local_size_x=1) in;\nlayout(set=1,binding=0,std430) readonly buffer MotionInputs { vec4 pixels[]; } input_values;\nlayout(set=1,binding=1,std430) buffer MotionOutputs { vec4 pixels[]; } output_values;\nvoid main() { uint i=gl_GlobalInvocationID.x; vec4 original=input_values.pixels[i]; vec4 color=original;\n");
        if self.input_alpha == AlphaMode::Premultiplied {
            header.push_str("color.rgb = original.a > 0.0 ? color.rgb/original.a : vec3(0.0);\n");
        }
        header.push_str("color=motion_ocio_transform(color); color.a=original.a;\n");
        if self.output_alpha == AlphaMode::Premultiplied {
            header.push_str("color.rgb*=original.a;\n");
        }
        header.push_str("output_values.pixels[i]=color; }\n");
        let mut module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Compute),
                &header,
            )
            .map_err(ColorProcessorError::Parse)?;
        validate_bindings(&module, self.tables.len(), false)?;
        add_dispatch_guard(&mut module)?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(ColorProcessorError::Validation)?;
        let wgsl = naga::back::wgsl::write_string(
            &module,
            &info,
            naga::back::wgsl::WriterFlags::EXPLICIT_TYPES,
        )
        .map_err(ColorProcessorError::Write)?;
        bounded("compiled shader bytes", wgsl.len(), 512 * 1024)?;
        let mut digest = Sha256::new();
        digest.update(b"motion-color-processor-v1\0");
        digest.update(self.version.to_le_bytes());
        digest.update(serde_json::to_vec(&(
            self.ocio_version,
            self.phase,
            self.input_alpha,
            self.output_alpha,
        ))?);
        digest.update(self.config_key);
        digest.update(self.selection_key);
        digest.update((self.shader.len() as u64).to_le_bytes());
        digest.update(self.shader.as_bytes());
        for table in &self.tables {
            digest.update(serde_json::to_vec(&(
                &table.texture_name,
                &table.sampler_name,
                table.binding,
                table.width,
                table.height,
                table.channels,
                table.interpolation,
            ))?);
            for value in &table.values {
                digest.update(value.to_le_bytes());
            }
        }
        digest.update(b"naga-30.0.1-buffer-table-v1\0");
        digest.update(wgsl.as_bytes());
        Ok(CompiledColorProcessor {
            key: digest.finalize().into(),
            wgsl,
            tables,
            phase: self.phase,
        })
    }
}

fn replace_expression(source: &str, from: &str, to: &str) -> String {
    let ident = |value: u8| value.is_ascii_alphanumeric() || value == b'_';
    let mut result = String::with_capacity(source.len());
    let mut last = 0;
    for (start, _) in source.match_indices(from) {
        let end = start + from.len();
        let prior = start > 0 && ident(source.as_bytes()[start - 1]);
        let next = source.as_bytes().get(end).is_some_and(|v| ident(*v));
        if !prior && !next {
            result.push_str(&source[last..start]);
            result.push_str(to);
            last = end;
        }
    }
    result.push_str(&source[last..]);
    result
}

fn sample_function(index: usize, table: &ColorProcessorTable) -> String {
    let width = table.width;
    let height = table.height;
    let mut code=format!("\nlayout(set=0,binding={index},std430) readonly buffer MotionTable{index} {{ vec4 samples[]; }} motion_table_{index};\nvec4 motion_fetch_{index}(ivec2 p) {{ return motion_table_{index}.samples[p.y*{width}+p.x]; }}\nvec4 motion_sample_{index}(vec2 uv) {{\n");
    match table.interpolation {
        TableInterpolation::Nearest => code.push_str(&format!("ivec2 p=ivec2(clamp(floor(uv*vec2({width}.0,{height}.0)),vec2(0.0),vec2({}.0,{}.0))); return motion_fetch_{index}(p); }}\n",width-1,height-1)),
        TableInterpolation::Linear => code.push_str(&format!("vec2 p=clamp(uv*vec2({width}.0,{height}.0)-vec2(0.5),vec2(0.0),vec2({}.0,{}.0)); ivec2 lo=ivec2(floor(p)); ivec2 hi=min(lo+ivec2(1),ivec2({},{})); vec2 t=p-vec2(lo); return mix(mix(motion_fetch_{index}(lo),motion_fetch_{index}(ivec2(hi.x,lo.y)),t.x),mix(motion_fetch_{index}(ivec2(lo.x,hi.y)),motion_fetch_{index}(hi),t.x),t.y); }}\n",width-1,height-1,width-1,height-1)),
    }
    code
}

fn validate_bindings(module: &naga::Module, tables: usize, image: bool) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (_, global) in module.global_variables.iter() {
        if let Some(binding) = &global.binding {
            let (group, index) = (binding.group, binding.binding);
            if image && group == 1 && index == 0 {
                if global.space != naga::AddressSpace::Handle
                    || module.types[global.ty].inner
                        != (naga::TypeInner::Image {
                            dim: naga::ImageDimension::D2,
                            arrayed: false,
                            class: naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Float,
                                multi: false,
                            },
                        })
                    || !seen.insert((group, index))
                {
                    return Err(invalid(
                        "image input",
                        "requires one non-arrayed Float32 sampled 2D image",
                    ));
                }
                continue;
            }
            let allowed =
                (group == 0 && (index as usize) < tables) || (!image && group == 1 && index <= 1);
            let naga::AddressSpace::Storage { access } = global.space else {
                return Err(invalid(
                    "shader binding",
                    "only explicit numeric storage buffers are permitted",
                ));
            };
            let expected = if group == 1 && index == 1 {
                naga::StorageAccess::LOAD | naga::StorageAccess::STORE
            } else {
                naga::StorageAccess::LOAD
            };
            if !allowed || access != expected || !seen.insert((group, index)) {
                return Err(invalid(
                    "shader binding",
                    "unexpected or writable processor resource",
                ));
            }
            let naga::TypeInner::Struct { members, .. } = &module.types[global.ty].inner else {
                return Err(invalid(
                    "shader buffer",
                    "requires the fixed numeric array block",
                ));
            };
            if members.len() != 1 {
                return Err(invalid("shader buffer", "requires one numeric array"));
            }
            let naga::TypeInner::Array {
                base,
                size: naga::ArraySize::Dynamic,
                stride: 16,
            } = module.types[members[0].ty].inner
            else {
                return Err(invalid(
                    "shader buffer",
                    "requires runtime vec4 array with stride 16",
                ));
            };
            if module.types[base].inner
                != (naga::TypeInner::Vector {
                    size: naga::VectorSize::Quad,
                    scalar: naga::Scalar::F32,
                })
            {
                return Err(invalid("shader buffer", "requires Float32 RGBA values"));
            }
        } else if !matches!(
            global.space,
            naga::AddressSpace::Private | naga::AddressSpace::Function
        ) {
            return Err(invalid(
                "shader globals",
                "unexpected non-private unbound resource",
            ));
        }
    }
    if seen.len() != tables + if image { 1 } else { 2 } || module.entry_points.len() != 1 {
        return Err(invalid(
            "shader interface",
            "requires the fixed processor entry point and all bindings",
        ));
    }
    let expected = if image {
        naga::ShaderStage::Fragment
    } else {
        naga::ShaderStage::Compute
    };
    if module.entry_points[0].stage != expected {
        return Err(invalid("entry point", "unexpected processor shader stage"));
    }
    Ok(())
}

/// Naga's GLSL array.length() path loses the storage-array pointer. Construct the
/// bounds guard in its public IR, then run the normal validator and WGSL writer.
/// This is our wrapper, not a patch or replacement of OCIO/Naga calculations.
fn add_dispatch_guard(module: &mut naga::Module) -> Result<()> {
    use naga::{BinaryOperator as B, Expression as E, Span, Statement as S};
    let globals: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(handle, var)| {
            var.binding
                .as_ref()
                .filter(|b| b.group == 1)
                .map(|b| (b.binding, handle))
        })
        .collect();
    let input = globals
        .iter()
        .find(|(binding, _)| *binding == 0)
        .ok_or_else(|| invalid("input", "missing input buffer"))?
        .1;
    let output = globals
        .iter()
        .find(|(binding, _)| *binding == 1)
        .ok_or_else(|| invalid("output", "missing output buffer"))?
        .1;
    let entry = &mut module.entry_points[0];
    if entry.function.arguments.len() != 1
        || !matches!(
            entry.function.arguments[0].binding,
            Some(naga::Binding::BuiltIn(naga::BuiltIn::GlobalInvocationId))
        )
    {
        return Err(invalid(
            "entry point",
            "requires only global invocation coordinates",
        ));
    }
    let e = &mut entry.function.expressions;
    let arg = e.append(E::FunctionArgument(0), Span::UNDEFINED);
    let input = e.append(E::GlobalVariable(input), Span::UNDEFINED);
    let output = e.append(E::GlobalVariable(output), Span::UNDEFINED);
    let start = e.len();
    let index = e.append(
        E::AccessIndex {
            base: arg,
            index: 0,
        },
        Span::UNDEFINED,
    );
    let input = e.append(
        E::AccessIndex {
            base: input,
            index: 0,
        },
        Span::UNDEFINED,
    );
    let output = e.append(
        E::AccessIndex {
            base: output,
            index: 0,
        },
        Span::UNDEFINED,
    );
    let input = e.append(E::ArrayLength(input), Span::UNDEFINED);
    let output = e.append(E::ArrayLength(output), Span::UNDEFINED);
    let input = e.append(
        E::Binary {
            op: B::GreaterEqual,
            left: index,
            right: input,
        },
        Span::UNDEFINED,
    );
    let output = e.append(
        E::Binary {
            op: B::GreaterEqual,
            left: index,
            right: output,
        },
        Span::UNDEFINED,
    );
    let condition = e.append(
        E::Binary {
            op: B::LogicalOr,
            left: input,
            right: output,
        },
        Span::UNDEFINED,
    );
    let mut body = naga::Block::new();
    body.push(S::Emit(e.range_from(start)), Span::UNDEFINED);
    let mut reject = naga::Block::new();
    reject.push(S::Return { value: None }, Span::UNDEFINED);
    body.push(
        S::If {
            condition,
            accept: reject,
            reject: naga::Block::new(),
        },
        Span::UNDEFINED,
    );
    body.extend_block(std::mem::take(&mut entry.function.body));
    entry.function.body = body;
    Ok(())
}
