//! Bounded container sample durations, independent of libav's FPS inference.
use motion_host::Result;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
};

const MAX_METADATA: u64 = 64 * 1024 * 1024;
const MAX_SAMPLES: u32 = motion_core::MAX_VIDEO_FRAMES;
const MAX_DEPTH: u8 = 8;

#[derive(Clone, Copy)]
struct Atom {
    kind: [u8; 4],
    start: u64,
    body: u64,
    end: u64,
}
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
struct Reader<'a> {
    file: Box<dyn ReadSeek>,
    length: u64,
    metadata: u64,
    atoms: u32,
    check: &'a dyn Fn() -> Result<()>,
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(format!("invalid MOV sample timing: {message}"))
    }
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset.checked_add(4).ok_or("MOV offset overflow")?)
        .ok_or("truncated MOV timing field")?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}
fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    let bytes = data
        .get(offset..offset.checked_add(8).ok_or("MOV offset overflow")?)
        .ok_or("truncated MOV timing field")?;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}
fn unique(atoms: &[Atom], kind: &[u8; 4]) -> Result<Option<Atom>> {
    let mut found = atoms.iter().filter(|atom| &atom.kind == kind);
    let result = found.next().copied();
    require(found.next().is_none(), "duplicate timing box")?;
    Ok(result)
}
fn mandatory(atoms: &[Atom], kind: &[u8; 4]) -> Result<Atom> {
    unique(atoms, kind)?
        .ok_or_else(|| format!("missing MOV timing box: {}", String::from_utf8_lossy(kind)))
}
impl Reader<'_> {
    fn read(&mut self, offset: u64, bytes: usize) -> Result<Vec<u8>> {
        (self.check)()?;
        self.metadata = self
            .metadata
            .checked_add(bytes as u64)
            .ok_or("MOV metadata size overflow")?;
        require(self.metadata <= MAX_METADATA, "metadata budget exceeded")?;
        require(
            offset
                .checked_add(bytes as u64)
                .is_some_and(|end| end <= self.length),
            "field exceeds source file",
        )?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut data = vec![0; bytes];
        for block in data.chunks_mut(64 * 1024) {
            (self.check)()?;
            self.file
                .read_exact(block)
                .map_err(|e| format!("read MOV timing: {e}"))?;
        }
        Ok(data)
    }
    fn children(&mut self, start: u64, end: u64, depth: u8) -> Result<Vec<Atom>> {
        require(
            depth <= MAX_DEPTH && start <= end && end <= self.length,
            "invalid box depth/range",
        )?;
        let mut offset = start;
        let mut atoms = Vec::new();
        while offset < end {
            require(end - offset >= 8, "truncated box header")?;
            self.atoms = self.atoms.checked_add(1).ok_or("MOV box count overflow")?;
            require(self.atoms <= MAX_SAMPLES, "box count budget exceeded")?;
            let header = self.read(offset, 8)?;
            let mut size = u64::from(u32_at(&header, 0)?);
            let kind = header[4..8].try_into().unwrap();
            let mut header_size = 8;
            if size == 1 {
                require(end - offset >= 16, "truncated extended box header")?;
                size = u64_at(&self.read(offset + 8, 8)?, 0)?;
                header_size = 16;
            } else if size == 0 {
                size = end - offset;
            }
            require(size >= header_size, "box smaller than its header")?;
            let next = offset.checked_add(size).ok_or("MOV box size overflow")?;
            require(next <= end, "box exceeds parent")?;
            atoms.push(Atom {
                kind,
                start: offset,
                body: offset + header_size,
                end: next,
            });
            offset = next;
        }
        Ok(atoms)
    }
    fn body(&mut self, atom: Atom) -> Result<Vec<u8>> {
        let length = atom.end - atom.body;
        require(length <= MAX_METADATA, "timing box exceeds metadata budget")?;
        self.read(
            atom.body,
            usize::try_from(length).map_err(|_| "MOV box size overflow")?,
        )
    }
}

