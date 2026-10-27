use crate::{compile, CompileOptions, CompileReport, Error, ErrorCode, GraphDocument, Result};
use motion_effects::{EffectPackage, Registry};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize)]
pub struct ExportReport {
    pub path: PathBuf,
    pub source_path: PathBuf,
    pub package_hash: String,
    pub plugin_id: String,
    pub version: String,
    pub effect_id: String,
    pub bytes: u64,
    pub compile: CompileReport,
}

/// Package already compiled code without touching the editing session or GPU.
pub fn compiled_package_bytes(
    manifest: &motion_effects::PluginManifest,
    source: &str,
) -> Result<Vec<u8>> {
    package_with_resources(manifest, source, &Default::default())
}

impl crate::CompiledGraph {
    pub fn resource(&self, path: &str) -> Option<&[u8]> {
        self.resources.get(path).map(|bytes| bytes.as_ref())
    }
    /// Package code and its declared numeric data together, including when a
    /// bound instance supplies a different package identity.
    pub fn package_bytes(&self, manifest: &motion_effects::PluginManifest) -> Result<Vec<u8>> {
        package_with_resources(manifest, &self.source, &self.resources)
    }
}

fn package_with_resources(manifest: &motion_effects::PluginManifest, source: &str,
    resources: &std::collections::BTreeMap<String, Arc<[u8]>>) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let settings = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    zip.start_file("manifest.json", settings)?;
    zip.write_all(&serde_json::to_vec_pretty(manifest)?)?;
    zip.start_file("shaders/graph.wgsl", settings)?;
    zip.write_all(source.as_bytes())?;
    for (path, bytes) in resources {
        zip.start_file(path, settings)?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

pub fn export_msfx(
    document: &GraphDocument,
    options: &CompileOptions,
    output: &Path,
) -> Result<ExportReport> {
    if output.extension().is_none_or(|ext| ext != "msfx") {
        return Err(Error::invalid(
            ErrorCode::Publication,
            None,
            None,
            "package destination must end in .msfx; the editable sibling uses .msgraph",
        ));
    }
    let compiled = compile(document, options)?;
    let source = document.to_json()?;
    let bytes = compiled.package_bytes(&compiled.manifest)?;
    let package = Arc::new(EffectPackage::from_bytes(bytes.clone()).map_err(|e| {
        Error::effect(
            ErrorCode::Manifest,
            "exported package rejected by host loader: ",
            e,
        )
    })?);
    let mut registry = Registry::default();
    registry
        .insert(package.clone())
        .map_err(|cause| Error::effect(ErrorCode::Manifest, "", cause))?;
    registry
        .resolve(
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
        )
        .map_err(|cause| Error::effect(ErrorCode::Manifest, "", cause))?;
    let output = destination(output)?;
    let source_path = output.with_extension("msgraph");
    // Do not overwrite an already published identity with new shader bytes.
    // The same version/hash is deterministic and can be exported again.
    if output.exists() {
        let previous = Arc::new(EffectPackage::open(&output).map_err(|e| {
            Error::effect(
                ErrorCode::Publication,
                "existing destination is not a valid effect package: ",
                e,
            )
        })?);
        let mut previous_registry = Registry::default();
        previous_registry
            .insert(previous)
            .map_err(|cause| Error::effect(ErrorCode::Manifest, "", cause))?;
        previous_registry.insert(package.clone()).map_err(|e| {
            Error::effect(
                ErrorCode::Publication,
                "increment the package version before replacing its contents: ",
                e,
            )
        })?;
    }
    let mut package_stage = StagedFile::new(&output, &bytes)?;
    let mut source_stage = StagedFile::new(&source_path, source.as_bytes())?;
    let reopened = EffectPackage::open(package_stage.path.as_ref().unwrap()).map_err(|e| {
        Error::effect(
            ErrorCode::Publication,
            "staged .msfx cannot be reopened: ",
            e,
        )
    })?;
    if reopened.hash != package.hash {
        return Err(Error::invalid(
            ErrorCode::Publication,
            None,
            None,
            "staged package hash changed",
        ));
    }
    let mut old_source = if source_path.exists() {
        if fs::metadata(&source_path)?.len() > crate::MAX_DOCUMENT_BYTES as u64 {
            return Err(Error::invalid(
                ErrorCode::Publication,
                None,
                None,
                "existing sibling source exceeds the graph metadata and image budget",
            ));
        }
        Some(StagedFile::new(&source_path, &fs::read(&source_path)?)?)
    } else {
        None
    };
    source_stage.publish(&source_path)?;
    if let Err(error) = package_stage.publish(&output) {
        let rollback = if let Some(previous) = &mut old_source {
            previous.publish(&source_path)
        } else {
            fs::remove_file(&source_path).map_err(Error::from)
        };
        if let Err(rollback_error) = rollback {
            let backup = old_source.as_mut().and_then(|f| f.path.take());
            return Err(Error::invalid(ErrorCode::Publication,None,None,format!("package publication failed ({error}); source rollback failed ({rollback_error}); recovery copy: {backup:?}")));
        }
        return Err(error);
    }
    Ok(ExportReport {
        path: output,
        source_path,
        package_hash: package.hash.clone(),
        plugin_id: package.manifest.id.clone(),
        version: package.manifest.version.clone(),
        effect_id: package.manifest.effects[0].id.clone(),
        bytes: bytes.len() as u64,
        compile: compiled.report,
    })
}

pub(crate) fn save_source(document: &GraphDocument, path: &Path) -> Result<()> {
    if path.extension().is_none_or(|ext| ext != "msgraph") {
        return Err(Error::invalid(
            ErrorCode::Publication,
            None,
            None,
            "node source destination must end in .msgraph",
        ));
    }
    let text = document.to_json()?;
    let path = destination(path)?;
    StagedFile::new(&path, text.as_bytes())?.publish(&path)
}
fn destination(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let file = path.file_name().ok_or_else(|| {
        Error::invalid(
            ErrorCode::Publication,
            None,
            None,
            "invalid export filename",
        )
    })?;
    // Both staged files and publication targets stay in this resolved directory.
    Ok(parent.canonicalize()?.join(file))
}
static NONCE: AtomicU64 = AtomicU64::new(0);
struct StagedFile {
    path: Option<PathBuf>,
}
impl StagedFile {
    fn new(destination: &Path, bytes: &[u8]) -> Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = destination.parent().unwrap().join(format!(
            ".msgraph-{}-{stamp}-{}.tmp",
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let (staged, mut file) = {
            let file = OpenOptions::new().write(true).create_new(true).open(&path)?;
            (Self { path: Some(path) }, file)
        };
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        Ok(staged)
    }
    fn publish(&mut self, destination: &Path) -> Result<()> {
        fs::rename(self.path.as_ref().unwrap(), destination)?;
        self.path = None;
        Ok(())
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("node source temporary cleanup failed: path={} reason={error}", path.display());
                }
            }
        }
    }
}
