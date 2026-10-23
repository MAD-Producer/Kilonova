//! Validate compact timing arithmetic before the native demuxer expands it.
//! Inspect every track, including unselected ones initialized by the reader.
use super::{find, u32be, Result};

pub(super) fn validate(mdia: &[u8], track: usize) -> Result<()> {
    let Some(minf) = find(mdia, b"minf")? else {
        return Ok(());
    };
    let Some(stbl) = find(minf, b"stbl")? else {
        return Ok(());
    };
    let Some(table) = find(stbl, b"stts")? else {
        return Ok(());
    };
    let failure = |reason| {
        crate::Error::from(format!(
            "invalid MP4 sample clock track={track} atom=stts phase=preflight: {reason}"
        ))
    };
    if table.len() < 8 {
        return Err(failure("truncated timing header"));
    }
    let entries = u32be(&table[4..])?;
    let expected = usize::try_from(entries)
        .ok()
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(8));
    if expected != Some(table.len()) {
        return Err(failure("timing entry count/length mismatch"));
    }
    let mut duration = 0u64;
    let mut samples = 0u32;
    for entry in table[8..].chunks_exact(8) {
        let count = u32be(entry)?;
        let delta = u32be(&entry[4..])?;
        duration = duration
            .checked_add(u64::from(count) * u64::from(delta))
            .ok_or_else(|| failure("duration overflow"))?;
        samples = samples
            .checked_add(count)
            .ok_or_else(|| failure("sample count overflow"))?;
    }
    // A compact run must describe actual packets, not an enormous first run
    // that can overflow the reader's per-sample index arithmetic.
    if let Some(sizes) = find(stbl, b"stsz")? {
        if sizes.len() < 12 || u32be(&sizes[8..])? != samples {
            return Err(failure("timing and size sample counts disagree"));
        }
    }
    Ok(())
}
