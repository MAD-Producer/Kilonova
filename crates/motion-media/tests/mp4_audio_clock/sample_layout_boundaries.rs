use super::*;

fn body(bytes: &[u8], kind: &[u8; 4]) -> Vec<u8> {
    let captured = std::cell::RefCell::new(None);
    metadata_case(bytes, kind, &|body| {
        assert!(captured.borrow().is_none());
        *captured.borrow_mut() = Some(body.to_vec());
        body.to_vec()
    });
    captured.into_inner().unwrap()
}

fn co64(bytes: &[u8]) -> Vec<u8> {
    metadata_case(bytes, b"stbl", &|table| {
        let mut out = Vec::new();
        let mut at = 0;
        while at < table.len() {
            let n = u32_at(table, at) as usize;
            let kind = &table[at + 4..at + 8];
            if kind == b"stco" {
                let original = &table[at + 8..at + n];
                let mut wide = original[..8].to_vec();
                for entry in original[8..].chunks_exact(4) {
                    wide.extend(u64::from(u32_at(entry, 0)).to_be_bytes());
                }
                out.extend(atom(b"co64", &wide));
            } else {
                out.extend(&table[at..at + n]);
            }
            at += n;
        }
        out
    })
}

#[test]
fn oversized_table_declarations_are_rejected_before_native_allocations() {
    let original = source();
    for (bytes, kind) in [
        (original.clone(), b"stsc"),
        (original.clone(), b"stco"),
        (co64(&original), b"co64"),
    ] {
        let modified = metadata_case(&bytes, kind, &|body| {
            let mut out = body.to_vec();
            set(&mut out, 4, u32::MAX);
            out
        });
        reject(
            "entry-allocation",
            &modified,
            std::str::from_utf8(kind).unwrap(),
            "entry count/length mismatch",
        );
    }
    let modified = metadata_case(&original, b"stts", &|body| {
        let mut out = body.to_vec();
        assert_eq!(u32_at(&out, 4), 1);
        set(&mut out, 8, u32::MAX);
        out
    });
    let modified = metadata_case(&modified, b"stsz", &|body| {
        let mut out = body.to_vec();
        set(&mut out, 8, u32::MAX);
        out
    });
    reject(
        "size-allocation",
        &modified,
        "stsz",
        "entry count/length mismatch",
    );
}

#[test]
fn variable_and_constant_packet_sizes_cannot_escape_the_actual_file() {
    let original = source();
    let variable = metadata_case(&original, b"stsz", &|body| {
        let mut out = body.to_vec();
        assert_eq!(u32_at(body, 4), 0);
        set(&mut out, 12, u32::MAX);
        out
    });
    reject(
        "variable-size",
        &variable,
        "stco",
        "chunk payload outside file",
    );
    let constant = metadata_case(&original, b"stsz", &|body| {
        let mut out = body[..12].to_vec();
        set(&mut out, 4, u32::MAX);
        out
    });
    reject(
        "constant-size",
        &constant,
        "stco",
        "chunk payload outside file",
    );
}

#[test]
fn narrow_and_wide_chunk_offsets_cannot_escape_or_wrap() {
    let original = source();
    let narrow = metadata_case(&original, b"stco", &|body| {
        let mut out = body.to_vec();
        set(&mut out, 8, u32::MAX);
        out
    });
    reject("offset-32", &narrow, "stco", "chunk payload outside file");
    let wide = metadata_case(&co64(&original), b"co64", &|body| {
        let mut out = body.to_vec();
        out[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
        out
    });
    reject("offset-64", &wide, "co64", "chunk payload outside file");
}

#[test]
fn mapping_counts_indices_and_descriptions_are_checked_against_real_tables() {
    for (entries, cause) in [
        (vec![(1, 94)], "chunk and size sample counts disagree"),
        (vec![(1, 96)], "chunk and size sample counts disagree"),
        (vec![(1, 1), (1, 94)], "chunk indices must increase"),
        (vec![(1, 1), (3, 94)], "chunk mapping outside offset table"),
        (vec![(1, 0)], "sample count and description must be nonzero"),
        (vec![], "empty mapping for nonempty samples or chunks"),
    ] {
        reject("mapping-contract", &mapping(&entries), "stsc", cause);
    }
    let modified = metadata_case(&source(), b"stsc", &|body| {
        let mut out = body.to_vec();
        set(&mut out, 16, 0);
        out
    });
    reject(
        "description-zero",
        &modified,
        "stsc",
        "sample count and description must be nonzero",
    );
}

#[test]
fn valid_multiple_runs_and_co64_preserve_every_decoded_pcm_byte() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let (reference, pcm) = decode(root.path(), "reference", &original, 4 << 20).unwrap();
    let sizes = body(&original, b"stsz");
    assert_eq!(u32_at(&sizes, 8), 95);
    let offset = u32_at(&body(&original, b"stco"), 8);
    let mut offsets = Vec::new();
    let mut next = offset;
    for sample in 0..95 {
        if sample == 0 || sample == 32 || sample == 64 {
            offsets.push(next);
        }
        next += u32_at(&sizes, 12 + sample * 4);
    }
    let split = metadata_case(&mapping(&[(1, 32), (3, 31)]), b"stco", &|_| {
        let mut out = vec![0; 4];
        out.extend(3u32.to_be_bytes());
        for offset in &offsets {
            out.extend(offset.to_be_bytes());
        }
        out
    });
    for (name, bytes) in [("split", split.clone()), ("split-wide", co64(&split))] {
        let (asset, actual) = decode(root.path(), name, &bytes, 4 << 20).unwrap();
        assert_eq!(asset.sample_frames, reference.sample_frames);
        assert_eq!(actual, pcm);
    }
}

#[test]
fn an_unselected_tracks_bad_mapping_is_rejected_before_reader_initialization() {
    let root = tempfile::tempdir().unwrap();
    let bad_track = atom(b"trak", &body(&mapping(&[(0, 95)]), b"trak"));
    let bad_track = metadata_case(&bad_track, b"tkhd", &|body| {
        let mut out = body.to_vec();
        set(&mut out, 12, 2);
        out
    });
    let modified = metadata_case(&source(), b"moov", &|body| {
        let mut out = body.to_vec();
        out.extend(&bad_track);
        out
    });
    reject_before_pcm(
        root.path(),
        "unselected-layout",
        &modified,
        "track=1 atom=stsc phase=preflight: first chunk must start at one",
    );
    let (asset, _) = decode(root.path(), "healthy", &source(), 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 96_000);
}
