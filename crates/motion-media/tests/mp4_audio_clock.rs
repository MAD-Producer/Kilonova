use std::{fs, path::Path};

#[path = "mp4_audio_clock/malformed_sample_table.rs"]
mod malformed_sample_table;

fn atom(kind: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(body);
    out
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}
fn set(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn rewrite(data: &[u8], deltas: &[u32], declared: u32, edited: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let size = u32_at(data, position) as usize;
        assert!(size >= 8 && position + size <= data.len());
        let kind = &data[position + 4..position + 8];
        let original = &data[position + 8..position + size];
        let mut body = original.to_vec();
        if matches!(
            kind,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts"
        ) {
            body = rewrite(original, deltas, declared, edited);
        } else if kind == b"stts" {
            let original_packets = (0..u32_at(original, 4) as usize)
                .map(|i| u32_at(original, 8 + i * 8))
                .sum::<u32>();
            assert_eq!(original_packets, deltas.len() as u32);
            body = vec![0; 8];
            set(&mut body, 4, deltas.len() as u32);
            for delta in deltas {
                body.extend(1u32.to_be_bytes());
                body.extend(delta.to_be_bytes());
            }
        } else if kind == b"mdhd" {
            assert_eq!(body[0], 0);
            set(&mut body, 16, declared);
        } else if kind == b"mvhd" {
            assert_eq!(body[0], 0);
            set(&mut body, 16, edited);
        } else if kind == b"tkhd" {
            assert_eq!(body[0], 0);
            set(&mut body, 20, edited);
        } else if kind == b"elst" {
            assert_eq!(body[0], 0);
            assert_eq!(u32_at(&body, 4), 1);
            set(&mut body, 8, edited);
        }
        out.extend(atom(kind, &body));
        position += size;
    }
    out
}
fn source() -> Vec<u8> {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone-stereo-48000.m4a"),
    )
    .unwrap();
    // This public synthetic fixture has moov after mdat; replacing metadata
    // leaves all encoded packet offsets and payloads intact.
    let mut positions = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        positions.push((bytes[at + 4..at + 8].to_vec(), at));
        at += u32_at(&bytes, at) as usize;
    }
    assert!(
        positions.iter().find(|v| v.0 == b"mdat").unwrap().1
            < positions.iter().find(|v| v.0 == b"moov").unwrap().1
    );
    bytes
}
fn decode(
    root: &Path,
    name: &str,
    bytes: &[u8],
    limit: u64,
) -> Result<(motion_model::AudioAsset, Vec<u8>), String> {
    let input = root.join(format!("{name}.m4a"));
    let pcm = root.join(format!("{name}.pcm"));
    fs::write(&input, bytes).unwrap();
    let asset = motion_media::decode_audio(&input, &pcm, None, limit, &mut |_| Ok(()))?;
    Ok((asset, fs::read(pcm).unwrap()))
}
#[test]
fn presentation_gaps_and_overlap_preserve_clock_and_decoded_sample_positions() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let (baseline, pcm) = decode(root.path(), "base", &original, 4 << 20).unwrap();
    assert_eq!(baseline.sample_frames, 96000);
    let mut deltas = vec![1024; 95];
    deltas[10] += 2400;
    deltas[11] -= 128;
    let modified = rewrite(&original, &deltas, deltas.iter().sum(), 98272);
    assert_eq!(
        modified,
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone-clock-48000.m4a"))
            .unwrap(),
        "the shared Android fixture must contain only the synthetic tone and clock edits"
    );
    let (asset, changed) = decode(root.path(), "clock", &modified, 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 98272);
    assert_eq!(changed.len(), 98272 * 8);
    assert_eq!(&changed[..10240 * 8], &pcm[..10240 * 8]);
    assert!(changed[10240 * 8..12640 * 8].iter().all(|v| *v == 0));
    assert_eq!(&changed[12640 * 8..13664 * 8], &pcm[10240 * 8..11264 * 8]);
    assert_eq!(&changed[13664 * 8..14560 * 8], &pcm[11392 * 8..12288 * 8]);
    assert!(pcm[10240 * 8..11264 * 8]
        .chunks_exact(4)
        .any(|v| f32::from_le_bytes(v.try_into().unwrap()).abs() > 0.1));
    assert!(decode(root.path(), "budget", &modified, 1024)
        .unwrap_err()
        .contains("cache exceeds limit"));
}
#[test]
fn movie_tick_rounding_is_bounded_and_missing_packets_are_still_rejected() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let deltas = vec![1024; 95];
    let duration = deltas.iter().sum();
    // This fixture's movie timebase equals its audio rate: one tick is one sample.
    let rounded = rewrite(&original, &deltas, duration, 96257);
    let (asset, pcm) = decode(root.path(), "rounded", &rounded, 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 96257);
    assert!(pcm[96256 * 8..].iter().all(|v| *v == 0));
    let excess = rewrite(&original, &deltas, duration, 96258);
    assert!(decode(root.path(), "excess", &excess, 4 << 20)
        .unwrap_err()
        .contains("truncated"));
    let missing = rewrite(&original, &deltas, duration + 1024, 96000);
    assert!(decode(root.path(), "missing", &missing, 4 << 20)
        .unwrap_err()
        .contains("sample table is truncated"));
}

