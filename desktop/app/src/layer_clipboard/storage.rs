use super::{LayerClipboard, Read, Stamp};
use crate::owned_file::OwnedFile;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read as _, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Directory {
    path: PathBuf,
    files: Vec<OwnedFile>,
}
impl Drop for Directory {
    fn drop(&mut self) {
        self.files.clear();
        if let Err(error) = fs::remove_dir(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::error!(
                    "layer clipboard snapshot cleanup failed: path={} reason={error}",
                    self.path.display()
                );
            }
        }
    }
}
struct Entry {
    source: String,
    cached: PathBuf,
    destination: String,
    bytes: u64,
}
pub(crate) struct Frozen {
    pub origin: Stamp,
    pub objects: Vec<u64>,
    pub frame: u32,
    clipboard: LayerClipboard,
    entries: Vec<Entry>,
    _directory: Directory,
}
pub(crate) struct Prepared {
    pub clipboard: LayerClipboard,
    pub target: Stamp,
    pub origin: Stamp,
    pub files: Vec<OwnedFile>,
}

fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("clipboard operation cancelled".into())
    } else {
        Ok(())
    }
}
fn io_error(phase: &str, path: &Path, error: impl std::fmt::Display) -> String {
    format!(
        "layer clipboard phase={phase} path={}: {error}",
        path.display()
    )
}
fn owned_source(root: &Path, relative: &str) -> Result<PathBuf, String> {
    motion_core::storage::validate_relative_path(relative).map_err(|e| e.to_string())?;
    let path = root.join(relative);
    let path = path
        .canonicalize()
        .map_err(|e| io_error("resolve", &path, e))?;
    if !path.starts_with(root) {
        return Err(io_error(
            "resolve",
            &path,
            "source resolves outside its owner",
        ));
    }
    Ok(path)
}
fn owned_parent(root: &Path, relative: &str) -> Result<PathBuf, String> {
    motion_core::storage::validate_relative_path(relative).map_err(|e| e.to_string())?;
    let mut parent = root.to_owned();
    for component in Path::new(relative)
        .parent()
        .ok_or("clipboard destination parent missing")?
        .components()
    {
        let path = parent.join(component);
        if !path
            .try_exists()
            .map_err(|e| io_error("target_directory", &path, e))?
        {
            // Never create through an unchecked symlink ancestor.
            fs::create_dir(&path).map_err(|e| io_error("target_directory", &path, e))?;
        }
        parent = path
            .canonicalize()
            .map_err(|e| io_error("target_directory", &path, e))?;
        let metadata =
            fs::metadata(&parent).map_err(|e| io_error("target_directory_metadata", &parent, e))?;
        if !parent.starts_with(root) || !metadata.is_dir() {
            return Err(io_error(
                "target_directory",
                &parent,
                "destination resolves outside its owner or is not a directory",
            ));
        }
    }
    Ok(parent)
}

fn copy(
    source: &mut File,
    target: &mut File,
    size: u64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut buffer = vec![0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    let mut copied = 0u64;
    loop {
        check(cancel)?;
        let n = source
            .read(&mut buffer)
            .map_err(|e| format!("read source: {e}"))?;
        if n == 0 {
            break;
        }
        copied = copied
            .checked_add(n as u64)
            .ok_or("clipboard resource size overflow")?;
        if copied > size {
            return Err("clipboard source changed size while copying".into());
        }
        target
            .write_all(&buffer[..n])
            .map_err(|e| format!("write owned source: {e}"))?;
    }
    if copied != size {
        return Err("clipboard source changed size while copying".into());
    }
    target
        .sync_all()
        .map_err(|e| format!("sync owned source: {e}"))
}

pub(super) fn capture(read: Read, cache: &Path, cancel: &AtomicBool) -> Result<Frozen, String> {
    check(cancel)?;
    let clipboard = read.clipboard.ok_or("select layers to copy")?;
    let view = clipboard.resolve().map_err(|e| e.to_string())?;
    // Deduplicate video and recoverable audio, which share one owned file.
    let mut sources: BTreeMap<&str, (u64, Option<u64>, bool)> = BTreeMap::new();
    for asset in &view.assets {
        sources.insert(&asset.path, (64 * 1024 * 1024, None, false));
    }
    for asset in &view.model_assets {
        sources.insert(&asset.path, (motion_core::model_assets::MAX_MODEL_ASSET_BYTES, Some(asset.bytes), false));
    }
    for font in &view.fonts {
        sources.insert(&font.path, (32 * 1024 * 1024, None, true));
    }
    for (path, bytes) in view
        .audio_assets
        .iter()
        .map(|a| (&a.path, a.bytes))
        .chain(view.video_assets.iter().map(|a| (&a.path, a.bytes)))
    {
        if let Some((_, previous, _)) = sources.get(path.as_str()) {
            if *previous != Some(bytes) {
                return Err(format!("clipboard source metadata conflict: path={path}"));
            }
        }
        sources.insert(
            path,
            (motion_core::storage::MAX_MEDIA_ASSET, Some(bytes), false),
        );
    }
    let root = read
        .stamp
        .root
        .canonicalize()
        .map_err(|e| io_error("owner", &read.stamp.root, e))?;
    fs::create_dir_all(cache).map_err(|e| io_error("cache", cache, e))?;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "clipboard snapshot ID space exhausted")?;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let token = format!("{}-{time}-{sequence}", std::process::id());
    let path = cache.join(format!("snapshot-{token}"));
    fs::create_dir(&path).map_err(|e| io_error("snapshot", &path, e))?;
    let mut directory = Directory {
        path,
        files: vec![],
    };
    let mut total = 0u64;
    let mut entries = vec![];
    for (index, (relative, (limit, expected, font))) in sources.into_iter().enumerate() {
        check(cancel)?;
        let source = owned_source(&root, relative)?;
        let mut file = File::open(&source).map_err(|e| io_error("open", &source, e))?;
        let before = file
            .metadata()
            .map_err(|e| io_error("metadata", &source, e))?;
        if !before.is_file() || before.len() > limit || expected.is_some_and(|n| n != before.len())
        {
            return Err(io_error(
                "metadata",
                &source,
                "source is invalid, changed or too large",
            ));
        }
        total = total
            .checked_add(before.len())
            .ok_or("clipboard resource size overflow")?;
        if total > motion_core::storage::MAX_PACKAGE {
            return Err("clipboard source budget exceeded".into());
        }
        let extension = Path::new(relative)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");
        let name = format!("clipboard-{token}-{index}.{extension}");
        let cached = directory.path.join(&name);
        let mut output = OwnedFile::create(&cached).map_err(|e| io_error("freeze", &cached, e))?;
        copy(&mut file, output.file(), before.len(), cancel)
            .map_err(|e| io_error("freeze", &source, e))?;
        let after = file
            .metadata()
            .map_err(|e| io_error("metadata_after", &source, e))?;
        if before.len() != after.len()
            || before
                .modified()
                .map_err(|e| io_error("modified_before", &source, e))?
                != after
                    .modified()
                    .map_err(|e| io_error("modified_after", &source, e))?
        {
            return Err(io_error("freeze", &source, "source changed while copying"));
        }
        output.close();
        directory.files.push(output);
        entries.push(Entry {
            source: relative.into(),
            cached,
            destination: format!("assets/{}{name}", if font { "fonts/" } else if relative.starts_with("assets/models/") { "models/" } else { "" }),
            bytes: before.len(),
        });
    }
    let objects = clipboard.objects.clone();
    Ok(Frozen {
        origin: read.stamp,
        frame: read.frame,
        objects,
        clipboard,
        entries,
        _directory: directory,
    })
}

