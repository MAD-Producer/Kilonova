//! Durable, verified inputs for queued movies. Call on a task worker, never in
//! panel painting. A prepared directory stays owned until the queue is saved.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;

const VERSION: u32 = 1;
const MANIFEST: &str = "snapshot.json";
const MAX_MANIFEST: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 8192;
const MAX_PACKAGE_FILE: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotInfo {
    pub composition: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u32,
    pub range: motion_core::WorkArea,
    pub revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    project: Project,
    range: motion_core::WorkArea,
    memory_policy: MemoryPolicy,
    revision: u64,
    protected_sources: BTreeSet<PathBuf>,
    files: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}

/// Dropping an unpublished snapshot cleans only its exclusively created tree.
/// Keep it alive until the referencing queue document has been published.
pub struct PreparedMovie {
    directory: Option<tempfile::TempDir>,
    path: PathBuf,
    info: SnapshotInfo,
}
impl PreparedMovie {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn info(&self) -> &SnapshotInfo {
        &self.info
    }
    pub fn publish(mut self) -> Result<PathBuf> {
        let directory = self
            .directory
            .take()
            .ok_or("prepared movie snapshot lost directory ownership")?;
        Ok(directory.keep())
    }
}
impl Drop for PreparedMovie {
    fn drop(&mut self) {
        if let Some(directory) = self.directory.take() {
            if let Err(error) = directory.close() {
                log::error!(
                    "queued movie snapshot cleanup failed path={} cause={error}",
                    self.path.display()
                );
            }
        }
    }
}

impl FrozenMovie {
    pub fn snapshot_info(&self) -> SnapshotInfo {
        SnapshotInfo {
            composition: self.project.composition_id.clone(),
            name: self.project.name.clone(),
            width: self.project.width,
            height: self.project.height,
            fps: self.project.fps,
            frames: self.project.frames,
            range: self.range,
            revision: self.revision,
        }
    }
    /// Copy already frozen sources, PCM/PTS caches and exact effect packages.
    /// Queue publication happens separately; a failed queue save drops this tree.
    pub fn prepare_persistent(
        &self,
        parent: &Path,
        control: &ExportControl,
    ) -> Result<PreparedMovie> {
        observe(
            "persist",
            self.observation_request,
            self.revision,
            parent,
            || {
                control.check()?;
                fs::create_dir_all(parent).map_err(|e| format!("create snapshot parent: {e}"))?;
                let directory = tempfile::Builder::new()
                    .prefix("movie-")
                    .tempdir_in(parent)
                    .map_err(|e| format!("create private snapshot: {e}"))?;
                let prepared = PreparedMovie {
                    path: directory.path().to_owned(),
                    directory: Some(directory),
                    info: self.snapshot_info(),
                };
                let mut files = Vec::new();
                let mut paths = Vec::new();
                collect(self.assets.path(), self.assets.path(), &mut paths)?;
                let mut total = 0u64;
                for relative in paths {
                    control.check()?;
                    let source = self.assets.path().join(&relative);
                    let destination = prepared.path.join(&relative);
                    fs::create_dir_all(destination.parent().ok_or("invalid snapshot file")?)
                        .map_err(|e| e.to_string())?;
                    let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
                    let mut output = fs::File::options()
                        .write(true)
                        .create_new(true)
                        .open(destination)
                        .map_err(|e| e.to_string())?;
                    let (bytes, sha256) = transfer(&mut input, Some(&mut output), control)?;
                    total = add_bytes(total, bytes)?;
                    output.sync_all().map_err(|e| e.to_string())?;
                    files.push(Entry {
                        path: relative
                            .to_str()
                            .ok_or("snapshot path is not UTF-8")?
                            .replace('\\', "/"),
                        bytes,
                        sha256,
                    });
                }
                let packages = prepared.path.join("packages");
                fs::create_dir(&packages).map_err(|e| e.to_string())?;
                for dependency in &self.project.plugin_dependencies {
                    control.check()?;
                    let package = self
                        .registry
                        .resolve(&dependency.plugin, &dependency.version, &dependency.hash)
                        .map_err(|e| e.to_string())?;
                    let relative = format!("packages/{}.msfx", package.hash);
                    let bytes = package.bytes.len() as u64;
                    total = add_bytes(total, bytes)?;
                    let mut output = fs::File::options()
                        .write(true)
                        .create_new(true)
                        .open(prepared.path.join(&relative))
                        .map_err(|e| e.to_string())?;
                    output
                        .write_all(&package.bytes)
                        .map_err(|e| e.to_string())?;
                    output.sync_all().map_err(|e| e.to_string())?;
                    files.push(Entry {
                        path: relative,
                        bytes,
                        sha256: format!("{:x}", Sha256::digest(&package.bytes)),
                    });
                }
                if files.len() > MAX_FILES {
                    return Err("queued movie snapshot file count exceeds limit".into());
                }
                let manifest = Manifest {
                    version: VERSION,
                    project: self.project.clone(),
                    range: self.range,
                    memory_policy: self.memory_policy,
                    revision: self.revision,
                    protected_sources: self.source_paths.clone(),
                    files,
                };
                let data = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
                if data.len() as u64 > MAX_MANIFEST {
                    return Err("queued movie snapshot manifest exceeds limit".into());
                }
                control.check()?;
                let mut file = fs::File::options()
                    .write(true)
                    .create_new(true)
                    .open(prepared.path.join(MANIFEST))
                    .map_err(|e| e.to_string())?;
                file.write_all(&data).map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                control.check()?;
                Ok(prepared)
            },
        )
    }

