use super::*;
use std::{fs::File, io::Read, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectWire {
    format_version: u32,
    package: PackageIdentity,
    logic: LogicWire,
    interface: NativeLayout,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum LogicWire {
    Graph { document: serde_json::Value },
    Wgsl { document: WgslDocument },
}
impl PluginProject {
    pub fn from_json(text: &str) -> Result<Self> {
        error::ensure(
            text.len() <= MAX_PROJECT_BYTES,
            Phase::Read,
            "document",
            "plugin source exceeds the authoring file byte budget",
        )?;
        let wire: ProjectWire =
            serde_json::from_str(text).map_err(|e| Error::json(Phase::Read, e))?;
        error::ensure(
            wire.format_version == PROJECT_VERSION,
            Phase::Structure,
            "format_version",
            "unsupported plugin project version",
        )?;
        let logic = match wire.logic {
            LogicWire::Graph { document } => PluginLogic::Graph {
                document: GraphDocument::from_json(
                    &serde_json::to_string(&document).map_err(|e| Error::json(Phase::Read, e))?,
                )
                .map_err(|cause| Error::graph(Phase::Read, cause))?,
            },
            LogicWire::Wgsl { document } => PluginLogic::Wgsl { document },
        };
        let project = Self {
            format_version: wire.format_version,
            package: wire.package,
            logic,
            interface: wire.interface,
        };
        project.validate_draft()?;
        Ok(project)
    }
    pub fn to_json(&self) -> Result<String> {
        self.validate_draft()?;
        let text =
            serde_json::to_string_pretty(self).map_err(|e| Error::json(Phase::Write, e))? + "\n";
        error::ensure(
            text.len() <= MAX_PROJECT_BYTES,
            Phase::Write,
            "document",
            "plugin source exceeds the authoring file byte budget",
        )?;
        Ok(text)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let io = |cause| Error::Io {
            phase: Phase::Read,
            path: path.into(),
            cause,
        };
        let file = File::open(path).map_err(&io)?;
        error::ensure(
            file.metadata().map_err(&io)?.len() <= MAX_PROJECT_BYTES as u64,
            Phase::Read,
            "document",
            "plugin source exceeds the authoring file byte budget",
        )?;
        let mut text = String::new();
        file.take(MAX_PROJECT_BYTES as u64 + 1)
            .read_to_string(&mut text)
            .map_err(io)?;
        Self::from_json(&text)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        error::ensure(
            path.extension().is_some_and(|ext| ext == "msplugin"),
            Phase::Write,
            "path",
            "plugin authoring destination must end in .msplugin",
        )?;
        // Serialize/validate before creating directories or staging any bytes.
        let text = self.to_json()?;
        crate::export::save_document_bytes(path, text.as_bytes())
            .map_err(|cause| Error::graph(Phase::Write, cause))
    }
}
