use crate::{ensure, shader, validate_path, Error, PluginManifest, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

static WRITE_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

struct TemporaryWrite(PathBuf);
impl Drop for TemporaryWrite {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("effect temporary file cleanup failed: path={} reason={error}", self.0.display());
            }
        }
    }
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    let directory = destination.parent().ok_or_else(|| Error::Invalid("missing destination directory".into()))?;
    // Own only files exclusively created by this write, including on unwind.
    let mut attempts = 0;
    let (temporary, mut file) = loop {
        if attempts >= 128 {
            return Err(Error::Invalid("effect temporary file collision limit exceeded".into()));
        }
        attempts += 1;
        let nonce = WRITE_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = directory.join(format!(".motion-effects-{}-{nonce}.tmp", std::process::id()));
        match File::options().write(true).create_new(true).open(&path) {
            Ok(file) => break (TemporaryWrite(path), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary.0, destination)?;
    Ok(())
}

const MAX_PACKAGE: usize = 16 * 1024 * 1024;
const MAX_MANIFEST: usize = 512 * 1024;
const MAX_SHADER: usize = 256 * 1024;
const MAX_DISABLED: usize = 512 * 1024;
fn resource_limit(name: &str) -> Result<usize> {
    if name == "manifest.json" { Ok(MAX_MANIFEST) }
    else if name.starts_with("shaders/") && name.ends_with(".wgsl") { Ok(MAX_SHADER) }
    else if name.starts_with("assets/") && name.ends_with(".png") { Ok(MAX_PACKAGE) }
    else if name.starts_with("ui/") && crate::editor_mime(name).is_some() { Ok(MAX_SHADER) }
    else { Err(Error::Invalid(format!("unexpected package file: {name}"))) }
}

/// Validate declared size before any payload allocation; detect growth/shrinkage
/// with at most one byte beyond that size, regardless of the actual stream size.
fn read_bounded(reader: impl Read, expected: u64, limit: usize, name: &str) -> Result<Vec<u8>> {
    ensure(expected <= limit as u64, &format!("{name}: effect resource exceeds size limit ({limit} bytes)"))?;
    let mut data = Vec::new();
    reader.take(expected + 1).read_to_end(&mut data)?;
    ensure(data.len() as u64 == expected, &format!("{name}: effect resource length mismatch"))?;
    Ok(data)
}
fn read_file_bounded(path: &Path, limit: usize, name: &str) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    ensure(metadata.is_file(), &format!("{name}: package resource is not a regular file"))?;
    read_bounded(file, metadata.len(), limit, name)
}
fn source_file(base: &Path, name: &str) -> Result<PathBuf> {
    let file = base.join(name).canonicalize()?;
    ensure(file.starts_with(base), &format!("{name}: package file resolves outside source directory"))?;
    Ok(file)
}

