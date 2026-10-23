//! Validate allocation counts, chunk mapping arithmetic and file extents before
//! the native demuxer builds sample tables or allocates packet buffers.
use super::{find, u32be, u64be, Result};

fn failure(track: usize, atom: &str, reason: &str) -> crate::Error {
    format!("invalid MP4 sample layout track={track} atom={atom} phase=preflight: {reason}").into()
}

fn table<'a>(
    stbl: &'a [u8],
    atom: &[u8; 4],
    name: &'static str,
    width: usize,
    track: usize,
) -> Result<Option<(&'a [u8], u32)>> {
    let Some(body) = find(stbl, atom)? else {
        return Ok(None);
    };
    if body.len() < 8 {
        return Err(failure(track, name, "truncated table header"));
    }
    let count = u32be(&body[4..])?;
    let expected = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(width))
        .and_then(|n| n.checked_add(8));
    if expected != Some(body.len()) {
        return Err(failure(track, name, "entry count/length mismatch"));
    }
    Ok(Some((&body[8..], count)))
}

pub(super) fn validate(mdia: &[u8], track: usize, file_len: u64) -> Result<()> {
    let Some(minf) = find(mdia, b"minf")? else {
        return Ok(());
    };
    let Some(stbl) = find(minf, b"stbl")? else {
        return Ok(());
    };
    let sizes = if let Some(body) = find(stbl, b"stsz")? {
        if body.len() < 12 {
            return Err(failure(track, "stsz", "truncated size header"));
        }
        let constant = u32be(&body[4..])?;
        let count = u32be(&body[8..])?;
        let expected = if constant != 0 {
            Some(12)
        } else {
            usize::try_from(count)
                .ok()
                .and_then(|n| n.checked_mul(4))
                .and_then(|n| n.checked_add(12))
        };
        if expected != Some(body.len()) {
            return Err(failure(track, "stsz", "entry count/length mismatch"));
        }
        Some((&body[12..], constant, count))
    } else {
        None
    };

    let mapping = table(stbl, b"stsc", "stsc", 12, track)?;
    if let Some((entries, _)) = mapping {
        let mut previous = None;
        let mut sample = 0u32;
        for entry in entries.chunks_exact(12) {
            let chunk = u32be(entry)?;
            let per_chunk = u32be(&entry[4..])?;
            if let Some((last, per)) = previous {
                let span = chunk
                    .checked_sub(last)
                    .filter(|n| *n != 0)
                    .ok_or_else(|| failure(track, "stsc", "chunk indices must increase"))?;
                sample = span
                    .checked_mul(per)
                    .and_then(|n| sample.checked_add(n))
                    .ok_or_else(|| failure(track, "stsc", "sample index overflow"))?;
            } else if chunk != 1 {
                return Err(failure(track, "stsc", "first chunk must start at one"));
            }
            if per_chunk == 0 || u32be(&entry[8..])? == 0 {
                return Err(failure(
                    track,
                    "stsc",
                    "sample count and description must be nonzero",
                ));
            }
            previous = Some((chunk, per_chunk));
        }
    }
    let narrow = table(stbl, b"stco", "stco", 4, track)?;
    let wide = table(stbl, b"co64", "co64", 8, track)?;
    if narrow.is_some() && wide.is_some() {
        return Err(failure(track, "stco", "multiple chunk offset tables"));
    }
    let offsets = narrow
        .map(|t| (t, 4, "stco"))
        .or_else(|| wide.map(|t| (t, 8, "co64")));
    let (
        Some((entries, count)),
        Some((values, constant, samples)),
        Some(((offsets, chunks), width, atom)),
    ) = (mapping, sizes, offsets)
    else {
        // Optional/absent tables remain the native reader's responsibility,
        // including fragmented streams. Available tables were checked above.
        return Ok(());
    };
    if count == 0 {
        return if chunks == 0 && samples == 0 {
            Ok(())
        } else {
            Err(failure(
                track,
                "stsc",
                "empty mapping for nonempty samples or chunks",
            ))
        };
    }
    let mut sample = 0u32;
    for (index, entry) in entries.chunks_exact(12).enumerate() {
        // The preceding pass established one-based strictly increasing indices.
        let first = u32be(entry)? - 1;
        let per_chunk = u32be(&entry[4..])?;
        let end = if index + 1 < count as usize {
            u32be(&entries[(index + 1) * 12..])? - 1
        } else {
            chunks
        };
        if first >= chunks || end > chunks {
            return Err(failure(track, "stsc", "chunk mapping outside offset table"));
        }
        for chunk in first..end {
            let next = sample
                .checked_add(per_chunk)
                .filter(|n| *n <= samples)
                .ok_or_else(|| failure(track, "stsc", "chunk and size sample counts disagree"))?;
            let bytes = if constant != 0 {
                u64::from(constant) * u64::from(per_chunk)
            } else {
                // The size table length bounds both this index and total work.
                values[sample as usize * 4..next as usize * 4]
                    .chunks_exact(4)
                    .try_fold(0u64, |total, size| {
                        total
                            .checked_add(u64::from(u32be(size)?))
                            .ok_or_else(|| failure(track, "stsz", "packet byte count overflow"))
                    })?
            };
            let at = chunk as usize * width;
            let offset = if width == 4 {
                u64::from(u32be(&offsets[at..])?)
            } else {
                u64be(&offsets[at..])?
            };
            if offset.checked_add(bytes).is_none_or(|end| end > file_len) {
                return Err(failure(track, atom, "chunk payload outside file"));
            }
            sample = next;
        }
    }
    if sample != samples {
        return Err(failure(
            track,
            "stsc",
            "chunk and size sample counts disagree",
        ));
    }
    Ok(())
}
