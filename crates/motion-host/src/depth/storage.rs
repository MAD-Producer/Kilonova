//! Session-scoped scalar spill storage. The owner only reads bounded metadata;
//! all file creation, writes and verified reads belong to the depth worker.
use super::*;
use motion_model::depth::{DepthDescriptor, MAX_DEPTH_RESOURCE_BYTES, MAX_VIDEO_DEPTH_SAMPLES};
use std::{collections::BTreeMap, fs::File, io, path::Path};

pub(crate) const MAX_STORED_BYTES: u64 = 2 << 30;
const CHUNK: usize = 64 << 10;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    pub samples: usize,
    /// Physical append extent, including superseded samples. Never exceeds the
    /// disk allowance; it is reset with the document-scoped Store.
    pub bytes: u64,
    /// Saturating lifetime capacity failures for this Store, reset on disposal.
    pub rejections: u64,
}

#[derive(Default)]
struct Index {
    entries: BTreeMap<(u64, u64), Arc<Sample>>,
    stats: Stats,
}
#[derive(Default)]
pub(crate) struct Spool {
    file: Option<Arc<File>>,
    extent: u64,
}
pub(crate) struct Store {
    // Never acquire this worker-only lock from the Session owner.
    writer: Mutex<Spool>,
    index: Mutex<Index>,
    byte_limit: u64,
    sample_limit: usize,
    directory: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub(crate) struct Plane {
    file: Arc<File>,
    offset: u64,
    bytes: u64,
    digest: [u8; 32],
    pub descriptor: DepthDescriptor,
    pub provider: String,
    costs: Costs,
    validity: bool,
    confidence: bool,
}
#[derive(Debug)]
pub(crate) struct Sample {
    pub request: u64,
    pub key: Key,
    pub input: PreparedDepthInput,
    pub end_us: u64,
    pub plane: Arc<Plane>,
}

impl Store {
    pub fn new() -> Self {
        Self::with_limits(MAX_STORED_BYTES, MAX_VIDEO_DEPTH_SAMPLES)
    }
    pub fn with_directory(directory: std::path::PathBuf) -> Self {
        Self {
            directory: Some(directory),
            ..Self::new()
        }
    }
    fn with_limits(byte_limit: u64, sample_limit: usize) -> Self {
        Self {
            writer: Mutex::new(Spool::default()),
            index: Mutex::new(Index::default()),
            byte_limit,
            sample_limit,
            directory: None,
        }
    }
    pub fn stats(&self) -> Result<Stats> {
        Ok(self.index.lock().map_err(|_| Error::State)?.stats)
    }
    /// No IO, hashing or plane allocation on the Session thread.
    pub fn lookup(&self, key: &VideoKey) -> Result<Option<Arc<Sample>>> {
        let index = self.index.lock().map_err(|_| Error::State)?;
        Ok(index
            .entries
            .range((key.object, 0)..=(key.object, key.target_us))
            .next_back()
            .map(|(_, sample)| sample)
            .filter(|s| {
                key.target_us < s.end_us
                    && s.key.source.asset == key.asset
                    && s.key.source.revision == key.revision
                    && s.key.geometry == key.geometry
                    && s.key.model == key.model
            })
            .cloned())
    }
    pub fn append(
        &self,
        root: &Path,
        request: u64,
        object: u64,
        input: &Input,
        fingerprint: PreparedDepthInput,
        estimate: &Estimate,
        cancel: &Cancellation,
    ) -> Result<()> {
        self.append_checked(root, request, object, input, fingerprint, estimate, &|| {
            cancel.check()
        })
    }
    fn append_checked(
        &self,
        root: &Path,
        request: u64,
        object: u64,
        input: &Input,
        fingerprint: PreparedDepthInput,
        estimate: &Estimate,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        check()?;
        estimate.validate(input.key())?;
        let end_us = input
            .video_end_us
            .ok_or(Error::Invalid("stored sample requires video interval"))?;
        let start_us = input.key.source.pts;
        if start_us.timescale != 1_000_000 || start_us.ticks < 0 || end_us <= start_us.ticks as u64
        {
            return Err(Error::Invalid("invalid stored video interval"));
        }
        fingerprint.validate(&input.key.geometry)?;
        let resource = &estimate.resource;
        let bytes = (resource.values().len() as u64)
            * (4 + u64::from(resource.validity().is_some())
                + 4 * u64::from(resource.confidence().is_some()));
        let id = (object, start_us.ticks as u64);
        let mut writer = self.writer.lock().map_err(|_| Error::State)?;
        let required_bytes = writer
            .extent
            .checked_add(bytes)
            .ok_or(Error::Invalid("stored depth extent overflow"))?;
        {
            let mut index = self.index.lock().map_err(|_| Error::State)?;
            if required_bytes > self.byte_limit
                || (!index.entries.contains_key(&id) && index.entries.len() >= self.sample_limit)
            {
                index.stats.rejections = index.stats.rejections.saturating_add(1);
                return Err(Error::StorageCapacity {
                    request,
                    object,
                    required_bytes,
                    byte_limit: self.byte_limit,
                    sample_limit: self.sample_limit,
                });
            }
        }
        let plane = writer.append(
            Some(self.directory.as_deref().unwrap_or(root)),
            estimate,
            check,
        )?;
        let sample = Arc::new(Sample {
            request,
            key: input.key.clone(),
            input: fingerprint,
            end_us,
            plane,
        });
        // The index lock never spans file IO. Publish only a complete sample;
        // its file Arc remains valid after Store/Session teardown.
        let mut index = self.index.lock().map_err(|_| Error::State)?;
        index.entries.insert(id, sample);
        writer.extent = required_bytes;
        index.stats.samples = index.entries.len();
        index.stats.bytes = required_bytes;
        Ok(())
    }
}

impl Spool {
    /// Used by streaming snapshot restoration on the export worker. All planes
    /// share one anonymous file, rather than keeping one open handle per frame.
    pub fn append_for_export(
        &mut self,
        estimate: &Estimate,
        check: &dyn Fn() -> crate::Result<()>,
    ) -> crate::Result<Arc<Plane>> {
        export_control(check, |checked| self.append(None, estimate, checked))
    }
    fn append(
        &mut self,
        root: Option<&Path>,
        estimate: &Estimate,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Arc<Plane>> {
        check()?;
        let resource = &estimate.resource;
        let bytes = resource.values().len() as u64
            * (4 + u64::from(resource.validity().is_some())
                + 4 * u64::from(resource.confidence().is_some()));
        let end = self
            .extent
            .checked_add(bytes)
            .filter(|n| *n <= MAX_STORED_BYTES)
            .ok_or(Error::Invalid("frozen depth spool exceeds 2 GiB"))?;
        if self.file.is_none() {
            self.file = Some(Arc::new(match root {
                Some(root) => tempfile::tempfile_in(root)?,
                None => tempfile::tempfile()?,
            }));
        }
        let file = self.file.as_ref().ok_or(Error::State)?.clone();
        let offset = self.extent;
        let result = (|| {
            let mut out = Output {
                file: &file,
                offset,
                hash: Sha256::new(),
            };
            out.floats(resource.values(), check)?;
            if let Some(mask) = resource.validity() {
                out.bytes(mask, check)?;
            }
            if let Some(scores) = resource.confidence() {
                out.floats(scores, check)?;
            }
            check()?;
            Ok(Arc::new(Plane {
                file: file.clone(),
                offset,
                bytes,
                digest: out.hash.finalize().into(),
                descriptor: resource.descriptor().clone(),
                provider: estimate.provider.clone(),
                costs: estimate.costs.clone(),
                validity: resource.validity().is_some(),
                confidence: resource.confidence().is_some(),
            }))
        })();
        match result {
            Ok(plane) => {
                self.extent = end;
                Ok(plane)
            }
            Err(primary) => match file.set_len(offset) {
                Ok(()) => Err(primary),
                Err(cleanup) => Err(Error::Cleanup {
                    primary: Box::new(primary),
                    cleanup,
                }),
            },
        }
    }
}

// Callback failures are protocol values. Preserve the first original failure
// across the depth IO boundary, including a second cleanup/closure failure.
fn export_control<T>(
    check: &dyn Fn() -> crate::Result<()>,
    run: impl FnOnce(&dyn Fn() -> Result<()>) -> Result<T>,
) -> crate::Result<T> {
    let primary = std::cell::RefCell::new(None);
    let result = run(&|| match check() {
        Ok(()) => Ok(()),
        Err(cause) => {
            if primary.borrow().is_none() {
                *primary.borrow_mut() = Some(cause);
            }
            Err(Error::Canceled)
        }
    });
    if let Some(cause) = primary.into_inner() {
        if let Err(Error::Cleanup { cleanup, .. }) = &result {
            log::warn!(
                "operation=frozen_depth phase=rollback cause={}",
                cleanup.to_string().chars().take(512).collect::<String>()
            );
        }
        return Err(cause);
    }
    result.map_err(crate::Error::from)
}

impl Sample {
    pub fn restore(
        &self,
        actual: &Input,
        fingerprint: PreparedDepthInput,
        cancel: &Cancellation,
    ) -> Result<Estimate> {
        if !self.key.same_sample(actual.key())
            || self.input != fingerprint
            || actual.video_end_us != Some(self.end_us)
        {
            return Err(Error::Invalid(
                "stored video depth RGB fingerprint or indexed interval changed",
            ));
        }
        self.plane.load(cancel)
    }
}

impl Plane {
    pub fn byte_len(&self) -> u64 {
        self.bytes
    }
    pub fn validity(&self) -> bool {
        self.validity
    }
    pub fn confidence(&self) -> bool {
        self.confidence
    }
    pub fn load_for_export(
        &self,
        check: &dyn Fn() -> crate::Result<()>,
    ) -> crate::Result<Estimate> {
        export_control(check, |checked| self.load_checked(checked))
    }
    pub fn write_to(
        &self,
        writer: &mut dyn io::Write,
        check: &dyn Fn() -> crate::Result<()>,
    ) -> crate::Result<()> {
        export_control(check, |checked| {
            let mut input = Reader {
                file: &self.file,
                offset: self.offset,
                hash: Sha256::new(),
            };
            let mut buffer = [0; CHUNK];
            let mut remaining = self.bytes;
            while remaining > 0 {
                let count = remaining.min(CHUNK as u64) as usize;
                input.bytes(&mut buffer[..count], checked)?;
                checked()?;
                writer.write_all(&buffer[..count])?;
                remaining -= count as u64;
            }
            checked()?;
            let digest: [u8; 32] = input.hash.finalize().into();
            if digest != self.digest {
                return Err(Error::Invalid("stored depth plane SHA-256 mismatch"));
            }
            Ok(())
        })
    }
    pub fn load(&self, cancel: &Cancellation) -> Result<Estimate> {
        self.load_checked(&|| cancel.check())
    }
    pub fn load_checked(&self, check: &dyn Fn() -> Result<()>) -> Result<Estimate> {
        check()?;
        let pixels = self.descriptor.validate()?;
        let bytes = pixels
            .checked_mul(4 + usize::from(self.validity) + 4 * usize::from(self.confidence))
            .ok_or(Error::Invalid("stored depth plane size overflow"))?;
        if bytes > MAX_DEPTH_RESOURCE_BYTES || bytes as u64 != self.bytes {
            return Err(Error::Invalid(
                "stored depth plane exceeds memory allowance",
            ));
        }
        let mut input = Reader {
            file: &self.file,
            offset: self.offset,
            hash: Sha256::new(),
        };
        let values = input.floats(pixels, check)?;
        let validity = if self.validity {
            let mut mask = vec![0; pixels];
            input.bytes(&mut mask, check)?;
            Some(mask)
        } else {
            None
        };
        let confidence = if self.confidence {
            Some(input.floats(pixels, check)?)
        } else {
            None
        };
        check()?;
        let digest: [u8; 32] = input.hash.finalize().into();
        if digest != self.digest {
            return Err(Error::Invalid("stored depth plane SHA-256 mismatch"));
        }
        Ok(Estimate {
            resource: DepthResource::new(self.descriptor.clone(), values, validity, confidence)?,
            provider: self.provider.clone(),
            costs: self.costs.clone(),
        })
    }
}

// Positional IO gives each immutable plane an independent cursor. File::try_clone
// alone would share a seek position on Unix and corrupt concurrent snapshot reads.
#[cfg(windows)]
fn read_at(file: &File, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, bytes, offset)
}
#[cfg(unix)]
fn read_at(file: &File, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, bytes, offset)
}
#[cfg(windows)]
fn write_at(file: &File, bytes: &[u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_write(file, bytes, offset)
}
#[cfg(unix)]
fn write_at(file: &File, bytes: &[u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::write_at(file, bytes, offset)
}
struct Output<'a> {
    file: &'a File,
    offset: u64,
    hash: Sha256,
}
impl Output<'_> {
    fn bytes(&mut self, bytes: &[u8], check: &dyn Fn() -> Result<()>) -> Result<()> {
        for mut chunk in bytes.chunks(CHUNK) {
            while !chunk.is_empty() {
                check()?;
                match write_at(self.file, chunk, self.offset) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                    Ok(count) => {
                        self.hash.update(&chunk[..count]);
                        self.offset += count as u64;
                        chunk = &chunk[count..];
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
    fn floats(&mut self, values: &[f32], check: &dyn Fn() -> Result<()>) -> Result<()> {
        let mut bytes = [0u8; CHUNK];
        for chunk in values.chunks(CHUNK / 4) {
            for (value, out) in chunk.iter().zip(bytes.chunks_exact_mut(4)) {
                out.copy_from_slice(&value.to_le_bytes());
            }
            self.bytes(&bytes[..chunk.len() * 4], check)?;
        }
        Ok(())
    }
}
struct Reader<'a> {
    file: &'a File,
    offset: u64,
    hash: Sha256,
}
impl Reader<'_> {
    fn bytes(&mut self, bytes: &mut [u8], check: &dyn Fn() -> Result<()>) -> Result<()> {
        for mut chunk in bytes.chunks_mut(CHUNK) {
            while !chunk.is_empty() {
                check()?;
                match read_at(self.file, chunk, self.offset) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                    Ok(count) => {
                        self.hash.update(&chunk[..count]);
                        self.offset += count as u64;
                        chunk = &mut chunk[count..];
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
    fn floats(&mut self, count: usize, check: &dyn Fn() -> Result<()>) -> Result<Vec<f32>> {
        let mut values = vec![0.; count];
        let mut bytes = [0u8; CHUNK];
        for chunk in values.chunks_mut(CHUNK / 4) {
            self.bytes(&mut bytes[..chunk.len() * 4], check)?;
            for (out, value) in chunk.iter_mut().zip(bytes.chunks_exact(4)) {
                *out = f32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            }
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests;