// Rewrite only metadata in the existing synthetic fixture. mdat precedes moov,
// so changing an edit atom's size does not change any encoded packet offset.
fn metadata_case(data: &[u8], target: &[u8; 4], change: &dyn Fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let size = u32_at(data, at) as usize;
        assert!(size >= 8 && at + size <= data.len());
        let kind = &data[at + 4..at + 8];
        let original = &data[at + 8..at + size];
        let body = if kind == target {
            change(original)
        } else if matches!(
            kind,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts"
        ) {
            metadata_case(original, target, change)
        } else {
            original.to_vec()
        };
        result.extend(atom(kind, &body));
        at += size;
    }
    result
}

fn reject_before_pcm(root: &Path, name: &str, bytes: &[u8], cause: &str) {
    let input = root.join(format!("{name}.m4a"));
    let pcm = root.join(format!("{name}.pcm"));
    let previous = b"previous cache must survive a rejected source clock";
    fs::write(&input, bytes).unwrap();
    fs::write(&pcm, previous).unwrap();
    let result = motion_media::decode_audio(&input, &pcm, None, 4 << 20, &mut |_| Ok(()));
    assert!(result.is_err(), "{name}: malformed clock was accepted");
    let error = result.unwrap_err().to_string();
    assert!(error.contains(cause), "{name}: {error}");
    assert_eq!(
        fs::read(pcm).unwrap(),
        previous,
        "{name}: rejected metadata changed PCM"
    );
}

#[test]
fn zero_container_timebases_return_errors_before_touching_pcm() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let (_, reference_pcm) = decode(root.path(), "reference", &original, 4 << 20).unwrap();
    for (kind, name) in [(b"mvhd", "movie"), (b"mdhd", "audio")] {
        let modified = metadata_case(&original, kind, &|body| {
            assert_eq!(body[0], 0);
            let mut body = body.to_vec();
            set(&mut body, 12, 0);
            body
        });
        assert_ne!(modified, original);
        reject_before_pcm(root.path(), name, &modified, "timebase");
        let wide = metadata_case(&original, kind, &|body| {
            assert_eq!(body[0], 0);
            let mut out = vec![1, 0, 0, 0];
            out.extend(0u64.to_be_bytes());
            out.extend(0u64.to_be_bytes());
            out.extend(u32_at(body, 12).to_be_bytes());
            out.extend(u64::from(u32_at(body, 16)).to_be_bytes());
            out.extend(&body[20..]);
            out
        });
        let (asset, pcm) =
            decode(root.path(), &format!("{name}-valid-v1"), &wide, 4 << 20).unwrap();
        assert_eq!(asset.sample_frames, 96000);
        assert_eq!(pcm, reference_pcm);
        let invalid_wide = metadata_case(&wide, kind, &|body| {
            assert_eq!(body[0], 1);
            let mut body = body.to_vec();
            set(&mut body, 20, 0);
            body
        });
        reject_before_pcm(
            root.path(),
            &format!("{name}-v1"),
            &invalid_wide,
            "timebase",
        );
    }
    // The same compressed packets still decode with their original clock.
    assert_eq!(
        decode(root.path(), "valid", &original, 4 << 20)
            .unwrap()
            .0
            .sample_frames,
        96000
    );
}

#[test]
fn unselected_track_with_zero_timebase_is_rejected_before_decoder_initialization() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let modified = metadata_case(&original, b"moov", &|body| {
        let mut at = 0;
        let mut second = None;
        while at < body.len() {
            let size = u32_at(body, at) as usize;
            if &body[at + 4..at + 8] == b"trak" {
                assert!(second.is_none());
                let track = metadata_case(&body[at..at + size], b"mdhd", &|header| {
                    let mut header = header.to_vec();
                    set(&mut header, 12, 0);
                    header
                });
                second = Some(metadata_case(&track, b"tkhd", &|header| {
                    let mut header = header.to_vec();
                    set(&mut header, 12, 2);
                    header
                }));
            }
            at += size;
        }
        let mut out = body.to_vec();
        out.extend(second.unwrap());
        out
    });
    reject_before_pcm(root.path(), "unselected", &modified, "track=Some(1)");
}

