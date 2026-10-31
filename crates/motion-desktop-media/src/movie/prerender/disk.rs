//! Optional native-byte cache. All methods perform blocking IO and belong on the
//! background owner, never inside foreground lookup/Present. Cache errors are
//! returned intact so the worker can observe them and render the normal fallback.
use super::{ExportControl, Result};
use motion_effects::{
    image_contract::{ImageContract, ImageMeaning, ImageStorage},
    AlphaMode, WorkingSpace,
};
use motion_render::{CapturedImage, CompositionFrameKey};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    time::SystemTime,
};

const MAGIC: &[u8; 8] = b"MSPC0001";
const HEADER_BYTES: u64 = 80;
const CHUNK: usize = 64 << 10;
const MAX_ENTRIES: usize = 4096;
const MAX_QUOTA: u64 = 64 << 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    directory: PathBuf,
    quota_bytes: u64,
}
impl Config {
    /// The caller supplies an application cache directory, never the project.
    /// Validation does no filesystem IO; opening is a background-only operation.
    pub fn new(directory: PathBuf, quota_bytes: u64) -> Result<Self> {
        if !directory.is_absolute()
            || directory.as_os_str().len() > 4096
            || !(HEADER_BYTES + 1..=MAX_QUOTA).contains(&quota_bytes)
        {
            return Err("prerender disk cache needs an absolute cache directory and quota in [81, 68719476736]".into());
        }
        Ok(Self {
            directory,
            quota_bytes,
        })
    }
    pub fn quota_bytes(&self) -> u64 {
        self.quota_bytes
    }
}

/// Transient document/device generations are deliberately absent from the disk
/// identity. The supplied composition key is retained for CURRENT-owner install.
/// A producer fingerprint is mandatory: schema/output versions alone do not
/// invalidate cached pixels after changing the executable during rapid iteration.
pub(super) struct FrameKey {
    pub composition: CompositionFrameKey,
    digest: [u8; 32],
    width: u32,
    height: u32,
    contract: ImageContract,
    storage: ImageStorage,
    bytes: u64,
}
impl FrameKey {
    pub fn new(
        composition: CompositionFrameKey,
        width: u32,
        height: u32,
        contract: ImageContract,
        producer: [u8; 32],
    ) -> Result<Self> {
        if contract.meaning
            != (ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            })
        {
            return Err("prerender disk cache requires native linear premultiplied color".into());
        }
        let storage = contract.preferred_storage().map_err(|e| e.to_string())?;
        let checked = CompositionFrameKey::new(
            composition.context(),
            composition.frame(),
            width,
            height,
            motion_render::image_formats::native_format(storage),
        )
        .map_err(|e| e.to_string())?;
        if checked != composition {
            return Err("prerender disk layout differs from composition key".into());
        }
        let bytes = storage
            .capacity(width, height)
            .ok_or("prerender disk frame byte overflow")?;
        let mut hash = Sha256::new();
        hash.update(b"motion-studio/native-disk-cache/v1\0");
        hash.update(producer);
        hash.update(composition.context().content);
        hash.update(composition.context().output);
        hash.update(composition.frame().to_bits().to_le_bytes());
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update(serde_json::to_vec(&(contract, storage)).map_err(|e| e.to_string())?);
        Ok(Self {
            composition,
            digest: hash.finalize().into(),
            width,
            height,
            contract,
            storage,
            bytes,
        })
    }
    fn name(&self) -> String {
        let mut name = String::with_capacity(69);
        for byte in self.digest {
            use std::fmt::Write;
            let _ = write!(name, "{byte:02x}");
        }
        name.push_str(".mspc");
        name
    }
    fn accepts(&self, image: &CapturedImage) -> bool {
        (
            image.width,
            image.height,
            image.contract,
            image.storage,
            image.pixels.len() as u64,
        ) == (
            self.width,
            self.height,
            self.contract,
            self.storage,
            self.bytes,
        )
    }
}

/// Source, compiler, target and feature identity is baked into the running code.
/// Replacing an executable path cannot make an old process use a new build's key.
pub(super) fn producer_fingerprint(control: &ExportControl) -> Result<[u8; 32]> {
    control.check()?;
    let mut identity = Sha256::new();
    identity.update(env!("MOTION_PRERENDER_PRODUCER").as_bytes());
    identity.update([u8::from(cfg!(test))]);
    Ok(identity.finalize().into())
}

