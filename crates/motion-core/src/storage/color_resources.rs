//! Closed, byte-verified OCIO resources. All IO happens during preparation.
use crate::{
    color_management::{ColorBundleResource, OcioProject, ProjectColorError},
    Error, Result,
};
use motion_effects::color_config::{ContentIdentity, VerifiedColorConfig};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};

fn io(resource: &ColorBundleResource, phase: &'static str, source: std::io::Error) -> Error {
    ProjectColorError::Io {
        phase,
        path: resource.path.clone(),
        source,
    }
    .into()
}
fn invalid(resource: &ColorBundleResource, phase: &'static str, reason: &'static str) -> Error {
    ProjectColorError::Resource {
        phase,
        path: resource.path.clone(),
        reason,
    }
    .into()
}

/// Verify and rewind the opened handle. The later package writer also verifies
/// the bytes it copies, because an opened file is not an immutable disk lease.
pub fn open_color_resource(root: &Path, resource: &ColorBundleResource) -> Result<File> {
    super::validate_relative_path(&resource.path)?;
    let limit = motion_effects::color_config::MAX_RESOURCE_BYTES;
    if !resource.path.starts_with("assets/color/")
        || resource.identity.bytes == 0
        || resource.identity.bytes > limit
    {
        return Err(invalid(
            resource,
            "descriptor",
            "invalid managed resource identity",
        ));
    }
    let base = root.canonicalize().map_err(|e| io(resource, "root", e))?;
    let path = base
        .join(&resource.path)
        .canonicalize()
        .map_err(|e| io(resource, "resolve", e))?;
    if !path.starts_with(&base) {
        return Err(invalid(
            resource,
            "resolve",
            "resource resolves outside project directory",
        ));
    }
    let mut file = File::open(path).map_err(|e| io(resource, "open", e))?;
    let before = file.metadata().map_err(|e| io(resource, "metadata", e))?;
    if !before.is_file() || before.len() != resource.identity.bytes as u64 {
        return Err(invalid(
            resource,
            "metadata",
            "resource size differs from declared identity",
        ));
    }
    let mut hash = Sha256::new();
    let mut copied = 0usize;
    let mut buffer = [0; super::TRANSFER_BUFFER_BYTES];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| io(resource, "verify", e))?;
        if n == 0 {
            break;
        }
        copied = copied.saturating_add(n);
        if copied > resource.identity.bytes {
            return Err(invalid(
                resource,
                "verify",
                "resource grew during verification",
            ));
        }
        hash.update(&buffer[..n]);
    }
    let after = file
        .metadata()
        .map_err(|e| io(resource, "metadata_after", e))?;
    if copied != resource.identity.bytes
        || before.len() != after.len()
        || before
            .modified()
            .map_err(|e| io(resource, "modified_before", e))?
            != after
                .modified()
                .map_err(|e| io(resource, "modified_after", e))?
        || format!("{:x}", hash.finalize()) != resource.identity.sha256
    {
        return Err(invalid(
            resource,
            "verify",
            "resource content differs from declared identity",
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io(resource, "rewind", e))?;
    Ok(file)
}

fn read(root: &Path, resource: &ColorBundleResource) -> Result<Arc<[u8]>> {
    let file = open_color_resource(root, resource)?;
    let mut bytes = Vec::with_capacity(resource.identity.bytes);
    file.take(resource.identity.bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io(resource, "read", e))?;
    if ContentIdentity::of(&bytes) != resource.identity {
        return Err(invalid(
            resource,
            "read",
            "resource changed after verification",
        ));
    }
    Ok(bytes.into())
}

/// Snapshot exact verified bytes for processor preparation; no OCIO syntax or
/// GPU support is inferred from successful content verification.
pub fn read_color_config(root: &Path, settings: &OcioProject) -> Result<VerifiedColorConfig> {
    let resources = settings.resources()?;
    let config = read(root, &resources[0])?;
    let mut files = BTreeMap::new();
    for (name, resource) in settings.manifest.resources.keys().zip(&resources[1..]) {
        files.insert(name.clone(), read(root, resource)?);
    }
    VerifiedColorConfig::new(settings.manifest.clone(), config, files)
        .map_err(|e| ProjectColorError::Config(e).into())
}
