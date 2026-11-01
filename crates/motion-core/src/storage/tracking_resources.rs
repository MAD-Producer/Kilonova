//! Complete original-document identity; tracking parsing stays in the host.
use crate::{ensure, Error, Result, TrackingAsset};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn io(phase: &'static str, asset: &TrackingAsset, source: std::io::Error) -> Error {
    Error::TrackingResourceIo {
        phase,
        path: asset.path.clone(),
        source,
    }
}

/// Verify and rewind a bounded managed document, without a payload-sized buffer.
pub fn open_tracking_resource(root: &Path, asset: &TrackingAsset) -> Result<File> {
    asset.validate()?;
    let base = root.canonicalize().map_err(|e| io("root", asset, e))?;
    let path = base
        .join(&asset.path)
        .canonicalize()
        .map_err(|e| io("resolve", asset, e))?;
    ensure(
        path.starts_with(&base),
        "tracking resource resolves outside project",
    )?;
    let mut file = File::open(path).map_err(|e| io("open", asset, e))?;
    let before = file.metadata().map_err(|e| io("metadata", asset, e))?;
    ensure(
        before.is_file() && before.len() == asset.bytes,
        &format!("tracking resource size mismatch: {}", asset.path),
    )?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; super::TRANSFER_BUFFER_BYTES];
    loop {
        let n = file.read(&mut buffer).map_err(|e| io("verify", asset, e))?;
        if n == 0 {
            break;
        }
        read += n as u64;
        ensure(
            read <= asset.bytes,
            &format!("tracking resource grew during verification: {}", asset.path),
        )?;
        hash.update(&buffer[..n]);
    }
    let after = file
        .metadata()
        .map_err(|e| io("metadata_after", asset, e))?;
    ensure(
        read == asset.bytes
            && before.len() == after.len()
            && before
                .modified()
                .map_err(|e| io("modified_before", asset, e))?
                == after
                    .modified()
                    .map_err(|e| io("modified_after", asset, e))?
            && format!("{:x}", hash.finalize()) == asset.sha256,
        &format!("tracking resource content mismatch: {}", asset.path),
    )?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io("rewind", asset, e))?;
    Ok(file)
}

/// Recheck after read because the verified handle is not an immutable disk lease.
pub fn read_tracking_resource(root: &Path, asset: &TrackingAsset) -> Result<Vec<u8>> {
    let file = open_tracking_resource(root, asset)?;
    let mut bytes = Vec::new();
    file.take(asset.bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io("read", asset, e))?;
    ensure(
        bytes.len() as u64 == asset.bytes && super::resource_sha256(&bytes) == asset.sha256,
        &format!(
            "tracking resource changed after verification: {}",
            asset.path
        ),
    )?;
    Ok(bytes)
}