    /// Reconstruct only from this task's saved model, caches and package bytes.
    /// Verify the *copied* inputs before handing a new snapshot to the exporter.
    pub fn restore_persistent(path: &Path, control: &ExportControl) -> Result<Self> {
        Self::restore_persistent_with_request(path, control, 0)
    }

    pub fn restore_persistent_with_request(
        path: &Path,
        control: &ExportControl,
        request: u64,
    ) -> Result<Self> {
        observe("restore", request, 0, path, || {
            control.check()?;
            let root = path.canonicalize().map_err(|e| e.to_string())?;
            let data = read_bounded(&safe_file(&root, MANIFEST)?, MAX_MANIFEST)?;
            let manifest: Manifest =
                serde_json::from_slice(&data).map_err(|e| format!("snapshot manifest: {e}"))?;
            if manifest.version != VERSION {
                return Err("unsupported queued movie snapshot version".into());
            }
            manifest.project.validate().map_err(|e| e.to_string())?;
            manifest
                .range
                .validate(manifest.project.frames)
                .map_err(|e| e.to_string())?;
            manifest
                .memory_policy
                .validate()
                .map_err(|e| e.to_string())?;
            if manifest.files.len() > MAX_FILES {
                return Err("queued movie snapshot file count exceeds limit".into());
            }
            let mut names = BTreeSet::new();
            let mut total = 0;
            for entry in &manifest.files {
                relative(&entry.path)?;
                if !names.insert(entry.path.to_lowercase())
                    || entry.sha256.len() != 64
                    || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("invalid or duplicate queued movie snapshot entry".into());
                }
                total = add_bytes(total, entry.bytes)?;
            }
            let mut registry = Registry::default();
            let mut package_names = BTreeSet::new();
            for dependency in &manifest.project.plugin_dependencies {
                control.check()?;
                let name = format!("packages/{}.msfx", dependency.hash);
                let entry = manifest
                    .files
                    .iter()
                    .find(|e| e.path == name)
                    .ok_or("snapshot is missing a required effect package")?;
                let bytes = read_bounded(&safe_file(&root, &name)?, MAX_PACKAGE_FILE)?;
                if bytes.len() as u64 != entry.bytes
                    || format!("{:x}", Sha256::digest(&bytes)) != entry.sha256
                {
                    return Err(format!("queued movie snapshot integrity failure: {name}"));
                }
                let package = Arc::new(
                    motion_effects::EffectPackage::from_bytes(bytes).map_err(|e| e.to_string())?,
                );
                if package.manifest.id != dependency.plugin
                    || package.manifest.version != dependency.version
                    || package.hash != dependency.hash
                {
                    return Err("queued movie snapshot effect identity mismatch".into());
                }
                registry.insert(package).map_err(|e| e.to_string())?;
                package_names.insert(name);
            }
            let plan = MovieFreezePlan::new(
                manifest.project,
                &root,
                registry,
                manifest.memory_policy,
                manifest.revision,
                control,
            )?;
            let mut frozen = plan.with_observation_request(request).freeze(control)?;
            let mut copied = Vec::new();
            collect(frozen.assets.path(), frozen.assets.path(), &mut copied)?;
            let copied_names: BTreeSet<_> = copied
                .iter()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .collect();
            if copied_names.len() + package_names.len() != manifest.files.len() {
                return Err("queued movie snapshot file set mismatch".into());
            }
            for entry in &manifest.files {
                if package_names.contains(&entry.path) {
                    continue;
                }
                if !copied_names.contains(&entry.path) {
                    return Err("queued movie snapshot file set mismatch".into());
                }
                let mut file = fs::File::open(safe_file(frozen.assets.path(), &entry.path)?)
                    .map_err(|e| e.to_string())?;
                let (bytes, hash) = transfer(&mut file, None, control)?;
                if bytes != entry.bytes || hash != entry.sha256 {
                    return Err(format!(
                        "queued movie snapshot integrity failure: {}",
                        entry.path
                    ));
                }
            }
            frozen.range = manifest.range;
            frozen.source_paths.extend(manifest.protected_sources);
            Ok(frozen)
        })
    }
}

