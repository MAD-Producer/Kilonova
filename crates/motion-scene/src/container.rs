use crate::{invalid, Result};

pub(crate) fn container(source: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    if !source.starts_with(b"glTF") {
        return Ok((source, None));
    }
    let read = |offset: usize| -> Result<u32> {
        let b = source
            .get(offset..offset + 4)
            .ok_or_else(|| invalid("container", "header", "truncated GLB header"))?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if read(4)? != 2 || read(8)? as usize != source.len() {
        return Err(invalid(
            "container",
            "header",
            "GLB version/length mismatch",
        ));
    }
    let mut position = 12usize;
    let mut json = None;
    let mut bin = None;
    let mut chunk_index = 0;
    while position < source.len() {
        let length = read(position)? as usize;
        let kind = read(position + 4)?;
        let start = position
            .checked_add(8)
            .ok_or_else(|| invalid("container", "chunk", "range overflow"))?;
        let end = start
            .checked_add(length)
            .filter(|&n| n <= source.len())
            .ok_or_else(|| invalid("container", "chunk", "chunk exceeds GLB"))?;
        if !length.is_multiple_of(4) || (chunk_index == 0 && kind != 0x4e4f534a) {
            return Err(invalid(
                "container",
                "chunk",
                "invalid chunk alignment/order",
            ));
        }
        match kind {
            0x4e4f534a if json.is_none() && chunk_index == 0 => json = Some(&source[start..end]),
            0x004e4942 if bin.is_none() && chunk_index == 1 => bin = Some(&source[start..end]),
            0x4e4f534a | 0x004e4942 => {
                return Err(invalid(
                    "container",
                    "chunk",
                    "duplicate or misplaced known chunk",
                ))
            }
            _ => {} // glTF clients must ignore unknown chunk types.
        }
        position = end;
        chunk_index += 1;
    }
    Ok((
        json.ok_or_else(|| invalid("container", "JSON", "missing JSON chunk"))?,
        bin,
    ))
}