#[derive(Clone, Copy, Default)]
struct Defaults {
    duration: u32,
    size: u32,
}
pub(crate) struct Duration {
    pub ticks: u64,
    pub timescale: u32,
}

impl Duration {
    /// Add the two native rational values before rounding once to microseconds.
    pub fn end_us(&self, pts: i64, numerator: i32, denominator: i32) -> Result<u64> {
        require(
            pts >= 0 && numerator > 0 && denominator > 0 && self.timescale > 0,
            "invalid time base",
        )?;
        let scale = i128::from(self.timescale);
        let denominator = i128::from(denominator);
        let divisor = denominator * scale;
        let ticks = i128::from(pts)
            .checked_mul(i128::from(numerator))
            .and_then(|v| v.checked_mul(scale))
            .and_then(|v| v.checked_add(i128::from(self.ticks) * denominator))
            .and_then(|v| v.checked_mul(1_000_000))
            .and_then(|v| v.checked_add(divisor / 2))
            .ok_or("video timestamp overflow")?;
        u64::try_from(ticks / divisor).map_err(|_| "video timestamp overflow".into())
    }
}

fn media_range(ranges: &[Atom], position: u64, size: u64) -> Result<()> {
    let index = ranges.partition_point(|atom| atom.body <= position);
    let end = position
        .checked_add(size)
        .ok_or("MOV sample position overflow")?;
    require(
        index > 0 && end <= ranges[index - 1].end,
        "sample is outside embedded media data (external references are disabled)",
    )
}
fn record(
    found: &mut Option<u64>,
    position: u64,
    size: u32,
    duration: u32,
    target: u64,
    bytes: usize,
) -> Result<()> {
    if position == target && usize::try_from(size).ok() == Some(bytes) {
        require(found.is_none(), "ambiguous sample position")?;
        require(duration > 0, "final sample duration is missing or zero")?;
        *found = Some(u64::from(duration));
    }
    Ok(())
}

