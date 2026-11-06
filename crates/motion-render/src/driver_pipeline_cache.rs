//! Driver compilation data, separate from the live render-pipeline reuse cache.
//! File operations are explicit: rendering and Present never read/write a file.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

#[cfg(test)]
mod tests;

const MAGIC: &[u8; 8] = b"MSPCv001";
const HEADER_BYTES: usize = 80;
pub const MAX_DRIVER_PIPELINE_CACHE_BYTES: usize = 16 << 20;
// Changing the dependency or our shader interface starts a new application
// namespace. wgpu additionally validates its own version/driver cache header.
const INTERFACE: &str = concat!(env!("CARGO_PKG_VERSION"), "/wgpu-30.0.1/effect-interface-1");

#[derive(Debug, thiserror::Error)]
pub enum DriverPipelineCacheError {
    #[error("pipeline cache {operation} path={path}: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid pipeline cache: {0}")]
    Invalid(&'static str),
    #[error(
        "pipeline cache data exceeds the {MAX_DRIVER_PIPELINE_CACHE_BYTES}-byte snapshot limit"
    )]
    TooLarge,
    #[error("supported GPU did not return pipeline cache data")]
    Unavailable,
    #[error("pipeline cache GPU creation: {0}")]
    Gpu(#[source] wgpu::Error),
    #[error("{primary}; additional GPU cache errors: {related:?}")]
    RelatedGpu {
        #[source]
        primary: Box<DriverPipelineCacheError>,
        related: Vec<wgpu::Error>,
    },
    #[error("{primary}; temporary cache cleanup also failed: {cleanup}")]
    Cleanup {
        #[source]
        primary: Box<DriverPipelineCacheError>,
        cleanup: std::io::Error,
    },
}
type Result<T> = std::result::Result<T, DriverPipelineCacheError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DriverPipelineCacheLoad {
    Unsupported,
    Missing,
    Incompatible,
    Restored,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverPipelineCacheInfo {
    pub supported: bool,
    pub adapter_key: Option<String>,
    pub snapshot_byte_limit: usize,
    /// Creation calls that actually passed this driver's cache to wgpu.
    /// This is not a driver-reported cache-hit count; wgpu exposes no such count.
    pub effect_pipeline_creations: u64,
}

pub(crate) struct DriverPipelineCache {
    pub(crate) cache: Option<wgpu::PipelineCache>,
    adapter_key: Option<String>,
    identity: [u8; 32],
}

fn io(operation: &'static str, path: &Path, source: std::io::Error) -> DriverPipelineCacheError {
    DriverPipelineCacheError::Io {
        operation,
        path: path.to_string_lossy().chars().take(384).collect(),
        source,
    }
}

impl DriverPipelineCache {
    pub(crate) fn new(device: &wgpu::Device, adapter: &wgpu::AdapterInfo) -> Result<Self> {
        let adapter_key = wgpu::util::pipeline_cache_key(adapter)
            .filter(|_| device.features().contains(wgpu::Features::PIPELINE_CACHE));
        let mut identity = Sha256::new();
        // Length prefixes prevent distinct field boundaries sharing a namespace.
        for field in [
            INTERFACE,
            adapter_key.as_deref().unwrap_or("unsupported"),
            &adapter.driver,
            &adapter.driver_info,
        ] {
            identity.update((field.len() as u64).to_le_bytes());
            identity.update(field.as_bytes());
        }
        let cache = if adapter_key.is_some() {
            Some(create(device, None)?)
        } else {
            None
        };
        Ok(Self {
            cache,
            adapter_key,
            identity: identity.finalize().into(),
        })
    }

    pub(crate) fn info(&self, effect_pipeline_creations: u64) -> DriverPipelineCacheInfo {
        DriverPipelineCacheInfo {
            supported: self.cache.is_some(),
            adapter_key: self.adapter_key.clone(),
            snapshot_byte_limit: MAX_DRIVER_PIPELINE_CACHE_BYTES,
            effect_pipeline_creations,
        }
    }

    /// The cache must be application-owned data, never a project/plugin import.
    /// Corrupt envelopes are rejected before any bytes reach the GPU driver.
    pub(crate) fn load(
        &mut self,
        device: &wgpu::Device,
        path: &Path,
    ) -> Result<DriverPipelineCacheLoad> {
        if self.cache.is_none() {
            return Ok(DriverPipelineCacheLoad::Unsupported);
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DriverPipelineCacheLoad::Missing)
            }
            Err(error) => return Err(io("open", path, error)),
        };
        let mut bytes = Vec::new();
        file.take((MAX_DRIVER_PIPELINE_CACHE_BYTES + HEADER_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| io("read", path, error))?;
        let Some(data) = decode(&bytes, &self.identity)? else {
            return Ok(DriverPipelineCacheLoad::Incompatible);
        };
        // Installation happens only after validation succeeds. The existing
        // empty/working cache remains installed on every read/creation failure.
        self.cache = Some(create(device, Some(data))?);
        Ok(DriverPipelineCacheLoad::Restored)
    }

    pub(crate) fn save(&self, path: &Path) -> Result<bool> {
        let Some(cache) = &self.cache else {
            return Ok(false);
        };
        let data = cache
            .get_data()
            .ok_or(DriverPipelineCacheError::Unavailable)?;
        let bytes = encode(&self.identity, &data)?;
        write_atomic(path, &bytes)?;
        Ok(true)
    }
}

