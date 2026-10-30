use super::*;
use motion_effects::{shader, ParamKind, PluginManifest};

/// Logic compilation only; packaging the new layout protocol is a later step.
pub enum CompiledLogic {
    Graph(crate::CompiledGraph),
    Wgsl {
        source: String,
        manifest: PluginManifest,
        shader: shader::CompiledShader,
        accessors: Vec<ParameterAccessor>,
    },
}
impl CompiledLogic {
    pub fn manifest(&self) -> &PluginManifest {
        match self {
            Self::Graph(graph) => &graph.manifest,
            Self::Wgsl { manifest, .. } => manifest,
        }
    }
    pub fn source(&self) -> &str {
        match self {
            Self::Graph(graph) => &graph.source,
            Self::Wgsl { source, .. } => source,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParameterAccessor {
    pub parameter: String,
    pub function: String,
    pub slot: usize,
}

impl PluginProject {
    /// Unsaved virtual WGSL document for a language server, including real ABI
    /// declarations and stable public-parameter helpers. Returns author row offset.
    /// Language analysis is advisory; compile_logic retains every runtime check.
    pub fn wgsl_language_document(&self) -> Result<Option<(String, usize)>> {
        let PluginLogic::Wgsl { document } = &self.logic else {
            return Ok(None);
        };
        let (parameters, _) = parameter_prelude(&document.parameters)?;
        let prefix = format!("{}{parameters}\n", shader::authoring_prelude());
        let rows = prefix.bytes().filter(|b| *b == b'\n').count();
        Ok(Some((format!("{prefix}{}", document.source), rows)))
    }
    pub fn compile_error_location(&self, error: &Error) -> Option<(usize, usize)> {
        let PluginLogic::Wgsl { document } = &self.logic else {
            return None;
        };
        let Error::Effect { cause, .. } = error else {
            return None;
        };
        let (prelude, _) = parameter_prelude(&document.parameters).ok()?;
        shader::author_location(
            cause,
            &format!("{prelude}\n{}", document.source),
            &document.source,
            &document.entry,
        )
    }
    pub fn compile_logic(&self) -> Result<CompiledLogic> {
        self.validate_bindings()?;
        match &self.logic {
            PluginLogic::Graph { document } => crate::compile(document, &self.compile_options())
                .map(CompiledLogic::Graph)
                .map_err(|cause| Error::graph(Phase::Compile, cause)),
            PluginLogic::Wgsl { document } => {
                let manifest = self.wgsl_manifest(document)?;
                let (prelude, accessors) = parameter_prelude(&document.parameters)?;
                let source = format!("{prelude}\n{}", document.source);
                error::ensure(
                    source.len() <= 256 * 1024,
                    Phase::Compile,
                    "logic.source",
                    "WGSL source plus parameter accessors exceeds 256 KiB",
                )?;
                let shader = shader::compile(&source, &document.entry)
                    .map_err(|cause| Error::effect(Phase::Compile, cause))?;
                Ok(CompiledLogic::Wgsl {
                    source,
                    manifest,
                    shader,
                    accessors,
                })
            }
        }
    }
    fn wgsl_manifest(&self, document: &WgslDocument) -> Result<PluginManifest> {
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
            "format_version":1,"sdk_version":6,"id":self.package.id,
            "version":self.package.version,"name":document.name,
            "author":self.package.author,"license":self.package.license,
            "effects":[{"id":document.id,"name":document.name,"english_name":document.english_name,
                "category":"Plugin","params":document.parameters,
                "passes":[{"shader":"shaders/graph.wgsl","entry":document.entry}],
                "working_space":document.working_space,"alpha_mode":document.alpha_mode,
                "required_capabilities":["single_frame","color_profile"]}]
        }))
        .map_err(|e| Error::json(Phase::Compile, e))?;
        manifest
            .validate()
            .map_err(|cause| Error::effect(Phase::Compile, cause))?;
        Ok(manifest)
    }
}

/// Reversible byte escaping makes helper names independent of slot order and
/// distinguishes IDs such as a-b, a_b and a. Names cannot inject WGSL syntax.
pub fn parameter_function(id: &str) -> String {
    let mut name = String::from("param_");
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() {
            name.push(byte as char);
        } else {
            name.push_str(&format!("_{byte:02x}_"));
        }
    }
    name
}
fn parameter_prelude(params: &[ParamDefinition]) -> Result<(String, Vec<ParameterAccessor>)> {
    let mut source = String::new();
    let mut accessors = Vec::new();
    for (slot, param) in params.iter().enumerate() {
        let (ty, value) = match param.kind {
            ParamKind::Float => ("f32", format!("fx.params[{slot}].x")),
            ParamKind::Vec2 => ("vec2<f32>", format!("fx.params[{slot}].xy")),
            ParamKind::Vec3 => ("vec3<f32>", format!("fx.params[{slot}].xyz")),
            ParamKind::Color => ("vec4<f32>", format!("fx.params[{slot}]")),
            ParamKind::Bool => ("bool", format!("fx.params[{slot}].x != 0.0")),
            ParamKind::Enum => ("i32", format!("i32(fx.params[{slot}].x)")),
            ParamKind::Curve => {
                return Err(Error::Invalid {
                    phase: Phase::Compile,
                    field: "logic.parameters",
                    message: "WGSL parameter accessors do not include curve objects",
                })
            }
        };
        let function = parameter_function(&param.id);
        source.push_str(&format!("fn {function}() -> {ty} {{ return {value}; }}\n"));
        accessors.push(ParameterAccessor {
            parameter: param.id.clone(),
            function,
            slot,
        });
    }
    Ok((source, accessors))
}