struct Sizes {
    count: u32,
    constant: u32,
    values: Vec<u32>,
}
impl Sizes {
    fn at(&self, index: u32) -> Result<u32> {
        require(index < self.count, "chunk samples exceed size table")?;
        if self.constant != 0 {
            Ok(self.constant)
        } else {
            self.values
                .get(index as usize)
                .copied()
                .ok_or("missing MOV sample size".into())
        }
    }
}
fn sizes(data: &[u8], compact: bool) -> Result<Sizes> {
    require(
        u32_at(data, 0)? == 0,
        "unsupported size-table flags/version",
    )?;
    let count = u32_at(data, 8)?;
    require(count <= MAX_SAMPLES, "sample count budget exceeded")?;
    if compact {
        let width = *data.get(7).ok_or("truncated compact size table")?;
        require(
            matches!(width, 4 | 8 | 16),
            "invalid compact sample-size width",
        )?;
        let required = 12 + (count as usize * width as usize).div_ceil(8);
        require(data.len() == required, "compact size table length mismatch")?;
        let mut values = Vec::with_capacity(count as usize);
        for index in 0..count as usize {
            values.push(match width {
                4 => u32::from((data[12 + index / 2] >> if index % 2 == 0 { 4 } else { 0 }) & 15),
                8 => u32::from(data[12 + index]),
                _ => u32::from(u16::from_be_bytes(
                    data[12 + index * 2..14 + index * 2].try_into().unwrap(),
                )),
            });
        }
        Ok(Sizes {
            count,
            constant: 0,
            values,
        })
    } else {
        let constant = u32_at(data, 4)?;
        require(
            data.len() == 12 + if constant == 0 { count as usize * 4 } else { 0 },
            "size table length mismatch",
        )?;
        let values = if constant == 0 {
            (0..count as usize)
                .map(|index| u32_at(data, 12 + index * 4))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Sizes {
            count,
            constant,
            values,
        })
    }
}
fn table(data: &[u8], width: usize) -> Result<u32> {
    require(
        u32_at(data, 0)? == 0,
        "unsupported sample-table flags/version",
    )?;
    let count = u32_at(data, 4)?;
    require(count <= MAX_SAMPLES, "table entry budget exceeded")?;
    require(
        data.len() == 8 + count as usize * width,
        "table length mismatch",
    )?;
    Ok(count)
}
fn static_samples(
    reader: &mut Reader<'_>,
    atoms: &[Atom],
    media: &[Atom],
    target: u64,
    bytes: usize,
    found: &mut Option<u64>,
) -> Result<u32> {
    let (size_atom, compact) = match (unique(atoms, b"stsz")?, unique(atoms, b"stz2")?) {
        (Some(atom), None) => (atom, false),
        (None, Some(atom)) => (atom, true),
        _ => return Err("missing or conflicting MOV sample-size table".into()),
    };
    let sizes = sizes(&reader.body(size_atom)?, compact)?;
    let stts = reader.body(mandatory(atoms, b"stts")?)?;
    let runs = table(&stts, 8)?;
    let mut durations = Vec::with_capacity(runs as usize);
    let mut sample_total = 0u32;
    for index in 0..runs as usize {
        let count = u32_at(&stts, 8 + index * 8)?;
        require(count > 0, "empty duration run")?;
        sample_total = sample_total
            .checked_add(count)
            .ok_or("MOV sample count overflow")?;
        require(sample_total <= MAX_SAMPLES, "sample count budget exceeded")?;
        durations.push((count, u32_at(&stts, 12 + index * 8)?));
    }
    require(
        sample_total == sizes.count,
        "duration and size sample counts disagree",
    )?;
    if let Some(ctts) = unique(atoms, b"ctts")? {
        let data = reader.body(ctts)?;
        let flags = u32_at(&data, 0)?;
        require(
            flags == 0 || flags == 0x01000000,
            "unsupported composition-table version/flags",
        )?;
        let count = u32_at(&data, 4)?;
        require(
            count <= MAX_SAMPLES && data.len() == 8 + count as usize * 8,
            "composition table length/count mismatch",
        )?;
        let mut total = 0u32;
        for index in 0..count as usize {
            let run = u32_at(&data, 8 + index * 8)?;
            require(run > 0, "empty composition run")?;
            total = total
                .checked_add(run)
                .ok_or("MOV composition count overflow")?;
        }
        require(
            total == sizes.count,
            "composition and size sample counts disagree",
        )?;
    }
    let stsc = reader.body(mandatory(atoms, b"stsc")?)?;
    let count = table(&stsc, 12)?;
    let mut chunks = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let first = u32_at(&stsc, 8 + index * 12)?;
        let samples = u32_at(&stsc, 12 + index * 12)?;
        require(
            first > 0 && samples > 0 && u32_at(&stsc, 16 + index * 12)? > 0,
            "invalid sample-to-chunk entry",
        )?;
        require(
            chunks.last().is_none_or(|(previous, _)| first > *previous),
            "unordered sample-to-chunk entries",
        )?;
        chunks.push((first, samples));
    }
    let (offset_atom, width) = match (unique(atoms, b"stco")?, unique(atoms, b"co64")?) {
        (Some(atom), None) => (atom, 4),
        (None, Some(atom)) => (atom, 8),
        _ => return Err("missing or conflicting MOV chunk offsets".into()),
    };
    let offsets = reader.body(offset_atom)?;
    let count = table(&offsets, width)?;
    if sizes.count == 0 {
        require(
            count == 0 && chunks.is_empty(),
            "empty sample table has chunks",
        )?;
        return Ok(0);
    }
    require(
        count > 0 && chunks.first().is_some_and(|entry| entry.0 == 1),
        "sample-to-chunk table must start at chunk one",
    )?;
    require(
        chunks.last().is_some_and(|entry| entry.0 <= count),
        "sample-to-chunk entry exceeds chunk count",
    )?;
    let (mut chunk_run, mut duration_run, mut remaining, mut sample) =
        (0usize, 0usize, durations[0].0, 0u32);
    for chunk in 1..=count {
        (reader.check)()?;
        if chunk_run + 1 < chunks.len() && chunks[chunk_run + 1].0 == chunk {
            chunk_run += 1;
        }
        let offset = 8 + (chunk as usize - 1) * width;
        let mut position = if width == 4 {
            u64::from(u32_at(&offsets, offset)?)
        } else {
            u64_at(&offsets, offset)?
        };
        for _ in 0..chunks[chunk_run].1 {
            if sample % 1024 == 0 {
                (reader.check)()?;
            }
            let size = sizes.at(sample)?;
            media_range(media, position, u64::from(size))?;
            record(
                found,
                position,
                size,
                durations[duration_run].1,
                target,
                bytes,
            )?;
            position = position
                .checked_add(u64::from(size))
                .ok_or("MOV sample position overflow")?;
            sample += 1;
            remaining -= 1;
            if remaining == 0 && sample < sizes.count {
                duration_run += 1;
                remaining = durations[duration_run].0;
            }
        }
    }
    require(
        sample == sizes.count,
        "chunks do not cover the sample table",
    )?;
    Ok(sample)
}

