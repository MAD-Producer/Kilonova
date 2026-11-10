//! Explicit persistent raw inference results, independent of Session spill and
//! frozen-export ownership. No calibration or alignment proof is serialized.
//! A cache directory has a strict shared quota; contention/stale lock, corruption
//! and unsupported identities fail the request rather than silently infer.

use super::*;
use motion_model::depth::{DepthDescriptor, DepthResource, MAX_DEPTH_RESOURCE_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"MSDCACH1";
const MAX_METADATA: usize = 64 << 10;
const MAX_ENTRIES: usize = 4096;
const MAX_DISK: u64 = 2 << 30;
const CHUNK: usize = 64 << 10;

/// Attested by the backend before lookup, never inferred from quality labels.
/// Plan/runtime hashes cover executable procedure, runtime and numerical mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub backend: String,
    pub provider: String,
    pub runtime_sha256: [u8; 32],
    pub plan_sha256: [u8; 32],
    pub precision: String,
}
impl Execution {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.backend, &self.provider, &self.precision] {
            if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err(Error::Invalid("unbounded cache execution identity"));
            }
        }
        if self.runtime_sha256 == [0; 32] || self.plan_sha256 == [0; 32] {
            return Err(Error::Invalid(
                "cache execution requires nonzero runtime and plan digests",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_result(&self, estimate: &Estimate) -> Result<()> {
        if estimate.provider != self.provider {
            return Err(Error::Invalid(
                "actual provider differs from cache execution attestation",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Application-owned, dedicated absolute directory; construction does no IO.
    pub directory: PathBuf,
    pub byte_limit: u64,
    pub entry_limit: usize,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !self.directory.is_absolute()
            || self.byte_limit == 0
            || self.byte_limit > MAX_DISK
            || self.entry_limit == 0
            || self.entry_limit > MAX_ENTRIES
        {
            return Err(Error::Invalid(
                "depth cache requires an absolute directory and bounded quota",
            ));
        }
        Ok(())
    }
    fn path(
        &self,
        input: &Input,
        fingerprint: PreparedDepthInput,
        execution: &Execution,
    ) -> Result<PathBuf> {
        fingerprint.validate(&input.key.geometry)?;
        execution.validate()?;
        let mut key = input.key.clone();
        let mut a = key.source.pts.ticks.unsigned_abs();
        let mut b = u64::from(key.source.pts.timescale);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        key.source.pts.ticks = (i128::from(key.source.pts.ticks) / i128::from(a)) as i64;
        key.source.pts.timescale = (u64::from(key.source.pts.timescale) / a) as u32;
        let mut hash = Sha256::new();
        hash.update(b"MotionStudio/depth/persistent-raw/v1\0");
        hash.update(serde_json::to_vec(&(key, fingerprint, execution))?);
        let name: String = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(self.directory.join(name + ".depth"))
    }
    fn inventory(&self, check: &dyn Fn() -> Result<()>) -> Result<(usize, u64)> {
        let mut entries = 0usize;
        let mut bytes = 0u64;
        for entry in fs::read_dir(&self.directory)? {
            check()?;
            let entry = entry?;
            if entry.file_name() == ".writer.lock" {
                continue;
            }
            // Include abandoned temporaries and foreign files in the quota.
            // Never follow links out of the dedicated cache directory.
            if !entry.file_type()?.is_file() {
                return Err(Error::Invalid("depth cache contains a non-file entry"));
            }
            entries = entries
                .checked_add(1)
                .ok_or(Error::Invalid("cache count overflow"))?;
            bytes = bytes
                .checked_add(entry.metadata()?.len())
                .ok_or(Error::Invalid("cache size overflow"))?;
            if entries > self.entry_limit || bytes > self.byte_limit {
                return Err(Error::Invalid("persistent depth cache quota exceeded"));
            }
        }
        Ok((entries, bytes))
    }
    pub(crate) fn read(
        &self,
        input: &Input,
        fingerprint: PreparedDepthInput,
        execution: &Execution,
        cancel: &Cancellation,
    ) -> Result<Option<Estimate>> {
        self.read_checked(input, fingerprint, execution, &|| cancel.check())
    }
    fn read_checked(
        &self,
        input: &Input,
        fingerprint: PreparedDepthInput,
        execution: &Execution,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Option<Estimate>> {
        self.validate()?;
        check()?;
        let path = self.path(input, fingerprint, execution)?;
        match fs::symlink_metadata(&self.directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(Error::Invalid(
                    "depth cache root must be a directory, not a link",
                ))
            }
            Ok(_) => (),
        }
        let _lock = DirectoryLock::acquire(&self.directory)?;
        self.inventory(check)?;
        self.read_entry(&path, input, fingerprint, execution, check)
    }
    fn read_entry(
        &self,
        path: &Path,
        input: &Input,
        fingerprint: PreparedDepthInput,
        execution: &Execution,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Option<Estimate>> {
        let meta = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(meta) => meta,
        };
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() > (MAX_DEPTH_RESOURCE_BYTES + MAX_METADATA + 48) as u64
        {
            return Err(Error::Invalid(
                "depth cache entry type or byte bound rejected",
            ));
        }
        let mut file = File::open(path)?;
        let mut prefix = [0u8; 16];
        file.read_exact(&mut prefix)?;
        if &prefix[..8] != MAGIC {
            return Err(Error::Invalid("unsupported current depth cache format"));
        }
        let size = u64::from_le_bytes(prefix[8..].try_into().map_err(|_| Error::State)?);
        if size == 0 || size > MAX_METADATA as u64 {
            return Err(Error::Invalid("cache metadata exceeds bounded header"));
        }
        let mut metadata = vec![0; size as usize];
        file.read_exact(&mut metadata)?;
        let header: Metadata = serde_json::from_slice(&metadata)?;
        if header.version != 1
            || !header.key.same_sample(input.key())
            || header.input != fingerprint
            || header.execution != *execution
            || header.descriptor.source != header.key.source
            || header.descriptor.geometry != header.key.geometry
            || header.descriptor.model != header.key.model
        {
            return Err(Error::Invalid(
                "depth cache source, input or execution identity differs",
            ));
        }
        let pixels = header.descriptor.validate()?;
        let bytes = pixels
            .checked_mul(4 + usize::from(header.validity) + 4 * usize::from(header.confidence))
            .ok_or(Error::Invalid("cache plane size overflow"))?;
        if bytes > MAX_DEPTH_RESOURCE_BYTES || meta.len() != 16 + size + bytes as u64 + 32 {
            return Err(Error::Invalid(
                "depth cache plane byte count rejected before allocation",
            ));
        }
        // Verify the exact encoded header and all planes before allocating them.
        let mut hash = Sha256::new();
        hash.update(prefix);
        hash.update(&metadata);
        let mut remaining = bytes;
        let mut chunk = [0u8; CHUNK];
        while remaining > 0 {
            check()?;
            let count = remaining.min(CHUNK);
            file.read_exact(&mut chunk[..count])?;
            hash.update(&chunk[..count]);
            remaining -= count;
        }
        let mut digest = [0; 32];
        file.read_exact(&mut digest)?;
        if <[u8; 32]>::from(hash.finalize()) != digest {
            return Err(Error::Invalid("depth cache content SHA-256 differs"));
        }
        file.seek(SeekFrom::Start(16 + size))?;
        let mut reader_hash = Sha256::new();
        reader_hash.update(prefix);
        reader_hash.update(&metadata);
        let values = read_floats(&mut file, pixels, &mut reader_hash, check)?;
        let validity = if header.validity {
            let mut mask = vec![0; pixels];
            for chunk in mask.chunks_mut(CHUNK) {
                check()?;
                file.read_exact(chunk)?;
                reader_hash.update(chunk);
            }
            Some(mask)
        } else {
            None
        };
        let confidence = if header.confidence {
            Some(read_floats(&mut file, pixels, &mut reader_hash, check)?)
        } else {
            None
        };
        check()?;
        if <[u8; 32]>::from(reader_hash.finalize()) != digest {
            return Err(Error::Invalid("depth cache changed during reconstruction"));
        }
        let mut descriptor = header.descriptor;
        descriptor.source = input.key.source.clone();
        Ok(Some(Estimate {
            resource: DepthResource::new(descriptor, values, validity, confidence)?,
            provider: execution.provider.clone(),
            costs: Costs::default(),
        }))
    }
    pub(crate) fn write(
        &self,
        input: &Input,
        fingerprint: PreparedDepthInput,
        execution: &Execution,
        estimate: &Estimate,
        cancel: &Cancellation,
    ) -> Result<()> {
        self.validate()?;
        cancel.check()?;
        estimate.validate(input.key())?;
        execution.validate_result(estimate)?;
        let path = self.path(input, fingerprint, execution)?;
        fs::create_dir_all(&self.directory)?;
        let root = fs::symlink_metadata(&self.directory)?;
        if !root.is_dir() || root.file_type().is_symlink() {
            return Err(Error::Invalid(
                "depth cache root must be a directory, not a link",
            ));
        }
        let _lock = DirectoryLock::acquire(&self.directory)?;
        let (entries, used) = self.inventory(&|| cancel.check())?;
        if self
            .read_entry(&path, input, fingerprint, execution, &|| cancel.check())?
            .is_some()
        {
            return Ok(());
        }
        let resource = &estimate.resource;
        let metadata = serde_json::to_vec(&Metadata {
            version: 1,
            key: input.key.clone(),
            input: fingerprint,
            execution: execution.clone(),
            descriptor: resource.descriptor().clone(),
            validity: resource.validity().is_some(),
            confidence: resource.confidence().is_some(),
        })?;
        if metadata.len() > MAX_METADATA {
            return Err(Error::Invalid("cache metadata byte limit exceeded"));
        }
        let bytes = resource.values().len() as u64
            * (4 + u64::from(resource.validity().is_some())
                + 4 * u64::from(resource.confidence().is_some()));
        let required = 16 + metadata.len() as u64 + bytes + 32;
        if bytes > MAX_DEPTH_RESOURCE_BYTES as u64
            || entries >= self.entry_limit
            || required > self.byte_limit.saturating_sub(used)
        {
            return Err(Error::Invalid(
                "persistent depth cache admission exceeds quota",
            ));
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("pending-")
            .tempfile_in(&self.directory)?;
        let result = (|| {
            let mut hash = Sha256::new();
            let mut write = |data: &[u8]| -> Result<()> {
                cancel.check()?;
                temporary.write_all(data)?;
                hash.update(data);
                Ok(())
            };
            write(MAGIC)?;
            write(&(metadata.len() as u64).to_le_bytes())?;
            write(&metadata)?;
            let mut buffer = Vec::with_capacity(CHUNK);
            for floats in resource.values().chunks(CHUNK / 4) {
                buffer.clear();
                for value in floats {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                write(&buffer)?;
            }
            if let Some(mask) = resource.validity() {
                for chunk in mask.chunks(CHUNK) {
                    write(chunk)?;
                }
            }
            if let Some(plane) = resource.confidence() {
                for floats in plane.chunks(CHUNK / 4) {
                    buffer.clear();
                    for value in floats {
                        buffer.extend_from_slice(&value.to_le_bytes());
                    }
                    write(&buffer)?;
                }
            }
            drop(write);
            temporary.write_all(&hash.finalize())?;
            temporary.flush()?;
            temporary.as_file().sync_all()?;
            cancel.check()?;
            Ok(())
        })();
        if let Err(primary) = result {
            return match temporary.close() {
                Ok(()) => Err(primary),
                Err(cleanup) => Err(Error::Cleanup {
                    primary: Box::new(primary),
                    cleanup,
                }),
            };
        }
        match temporary.persist_noclobber(path) {
            Ok(_) => Ok(()),
            Err(error) => {
                let primary = Error::Io(error.error);
                match error.file.close() {
                    Ok(()) => Err(primary),
                    Err(cleanup) => Err(Error::Cleanup {
                        primary: Box::new(primary),
                        cleanup,
                    }),
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u32,
    key: Key,
    input: PreparedDepthInput,
    execution: Execution,
    descriptor: DepthDescriptor,
    validity: bool,
    confidence: bool,
}

fn read_floats(
    file: &mut File,
    count: usize,
    hash: &mut Sha256,
    check: &dyn Fn() -> Result<()>,
) -> Result<Vec<f32>> {
    let mut values = Vec::with_capacity(count);
    let mut chunk = [0; CHUNK];
    while values.len() < count {
        check()?;
        let bytes = (count - values.len()).min(CHUNK / 4) * 4;
        file.read_exact(&mut chunk[..bytes])?;
        hash.update(&chunk[..bytes]);
        values.extend(
            chunk[..bytes]
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]])),
        );
    }
    Ok(values)
}

struct DirectoryLock {
    path: PathBuf,
    _file: File,
}
impl DirectoryLock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join(".writer.lock");
        // No waiting, stale-lock deletion, business locks or hidden retry.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self { path, _file: file })
    }
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            log::error!("operation=depth_cache phase=lock_cleanup cause={error}");
        }
    }
}

#[cfg(test)]
mod tests;