fn observe<T>(
    phase: &str,
    request: u64,
    revision: u64,
    path: &Path,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let began = Instant::now();
    // Bounded path context, never project text or serialized payloads.
    let path: String = path.to_string_lossy().chars().take(1024).collect();
    log::info!("queued movie snapshot start request={request} phase={phase} revision={revision} path={path}");
    log::logger().flush();
    let result = action();
    let cause = result
        .as_ref()
        .err()
        .map(|error| error.chars().take(2048).collect::<String>());
    match &result {
        Err(error) if error == "movie export cancelled" => log::info!("queued movie snapshot cancelled request={request} phase={phase} revision={revision} elapsed_ms={}", began.elapsed().as_millis()),
        Err(error) => log::error!("queued movie snapshot failed request={request} phase={phase} revision={revision} path={path} elapsed_ms={} cause={} truncated={}", began.elapsed().as_millis(), cause.as_deref().unwrap_or(""), cause.as_ref().is_some_and(|cause| cause.len() < error.len())),
        Ok(_) => log::debug!("queued movie snapshot done request={request} phase={phase} revision={revision} elapsed_ms={}", began.elapsed().as_millis()),
    }
    result
}
fn relative(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\\')
        || !Path::new(path)
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_)))
    {
        return Err("invalid queued movie snapshot relative path".into());
    }
    Ok(())
}
fn safe_file(root: &Path, path: &str) -> Result<PathBuf> {
    relative(path)?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let file = root
        .join(path)
        .canonicalize()
        .map_err(|e| format!("snapshot file {path}: {e}"))?;
    if !file.starts_with(&root) || !file.is_file() {
        return Err(format!("snapshot file escapes its directory: {path}"));
    }
    Ok(file)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("queued movie snapshot file exceeds read limit".into());
    }
    Ok(bytes)
}
fn add_bytes(total: u64, bytes: u64) -> Result<u64> {
    total
        .checked_add(bytes)
        .filter(|v| *v <= motion_core::storage::MAX_PACKAGE)
        .ok_or_else(|| "queued movie snapshot exceeds resource budget".into())
}
fn transfer(
    input: &mut fs::File,
    mut output: Option<&mut fs::File>,
    control: &ExportControl,
) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut buffer = vec![0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    let mut bytes = 0;
    loop {
        control.check()?;
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        bytes = add_bytes(bytes, count as u64)?;
        hash.update(&buffer[..count]);
        if let Some(output) = &mut output {
            output
                .write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}
fn collect(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect(root, &entry.path(), files)?;
        } else if kind.is_file() {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_owned(),
            );
        } else {
            return Err("queued movie snapshot contains a non-regular file".into());
        }
        if files.len() > MAX_FILES {
            return Err("queued movie snapshot file count exceeds limit".into());
        }
    }
    files.sort();
    Ok(())
}

#[cfg(test)]
mod tests;
