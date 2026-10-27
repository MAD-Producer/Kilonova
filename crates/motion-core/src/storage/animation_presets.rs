//! Bounded preset packages, published atomically and staged without overwrites.
use crate::{ensure, AnimationPreset, Error, Result};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
const MANIFEST: &str = "animation-preset.json";
const MAX_IMAGE: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE: u64 = 256 * 1024 * 1024;
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure(
        !cancel.load(Ordering::Acquire),
        "animation preset operation cancelled",
    )
}
fn token() -> Result<String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .map_err(|_| Error::Invalid("preset temporary ID space exhausted".into()))?;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .as_nanos();
    Ok(format!("{}-{time}-{id}", std::process::id()))
}
#[derive(Default)]
struct OwnedPaths {
    paths: Vec<PathBuf>,
}
impl OwnedPaths {
    fn keep(mut self) {
        self.paths.clear();
    }
}
impl Drop for OwnedPaths {
    fn drop(&mut self) {
        for path in &self.paths {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::error!(
                        "animation preset cleanup failed phase=remove path={} reason={}",
                        path.to_string_lossy().chars().take(384).collect::<String>(),
                        error.to_string().chars().take(512).collect::<String>()
                    );
                }
            }
        }
    }
}
pub struct StagedAnimationPreset {
    pub preset: AnimationPreset,
    files: OwnedPaths,
}
impl StagedAnimationPreset {
    /// Call after a revision has actually committed, including a later snapshot
    /// failure. Undo/redo may still reference the newly imported files.
    pub fn keep_resources(self) {
        self.files.keep();
    }
}
fn read_manifest(archive: &mut ZipArchive<File>) -> Result<AnimationPreset> {
    ensure(
        archive.len() <= motion_effects::MAX_EFFECTS_PER_LAYER * 2 + 1,
        "too many preset package entries",
    )?;
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name()?.into_owned();
        ensure(seen.insert(name.clone()), "duplicate preset package entry")?;
        ensure(
            !file.is_dir()
                && file
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 != 0o120000),
            "preset links/directories are not allowed",
        )?;
        if name == MANIFEST {
            ensure(
                file.size() <= crate::animation_presets::MAX_ANIMATION_PRESET_BYTES as u64,
                "preset manifest exceeds 16 MiB",
            )?;
        } else {
            super::validate_relative_path(&name)?;
            ensure(file.size() <= MAX_IMAGE, "preset image exceeds 64 MiB")?;
        }
        total = total
            .checked_add(file.size())
            .ok_or_else(|| Error::Invalid("preset size overflow".into()))?;
        ensure(total <= MAX_PACKAGE, "preset package exceeds 256 MiB")?;
    }
    let mut bytes = Vec::new();
    archive
        .by_name(MANIFEST)?
        .take(crate::animation_presets::MAX_ANIMATION_PRESET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let preset = AnimationPreset::from_json(&bytes)?;
    let expected: BTreeSet<_> = std::iter::once(MANIFEST.to_owned())
        .chain(preset.assets.iter().map(|a| a.path.clone()))
        .collect();
    ensure(
        seen == expected,
        "preset package resources do not match its declaration",
    )?;
    Ok(preset)
}
fn open(input: &Path) -> Result<ZipArchive<File>> {
    let file = File::open(input)?;
    ensure(
        file.metadata()?.len() <= MAX_PACKAGE + 1024 * 1024,
        "preset archive exceeds its size limit",
    )?;
    Ok(ZipArchive::new(file)?)
}
pub fn read_animation_preset(input: &Path) -> Result<AnimationPreset> {
    read_manifest(&mut open(input)?)
}
fn copy(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut buffer = [0; super::TRANSFER_BUFFER_BYTES];
    let mut copied = 0u64;
    loop {
        check(cancel)?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        copied += n as u64;
        ensure(copied <= size, "preset resource grew during transfer")?;
        output.write_all(&buffer[..n])?;
    }
    ensure(copied == size, "preset resource changed during transfer")
}
pub fn save_animation_preset(
    root: &Path,
    preset: &AnimationPreset,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    preset.validate()?;
    check(cancel)?;
    let root = root.canonicalize()?;
    let parent = output
        .parent()
        .ok_or_else(|| Error::Invalid("preset output parent missing".into()))?
        .canonicalize()?;
    let destination = parent.join(
        output
            .file_name()
            .ok_or_else(|| Error::Invalid("preset output name missing".into()))?,
    );
    let temporary = parent.join(format!(".animation-preset-{}.tmp", token()?));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let owned = OwnedPaths {
        paths: vec![temporary.clone()],
    };
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options)?;
    zip.write_all(&serde_json::to_vec(preset)?)?;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for asset in &preset.assets {
        if !names.insert(asset.path.clone()) {
            continue;
        }
        check(cancel)?;
        let path = root.join(&asset.path).canonicalize()?;
        ensure(
            path.starts_with(&root),
            "preset resource resolves outside project",
        )?;
        ensure(
            path != destination,
            "preset output would overwrite its source",
        )?;
        let mut source = File::open(path)?;
        let before = source.metadata()?;
        total = total
            .checked_add(before.len())
            .ok_or_else(|| Error::Invalid("preset size overflow".into()))?;
        ensure(
            before.is_file()
                && before.len() <= MAX_IMAGE
                && total
                    <= MAX_PACKAGE - crate::animation_presets::MAX_ANIMATION_PRESET_BYTES as u64,
            "preset resource budget exceeded",
        )?;
        zip.start_file(&asset.path, options)?;
        copy(&mut source, &mut zip, before.len(), cancel)?;
        let after = source.metadata()?;
        ensure(
            before.len() == after.len() && before.modified()? == after.modified()?,
            "preset source changed during transfer",
        )?;
    }
    let file = zip.finish()?;
    file.sync_all()?;
    drop(file);
    check(cancel)?;
    super::publication::publish_preset(&temporary, &destination)?;
    owned.keep();
    Ok(())
}
pub fn stage_animation_preset(
    input: &Path,
    root: &Path,
    cancel: &AtomicBool,
) -> Result<StagedAnimationPreset> {
    check(cancel)?;
    let mut archive = open(input)?;
    let mut preset = read_manifest(&mut archive)?;
    if preset.assets.is_empty() {
        check(cancel)?;
        return Ok(StagedAnimationPreset {
            preset,
            files: OwnedPaths::default(),
        });
    }
    let root = root.canonicalize()?;
    let assets = root.join("assets");
    if !assets.try_exists()? {
        fs::create_dir(&assets)?;
    }
    let directory = assets.canonicalize()?;
    ensure(
        directory.starts_with(&root) && directory.is_dir(),
        "preset asset directory resolves outside project",
    )?;
    let token = token()?;
    let mut files = OwnedPaths::default();
    let mut paths: BTreeMap<String, String> = BTreeMap::new();
    for asset in &mut preset.assets {
        check(cancel)?;
        if let Some(path) = paths.get(&asset.path) {
            asset.path = path.clone();
            continue;
        }
        let source_path = asset.path.clone();
        let extension = Path::new(&source_path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");
        let name = format!("animation-preset-{token}-{}.{extension}", asset.id);
        let path = directory.join(&name);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        files.paths.push(path);
        let mut source = archive.by_name(&source_path)?;
        let bytes = source.size();
        copy(&mut source, &mut output, bytes, cancel)?;
        output.sync_all()?;
        drop(output);
        asset.path = format!("assets/{name}");
        paths.insert(source_path, asset.path.clone());
    }
    check(cancel)?;
    preset.validate()?;
    Ok(StagedAnimationPreset { preset, files })
}

use std::collections::BTreeMap;