fn fragments(
    reader: &mut Reader<'_>,
    moof: Atom,
    media: &[Atom],
    defaults: &BTreeMap<u32, Defaults>,
    selected: u32,
    target: u64,
    bytes: usize,
    found: &mut Option<u64>,
    samples: &mut u32,
) -> Result<()> {
    let children = reader.children(moof.body, moof.end, 1)?;
    let mut previous_end = moof.start;
    for traf in children.iter().filter(|atom| &atom.kind == b"traf") {
        let atoms = reader.children(traf.body, traf.end, 2)?;
        let data = reader.body(mandatory(&atoms, b"tfhd")?)?;
        let flags = u32_at(&data, 0)?;
        require(
            flags & !0x03003b == 0,
            "unsupported fragment-header flags/version",
        )?;
        let empty = flags & 0x010000 != 0;
        let track = u32_at(&data, 4)?;
        let mut default = defaults.get(&track).copied().unwrap_or_default();
        let mut offset = 8;
        let base = if flags & 1 != 0 {
            let value = u64_at(&data, offset)?;
            offset += 8;
            value
        } else if flags & 0x020000 != 0 {
            moof.start
        } else {
            previous_end
        };
        if flags & 2 != 0 {
            require(
                u32_at(&data, offset)? > 0,
                "invalid fragment sample description",
            )?;
            offset += 4;
        }
        if flags & 8 != 0 {
            default.duration = u32_at(&data, offset)?;
            offset += 4;
        }
        if flags & 16 != 0 {
            default.size = u32_at(&data, offset)?;
            offset += 4;
        }
        if flags & 32 != 0 {
            u32_at(&data, offset)?;
            offset += 4;
        }
        require(offset == data.len(), "fragment-header length mismatch")?;
        let mut cursor = base;
        for run in atoms.iter().filter(|atom| &atom.kind == b"trun") {
            let data = reader.body(*run)?;
            let flags = u32_at(&data, 0)?;
            require(
                flags >> 24 <= 1 && flags & 0x00ffffff & !0x000f05 == 0,
                "unsupported fragment-run flags/version",
            )?;
            let count = u32_at(&data, 4)?;
            require(
                count <= MAX_SAMPLES,
                "fragment sample count budget exceeded",
            )?;
            if track == selected {
                *samples = samples
                    .checked_add(count)
                    .ok_or("MOV fragment count overflow")?;
                require(*samples <= MAX_SAMPLES, "sample count budget exceeded")?;
            }
            require(!empty || count == 0, "empty fragment contains samples")?;
            require(flags & 0x404 != 0x404, "conflicting fragment sample flags")?;
            let mut offset = 8;
            if flags & 1 != 0 {
                let delta = i64::from(u32_at(&data, offset)? as i32);
                cursor = u64::try_from(i128::from(base) + i128::from(delta))
                    .map_err(|_| "MOV fragment data offset overflow")?;
                offset += 4;
            }
            if flags & 4 != 0 {
                u32_at(&data, offset)?;
                offset += 4;
            }
            let width = (flags & 0xf00).count_ones() as usize * 4;
            require(
                data.len() == offset + count as usize * width,
                "fragment-run length mismatch",
            )?;
            for index in 0..count {
                if index % 1024 == 0 {
                    (reader.check)()?;
                }
                let duration = if flags & 0x100 != 0 {
                    let value = u32_at(&data, offset)?;
                    offset += 4;
                    value
                } else {
                    default.duration
                };
                let size = if flags & 0x200 != 0 {
                    let value = u32_at(&data, offset)?;
                    offset += 4;
                    value
                } else {
                    default.size
                };
                if flags & 0x400 != 0 {
                    offset += 4;
                }
                if flags & 0x800 != 0 {
                    offset += 4;
                }
                media_range(media, cursor, u64::from(size))?;
                if track == selected {
                    record(found, cursor, size, duration, target, bytes)?;
                }
                cursor = cursor
                    .checked_add(u64::from(size))
                    .ok_or("MOV fragment sample offset overflow")?;
            }
        }
        previous_end = cursor;
    }
    Ok(())
}