pub struct EffectPackage {
    pub manifest: PluginManifest,
    pub hash: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub shaders: BTreeMap<(String, usize), shader::CompiledShader>,
    pub bytes: Vec<u8>,
}
impl EffectPackage {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        ensure(bytes.len() <= MAX_PACKAGE, "effect package exceeds 16 MiB")?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let mut archive = ZipArchive::new(std::io::Cursor::new(&bytes))?;
        ensure(archive.len() <= 256, "too many effect package entries")?;
        let mut files = BTreeMap::new();
        let mut names = BTreeSet::new();
        let mut total = 0usize;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_owned();
            validate_path(&name)?;
            ensure(
                !entry.is_dir() && entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
                "directories and symlinks are not effect resources",
            )?;
            ensure(names.insert(name.to_lowercase()), "duplicate package entry")?;
            let limit = resource_limit(&name)?.min(MAX_PACKAGE - total);
            let expected = entry.size();
            let data = read_bounded(&mut entry, expected, limit, &name)?;
            total = total
                .checked_add(data.len())
                .ok_or_else(|| Error::Invalid("package size overflow".into()))?;
            ensure(
                total <= MAX_PACKAGE,
                "expanded effect package exceeds 16 MiB",
            )?;
            files.insert(name, data);
        }
        let manifest: PluginManifest = serde_json::from_slice(
            files
                .get("manifest.json")
                .ok_or_else(|| Error::Invalid("manifest.json is missing".into()))?,
        )?;
        manifest.validate()?;
        let mut shaders = BTreeMap::new();
        let mut referenced = BTreeSet::from(["manifest.json".to_owned()]);
        let mut decoded_resources = BTreeSet::new();
        let mut decoded_bytes = 0u64;
        for effect in &manifest.effects {
            if let Some(editor) = &effect.editor {
                for path in &editor.files {
                    let data = files
                        .get(path)
                        .ok_or_else(|| Error::Invalid(format!("editor resource {path} missing")))?;
                    if !path.ends_with(".png") {
                        ensure(
                            std::str::from_utf8(data).is_ok() && !data.contains(&0),
                            "editor text must be UTF-8 without NUL",
                        )?;
                    } else if decoded_resources.insert(path.clone()) {
                        let (w, h) = image::ImageReader::new(std::io::Cursor::new(data))
                            .with_guessed_format()
                            .map_err(Error::Io)?
                            .into_dimensions()
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        decoded_bytes = decoded_bytes
                            .checked_add(u64::from(w) * u64::from(h) * 4)
                            .ok_or_else(|| Error::Invalid("editor PNG size overflow".into()))?;
                        ensure(
                            w > 0 && h > 0 && decoded_bytes <= 128 * 1024 * 1024,
                            "editor PNG exceeds decoded resource budget",
                        )?;
                        image::load_from_memory_with_format(data, image::ImageFormat::Png)
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                    }
                    referenced.insert(path.clone());
                }
            }
            for (i, pass) in effect.passes.iter().enumerate() {
                referenced.insert(pass.shader.clone());
                let source =
                    std::str::from_utf8(files.get(&pass.shader).ok_or_else(|| {
                        Error::Invalid(format!("shader {} is missing", pass.shader))
                    })?)
                    .map_err(|_| Error::Invalid("shader must be UTF-8".into()))?;
                let compiled = if effect.output_bounds.is_some() {
                    if manifest.sdk_version >= 6 { shader::compile_spatial_v6(source, &pass.entry) }
                    else if manifest.sdk_version >= 4 { shader::compile_spatial_image(source, &pass.entry) }
                    else { shader::compile_rect_image(source, &pass.entry) }
                } else {
                    shader::compile_mode(
                        source,
                        &pass.entry,
                        effect.renderer != crate::RendererKind::Image,
                        effect.blend == crate::SpriteBlend::Additive,
                    )
                }
                .map_err(|e| Error::Invalid(format!("effect {}, pass {i}: {e}", effect.id)))?;
                shaders.insert((effect.id.clone(), i), compiled);
            }
            for name in &effect.resources {
                referenced.insert(name.clone());
                let data = files
                    .get(name)
                    .ok_or_else(|| Error::Invalid(format!("resource {name} is missing")))?;
                let reader = image::ImageReader::new(std::io::Cursor::new(data))
                    .with_guessed_format()
                    .map_err(Error::Io)?;
                let (w, h) = reader
                    .into_dimensions()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                ensure(
                    w > 0 && h > 0 && u64::from(w) * u64::from(h) * 4 <= 128 * 1024 * 1024,
                    "effect PNG exceeds decoded texture budget",
                )?;
                if decoded_resources.insert(name.clone()) {
                    decoded_bytes += u64::from(w) * u64::from(h) * 4;
                    ensure(
                        decoded_bytes <= 128 * 1024 * 1024,
                        "package PNG resources exceed 128 MiB",
                    )?;
                    image::load_from_memory_with_format(data, image::ImageFormat::Png)
                        .map_err(|e| Error::Invalid(format!("resource {name}: {e}")))?;
                }
            }
        }
        ensure(
            referenced == files.keys().cloned().collect(),
            "effect package contains unreferenced files",
        )?;
        Ok(Self {
            manifest,
            hash,
            files,
            shaders,
            bytes,
        })
    }
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = read_file_bounded(path, MAX_PACKAGE, &path.to_string_lossy())?;
        Self::from_bytes(bytes)
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFilePlan {
    pub name: String,
    pub byte_limit: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePackagePlan {
    pub protocol: u32,
    pub plugin: String,
    pub version: String,
    pub manifest_sha256: String,
    pub manifest_byte_limit: usize,
    pub package_byte_limit: usize,
    pub files: Vec<SourceFilePlan>,
}
fn declared_files(manifest: &PluginManifest) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::from(["manifest.json".to_owned()]);
    for effect in &manifest.effects {
        if let Some(editor) = &effect.editor { names.extend(editor.files.iter().cloned()); }
        names.extend(effect.passes.iter().map(|pass| pass.shader.clone()));
        names.extend(effect.resources.iter().cloned());
    }
    ensure(names.len() <= 256, "too many effect package entries")?;
    let lowered: BTreeSet<_> = names.iter().map(|name| name.to_lowercase()).collect();
    ensure(lowered.len() == names.len(), "duplicate case-insensitive package entry")?;
    Ok(names)
}
/// Use the loader's schema, reference set and budgets for external packers.
/// Full PNG/WGSL validation still runs on the resulting archive before publication.
pub fn package_source_plan(path: &Path) -> Result<SourcePackagePlan> {
    let data = read_file_bounded(path, MAX_MANIFEST, "manifest.json")?;
    let manifest: PluginManifest = serde_json::from_slice(&data)?;
    manifest.validate()?;
    let files = declared_files(&manifest)?.into_iter().map(|name| {
        Ok(SourceFilePlan { byte_limit: resource_limit(&name)?, name })
    }).collect::<Result<Vec<_>>>()?;
    Ok(SourcePackagePlan {
        protocol: 1, plugin: manifest.id, version: manifest.version,
        manifest_sha256: format!("{:x}", Sha256::digest(&data)),
        manifest_byte_limit: MAX_MANIFEST, package_byte_limit: MAX_PACKAGE, files,
    })
}

