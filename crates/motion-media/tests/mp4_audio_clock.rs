use std::{fs, path::Path};

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
    deltas[1] += 2400;
    deltas[2] -= 128;
    let modified = rewrite(&original, &deltas, deltas.iter().sum(), 98272);
    assert_eq!(modified,fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone-clock-48000.m4a")).unwrap(),
        "the shared Android fixture must contain only the synthetic tone and clock edits");
    let (asset, changed) = decode(root.path(), "clock", &modified, 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 98272);
    assert_eq!(changed.len(), 98272 * 8);
    assert_eq!(&changed[..1024 * 8], &pcm[..1024 * 8]);
    assert!(changed[1024 * 8..3424 * 8].iter().all(|v| *v == 0));
    assert_eq!(&changed[3424 * 8..4448 * 8], &pcm[1024 * 8..2048 * 8]);
    assert_eq!(&changed[4448 * 8..5344 * 8], &pcm[2176 * 8..3072 * 8]);
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
