use super::*;
use crate::tracking::{
    asset::{Coordinates, Kind, Rate, Sample, Track, Visibility, VERSION},
    plate::Extent,
};
use std::{error::Error as _, io::Cursor};

fn digest(hex: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
}
fn asset(hash: [u8; 32]) -> Asset {
    Asset {
        version: VERSION,
        source_id: "original-source".into(),
        source_sha256: hash,
        source_extent: Extent::new(640, 480).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 1000,
        nominal_rate: Rate::new(30, 1).unwrap(),
        source_pts: vec![-30, 4, 39],
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "occluded-manual-point".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 1,
                visibility: Visibility::Occluded,
                confidence: 0.,
                observation: None,
                manual_correction: true,
            }],
        }],
    }
}
fn abc() -> Asset {
    asset(digest(
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ))
}

#[test]
fn verifies_known_original_bytes_without_mutating_solve() {
    let a = abc();
    let before = crate::tracking::encode::to_vec(&a).unwrap();
    let proof = verify(&a, &mut Cursor::new(b"abc"), 3, || false).unwrap();
    assert_eq!(proof.sha256(), a.source_sha256);
    assert_eq!(proof.byte_len(), 3);
    assert_eq!(crate::tracking::encode::to_vec(&a).unwrap(), before);
}

#[test]
fn content_mismatch_is_rejected_even_with_equal_length_and_source_label() {
    let mut a = abc();
    for label in ["original-source", "same-path-after-replace"] {
        a.source_id = label.into();
        assert!(matches!(
            verify(&a, &mut Cursor::new(b"abd"), 3, || false),
            Err(Error::DigestMismatch)
        ));
    }
}

#[test]
fn supports_empty_input_only_when_the_complete_digest_matches() {
    let a = asset(digest(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ));
    assert_eq!(
        verify(&a, &mut Cursor::new([]), 1, || false)
            .unwrap()
            .byte_len(),
        0
    );
    assert!(matches!(
        verify(&abc(), &mut Cursor::new([]), 1, || false),
        Err(Error::DigestMismatch)
    ));
}

#[test]
fn over_budget_reads_at_most_one_sentinel_byte_and_never_returns_a_proof() {
    for budget in [1, 2, 3, 65535, 65536, 65537] {
        let input = vec![7u8; budget as usize + 100];
        let mut reader = Cursor::new(input);
        assert!(
            matches!(verify(&abc(),&mut reader,budget,||false),Err(Error::Limit{max_bytes}) if max_bytes==budget)
        );
        assert_eq!(reader.position(), budget + 1);
    }
    let mut reader = Cursor::new(b"abc");
    assert!(matches!(
        verify(&abc(), &mut reader, 0, || false),
        Err(Error::InvalidBudget)
    ));
    assert_eq!(reader.position(), 0);
}

#[test]
fn many_chunks_match_an_independent_python_hashlib_golden() {
    let input: Vec<u8> = (0..150123).map(|i| (i % 251) as u8).collect();
    let hash = digest("c2e38014e6de2fa5aac827ed640910f64d11b3cc234bcc93b13b74972a8e1dab");
    let proof = verify(&asset(hash), &mut Cursor::new(input), 150123, || false).unwrap();
    assert_eq!(proof.sha256(), hash);
    assert_eq!(proof.byte_len(), 150123);
    assert_eq!(
        verify(&abc(), &mut Cursor::new(b"abc"), u64::MAX, || false)
            .unwrap()
            .byte_len(),
        3
    );
}

struct Fragmented {
    inner: Cursor<&'static [u8]>,
    interrupted: bool,
}
impl Read for Fragmented {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.interrupted = !self.interrupted;
        if self.interrupted {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.inner.read(&mut buffer[..1])
    }
}
#[test]
fn short_reads_and_interrupted_reads_preserve_all_original_bytes() {
    let mut reader = Fragmented {
        inner: Cursor::new(b"abc"),
        interrupted: false,
    };
    assert_eq!(
        verify(&abc(), &mut reader, 3, || false).unwrap().byte_len(),
        3
    );
}

struct NeverRead;
impl Read for NeverRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("reader must not be touched")
    }
}
#[test]
fn invalid_asset_and_initial_cancellation_do_not_read_the_source() {
    assert!(matches!(
        verify(&abc(), &mut NeverRead, 3, || true),
        Err(Error::Cancelled)
    ));
    let mut a = abc();
    a.version = 0;
    let error = verify(&a, &mut NeverRead, 3, || false).unwrap_err();
    assert!(matches!(error, Error::Asset(_)));
    assert!(error.source().unwrap().is::<asset::Error>());
}

struct Interrupted;
impl Read for Interrupted {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Interrupted.into())
    }
}
#[test]
fn repeated_interruption_still_observes_cancellation() {
    let mut checks = 0;
    assert!(matches!(
        verify(&abc(), &mut Interrupted, 3, || {
            checks += 1;
            checks == 4
        }),
        Err(Error::Cancelled)
    ));
    assert_eq!(checks, 4);
}

#[test]
fn cancellation_after_a_chunk_does_not_consume_the_remainder() {
    let mut reader = Fragmented {
        inner: Cursor::new(b"abc"),
        interrupted: true,
    };
    let mut checks = 0;
    assert!(matches!(
        verify(&abc(), &mut reader, 3, || {
            checks += 1;
            checks > 1
        }),
        Err(Error::Cancelled)
    ));
    assert_eq!(reader.inner.position(), 1);
}

struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from_raw_os_error(123))
    }
}
#[test]
fn read_failure_retains_original_typed_io_cause_without_retry() {
    let error = verify(&abc(), &mut Broken, 3, || false).unwrap_err();
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(123)
    );
    assert!(error.to_string().starts_with("tracking source read:"));
}
