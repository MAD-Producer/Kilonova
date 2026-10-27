//! Bounded, content-verified files owned by editable node effects.
use crate::{ensure, Error, NodeResource, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

fn io(phase: &'static str, path: &str, source: std::io::Error) -> Error {
    Error::NodeResourceIo {
        phase,
        path: path.into(),
        source,
    }
}
fn validate(resource: &NodeResource) -> Result<()> {
    resource.validate(if resource.path.ends_with(".png") {
        "png"
    } else {
        "msfx"
    })
}
pub fn resource_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Return a verified, rewound handle. Hashing uses a fixed transfer buffer, not
/// a payload-sized allocation during project save or archive snapshot creation.
pub fn open_node_resource(root: &Path, resource: &NodeResource) -> Result<File> {
    validate(resource)?;
    let base = root
        .canonicalize()
        .map_err(|e| io("root", &resource.path, e))?;
    let path = base
        .join(&resource.path)
        .canonicalize()
        .map_err(|e| io("resolve", &resource.path, e))?;
    ensure(
        path.starts_with(&base),
        "node resource resolves outside project",
    )?;
    let mut file = File::open(path).map_err(|e| io("open", &resource.path, e))?;
    let before = file
        .metadata()
        .map_err(|e| io("metadata", &resource.path, e))?;
    ensure(
        before.is_file() && before.len() == resource.bytes,
        &format!("node resource size mismatch: {}", resource.path),
    )?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; super::TRANSFER_BUFFER_BYTES];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| io("verify", &resource.path, e))?;
        if n == 0 {
            break;
        }
        read += n as u64;
        ensure(
            read <= resource.bytes,
            "node resource grew during verification",
        )?;
        hash.update(&buffer[..n]);
    }
    let after = file
        .metadata()
        .map_err(|e| io("metadata_after", &resource.path, e))?;
    ensure(
        read == resource.bytes
            && before.len() == after.len()
            && before
                .modified()
                .map_err(|e| io("modified_before", &resource.path, e))?
                == after
                    .modified()
                    .map_err(|e| io("modified_after", &resource.path, e))?
            && format!("{:x}", hash.finalize()) == resource.sha256,
        &format!("node resource content mismatch: {}", resource.path),
    )?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io("rewind", &resource.path, e))?;
    Ok(file)
}
pub fn read_node_resource(root: &Path, resource: &NodeResource) -> Result<Vec<u8>> {
    let file = open_node_resource(root, resource)?;
    let mut bytes = Vec::new();
    file.take(resource.bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io("read", &resource.path, e))?;
    ensure(
        bytes.len() as u64 == resource.bytes && resource_sha256(&bytes) == resource.sha256,
        &format!(
            "node resource changed after verification: {}",
            resource.path
        ),
    )?;
    Ok(bytes)
}

/// Keeps only files exclusively created by this transaction. The caller retains
/// them after the model revision commits, even if a subsequent snapshot fails.
pub struct NodeResourceStage {
    root: PathBuf,
    paths: Vec<PathBuf>,
}
impl NodeResourceStage {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            paths: Vec::new(),
        })
    }
    pub fn stage(&mut self, resource: &NodeResource, bytes: &[u8]) -> Result<()> {
        validate(resource)?;
        ensure(
            bytes.len() as u64 == resource.bytes && resource_sha256(bytes) == resource.sha256,
            "node resource staging identity mismatch",
        )?;
        let directory = self.root.join("assets");
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io("directory", &resource.path, e)),
        }
        ensure(
            directory.canonicalize()?.starts_with(&self.root) && directory.is_dir(),
            "node resource directory resolves outside project",
        )?;
        let path = self.root.join(&resource.path);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                open_node_resource(&self.root, resource)?;
                return Ok(());
            }
            Err(e) => return Err(io("create", &resource.path, e)),
        };
        self.paths.push(path);
        file.write_all(bytes)
            .map_err(|e| io("write", &resource.path, e))?;
        file.sync_all().map_err(|e| io("sync", &resource.path, e))?;
        Ok(())
    }
    pub fn keep_resources(mut self) {
        self.paths.clear();
    }
}
impl Drop for NodeResourceStage {
    fn drop(&mut self) {
        for path in &self.paths {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::error!(
                        "node resource cleanup failed phase=remove path={} cause={}",
                        path.to_string_lossy().chars().take(384).collect::<String>(),
                        error.to_string().chars().take(512).collect::<String>()
                    );
                }
            }
        }
    }
}
