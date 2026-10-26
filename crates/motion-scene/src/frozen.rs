//! Self-contained snapshot of the exact source and owned external dependencies.
//! Decoding reuses the importer and validates every digest; no serialized runtime IR is trusted.
use crate::{
    import, invalid, resources::relative_path_checked, AssetResolver, ImportContext, ImportLimits,
    Resource, ResourceKind, Result, Scene,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io};

const MAGIC: &[u8; 8] = b"MSCN\0\0\0\x01";

impl Scene {
    /// Freeze the exact imported source plus every owned external dependency. The source
    /// digest must match, so accidentally pairing a scene with another document fails.
    /// `max_bytes` bounds the encoded bundle independently of decoded import limits.
    pub fn freeze(&self, source: &[u8], max_bytes: usize) -> Result<Vec<u8>> {
        let digest: [u8; 32] = Sha256::digest(source).into();
        if digest != self.source_sha256 {
            return Err(invalid(
                "freeze",
                "source",
                "source digest does not match imported scene",
            ));
        }
        let mut resources: BTreeMap<&str, &Resource> = BTreeMap::new();
        for resource in self
            .buffers
            .iter()
            .chain(self.images.iter().map(|i| &i.resource))
        {
            if let Some(path) = resource.locator.as_deref() {
                if resources
                    .insert(path, resource)
                    .is_some_and(|other| other.sha256 != resource.sha256)
                {
                    return Err(invalid(
                        "freeze",
                        path,
                        "same locator resolved to different content during import",
                    ));
                }
            }
        }
        let mut out = Encoder {
            bytes: Vec::new(),
            limit: max_bytes,
        };
        out.append(MAGIC)?;
        out.append(&(source.len() as u64).to_le_bytes())?;
        out.append(&digest)?;
        out.append(source)?;
        let count = u32::try_from(resources.len())
            .map_err(|_| invalid("freeze", "resources", "resource count exceeds u32"))?;
        out.append(&count.to_le_bytes())?;
        for (path, resource) in resources {
            out.append(&(path.len() as u32).to_le_bytes())?;
            out.append(path.as_bytes())?;
            out.append(&(resource.bytes.len() as u64).to_le_bytes())?;
            out.append(&resource.sha256)?;
            out.append(&resource.bytes)?;
        }
        Ok(out.bytes)
    }
}

struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
impl Encoder {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        self.bytes
            .len()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limit)
            .ok_or_else(|| invalid("freeze", "bundle", "bundle byte limit exceeded"))?;
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| invalid("freeze", "bundle", "bundle allocation failed"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

/// Reimport a source snapshot with no filesystem/network reads. This intentionally rejects
/// any version other than the current one; there is no legacy migration path.
pub fn import_frozen(
    context: ImportContext,
    bytes: &[u8],
    max_bundle_bytes: usize,
    limits: &ImportLimits,
) -> Result<Scene> {
    // Publish before digest verification or parsing as well as the importer's own start.
    let began = std::time::Instant::now();
    log::info!(
        "start #{} gltf_unfreeze source={}",
        context.request,
        context.source
    );
    let result = decode(context, bytes, max_bundle_bytes, limits);
    if let Err(error) = &result {
        let cause: String = error.to_string().chars().take(1024).collect();
        log::error!(
            "gltf_unfreeze #{} source={} elapsed_ms={} cause={}",
            context.request,
            context.source,
            began.elapsed().as_millis(),
            cause
        );
    } else {
        log::debug!(
            "done #{} gltf_unfreeze source={} elapsed_ms={}",
            context.request,
            context.source,
            began.elapsed().as_millis()
        );
    }
    result
}

fn decode(
    context: ImportContext,
    bytes: &[u8],
    max_bundle_bytes: usize,
    limits: &ImportLimits,
) -> Result<Scene> {
    if bytes.len() > max_bundle_bytes {
        return Err(invalid("unfreeze", "bundle", "bundle byte limit exceeded"));
    }
    let mut cursor = Cursor { bytes, position: 0 };
    if cursor.take(8)? != MAGIC {
        return Err(invalid(
            "unfreeze",
            "version",
            "unsupported scene bundle version",
        ));
    }
    let source_len = cursor.length()?;
    if source_len > limits.max_source_bytes {
        return Err(invalid("unfreeze", "source", "source byte limit exceeded"));
    }
    let expected = cursor.take(32)?;
    let source = cursor.take(source_len)?;
    if Sha256::digest(source).as_slice() != expected {
        return Err(invalid("unfreeze", "source", "source digest mismatch"));
    }
    let count = cursor.u32()? as usize;
    if count > limits.max_items.saturating_mul(2) {
        return Err(invalid(
            "unfreeze",
            "resources",
            "resource count limit exceeded",
        ));
    }
    let mut resources = BTreeMap::new();
    let mut total = 0usize;
    for id in 0..count {
        let path_len = cursor.u32()? as usize;
        if path_len > 4096 {
            return Err(invalid("unfreeze", id, "resource locator length exceeded"));
        }
        let path = std::str::from_utf8(cursor.take(path_len)?)
            .map_err(|_| invalid("unfreeze", id, "resource locator is not UTF-8"))?;
        relative_path_checked(path).map_err(|reason| invalid("unfreeze", id, reason))?;
        let length = cursor.length()?;
        total = total
            .checked_add(length)
            .filter(|&n| n <= limits.max_retained_bytes)
            .ok_or_else(|| invalid("unfreeze", id, "total resource byte limit exceeded"))?;
        if length > limits.max_resource_bytes {
            return Err(invalid("unfreeze", id, "resource byte limit exceeded"));
        }
        let expected = cursor.take(32)?;
        let data = cursor.take(length)?;
        if Sha256::digest(data).as_slice() != expected {
            return Err(invalid("unfreeze", id, "resource digest mismatch"));
        }
        if resources.insert(path, data).is_some() {
            return Err(invalid("unfreeze", id, "duplicate resource locator"));
        }
    }
    if cursor.position != bytes.len() {
        return Err(invalid("unfreeze", "bundle", "trailing bundle bytes"));
    }
    import(context, source, &mut OwnedResolver(resources), limits)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .filter(|&n| n <= self.bytes.len())
            .ok_or_else(|| {
                invalid(
                    "unfreeze",
                    "bundle",
                    "truncated or overflowing bundle range",
                )
            })?;
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn length(&mut self) -> Result<usize> {
        let b = self.take(8)?;
        let n = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        usize::try_from(n)
            .map_err(|_| invalid("unfreeze", "length", "length cannot fit address space"))
    }
}
struct OwnedResolver<'a>(BTreeMap<&'a str, &'a [u8]>);
impl AssetResolver for OwnedResolver<'_> {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        let bytes = self.0.get(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "resource absent from frozen bundle",
            )
        })?;
        if bytes.len() > limit {
            return Err(io::Error::other("frozen resource byte limit exceeded"));
        }
        Ok(bytes.to_vec())
    }
}