fn create(device: &wgpu::Device, data: Option<&[u8]>) -> Result<wgpu::PipelineCache> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    // SAFETY: None creates an empty cache. Some is exclusively an unchanged
    // get_data snapshot read from the application's cache directory, with the
    // envelope identity/size/SHA-256 validated above. As documented by wgpu,
    // disk provenance cannot be fully proven; these files are not import APIs.
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("Motion Studio effect compilation cache"),
            data,
            fallback: true,
        })
    };
    // Scopes belong to this creation operation; pop all even if one failed.
    let validation = pollster::block_on(validation.pop());
    let internal = pollster::block_on(internal.pop());
    let memory = pollster::block_on(memory.pop());
    let mut errors = validation.into_iter().chain(internal).chain(memory);
    if let Some(error) = errors.next() {
        let primary = DriverPipelineCacheError::Gpu(error);
        let related: Vec<_> = errors.collect();
        return Err(if related.is_empty() {
            primary
        } else {
            DriverPipelineCacheError::RelatedGpu {
                primary: Box::new(primary),
                related,
            }
        });
    }
    Ok(cache)
}

fn encode(identity: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() > MAX_DRIVER_PIPELINE_CACHE_BYTES {
        return Err(DriverPipelineCacheError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES + data.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(identity);
    bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(data));
    bytes.extend_from_slice(data);
    Ok(bytes)
}

fn decode<'a>(bytes: &'a [u8], identity: &[u8; 32]) -> Result<Option<&'a [u8]>> {
    if bytes.len() > HEADER_BYTES + MAX_DRIVER_PIPELINE_CACHE_BYTES {
        return Err(DriverPipelineCacheError::TooLarge);
    }
    if bytes.len() < HEADER_BYTES || &bytes[..8] != MAGIC {
        return Err(DriverPipelineCacheError::Invalid("header"));
    }
    let declared = u64::from_le_bytes(
        bytes[40..48]
            .try_into()
            .map_err(|_| DriverPipelineCacheError::Invalid("length"))?,
    );
    if declared != (bytes.len() - HEADER_BYTES) as u64 {
        return Err(DriverPipelineCacheError::Invalid("length"));
    }
    let data = &bytes[HEADER_BYTES..];
    if Sha256::digest(data).as_slice() != &bytes[48..80] {
        return Err(DriverPipelineCacheError::Invalid("checksum"));
    }
    if &bytes[8..40] != identity {
        return Ok(None);
    }
    Ok(Some(data))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| DriverPipelineCacheError::Invalid("system clock before epoch"))?
        .as_nanos();
    let temporary = parent.join(format!(
        ".motion-pipeline-{}-{stamp}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io("create temporary", &temporary, error))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|error| io("write", &temporary, error))?;
        file.sync_all()
            .map_err(|error| io("sync", &temporary, error))?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| io("publish", path, error))
    })();
    if let Err(primary) = result {
        return match fs::remove_file(&temporary) {
            Ok(()) => Err(primary),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(primary),
            Err(cleanup) => Err(DriverPipelineCacheError::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
        };
    }
    Ok(())
}
