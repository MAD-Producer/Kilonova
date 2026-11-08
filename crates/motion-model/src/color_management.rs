//! Saved, explicit color interpretation. Catalogue validation is not evidence
//! that the declared files exist or that a rendering backend is prepared.
use crate::Project;
use motion_effects::{
    color_config::{
        ColorConfigError, ColorConfigManifest, ColorSelection, ContentIdentity, OutputSelection,
        ReferenceSpace, ResolvedColorOutput, SpaceReference,
    },
    image_contract::ImageMeaning,
    AlphaMode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_SOURCE_COLOR_ASSIGNMENTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SignalRange {
    /// No legal-range rescaling. Negative and over-one floating values survive.
    Full {},
    /// Black/white code values after division by the source integer maximum.
    Limited { black: f64, white: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorSignal {
    /// The OCIO encoding category, not a substitute for its transfer function.
    pub encoding: String,
    /// CIE xy red, green and blue. Imaginary wide-gamut primaries are allowed.
    pub primaries: [[f64; 2]; 3],
    pub white_point: [f64; 2],
    pub range: SignalRange,
    pub reference: ReferenceSpace,
    pub alpha: AlphaMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorInput {
    pub space: SpaceReference,
    pub signal: ColorSignal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceInterpretation {
    Color {
        input: ColorInput,
    },
    /// Numeric channels bypass all color transforms, including display views.
    Data {
        meaning: ImageMeaning,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceColorAssignment {
    pub asset: u64,
    pub interpretation: SourceInterpretation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcioProject {
    pub manifest: ColorConfigManifest,
    pub selection: ColorSelection,
    /// Interprets sources without an explicit override; never inferred from names.
    pub default_input: ColorSignal,
    /// Solid colors, vector paints and other authored color values.
    pub authored_input: ColorInput,
    pub working: ColorSignal,
    pub display: ColorSignal,
    pub output: ColorSignal,
    pub sources: Vec<SourceColorAssignment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectColorManagement {
    /// Explicitly preserves the existing linear/sRGB workflow.
    Unmanaged {},
    Ocio {
        settings: Box<OcioProject>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorBundleResource {
    pub path: String,
    pub identity: ContentIdentity,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectColorError {
    #[error("project color phase=catalogue: {0}")]
    Config(#[from] ColorConfigError),
    #[error("project color phase=source_binding asset={asset}: {source}")]
    Source {
        asset: u64,
        #[source]
        source: Box<ProjectColorError>,
    },
    #[error("project color field={field} asset={asset:?}: {reason}")]
    Invalid {
        field: &'static str,
        asset: Option<u64>,
        reason: &'static str,
    },
    #[error(
        "project color phase={phase}: OCIO input/working/display/output runtime is not installed"
    )]
    RuntimeNotPrepared { phase: &'static str },
    #[error("project color phase={phase} path={path}: {source}")]
    Io {
        phase: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("project color phase={phase} path={path}: {reason}")]
    Resource {
        phase: &'static str,
        path: String,
        reason: &'static str,
    },
}
type Result<T> = std::result::Result<T, ProjectColorError>;
fn invalid(field: &'static str, asset: Option<u64>, reason: &'static str) -> ProjectColorError {
    ProjectColorError::Invalid {
        field,
        asset,
        reason,
    }
}

impl ColorSignal {
    pub fn validate(&self) -> Result<()> {
        if self.encoding.is_empty()
            || self.encoding.len() > 256
            || self.encoding.chars().any(char::is_control)
        {
            return Err(invalid(
                "encoding",
                None,
                "requires a bounded encoding category",
            ));
        }
        if !self
            .primaries
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 8.)
        {
            return Err(invalid(
                "primaries",
                None,
                "requires finite CIE xy coordinates within [-8,8]",
            ));
        }
        let [r, g, b] = self.primaries;
        let area = (g[0] - r[0]) * (b[1] - r[1]) - (b[0] - r[0]) * (g[1] - r[1]);
        if area.abs() <= 1e-12 {
            return Err(invalid("primaries", None, "RGB primaries are degenerate"));
        }
        let [x, y] = self.white_point;
        if !x.is_finite() || !y.is_finite() || x <= 0. || y <= 0. || x + y > 1. {
            return Err(invalid(
                "white_point",
                None,
                "requires a finite positive CIE xy white point",
            ));
        }
        if let SignalRange::Limited { black, white } = self.range {
            if !black.is_finite()
                || !white.is_finite()
                || black < 0.
                || black >= white
                || white > 1.
            {
                return Err(invalid("range", None, "requires 0 <= black < white <= 1"));
            }
        }
        Ok(())
    }
}

impl OcioProject {
    fn binding(&self, reference: &SpaceReference, signal: &ColorSignal) -> Result<()> {
        signal.validate()?;
        let name = self.manifest.declared_color_space(reference)?;
        let space = &self.manifest.spaces[name];
        if signal.encoding != space.encoding || signal.reference != space.reference {
            return Err(invalid(
                "signal",
                None,
                "encoding/reference differ from selected catalogue space",
            ));
        }
        Ok(())
    }

    pub fn validate(&self, project: &Project) -> Result<()> {
        let resolved = self.manifest.resolve_declared(&self.selection)?;
        self.binding(&self.selection.input, &self.default_input)?;
        self.binding(&self.selection.working, &self.working)?;
        self.binding(&self.authored_input.space, &self.authored_input.signal)?;
        self.display.validate()?;
        self.output.validate()?;
        if self.display.reference != ReferenceSpace::Display {
            return Err(invalid(
                "display",
                None,
                "a display view requires display-referred samples",
            ));
        }
        match (&self.selection.output, resolved.output) {
            (OutputSelection::Space { space }, ResolvedColorOutput::Space(_)) => {
                self.binding(space, &self.output)?
            }
            (OutputSelection::Display { .. }, _)
                if self.output.reference != ReferenceSpace::Display =>
            {
                return Err(invalid(
                    "output",
                    None,
                    "a display output requires display-referred samples",
                ))
            }
            _ => {}
        }
        if self.sources.len() > MAX_SOURCE_COLOR_ASSIGNMENTS {
            return Err(invalid("sources", None, "too many source interpretations"));
        }
        let assets: HashSet<_> = project
            .assets
            .iter()
            .map(|a| a.id)
            .chain(project.video_assets.iter().map(|a| a.id))
            .collect();
        let mut seen = HashSet::new();
        for source in &self.sources {
            if !assets.contains(&source.asset) || !seen.insert(source.asset) {
                return Err(invalid(
                    "sources",
                    Some(source.asset),
                    "requires a unique existing image/video asset",
                ));
            }
            match &source.interpretation {
                SourceInterpretation::Color { input } => self
                    .binding(&input.space, &input.signal)
                    .map_err(|cause| ProjectColorError::Source {
                        asset: source.asset,
                        source: Box::new(cause),
                    })?,
                SourceInterpretation::Data {
                    meaning: ImageMeaning::Color { .. },
                } => {
                    return Err(invalid(
                        "sources",
                        Some(source.asset),
                        "data interpretation cannot contain a color meaning",
                    ))
                }
                SourceInterpretation::Data { .. } => {}
            }
        }
        self.resources()?;
        Ok(())
    }

    /// Deterministic, project-local paths. No ambient OCIO paths or environment.
    pub fn resources(&self) -> Result<Vec<ColorBundleResource>> {
        let key = self.manifest.declared_key()?;
        let digest: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
        let prefix = format!("assets/color/{digest}/");
        let mut result = vec![ColorBundleResource {
            path: format!("{prefix}config.ocio"),
            identity: self.manifest.config.clone(),
        }];
        result.extend(
            self.manifest
                .resources
                .iter()
                .map(|(path, identity)| ColorBundleResource {
                    path: format!("{prefix}{path}"),
                    identity: identity.clone(),
                }),
        );
        let mut names = HashSet::new();
        for resource in &result {
            if !portable_path(&resource.path) || !names.insert(resource.path.to_lowercase()) {
                return Err(invalid(
                    "resource",
                    None,
                    "requires distinct portable managed file paths",
                ));
            }
        }
        for name in &names {
            for (index, _) in name.match_indices('/') {
                if names.contains(&name[..index]) {
                    return Err(invalid(
                        "resource",
                        None,
                        "a managed file cannot also be a directory",
                    ));
                }
            }
        }
        Ok(result)
    }
}

fn portable_path(value: &str) -> bool {
    value.len() <= 1024
        && value.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "<>:\\|?*\"".contains(c))
                && !matches!(
                    stem.as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}

impl ProjectColorManagement {
    pub fn validate(&self, project: &Project) -> Result<()> {
        match self {
            Self::Unmanaged {} => Ok(()),
            Self::Ocio { settings } => settings.validate(project),
        }
    }
    pub fn resources(&self) -> Result<Vec<ColorBundleResource>> {
        match self {
            Self::Unmanaged {} => Ok(Vec::new()),
            Self::Ocio { settings } => settings.resources(),
        }
    }
    /// Until a complete pipeline is installed, legacy render paths must refuse
    /// a managed document instead of silently treating ACES values as sRGB.
    pub fn require_unmanaged(&self, phase: &'static str) -> Result<()> {
        match self {
            Self::Unmanaged {} => Ok(()),
            Self::Ocio { .. } => Err(ProjectColorError::RuntimeNotPrepared { phase }),
        }
    }
}