fn equal(
    left_path: &Path,
    right_path: &Path,
    size: u64,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let mut left = File::open(left_path).map_err(|e| io_error("cached_open", left_path, e))?;
    let mut right = File::open(right_path).map_err(|e| io_error("existing_open", right_path, e))?;
    if left
        .metadata()
        .map_err(|e| io_error("cached_metadata", left_path, e))?
        .len()
        != size
        || right
            .metadata()
            .map_err(|e| io_error("existing_metadata", right_path, e))?
            .len()
            != size
    {
        return Ok(false);
    }
    let mut a = vec![0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    let mut b = vec![0; a.len()];
    let mut remaining = size;
    while remaining != 0 {
        check(cancel)?;
        let n = remaining.min(a.len() as u64) as usize;
        left.read_exact(&mut a[..n])
            .map_err(|e| io_error("cached_read", left_path, e))?;
        right
            .read_exact(&mut b[..n])
            .map_err(|e| io_error("existing_read", right_path, e))?;
        if a[..n] != b[..n] {
            return Ok(false);
        }
        remaining -= n as u64;
    }
    Ok(true)
}

pub(super) fn prepare(
    frozen: &Frozen,
    target: Stamp,
    project: &motion_core::Project,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    check(cancel)?;
    let root = target
        .root
        .canonicalize()
        .map_err(|e| io_error("target", &target.root, e))?;
    let mut clipboard = frozen.clipboard.clone();
    let mut files = vec![];
    let registered: std::collections::BTreeSet<_> = project
        .assets
        .iter()
        .map(|a| a.path.as_str())
        .chain(project.fonts.iter().map(|a| a.path.as_str()))
        .chain(project.audio_assets.iter().map(|a| a.path.as_str()))
        .chain(project.video_assets.iter().map(|a| a.path.as_str()))
        .chain(project.model_assets.iter().map(|a| a.path.as_str()))
        .collect();
    for entry in &frozen.entries {
        check(cancel)?;
        let parent = owned_parent(&root, &entry.destination)?;
        let name = Path::new(&entry.destination)
            .file_name()
            .ok_or("clipboard resource filename missing")?;
        let path = parent.join(name);
        if registered.contains(entry.destination.as_str()) {
            let actual = owned_source(&root, &entry.destination)?;
            if !equal(&entry.cached, &actual, entry.bytes, cancel)? {
                return Err(io_error(
                    "reuse",
                    &actual,
                    "registered clipboard resource bytes changed",
                ));
            }
        } else {
            let mut output = OwnedFile::create(&path).map_err(|e| io_error("stage", &path, e))?;
            let mut source =
                File::open(&entry.cached).map_err(|e| io_error("cached_open", &entry.cached, e))?;
            copy(&mut source, output.file(), entry.bytes, cancel)
                .map_err(|e| io_error("stage", &path, e))?;
            output.close();
            files.push(output);
        }
        let source = &mut clipboard.source;
        for path in source
            .assets
            .iter_mut()
            .map(|a| &mut a.path)
            .chain(source.fonts.iter_mut().map(|a| &mut a.path))
            .chain(source.audio_assets.iter_mut().map(|a| &mut a.path))
            .chain(source.video_assets.iter_mut().map(|a| &mut a.path))
            .chain(source.model_assets.iter_mut().map(|a| &mut a.path))
        {
            if *path == entry.source {
                *path = entry.destination.clone();
            }
        }
    }
    clipboard.resolve().map_err(|e| e.to_string())?;
    Ok(Prepared {
        clipboard,
        target,
        origin: frozen.origin.clone(),
        files,
    })
}
