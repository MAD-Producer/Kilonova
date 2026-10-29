//! Editable plugin sources, independent of a desktop window or GPU instance.
//! Draft persistence is separate from binding checks and logic compilation.
mod compile;
mod error;
mod storage;
use crate::{CompileOptions, GraphDocument};
pub use compile::{parameter_function, CompiledLogic, ParameterAccessor};
pub use error::{Error, Phase, Result};
use motion_effects::{native_layout::*, AlphaMode, ParamDefinition, WorkingSpace};
use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: u32 = 1;
pub const MAX_SOURCE_BYTES: usize = 240 * 1024;
pub const MAX_PARAMETER_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_PROJECT_BYTES: usize = crate::MAX_DOCUMENT_BYTES + 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub id: String,
    pub version: String,
    pub author: String,
    pub license: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginLogic {
    Graph { document: GraphDocument },
    Wgsl { document: WgslDocument },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WgslDocument {
    pub id: String,
    pub name: String,
    pub english_name: String,
    pub source: String,
    pub entry: String,
    pub parameters: Vec<ParamDefinition>,
    pub working_space: WorkingSpace,
    pub alpha_mode: AlphaMode,
}

/// Deserialize through `from_json` so graph image validation keeps its native
/// error causes and every entry point applies the authoring file budget.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PluginProject {
    pub format_version: u32,
    pub package: PackageIdentity,
    pub logic: PluginLogic,
    pub interface: NativeLayout,
}

impl PluginProject {
    pub fn from_graph(document: GraphDocument, package: PackageIdentity) -> Result<Self> {
        let parameters = crate::compiler::public_parameter_definitions(&document)
            .map_err(|cause| Error::graph(Phase::Bindings, cause))?;
        let project = Self {
            format_version: PROJECT_VERSION,
            package,
            logic: PluginLogic::Graph { document },
            interface: parameter_layout(&parameters),
        };
        project.validate_draft()?;
        Ok(project)
    }
    pub fn from_wgsl(document: WgslDocument, package: PackageIdentity) -> Result<Self> {
        let interface = parameter_layout(&document.parameters);
        let project = Self {
            format_version: PROJECT_VERSION,
            package,
            logic: PluginLogic::Wgsl { document },
            interface,
        };
        project.validate_draft()?;
        Ok(project)
    }

    pub fn parameters(&self) -> Result<Vec<ParamDefinition>> {
        match &self.logic {
            PluginLogic::Graph { document } => {
                crate::compiler::public_parameter_definitions(document)
                    .map_err(|cause| Error::graph(Phase::Bindings, cause))
            }
            PluginLogic::Wgsl { document } => Ok(document.parameters.clone()),
        }
    }

    /// Structural validity only. Unconnected graph inputs, WGSL syntax errors
    /// and dangling layout bindings remain editable, saveable draft state.
    pub fn validate_draft(&self) -> Result<()> {
        error::ensure(
            self.format_version == PROJECT_VERSION,
            Phase::Structure,
            "format_version",
            "unsupported plugin project version",
        )?;
        error::ensure(
            motion_effects::valid_id(&self.package.id),
            Phase::Structure,
            "package.id",
            "invalid plugin identity",
        )?;
        for (field, value, empty) in [
            ("package.version", &self.package.version, false),
            ("package.author", &self.package.author, true),
            ("package.license", &self.package.license, false),
        ] {
            error::ensure(
                (empty || !value.trim().is_empty()) && value.len() <= 256,
                Phase::Structure,
                field,
                "metadata exceeds its text limit or is empty",
            )?;
        }
        self.interface
            .validate_structure()
            .map_err(|cause| Error::effect(Phase::Structure, cause))?;
        match &self.logic {
            PluginLogic::Graph { document } => {
                // Reuse graph image identities, byte limits and required graph
                // versions. This does not require a complete/compilable DAG.
                document
                    .to_json()
                    .map_err(|cause| Error::graph(Phase::Structure, cause))?;
            }
            PluginLogic::Wgsl { document } => {
                error::ensure(
                    document.source.len() <= MAX_SOURCE_BYTES,
                    Phase::Structure,
                    "logic.source",
                    "WGSL source exceeds 240 KiB",
                )?;
                error::ensure(
                    document.parameters.len() <= motion_effects::MAX_PARAMS,
                    Phase::Structure,
                    "logic.parameters",
                    "more than 32 public parameters",
                )?;
                for (field, value) in [("logic.id", &document.id), ("logic.entry", &document.entry)]
                {
                    error::ensure(
                        motion_effects::valid_id(value),
                        Phase::Structure,
                        field,
                        "invalid source identity or entry",
                    )?;
                }
                for value in [&document.name, &document.english_name] {
                    error::ensure(
                        !value.trim().is_empty() && value.len() <= 120,
                        Phase::Structure,
                        "logic.name",
                        "invalid effect display name",
                    )?;
                }
                // serde_json would otherwise turn a programmatically authored
                // NaN/Infinity into null, losing the original value on save.
                let mut metadata_bytes = 0usize;
                for param in &document.parameters {
                    metadata_bytes = [
                        &param.id,
                        &param.name,
                        &param.units,
                        &param.reference_match_name,
                    ]
                    .into_iter()
                    .chain(param.options.iter())
                    .fold(metadata_bytes, |size, text| size.saturating_add(text.len()));
                    error::ensure(
                        param.options.len() <= 256
                            && metadata_bytes <= MAX_PARAMETER_METADATA_BYTES,
                        Phase::Structure,
                        "logic.parameters",
                        "parameter metadata exceeds its authoring budget",
                    )?;
                    error::ensure(
                        param
                            .default
                            .iter()
                            .chain([&param.min, &param.max, &param.step])
                            .all(|v| v.is_finite())
                            && param
                                .relative_default
                                .is_none_or(|values| values.iter().all(|v| v.is_finite())),
                        Phase::Structure,
                        "logic.parameters",
                        "parameter contains a non-finite value",
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn validate_bindings(&self) -> Result<()> {
        self.validate_draft()?;
        self.interface
            .validate_bindings(&self.parameters()?)
            .map_err(|cause| Error::effect(Phase::Bindings, cause))
    }

    pub fn compile_options(&self) -> CompileOptions {
        CompileOptions {
            plugin_id: self.package.id.clone(),
            version: self.package.version.clone(),
            author: self.package.author.clone(),
            license: self.package.license.clone(),
            ..Default::default()
        }
    }
}

fn parameter_layout(parameters: &[ParamDefinition]) -> NativeLayout {
    NativeLayout {
        format_version: LAYOUT_VERSION,
        root: LayoutNode {
            id: "root".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Column {
                gap: LayoutSpacing::None,
                align: LayoutAlign::Stretch,
                children: parameters
                    .iter()
                    .enumerate()
                    .map(|(index, param)| LayoutNode {
                        // Assigned once on creation, persisted thereafter. Reordering
                        // never regenerates IDs or uses indices as binding targets.
                        id: format!("field-{index}"),
                        width: LayoutExtent::Auto,
                        height: LayoutExtent::Auto,
                        content: LayoutContent::Parameter {
                            parameter: param.id.clone(),
                            control: ParameterControl::Auto,
                        },
                    })
                    .collect(),
            },
        },
    }
}
