//! Hash actual frozen bytes, independently of queue ranges and live file paths.
use super::*;
use sha2::{Digest, Sha256};

struct HashWriter<'a> {
    digest: Sha256,
    bytes: u64,
    limit: u64,
    control: &'a ExportControl,
}
impl<'a> HashWriter<'a> {
    fn new(limit: u64, control: &'a ExportControl) -> Self {
        Self {
            digest: Sha256::new(),
            bytes: 0,
            limit,
            control,
        }
    }
    fn finish(self) -> (u64, [u8; 32]) {
        (self.bytes, self.digest.finalize().into())
    }
}
impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.control.check().map_err(std::io::Error::other)?;
        let count = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|count| *count <= self.limit)
            .ok_or_else(|| {
                std::io::Error::other("prerender content exceeds snapshot byte limit")
            })?;
        self.digest.update(bytes);
        self.bytes = count;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn record(digest: &mut Sha256, label: &[u8], (bytes, hash): (u64, [u8; 32])) {
    digest.update((label.len() as u64).to_le_bytes());
    digest.update(label);
    digest.update(bytes.to_le_bytes());
    digest.update(hash);
}
fn files(root: &Path, control: &ExportControl) -> Result<Vec<PathBuf>> {
    // Iterative traversal bounds directory fanout/depth as well as file count.
    let mut pending = vec![(PathBuf::new(), 0usize)];
    let mut result = Vec::new();
    let mut visited = 0usize;
    while let Some((relative, depth)) = pending.pop() {
        control.check()?;
        for entry in fs::read_dir(root.join(&relative))
            .map_err(|e| format!("read frozen source directory: {e}"))?
        {
            control.check()?;
            visited += 1;
            if visited > super::super::durable::MAX_FILES || depth > 64 {
                return Err("prerender snapshot entry/depth limit exceeded".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let path = relative.join(entry.file_name());
            if kind.is_file() {
                result.push(path);
            } else if kind.is_dir() {
                pending.push((path, depth + 1));
            } else {
                return Err("prerender snapshot contains a non-regular entry".into());
            }
        }
    }
    result.sort();
    Ok(result)
}

pub(super) fn context(
    frozen: &FrozenMovie,
    document_generation: u64,
    device_generation: u64,
    control: &ExportControl,
) -> Result<motion_render::CompositionFrameContext> {
    control.check()?;
    let mut digest = Sha256::new();
    digest.update(b"motion-studio/native-prerender-content/v1\0");
    let mut project = HashWriter::new(super::super::durable::MAX_MANIFEST, control);
    serde_json::to_writer(&mut project, &frozen.project)
        .map_err(|e| format!("hash frozen document: {e}"))?;
    record(&mut digest, b"project", project.finish());
    let mut total = 0u64;
    let mut buffer = vec![0; motion_core::storage::TRANSFER_BUFFER_BYTES];
    for relative in files(frozen.assets.path(), control)? {
        control.check()?;
        let path = relative
            .to_str()
            .ok_or("frozen source name is not UTF-8")?
            .replace('\\', "/");
        let mut input = fs::File::open(frozen.assets.path().join(&relative))
            .map_err(|e| format!("open frozen source for hashing: {e}"))?;
        let mut file = HashWriter::new(
            motion_core::storage::MAX_PACKAGE.saturating_sub(total),
            control,
        );
        loop {
            control.check()?;
            let count = input
                .read(&mut buffer)
                .map_err(|e| format!("hash frozen source: {e}"))?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
        }
        let hashed = file.finish();
        total = total
            .checked_add(hashed.0)
            .ok_or("frozen source size overflow")?;
        record(&mut digest, format!("source/{path}").as_bytes(), hashed);
    }
    for dependency in &frozen.project.plugin_dependencies {
        control.check()?;
        let package = frozen
            .registry
            .resolve(&dependency.plugin, &dependency.version, &dependency.hash)
            .map_err(|e| format!("hash frozen effect: {e}"))?;
        let mut writer = HashWriter::new(
            motion_core::storage::MAX_PACKAGE.saturating_sub(total),
            control,
        );
        // No retained package-sized buffer and cancellation between bounded chunks.
        for chunk in package.bytes.chunks(buffer.len()) {
            writer.write_all(chunk).map_err(|e| e.to_string())?;
        }
        let hashed = writer.finish();
        total = total
            .checked_add(hashed.0)
            .ok_or("frozen effect size overflow")?;
        record(
            &mut digest,
            format!("effect/{}", dependency.plugin).as_bytes(),
            hashed,
        );
    }
    let mut depth = HashWriter::new(super::super::durable::MAX_DEPTH_PAYLOAD, control);
    frozen
        .depth
        .write_to(&mut depth, &|| control.check().map_err(Into::into))
        .map_err(|e| format!("hash frozen depth: {e}"))?;
    record(&mut digest, b"depth", depth.finish());
    let mut output = Sha256::new();
    // A fixed full-resolution authored-camera profile. Observer/preview modes
    // must get a different profile before a foreground consumer can use them.
    output.update(b"motion-studio/native-prerender-output/full-authored-linear-premultiplied/v1\0");
    output.update(frozen.project.width.to_le_bytes());
    output.update(frozen.project.height.to_le_bytes());
    output.update([frozen.project.working_precision as u8]);
    Ok(motion_render::CompositionFrameContext {
        document_generation,
        device_generation,
        content: digest.finalize().into(),
        output: output.finalize().into(),
    })
}
