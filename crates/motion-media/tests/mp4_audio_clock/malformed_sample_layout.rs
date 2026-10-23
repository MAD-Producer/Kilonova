use super::*;

#[path = "sample_layout_boundaries.rs"]
mod boundaries;

fn mapping(entries: &[(u32, u32)]) -> Vec<u8> {
    metadata_case(&source(), b"stsc", &|_| {
        let mut body = vec![0; 4];
        body.extend(u32::try_from(entries.len()).unwrap().to_be_bytes());
        for &(chunk, samples) in entries {
            body.extend(chunk.to_be_bytes());
            body.extend(samples.to_be_bytes());
            body.extend(1u32.to_be_bytes());
        }
        body
    })
}

fn reject(name: &str, bytes: &[u8], atom: &str, cause: &str) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join(format!("{name}.m4a"));
    let pcm = root.path().join(format!("{name}.pcm"));
    let previous = b"existing PCM survives malformed sample layout";
    fs::write(&input, bytes).unwrap();
    fs::write(&pcm, previous).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        motion_media::decode_audio(&input, &pcm, None, 4 << 20, &mut |_| Ok(()))
    });
    assert!(outcome.is_ok(), "{name}: malformed sample layout panicked");
    let error = outcome.unwrap().expect_err("malformed layout was accepted");
    let text = error.to_string();
    assert!(text.contains(cause), "{name}: {text}");
    assert!(
        text.contains(&format!("track=0 atom={atom} phase=preflight")),
        "{name}: {text}"
    );
    assert_eq!(fs::read(&pcm).unwrap(), previous);
    assert_eq!(fs::read(&input).unwrap(), bytes);
    let (asset, _) = decode(root.path(), "healthy", &source(), 4 << 20).unwrap();
    assert_eq!(asset.sample_frames, 96_000);
}

#[test]
fn zero_first_chunk_returns_error_without_native_underflow() {
    reject(
        "zero-first-chunk",
        &mapping(&[(0, 95)]),
        "stsc",
        "first chunk must start at one",
    );
}

#[test]
fn chunk_run_product_overflow_returns_error_without_native_panic() {
    reject(
        "chunk-product",
        &mapping(&[(1, u32::MAX), (3, 1)]),
        "stsc",
        "sample index overflow",
    );
}

#[test]
fn chunk_run_accumulator_overflow_returns_error_without_native_panic() {
    reject(
        "chunk-sum",
        &mapping(&[(1, u32::MAX - 1), (2, 2), (3, 1)]),
        "stsc",
        "sample index overflow",
    );
}