pub(super) struct Cache {
    config: Config,
    directory: PathBuf,
    max_frame_bytes: u64,
    admitted: bool,
}
impl Cache {
    pub fn open(config: Config, max_frame_bytes: u64, control: &ExportControl) -> Result<Self> {
        control.check()?;
        if max_frame_bytes == 0 {
            return Err("prerender disk frame quota is disabled".into());
        }
        let directory = config.directory.join("native-v1");
        fs::create_dir_all(&directory).map_err(|e| format!("create prerender cache: {e}"))?;
        if !fs::symlink_metadata(&directory)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_dir()
        {
            return Err("prerender cache directory is not a regular directory".into());
        }
        let mut cache = Self {
            config,
            directory,
            max_frame_bytes,
            admitted: false,
        };
        // A smaller quota applies before any warm lookup, even if all future
        // frames hit. A busy writer is a normal hold, never a blocking wait.
        if let Some(_writer) = cache.writer()? {
            cache.trim(0, 0, control)?;
            cache.admitted = true;
        }
        Ok(cache)
    }
    #[cfg(test)]
    fn config(&self) -> &Config {
        &self.config
    }
    pub fn load(&self, key: &FrameKey, control: &ExportControl) -> Result<Option<CapturedImage>> {
        control.check()?;
        if !self.admitted {
            return Ok(None);
        }
        if key.bytes > self.max_frame_bytes || key.bytes + HEADER_BYTES > self.config.quota_bytes {
            return Ok(None);
        }
        let path = self.directory.join(key.name());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("inspect prerender cache entry: {error}")),
        };
        if !metadata.file_type().is_file() || metadata.len() != key.bytes + HEADER_BYTES {
            return Err("prerender cache entry has invalid type or length".into());
        }
        let mut file = match File::open(path) {
            Ok(file) => file,
            // Another cache owner may evict between metadata and open.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("open prerender cache entry: {error}")),
        };
        let mut header = [0u8; HEADER_BYTES as usize];
        file.read_exact(&mut header)
            .map_err(|e| format!("read prerender cache header: {e}"))?;
        if &header[..8] != MAGIC
            || header[8..40] != key.digest
            || header[40..48] != key.bytes.to_le_bytes()
        {
            return Err("prerender cache entry version, identity or payload length differs".into());
        }
        let count =
            usize::try_from(key.bytes).map_err(|_| "prerender disk frame exceeds address space")?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|e| format!("reserve admitted prerender cache bytes: {e}"))?;
        pixels.resize(count, 0);
        let mut hash = Sha256::new();
        for chunk in pixels.chunks_mut(CHUNK) {
            control.check()?;
            file.read_exact(chunk)
                .map_err(|e| format!("read prerender cache payload: {e}"))?;
            hash.update(chunk);
        }
        let digest: [u8; 32] = hash.finalize().into();
        if header[48..80] != digest {
            return Err("prerender cache payload checksum mismatch".into());
        }
        let mut trailing = [0u8; 1];
        if file
            .read(&mut trailing)
            .map_err(|e| format!("finish prerender cache payload: {e}"))?
            != 0
        {
            return Err("prerender cache payload has trailing bytes".into());
        }
        control.check()?;
        Ok(Some(CapturedImage {
            width: key.width,
            height: key.height,
            contract: key.contract,
            storage: key.storage,
            pixels,
        }))
    }
    pub fn store(
        &self,
        key: &FrameKey,
        image: &CapturedImage,
        control: &ExportControl,
    ) -> Result<bool> {
        self.store_with(key, image, control, |_, _| Ok(()))
    }
    fn store_with(
        &self,
        key: &FrameKey,
        image: &CapturedImage,
        control: &ExportControl,
        mut before_chunk: impl FnMut(usize, &ExportControl) -> Result<()>,
    ) -> Result<bool> {
        control.check()?;
        if !key.accepts(image) {
            return Err("prerender disk image differs from native layout".into());
        }
        if !self.admitted {
            return Ok(false);
        }
        if key.bytes > self.max_frame_bytes || key.bytes + HEADER_BYTES > self.config.quota_bytes {
            return Ok(false);
        }
        let Some(_writer) = self.writer()? else {
            return Ok(false);
        };
        control.check()?;
        self.trim(key.bytes + HEADER_BYTES, 1, control)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".mspc-")
            .suffix(".partial")
            .tempfile_in(&self.directory)
            .map_err(|e| format!("stage prerender cache entry: {e}"))?;
        let mut header = Vec::with_capacity(HEADER_BYTES as usize);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&key.digest);
        header.extend_from_slice(&key.bytes.to_le_bytes());
        let mut hash = Sha256::new();
        for chunk in image.pixels.chunks(CHUNK) {
            control.check()?;
            hash.update(chunk);
        }
        header.extend_from_slice(&hash.finalize());
        temporary
            .write_all(&header)
            .map_err(|e| format!("write prerender cache header: {e}"))?;
        for (index, chunk) in image.pixels.chunks(CHUNK).enumerate() {
            before_chunk(index, control)?;
            control.check()?;
            temporary
                .write_all(chunk)
                .map_err(|e| format!("write prerender cache payload: {e}"))?;
        }
        control.check()?;
        temporary
            .as_file()
            .sync_data()
            .map_err(|e| format!("flush prerender cache entry: {e}"))?;
        control.check()?;
        temporary
            .persist(self.directory.join(key.name()))
            .map_err(|e| format!("publish prerender cache entry: {}", e.error))?;
        Ok(true)
    }
    fn writer(&self) -> Result<Option<File>> {
        // Independent OS handles serialize owners across threads/processes.
        // This zero-byte control file is outside the payload namespace/quota.
        let writer = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.config.directory.join("native-v1.lock"))
            .map_err(|e| format!("open prerender cache writer lock: {e}"))?;
        match writer.try_lock() {
            Ok(()) => Ok(Some(writer)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => {
                Err(format!("lock prerender cache writer: {error}"))
            }
        }
    }
    fn trim(&self, incoming: u64, incoming_entries: usize, control: &ExportControl) -> Result<()> {
        let mut entries = Vec::new();
        let mut bytes = 0u64;
        let mut count = 0usize;
        let directory = fs::read_dir(&self.directory)
            .map_err(|e| format!("scan prerender cache quota: {e}"))?;
        for (index, entry) in directory.enumerate() {
            control.check()?;
            if index >= MAX_ENTRIES {
                return Err("prerender cache entry limit exceeded".into());
            }
            count += 1;
            let entry = entry.map_err(|e| format!("read prerender cache quota entry: {e}"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|e| format!("inspect prerender cache quota entry: {e}"))?;
            if metadata.file_type().is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or("prerender disk quota overflow")?;
                if owned_name(&entry.file_name().to_string_lossy()) {
                    entries.push((
                        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        entry.file_name(),
                        metadata.len(),
                    ));
                }
            }
        }
        entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (_, path, size) in entries {
            if bytes <= self.config.quota_bytes - incoming
                && count <= MAX_ENTRIES - incoming_entries
            {
                break;
            }
            control.check()?;
            fs::remove_file(self.directory.join(path))
                .map_err(|e| format!("evict regenerable prerender cache entry: {e}"))?;
            bytes -= size;
            count -= 1;
        }
        if bytes > self.config.quota_bytes - incoming || count > MAX_ENTRIES - incoming_entries {
            return Err("prerender cache quota occupied by unowned entries".into());
        }
        Ok(())
    }
}
fn owned_name(name: &str) -> bool {
    if let Some(stem) = name.strip_suffix(".mspc") {
        return stem.len() == 64
            && stem
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    }
    name.strip_prefix(".mspc-")
        .and_then(|s| s.strip_suffix(".partial"))
        .is_some_and(|s| {
            (6..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

/// Saturating totals cover this worker's lifetime, across preparation changes.
/// Consecutive IO fault counters reset on success, a normal miss, cancellation
/// or a changed disk configuration. Re-preparing the same unavailable cache
/// does not reset open failures. The last cause is bounded to 512 characters.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub enabled: bool,
    pub ready: bool,
    pub quota_bytes: u64,
    pub reads: u64,
    pub hits: u64,
    pub misses: u64,
    pub writes: u64,
    /// Writes skipped because the frame exceeds quota or another writer owns
    /// the nonblocking cache lock. Neither condition is an IO failure.
    pub write_skips: u64,
    /// Preparations held because a different writer owns quota admission.
    pub open_holds: u64,
    pub io_failures: u64,
    pub last_failure: Option<String>,
}
#[derive(Default)]
pub(super) struct Owner {
    cache: Option<Cache>,
    config: Option<Config>,
    producer: [u8; 32],
    pub stats: Stats,
    consecutive: [u8; 3],
}
impl Owner {
    pub fn configure(
        &mut self,
        config: Option<Config>,
        limit: u64,
        id: u64,
        control: &ExportControl,
    ) -> Result<()> {
        self.cache = None;
        if self.config != config || config.is_none() {
            self.consecutive = [0; 3];
        }
        self.config = config.clone();
        self.stats.ready = false;
        self.stats.enabled = config.is_some();
        self.stats.quota_bytes = config.as_ref().map_or(0, Config::quota_bytes);
        let Some(config) = config else {
            return Ok(());
        };
        self.producer = match producer_fingerprint(control) {
            Ok(producer) => producer,
            Err(cancelled) => {
                self.consecutive[0] = 0;
                return Err(cancelled);
            }
        };
        match Cache::open(config, limit, control) {
            Ok(cache) => {
                self.consecutive[0] = 0;
                if cache.admitted {
                    self.cache = Some(cache);
                    self.stats.ready = true;
                    self.stats.last_failure = None;
                } else {
                    self.stats.open_holds = self.stats.open_holds.saturating_add(1);
                }
            }
            Err(cause) => {
                if let Err(cancelled) = control.check() {
                    self.consecutive[0] = 0;
                    return Err(cancelled);
                }
                self.fault(0, id, None, &cause);
            }
        }
        Ok(())
    }
    pub fn key(&self, input: &super::PreparedPrerender, frame: f64) -> Result<Option<FrameKey>> {
        if self.cache.is_none() {
            return Ok(None);
        }
        let contract = ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            input.frozen.project.working_precision,
        );
        FrameKey::new(
            input.frame_key(frame)?,
            input.frozen.project.width,
            input.frozen.project.height,
            contract,
            self.producer,
        )
        .map(Some)
    }
    pub fn load(
        &mut self,
        key: &FrameKey,
        id: u64,
        control: &ExportControl,
    ) -> Result<Option<CapturedImage>> {
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        self.stats.reads = self.stats.reads.saturating_add(1);
        match cache.load(key, control) {
            Ok(image) => {
                self.consecutive[1] = 0;
                if image.is_some() {
                    self.stats.hits = self.stats.hits.saturating_add(1);
                } else {
                    self.stats.misses = self.stats.misses.saturating_add(1);
                }
                Ok(image)
            }
            Err(cause) => {
                if let Err(cancelled) = control.check() {
                    self.consecutive[1] = 0;
                    return Err(cancelled);
                }
                self.fault(1, id, Some(key.composition.frame()), &cause);
                Ok(None)
            }
        }
    }
    pub fn store(
        &mut self,
        key: &FrameKey,
        image: &CapturedImage,
        id: u64,
        control: &ExportControl,
    ) -> Result<()> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        match cache.store(key, image, control) {
            Ok(stored) => {
                self.consecutive[2] = 0;
                if stored {
                    self.stats.writes = self.stats.writes.saturating_add(1);
                } else {
                    self.stats.write_skips = self.stats.write_skips.saturating_add(1);
                }
            }
            Err(cause) => {
                if let Err(cancelled) = control.check() {
                    self.consecutive[2] = 0;
                    return Err(cancelled);
                }
                self.fault(2, id, Some(key.composition.frame()), &cause);
            }
        }
        Ok(())
    }
    fn fault(&mut self, site: usize, id: u64, frame: Option<f64>, cause: &str) {
        self.stats.io_failures = self.stats.io_failures.saturating_add(1);
        let cause: String = cause.chars().take(512).collect();
        self.stats.last_failure = Some(cause.clone());
        let count = self.consecutive[site].saturating_add(1);
        self.consecutive[site] = count;
        let phase = ["open", "read", "write"][site];
        let directory = self
            .config
            .as_ref()
            .map(|config| {
                config
                    .directory
                    .to_string_lossy()
                    .chars()
                    .take(512)
                    .collect::<String>()
            })
            .unwrap_or_default();
        // Optional cache faults do not fail rendering. Preserve the original
        // cause at Info defaults immediately, and escalate a sustained site once.
        if count == 1 {
            log::warn!("prerender disk ticket={id} phase={phase} frame={frame:?} consecutive={count} cache={directory} cause={cause}");
        }
        if count == 60 {
            log::error!("prerender disk ticket={id} phase={phase} frame={frame:?} consecutive={count} cache={directory} cause={cause}");
        }
    }
}

#[cfg(test)]
mod tests;