/// Locate the selected packet's original sample, including fragmented movies.
/// `check` includes the caller's cancellation and timestamp-scan deadline.
pub(crate) fn sample_duration(
    path: &Path,
    track: u32,
    position: u64,
    bytes: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<Duration> {
    check()?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut reader = Reader {
        file: Box::new(file),
        length,
        metadata: 0,
        atoms: 0,
        check,
    };
    let top = reader.children(0, length, 0)?;
    let media: Vec<_> = top
        .iter()
        .filter(|atom| &atom.kind == b"mdat")
        .copied()
        .collect();
    require(!media.is_empty(), "media data missing")?;
    let moov = mandatory(&top, b"moov")?;
    let mut moov = reader.children(moov.body, moov.end, 1)?;
    let mut expanded = if let Some(cmov) = unique(&moov, b"cmov")? {
        require(moov.len() == 1, "compressed movie has conflicting metadata")?;
        let atoms = reader.children(cmov.body, cmov.end, 2)?;
        require(
            reader.body(mandatory(&atoms, b"dcom")?)? == b"zlib",
            "unsupported movie compression",
        )?;
        let data = reader.body(mandatory(&atoms, b"cmvd")?)?;
        let expected = u64::from(u32_at(&data, 0)?);
        require(
            expected > 0 && expected <= MAX_METADATA,
            "expanded metadata budget exceeded",
        )?;
        reader.metadata = reader
            .metadata
            .checked_add(expected)
            .ok_or("MOV metadata size overflow")?;
        require(reader.metadata <= MAX_METADATA, "metadata budget exceeded")?;
        let mut decoder = flate2::read::ZlibDecoder::new(&data[4..]);
        let mut output = Vec::with_capacity(expected as usize);
        let mut block = [0u8; 64 * 1024];
        loop {
            check()?;
            let count = decoder
                .read(&mut block)
                .map_err(|e| format!("invalid compressed MOV metadata: {e}"))?;
            if count == 0 {
                break;
            }
            require(
                output.len() as u64 + count as u64 <= expected,
                "expanded metadata exceeds declared length",
            )?;
            output.extend_from_slice(&block[..count]);
        }
        require(
            output.len() as u64 == expected && decoder.total_in() == (data.len() - 4) as u64,
            "compressed metadata length mismatch",
        )?;
        Some(Reader {
            file: Box::new(Cursor::new(output)),
            length: expected,
            metadata: reader.metadata,
            atoms: reader.atoms,
            check,
        })
    } else {
        None
    };
    let compressed = expanded.is_some();
    let metadata = expanded.as_mut().unwrap_or(&mut reader);
    if compressed {
        moov = metadata.children(0, metadata.length, 2)?;
        if let Some(atom) = unique(&moov, b"moov")? {
            require(moov.len() == 1, "expanded movie has conflicting metadata")?;
            moov = metadata.children(atom.body, atom.end, 3)?;
        }
        require(
            unique(&moov, b"cmov")?.is_none(),
            "nested movie compression unsupported",
        )?;
    }
    let mut chosen = None;
    for trak in moov.iter().filter(|atom| &atom.kind == b"trak") {
        let atoms = metadata.children(trak.body, trak.end, 3)?;
        let tkhd = metadata.body(mandatory(&atoms, b"tkhd")?)?;
        let version = *tkhd.first().ok_or("truncated track header")?;
        require(version <= 1, "unsupported track header version")?;
        if u32_at(&tkhd, if version == 0 { 12 } else { 20 })? != track {
            continue;
        }
        require(chosen.is_none(), "duplicate track identity")?;
        // libav maps edits into packet PTS. Durations remain in media units;
        // retain its existing edit-rate handling rather than reinterpreting PTS.
        if let Some(edts) = unique(&atoms, b"edts")? {
            let edits = metadata.children(edts.body, edts.end, 4)?;
            if let Some(elst) = unique(&edits, b"elst")? {
                let data = metadata.body(elst)?;
                let version = *data.first().ok_or("truncated edit list")?;
                require(version <= 1, "unsupported edit-list version")?;
                let count = u32_at(&data, 4)?;
                require(count <= MAX_SAMPLES, "edit count budget exceeded")?;
                let width = if version == 0 { 12 } else { 20 };
                require(
                    data.len() == 8 + count as usize * width,
                    "edit-list length mismatch",
                )?;
            }
        }
        let mdia = mandatory(&atoms, b"mdia")?;
        let mdia = metadata.children(mdia.body, mdia.end, 4)?;
        let mdhd = metadata.body(mandatory(&mdia, b"mdhd")?)?;
        let version = *mdhd.first().ok_or("truncated media header")?;
        require(version <= 1, "unsupported media header version")?;
        let timescale = u32_at(&mdhd, if version == 0 { 12 } else { 20 })?;
        require(timescale > 0, "media timescale is zero")?;
        let minf = mandatory(&mdia, b"minf")?;
        let minf = metadata.children(minf.body, minf.end, 5)?;
        let stbl = mandatory(&minf, b"stbl")?;
        let stbl = metadata.children(stbl.body, stbl.end, 6)?;
        chosen = Some((timescale, stbl));
    }
    let (timescale, stbl) = chosen.ok_or("selected MOV track identity is missing")?;
    let mut found = None;
    let mut samples = static_samples(metadata, &stbl, &media, position, bytes, &mut found)?;
    let mut defaults = BTreeMap::new();
    if let Some(mvex) = unique(&moov, b"mvex")? {
        let atoms = metadata.children(mvex.body, mvex.end, 3)?;
        for trex in atoms.iter().filter(|atom| &atom.kind == b"trex") {
            let data = metadata.body(*trex)?;
            require(
                data.len() == 24 && u32_at(&data, 0)? == 0,
                "invalid fragment defaults",
            )?;
            let id = u32_at(&data, 4)?;
            require(
                defaults
                    .insert(
                        id,
                        Defaults {
                            duration: u32_at(&data, 12)?,
                            size: u32_at(&data, 16)?,
                        },
                    )
                    .is_none(),
                "duplicate fragment defaults",
            )?;
        }
    }
    if let Some(expanded) = expanded {
        reader.metadata = expanded.metadata;
        reader.atoms = expanded.atoms;
    }
    for moof in top.iter().filter(|atom| &atom.kind == b"moof") {
        fragments(
            &mut reader,
            *moof,
            &media,
            &defaults,
            track,
            position,
            bytes,
            &mut found,
            &mut samples,
        )?;
    }
    let ticks = found.ok_or("selected MOV packet does not match its sample tables")?;
    Ok(Duration { ticks, timescale })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../motion-media/tests/fixtures/video")
            .join(name)
    }
    fn words(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }
    fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [
            words(&[body.len() as u32 + 8]),
            kind.to_vec(),
            body.to_vec(),
        ]
        .concat()
    }
    fn movie(stbl: Vec<u8>, defaults: bool) -> Vec<u8> {
        let tkhd = atom(b"tkhd", &words(&[0, 0, 0, 7]));
        let mdhd = atom(b"mdhd", &words(&[0, 0, 0, 90000]));
        let minf = atom(b"minf", &atom(b"stbl", &stbl));
        let mdia = atom(b"mdia", &[mdhd, minf].concat());
        let trak = atom(b"trak", &[tkhd, mdia].concat());
        let mvex = if defaults {
            atom(b"mvex", &atom(b"trex", &words(&[0, 7, 1, 902, 5, 0])))
        } else {
            Vec::new()
        };
        atom(b"moov", &[trak, mvex].concat())
    }
    fn read_bytes(data: &[u8], track: u32, pos: u64, bytes: usize) -> Result<Duration> {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(data).unwrap();
        sample_duration(file.path(), track, pos, bytes, &|| Ok(()))
    }

    #[test]
    fn real_sample_tables_match_packet_positions_for_edits_b_frames_fragments_and_cmov() {
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture("manifest.json")).unwrap()).unwrap();
        for (name, pos, size, duration, scale, pts, expected) in [
            (
                "video-delayed.mp4",
                3801,
                23,
                41667,
                1000000,
                3208333,
                3250000,
            ),
            ("video-ctts.mp4", 3801, 23, 20001, 1000000, 2978333, 2998334),
            (
                "video-fragmented.mp4",
                5736,
                23,
                20001,
                1000000,
                2978333,
                2998334,
            ),
            ("video-tail-90k.mp4", 3549, 23, 902, 90000, 291000, 3243356),
            ("video-tail-cmov.mov", 3549, 23, 902, 90000, 291000, 3243356),
        ] {
            let timing = sample_duration(&fixture(name), 1, pos, size, &|| Ok(())).unwrap();
            assert_eq!(timing.ticks, duration, "{name}");
            assert_eq!(timing.timescale, scale, "{name}");
            assert_eq!(
                timing.end_us(pts, 1, scale as i32).unwrap(),
                expected,
                "{name}"
            );
            assert_eq!(manifest[name]["video_end_us"], expected, "{name}");
        }
    }

    #[test]
    fn compact_sizes_and_64_bit_chunk_offsets_map_the_exact_sample() {
        let stts = atom(b"stts", &words(&[0, 1, 2, 11]));
        let stsc = atom(b"stsc", &words(&[0, 1, 1, 2, 1]));
        let stz2 = atom(b"stz2", &[words(&[0, 4, 2]), vec![0x23]].concat());
        let co64 = atom(
            b"co64",
            &[words(&[0, 1]), 8u64.to_be_bytes().to_vec()].concat(),
        );
        let data = [
            atom(b"mdat", &[1; 5]),
            movie([stts, stsc, stz2, co64].concat(), false),
        ]
        .concat();
        assert_eq!(read_bytes(&data, 7, 10, 3).unwrap().ticks, 11);
        assert!(read_bytes(&data, 7, 10, 2).is_err());
        assert!(read_bytes(&data, 8, 10, 3).is_err());
    }

    #[test]
    fn fragment_duration_comes_from_trun_tfhd_or_trex_without_packet_guessing() {
        let empty = [
            atom(b"stts", &words(&[0, 0])),
            atom(b"stsc", &words(&[0, 0])),
            atom(b"stsz", &words(&[0, 0, 0])),
            atom(b"stco", &words(&[0, 0])),
        ]
        .concat();
        for (tfhd_duration, trun_duration, expected) in [
            (None, None, 902),
            (Some(777), None, 777),
            (Some(777), Some(666), 666),
        ] {
            let moov = movie(empty.clone(), true);
            let tfhd = atom(
                b"tfhd",
                &words(&if let Some(duration) = tfhd_duration {
                    vec![0x20008, 7, duration]
                } else {
                    vec![0x20000, 7]
                }),
            );
            let make = |offset| {
                let mut fields = vec![if trun_duration.is_some() { 0x101 } else { 1 }, 1, offset];
                if let Some(duration) = trun_duration {
                    fields.push(duration);
                }
                atom(
                    b"moof",
                    &atom(
                        b"traf",
                        &[tfhd.clone(), atom(b"trun", &words(&fields))].concat(),
                    ),
                )
            };
            let moof = make(make(0).len() as u32 + 8);
            let pos = (moov.len() + moof.len() + 8) as u64;
            let data = [moov, moof, atom(b"mdat", &[1; 5])].concat();
            assert_eq!(read_bytes(&data, 7, pos, 5).unwrap().ticks, expected);
            let mut bad = data.clone();
            let index = bad.windows(4).position(|bytes| bytes == b"tfhd").unwrap();
            bad[index + 5] |= 1; // duration-is-empty, with one sample
            assert!(read_bytes(&bad, 7, pos, 5).is_err());
        }
    }

    #[test]
    fn corrupted_counts_offsets_and_compressed_lengths_fail_explicitly() {
        let original = std::fs::read(fixture("video-ctts.mp4")).unwrap();
        for kind in [b"stts", b"ctts", b"stco"] {
            let mut bad = original.clone();
            let index = bad.windows(4).position(|bytes| bytes == kind).unwrap();
            let field = index + 12; // first run count or first chunk offset
            bad[field..field + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(read_bytes(&bad, 1, 3801, 23).is_err(), "{kind:?}");
        }
        let original = std::fs::read(fixture("video-tail-cmov.mov")).unwrap();
        let index = original
            .windows(4)
            .position(|bytes| bytes == b"cmvd")
            .unwrap()
            + 4;
        for length in [1, (MAX_METADATA + 1) as u32] {
            let mut bad = original.clone();
            bad[index..index + 4].copy_from_slice(&length.to_be_bytes());
            assert!(read_bytes(&bad, 1, 3549, 23).is_err());
        }
        assert!(read_bytes(&original[..original.len() - 1], 1, 3549, 23).is_err());
    }

    #[test]
    fn cancellation_interrupts_metadata_and_sample_mapping() {
        let calls = std::cell::Cell::new(0);
        let check = || {
            calls.set(calls.get() + 1);
            if calls.get() >= 12 {
                Err("cancelled during timing read".into())
            } else {
                Ok(())
            }
        };
        assert_eq!(
            sample_duration(&fixture("video-fragmented.mp4"), 1, 5736, 23, &check)
                .err()
                .unwrap(),
            "cancelled during timing read"
        );
    }

    #[test]
    fn mixed_native_time_bases_round_the_sum_once_and_reject_overflow() {
        let timing = Duration {
            ticks: 902,
            timescale: 90000,
        };
        assert_eq!(timing.end_us(291000, 1, 90000).unwrap(), 3243356);
        assert_eq!(timing.end_us(3233333, 1, 1000000).unwrap(), 3243355);
        assert!(timing.end_us(-1, 1, 90000).is_err());
        assert!(timing.end_us(1, 1, 0).is_err());
        assert!(Duration {
            ticks: u64::MAX,
            timescale: 1
        }
        .end_us(i64::MAX, i32::MAX, 1)
        .is_err());
    }
}