#[test]
fn duplicate_movie_or_media_headers_cannot_bypass_clock_validation() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let mut second_movie = None;
    let mut at = 0;
    while at < original.len() {
        let size = u32_at(&original, at) as usize;
        if &original[at + 4..at + 8] == b"moov" {
            second_movie = Some(original[at..at + size].to_vec());
        }
        at += size;
    }
    let mut duplicate = original.clone();
    duplicate.extend(second_movie.unwrap());
    reject_before_pcm(
        root.path(),
        "duplicate-moov",
        &duplicate,
        "duplicate MP4 atom moov",
    );
    let duplicate = metadata_case(&original, b"mdia", &|body| {
        let mut at = 0;
        let mut result = body.to_vec();
        while at < body.len() {
            let size = u32_at(body, at) as usize;
            if &body[at + 4..at + 8] == b"mdhd" {
                let mut invalid = body[at + 8..at + size].to_vec();
                set(&mut invalid, 12, 0);
                result.extend(atom(b"mdhd", &invalid));
            }
            at += size;
        }
        result
    });
    reject_before_pcm(
        root.path(),
        "duplicate-mdhd",
        &duplicate,
        "duplicate MP4 atom mdhd",
    );
}

#[test]
fn atom_budget_accepts_the_boundary_and_rejects_excess_before_touching_pcm() {
    let root = tempfile::tempdir().unwrap();
    let mut boundary = source();
    let mut at = 0;
    let mut count = 0;
    while at < boundary.len() {
        at += u32_at(&boundary, at) as usize;
        count += 1;
    }
    assert!(count < 4096);
    for _ in count..4096 {
        boundary.extend(atom(b"free", &[]));
    }
    let (asset, pcm) = decode(root.path(), "boundary", &boundary, 4 << 20).unwrap();
    let (_, original_pcm) = decode(root.path(), "reference", &source(), 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 96000);
    assert_eq!(pcm, original_pcm);
    boundary.extend(atom(b"free", &[]));
    reject_before_pcm(
        root.path(),
        "excess-atoms",
        &boundary,
        "MP4 atom budget exceeded",
    );
}

#[test]
fn wide_edit_overflows_return_errors_before_touching_pcm() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    for (name, duration, start, cause) in [
        ("duration", u64::MAX, 0i64, "edit duration overflow"),
        ("start", 96000, i64::MAX, "edit start overflow"),
    ] {
        let modified = metadata_case(&original, b"elst", &|body| {
            assert_eq!(u32_at(body, 4), 1);
            let mut result = vec![1, 0, 0, 0];
            result.extend(1u32.to_be_bytes());
            result.extend(duration.to_be_bytes());
            result.extend(start.to_be_bytes());
            result.extend([0, 1, 0, 0]);
            result
        });
        assert_ne!(modified, original);
        reject_before_pcm(root.path(), name, &modified, cause);
    }
}

#[test]
fn timestamp_gap_beyond_budget_is_rejected_instead_of_shifting_audio() {
    let root = tempfile::tempdir().unwrap();
    let original = source();
    let modified = metadata_case(&original, b"stts", &|body| {
        let mut body = body.to_vec();
        let count = u32_at(&body, 4) as usize;
        assert_eq!(
            (0..count).map(|i| u32_at(&body, 8 + i * 8)).sum::<u32>(),
            95
        );
        body.resize(24, 0);
        set(&mut body, 4, 2);
        set(&mut body, 8, 1);
        set(&mut body, 12, u32::MAX);
        set(&mut body, 16, 94);
        set(&mut body, 20, 1024);
        body
    });
    // This large gap is outside the original two-second edit. Cropping it is
    // valid: the priming packet is skipped, then only silence is presented.
    let (cropped, samples) = decode(root.path(), "cropped", &modified, 4 << 20).unwrap();
    assert_eq!(cropped.sample_frames, 96000);
    assert_eq!(samples.len(), 96000 * 8);
    assert!(samples.iter().all(|byte| *byte == 0));

    // Include a two-hour gap in the requested presentation, rather than
    // incorrectly requiring rejection for an edit that crops that gap away.
    let mut deltas = vec![1024; 95];
    deltas[0] = 48000 * 7200;
    let duration = deltas.iter().sum();
    let unbounded = rewrite(&original, &deltas, duration, duration);
    let error = match decode(root.path(), "gap", &unbounded, 4 << 20) {
        Err(error) => error,
        Ok((asset, _)) => panic!("accepted a two-hour edit: frames={}", asset.sample_frames),
    };
    assert!(
        error.contains("budget") || error.contains("limit"),
        "{error}"
    );
    // Never materialize the declared multi-hour gap as a giant cache file.
    let pcm = root.path().join("gap.pcm");
    if pcm.exists() {
        assert!(fs::metadata(pcm).unwrap().len() <= 4 << 20);
    }
}
