use crate::{invalid, Cause, Error, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    Buffer,
    Image,
    MaterialLibrary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Relative locator for relinking; data/GLB resources have no external path.
    pub locator: Option<String>,
    pub sha256: [u8; 32],
    pub bytes: Vec<u8>,
}

impl Resource {
    pub(crate) fn new(locator: Option<String>, bytes: Vec<u8>) -> Self {
        Self {
            sha256: Sha256::digest(&bytes).into(),
            locator,
            bytes,
        }
    }
}

/// The resolver must honor `max_bytes` before allocating or reading an unbounded resource.
/// Receives a validated, decoded relative path, never a URL, data URI or traversal path.
pub trait AssetResolver {
    fn read(
        &mut self,
        relative_path: &str,
        kind: ResourceKind,
        max_bytes: usize,
    ) -> std::io::Result<Vec<u8>>;
}

/// Explicit local-directory import. No network, parent traversal or symlink escape.
/// Canonical confinement assumes the directory is not concurrently renamed by an adversary;
/// sandboxed callers can provide an already-owned resource-map resolver instead.
pub struct DirectoryResolver {
    root: PathBuf,
}

impl DirectoryResolver {
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().canonicalize()?;
        if !root.is_dir() {
            return Err(std::io::Error::other(
                "scene resource root is not a directory",
            ));
        }
        Ok(Self { root })
    }
}

impl AssetResolver for DirectoryResolver {
    fn read(
        &mut self,
        relative_path: &str,
        _kind: ResourceKind,
        max_bytes: usize,
    ) -> std::io::Result<Vec<u8>> {
        // Validate again: the trait is public and can be called outside the importer.
        let relative = relative_path_checked(relative_path).map_err(std::io::Error::other)?;
        let target = self.root.join(relative).canonicalize()?;
        if !target.starts_with(&self.root) {
            return Err(std::io::Error::other(
                "scene resource escapes import directory",
            ));
        }
        let before_open = std::fs::metadata(&target)?;
        if !before_open.is_file() || before_open.len() > max_bytes as u64 {
            return Err(std::io::Error::other(
                "scene resource is not a file or exceeds byte limit",
            ));
        }
        let file = File::open(&target)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > max_bytes as u64 {
            return Err(std::io::Error::other(
                "scene resource is not a file or exceeds byte limit",
            ));
        }
        let mut bytes = Vec::new();
        file.take((max_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(std::io::Error::other(
                "scene resource grew beyond byte limit",
            ));
        }
        Ok(bytes)
    }
}

pub(crate) fn relative_path_checked(path: &str) -> std::result::Result<&str, &'static str> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':', '?', '#', '\0'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("resource URI must be a bounded local relative path");
    }
    Ok(path)
}

pub(crate) fn decode_uri_path(uri: &str) -> Result<String> {
    // percent-encoding intentionally preserves malformed sequences; reject them first.
    let bytes = uri.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(invalid("resource", "uri", "invalid percent encoding"));
        }
    }
    let decoded = percent_encoding::percent_decode_str(uri)
        .decode_utf8()
        .map_err(|_| invalid("resource", "uri", "URI is not UTF-8"))?;
    relative_path_checked(&decoded).map_err(|reason| invalid("resource", "uri", reason))?;
    Ok(decoded.into_owned())
}

pub(crate) fn load_uri(
    uri: &str,
    kind: ResourceKind,
    limit: usize,
    resolver: &mut impl AssetResolver,
) -> Result<(Resource, Option<String>)> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (header, encoded) = data
            .split_once(',')
            .ok_or_else(|| invalid("resource", "data", "missing data URI separator"))?;
        let mime = header
            .strip_suffix(";base64")
            .ok_or_else(|| invalid("resource", "data", "only base64 data URIs are supported"))?;
        let allowed = match kind {
            ResourceKind::Buffer => {
                ["application/octet-stream", "application/gltf-buffer"].contains(&mime)
            }
            ResourceKind::Image => ["image/png", "image/jpeg"].contains(&mime),
            ResourceKind::MaterialLibrary => false,
        };
        if !allowed {
            return Err(invalid(
                "resource",
                "data",
                "unsupported data URI media type",
            ));
        }
        // Checked upper bound before decoder allocation. Account for base64 padding exactly.
        let padding = encoded
            .as_bytes()
            .iter()
            .rev()
            .take_while(|&&b| b == b'=')
            .count();
        let length = (encoded.len() / 4)
            .checked_mul(3)
            .and_then(|n| n.checked_sub(padding))
            .filter(|_| encoded.len().is_multiple_of(4) && padding <= 2)
            .ok_or_else(|| invalid("resource", "data", "invalid base64 length"))?;
        if length > limit {
            return Err(invalid("resource", "data", "resource byte limit exceeded"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| invalid("resource", "data", "invalid base64 payload"))?;
        return Ok((Resource::new(None, bytes), Some(mime.to_owned())));
    }
    if uri.len() > 4096 {
        return Err(invalid("resource", "uri", "URI length limit exceeded"));
    }
    let path = decode_uri_path(uri)?;
    let bytes = resolver.read(&path, kind, limit).map_err(|cause| Error {
        phase: "resource",
        object: path.clone(),
        cause: Cause::Io(cause),
    })?;
    if bytes.len() > limit {
        return Err(invalid("resource", path, "resolver exceeded byte limit"));
    }
    Ok((Resource::new(Some(path), bytes), None))
}

/// OBJ filenames are literal filesystem names, not percent-encoded URIs.
pub(crate) fn load_path(
    path: &str,
    kind: ResourceKind,
    limit: usize,
    resolver: &mut impl AssetResolver,
) -> Result<Resource> {
    relative_path_checked(path).map_err(|reason| invalid("resource", "path", reason))?;
    let bytes = resolver.read(path, kind, limit).map_err(|cause| Error {
        phase: "resource",
        object: path.to_owned(),
        cause: Cause::Io(cause),
    })?;
    if bytes.len() > limit {
        return Err(invalid("resource", path, "resolver exceeded byte limit"));
    }
    Ok(Resource::new(Some(path.to_owned()), bytes))
}