pub fn package_directory(path: &Path) -> Result<Vec<u8>> {
    let base = path.canonicalize()?;
    let mut manifest_data = read_file_bounded(&source_file(&base, "manifest.json")?, MAX_MANIFEST, "manifest.json")?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_data)?;
    manifest.validate()?;
    let names = declared_files(&manifest)?;
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let mut size = manifest_data.len();
    for name in names {
        let data = if name == "manifest.json" { std::mem::take(&mut manifest_data) }
            else {
                let limit = resource_limit(&name)?.min(MAX_PACKAGE - size);
                let data = read_file_bounded(&source_file(&base, &name)?, limit, &name)?;
                size = size.checked_add(data.len()).ok_or_else(|| Error::Invalid("package size overflow".into()))?;
                data
            };
        writer.start_file(name, options)?;
        writer.write_all(&data)?;
    }
    let bytes = writer.finish()?.into_inner();
    EffectPackage::from_bytes(bytes.clone())?;
    Ok(bytes)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub version: String,
    pub hash: String,
    pub enabled: bool,
}
#[derive(Default, Clone)]
pub struct Registry {
    pub packages: BTreeMap<(String, String, String), Arc<EffectPackage>>,
    pub disabled: BTreeSet<(String, String, String)>,
    pub diagnostics: Vec<String>,
}
impl Registry {
    pub fn new_with_builtins() -> Result<Self> {
        let mut r = Self::default();
        for package in crate::builtin::packages()? {
            r.insert(package)?;
        }
        Ok(r)
    }
    pub fn insert(&mut self, package: Arc<EffectPackage>) -> Result<()> {
        for (id, version, hash) in self.packages.keys() {
            ensure(
                id != &package.manifest.id
                    || version != &package.manifest.version
                    || hash == &package.hash,
                "same plugin version has different package hash; increment the version",
            )?;
        }
        self.packages.insert(
            (
                package.manifest.id.clone(),
                package.manifest.version.clone(),
                package.hash.clone(),
            ),
            package,
        );
        Ok(())
    }
    pub fn load(directory: &Path) -> Result<Self> {
        let mut r = Self::new_with_builtins()?;
        // Persist each bundled version before an App upgrade replaces its bytes.
        let builtins = crate::builtin::packages()?;
        fs::create_dir_all(directory)?;
        for builtin in &builtins {
            let bundled = directory.join(format!("{}.msfx", builtin.hash));
            if !bundled.exists() {
                atomic_write(&bundled, &builtin.bytes)?;
            }
        }
        if directory.exists() {
            for e in fs::read_dir(directory)? {
                let e = e?;
                if e.file_type()?.is_file() && e.path().extension().is_some_and(|v| v == "msfx") {
                    let cached = builtins
                        .iter()
                        .find(|p| e.file_name().to_string_lossy() == format!("{}.msfx", p.hash));
                    let package = if let Some(builtin) = cached {
                        // Reuse the already validated embedded package on repeated session creation.
                        fs::read(e.path())
                            .and_then(|bytes| {
                                if bytes == builtin.bytes {
                                    Ok(builtin.clone())
                                } else {
                                    Err(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        "bundled package cache is corrupt",
                                    ))
                                }
                            })
                            .map_err(Error::Io)
                    } else {
                        EffectPackage::open(&e.path()).map(Arc::new)
                    };
                    match package.and_then(|p| {
                        // Intentional retirement, not a failed load. Keep the
                        // user's cached bytes untouched without rediscovering
                        // removed product effects on every application start.
                        if crate::builtin::is_retired_plugin(&p.manifest.id) {
                            return Ok(());
                        }
                        r.insert(p)
                    }) {
                        Ok(()) => {}
                        Err(e) => r.diagnostics.push(e.to_string()),
                    }
                }
            }
        }
        match read_file_bounded(&directory.join("disabled.json"), MAX_DISABLED, "disabled.json")
            .and_then(|data| serde_json::from_slice(&data).map_err(Error::from))
        {
            Ok(disabled) => r.disabled = disabled,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => r.diagnostics.push(format!(
                "{}: disabled plugin settings could not be loaded ({error}); disabled settings were not applied. The file was preserved; repair it or remove it to reset these settings.",
                directory.join("disabled.json").display()
            )),
        }
        Ok(r)
    }
    pub fn resolve(&self, id: &str, version: &str, hash: &str) -> Result<Arc<EffectPackage>> {
        let key = (id.to_owned(), version.to_owned(), hash.to_owned());
        ensure(
            !self.disabled.contains(&key),
            format!("plugin {id} {version} is disabled"),
        )?;
        self.packages
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                if crate::builtin::is_retired_plugin(id) {
                    Error::Invalid(format!("retired bundled plugin {id} {version} ({hash}) is no longer available; replace this effect with a user-authored package"))
                } else {
                    Error::Invalid(format!("missing plugin {id} {version} ({hash})"))
                }
            })
    }
    pub fn install(&mut self, directory: &Path, path: &Path) -> Result<InstalledPlugin> {
        let package = Arc::new(EffectPackage::open(path)?);
        ensure(
            !crate::builtin::is_retired_plugin(&package.manifest.id),
            format!("retired bundled plugin {} is no longer installable; use an independently authored package ID", package.manifest.id),
        )?;
        let mut next = self.clone();
        // Another session may have installed a version since this registry was opened.
        for installed in Self::load(directory)?.packages.into_values() {
            next.insert(installed)?;
        }
        next.insert(package.clone())?;
        fs::create_dir_all(directory)?;
        let destination = directory.join(format!("{}.msfx", package.hash));
        if !destination.exists() {
            atomic_write(&destination, &package.bytes)?;
        }
        let info = InstalledPlugin {
            id: package.manifest.id.clone(),
            version: package.manifest.version.clone(),
            hash: package.hash.clone(),
            enabled: !next.disabled.contains(&(
                package.manifest.id.clone(),
                package.manifest.version.clone(),
                package.hash.clone(),
            )),
        };
        *self = next;
        Ok(info)
    }
    pub fn enable(
        &mut self,
        directory: &Path,
        id: &str,
        version: &str,
        hash: &str,
        enabled: bool,
    ) -> Result<()> {
        let key = (id.into(), version.into(), hash.into());
        ensure(self.packages.contains_key(&key), "plugin does not exist")?;
        let mut disabled = self.disabled.clone();
        if enabled {
            disabled.remove(&key);
        } else {
            disabled.insert(key);
        }
        fs::create_dir_all(directory)?;
        atomic_write(&directory.join("disabled.json"), &serde_json::to_vec(&disabled)?)?;
        self.disabled = disabled;
        Ok(())
    }
    pub fn uninstall(
        &mut self,
        directory: &Path,
        id: &str,
        version: &str,
        hash: &str,
    ) -> Result<()> {
        ensure(
            id != crate::builtin::PLUGIN_ID,
            "the preinstalled core library cannot be uninstalled",
        )?;
        let key = (id.into(), version.into(), hash.into());
        ensure(self.packages.contains_key(&key), "plugin does not exist")?;
        let path: PathBuf = directory.join(format!("{hash}.msfx"));
        ensure(
            hash.len() == 64 && hash.bytes().all(|v| v.is_ascii_hexdigit()),
            "invalid package hash",
        )?;
        let was_disabled = self.disabled.contains(&key);
        // Persist metadata before removing the only package payload.
        self.enable(directory, id, version, hash, true)?;
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                if was_disabled {
                    if let Err(rollback) = self.enable(directory, id, version, hash, false) {
                        return Err(Error::Invalid(format!(
                            "uninstall {} failed: {error}; restore disabled state failed: {rollback}",
                            path.display()
                        )));
                    }
                }
                return Err(error.into());
            }
        }
        self.packages.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod bounded_read_tests {
    use super::*;
    struct Unreadable;
    impl Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> { panic!("oversized input was read"); }
    }
    #[test]
    fn gigabyte_metadata_is_rejected_without_a_single_payload_read() {
        assert!(read_bounded(Unreadable, 3 << 30, MAX_PACKAGE, "assets/huge.png").unwrap_err()
            .to_string().contains("assets/huge.png"));
    }
    #[test]
    fn changed_streams_stop_after_one_byte_and_short_reads_are_rejected() {
        let mut source = std::io::Cursor::new([7u8; 64]);
        assert!(read_bounded(&mut source, 4, MAX_PACKAGE, "shaders/changed.wgsl").is_err());
        assert_eq!(source.position(), 5);
        assert!(read_bounded(std::io::Cursor::new([7u8; 3]), 4, MAX_PACKAGE, "short").is_err());
        assert_eq!(read_bounded(std::io::Cursor::new([7u8; 4]), 4, 4, "exact").unwrap(), vec![7;4]);
    }
}

#[cfg(test)]
mod publication_failure_tests {
    use super::*;

    #[test]
    fn failed_atomic_publication_cleans_only_the_owned_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("disabled.json");
        fs::create_dir(&destination).unwrap();
        let foreign = directory.path().join("disabled.json.tmp");
        fs::write(&foreign, b"other transaction").unwrap();
        assert!(atomic_write(&destination, b"[]").is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read(&foreign).unwrap(), b"other transaction");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
