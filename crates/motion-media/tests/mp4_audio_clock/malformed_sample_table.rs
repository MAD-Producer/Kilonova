use super::*;

fn sample_table(entries: &[(u32, u32)]) -> Vec<u8> {
    metadata_case(&source(), b"stts", &|_| {
        let mut body = vec![0; 4];
        body.extend(u32::try_from(entries.len()).unwrap().to_be_bytes());
        for (count, delta) in entries {
            body.extend(count.to_be_bytes());
            body.extend(delta.to_be_bytes());
        }
        body
    })
}

fn reject_without_unwind(name: &str, entries: &[(u32, u32)], cause: &str) {
    let root = tempfile::tempdir().unwrap();
    let bytes = sample_table(entries);
    let input = root.path().join(format!("{name}.m4a"));
    let pcm = root.path().join(format!("{name}.pcm"));
    let sentinel = b"a rejected clock must not truncate existing PCM";
    fs::write(&input, &bytes).unwrap();
    fs::write(&pcm, sentinel).unwrap();
    let result = std::panic::catch_unwind(|| {
        motion_media::decode_audio(&input, &pcm, None, 4 << 20, &mut |_| Ok(()))
    });
    assert!(result.is_ok(), "{name}: malformed media clock panicked");
    let error = result
        .unwrap()
        .expect_err("malformed sample table accepted");
    let text = error.to_string();
    assert!(text.contains(cause), "{name}: {text}");
    assert!(
        text.contains("track=0") && text.contains("atom=stts"),
        "{name}: {text}"
    );
    assert_eq!(fs::read(pcm).unwrap(), sentinel);
    assert_eq!(fs::read(input).unwrap(), bytes);
    let (asset, _) = decode(root.path(), "healthy", &source(), 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 96_000);
}

#[test]
fn duration_accumulator_overflow_returns_error_before_decoder_and_pcm() {
    reject_without_unwind(
        "duration-overflow",
        &[(u32::MAX, u32::MAX), (u32::MAX, u32::MAX)],
        "duration overflow",
    );
}

#[test]
fn sample_count_overflow_returns_error_before_decoder_and_pcm() {
    reject_without_unwind(
        "count-overflow",
        &[(u32::MAX, 1), (1, 1)],
        "sample count overflow",
    );
}

#[test]
fn oversized_single_run_is_rejected_against_the_actual_sample_count() {
    reject_without_unwind(
        "count-mismatch",
        &[(u32::MAX, 1024)],
        "timing and size sample counts disagree",
    );
}

#[test]
fn declared_entry_count_is_checked_before_native_table_allocation() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let modified = metadata_case(&original, b"stts", &|body| {
        let mut out = body.to_vec();
        set(&mut out, 4, u32::MAX);
        out
    });
    reject_before_pcm(
        root.path(),
        "table-allocation",
        &modified,
        "timing entry count/length mismatch",
    );
}

#[test]
fn an_unselected_tracks_invalid_clock_is_rejected_before_reader_initialization() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let malformed = sample_table(&[(u32::MAX, 1), (1, 1)]);
    let mut track = None;
    // Locate the already-bounded top-level moov and its single synthetic track.
    let mut at = 0;
    while at < malformed.len() {
        let size = u32_at(&malformed, at) as usize;
        if &malformed[at + 4..at + 8] == b"moov" {
            let mut child = at + 8;
            while child < at + size {
                let n = u32_at(&malformed, child) as usize;
                if &malformed[child + 4..child + 8] == b"trak" {
                    track = Some(malformed[child..child + n].to_vec());
                }
                child += n;
            }
        }
        at += size;
    }
    let track = metadata_case(&track.unwrap(), b"tkhd", &|body| {
        let mut body = body.to_vec();
        set(&mut body, 12, 2);
        body
    });
    let modified = metadata_case(&original, b"moov", &|body| {
        let mut body = body.to_vec();
        body.extend(&track);
        body
    });
    reject_before_pcm(
        root.path(),
        "unselected-sample-clock",
        &modified,
        "track=1 atom=stts phase=preflight: sample count overflow",
    );
}
