use super::*;
use std::sync::atomic::Ordering;

fn source(root: &std::path::Path, name: &str, bytes: &[u8], w: u32, h: u32) -> Source {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let name = format!("assets/{name}");
    std::fs::write(root.join(&name), bytes).unwrap();
    Source::new(
        root,
        &motion_model::Asset {
            id: 91,
            path: name.into(),
            width: w,
            height: h,
        },
    )
    .unwrap()
}
fn encoded(color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 3, 2);
        encoder.set_color(color);
        encoder.set_depth(depth);
        encoder.set_source_gamma(png::ScaledFloat::new(0.45455));
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
    }
    bytes
}
#[test]
fn original_png_integer_samples_preserve_precision_alpha_and_gray_semantics() {
    let root = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    for color in [
        png::ColorType::Rgba,
        png::ColorType::Rgb,
        png::ColorType::GrayscaleAlpha,
        png::ColorType::Grayscale,
    ] {
        for depth in [png::BitDepth::Eight, png::BitDepth::Sixteen] {
            let divisor: u32 = if depth == png::BitDepth::Eight {
                255
            } else {
                65535
            };
            let samples: Vec<u16> = (0..6 * color.samples())
                .map(|i| {
                    // Adjacent 16-bit codes must not collapse to one RGBA8 proxy.
                    if i < 4 {
                        ([32767u32, 32768, 32769, 0][i] % (divisor + 1)) as u16
                    } else {
                        ((i * 1097) as u32 % (divisor + 1)) as u16
                    }
                })
                .collect();
            let bytes: Vec<u8> = samples
                .iter()
                .flat_map(|v| {
                    if divisor == 255 {
                        vec![*v as u8]
                    } else {
                        v.to_be_bytes().to_vec()
                    }
                })
                .collect();
            let path = source(root.path(), "raw.png", &encoded(color, depth, &bytes), 3, 2);
            let actual = decode_color_f32(&path, 96, &cancel).unwrap();
            assert_eq!((actual.width, actual.height), (3, 2));
            assert_eq!(
                actual.source_precision,
                if divisor == 255 {
                    ChannelPrecision::Unorm8
                } else {
                    ChannelPrecision::Unorm16
                }
            );
            let expected: Vec<[f32; 4]> = samples
                .chunks_exact(color.samples())
                .map(|p| {
                    let s = |i| p[i] as f32 / divisor as f32;
                    match color {
                        png::ColorType::Rgba => [s(0), s(1), s(2), s(3)],
                        png::ColorType::Rgb => [s(0), s(1), s(2), 1.],
                        png::ColorType::GrayscaleAlpha => [s(0), s(0), s(0), s(1)],
                        png::ColorType::Grayscale => [s(0), s(0), s(0), 1.],
                        _ => unreachable!(),
                    }
                })
                .collect();
            assert_eq!(actual.rgba, expected, "{color:?} {depth:?}");
            if depth == png::BitDepth::Sixteen && color == png::ColorType::Rgba {
                assert!(
                    actual.rgba[0][0] < actual.rgba[0][1] && actual.rgba[0][1] < actual.rgba[0][2]
                );
                assert_eq!(actual.rgba[0][3], 0.);
                assert!(
                    actual.rgba[0][0] > 0.,
                    "unassociated RGB at zero alpha is retained"
                );
            }
            assert!(!root.path().join(".cache").exists());
        }
    }
}

// Independently authored tiny PNG writer: stored DEFLATE blocks, PNG CRC32 and
// zlib Adler32; avoids relying on an encoder which only sets the Adam7 flag.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    let mut crc = !0u32;
    for b in kind.iter().chain(data) {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    out.extend((!crc).to_be_bytes());
}
fn adam7(w: u32, h: u32, samples: &[[u16; 4]]) -> Vec<u8> {
    let mut scan = Vec::new();
    for (x0, y0, dx, dy) in [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ] {
        if x0 >= w {
            continue;
        }
        for y in (y0..h).step_by(dy) {
            scan.push(0);
            for x in (x0..w).step_by(dx) {
                for v in samples[(y * w + x) as usize] {
                    scan.extend(v.to_be_bytes());
                }
            }
        }
    }
    let n = scan.len() as u16;
    let mut z = vec![0x78, 0x01, 0x01];
    z.extend(n.to_le_bytes());
    z.extend((!n).to_le_bytes());
    z.extend(&scan);
    let (mut a, mut b) = (1u32, 0u32);
    for v in scan {
        a = (a + u32::from(v)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend(w.to_be_bytes());
    header.extend(h.to_be_bytes());
    header.extend([16, 6, 0, 0, 1]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
#[test]
fn adam7_16bit_odd_and_small_images_scatter_all_passes_without_reordering() {
    let root = tempfile::tempdir().unwrap();
    for (w, h) in [(1, 1), (1, 7), (7, 1), (9, 11)] {
        let samples: Vec<[u16; 4]> = (0..w * h)
            .map(|i| {
                [
                    i as u16,
                    (32001 + i) as u16,
                    (65000 - i) as u16,
                    (i * 487) as u16,
                ]
            })
            .collect();
        let bytes = adam7(w, h, &samples);
        let independent = image::load_from_memory(&bytes).unwrap().into_rgba16();
        assert_eq!(
            independent.as_raw(),
            &samples.iter().flatten().copied().collect::<Vec<_>>()
        );
        let path = source(root.path(), "interlaced.png", &bytes, w, h);
        let actual =
            decode_color_f32(&path, u64::from(w * h) * 16, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            actual.rgba,
            samples
                .iter()
                .map(|v| v.map(|x| f32::from(x) / 65535.))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn palette_transparency_expands_without_multiplication_or_gamma() {
    let root = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, 3, 2);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Two);
        e.set_palette(vec![128, 64, 32, 1, 2, 3, 255, 128, 0]);
        e.set_trns(vec![0, 127, 255]);
        e.write_header()
            .unwrap()
            .write_image_data(&[0b00_01_10_00, 0b10_01_00_00])
            .unwrap();
    }
    let path = source(root.path(), "indexed.png", &bytes, 3, 2);
    let actual = decode_color_f32(&path, 96, &AtomicBool::new(false)).unwrap();
    let values = [[128, 64, 32, 0], [1, 2, 3, 127], [255, 128, 0, 255]];
    assert_eq!(
        actual.rgba,
        [0, 1, 2, 2, 1, 0].map(|i| values[i].map(|v| v as f32 / 255.))
    );
}
#[test]
fn decode_budget_cancellation_crc_and_disappearing_files_keep_native_causes() {
    let root = tempfile::tempdir().unwrap();
    let bytes = encoded(png::ColorType::Rgba, png::BitDepth::Sixteen, &[0u8; 48]);
    let path = source(root.path(), "fail.png", &bytes, 3, 2);
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        decode_color_f32(&path, 96, &cancel),
        Err(Error::Cancelled)
    ));
    cancel.store(false, Ordering::Relaxed);
    let e = decode_color_f32(&path, 95, &cancel).unwrap_err();
    assert!(e.resource_rejection().is_some());
    let mut broken = bytes.clone();
    // Corrupt IEND CRC; row decoding alone would miss this terminal failure.
    *broken.last_mut().unwrap() ^= 1;
    std::fs::write(&path.path, broken).unwrap();
    let error = decode_color_f32(&path, 96, &cancel).unwrap_err();
    assert!(matches!(error, Error::Png(_)), "{error:?}");
    use std::error::Error as _;
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<png::DecodingError>()
        .is_some());
    std::fs::remove_file(&path.path).unwrap();
    assert!(
        matches!(decode_color_f32(&path,96,&cancel),Err(Error::Io(e)) if e.kind()==std::io::ErrorKind::NotFound)
    );
    std::fs::write(&path.path, bytes).unwrap();
    assert_eq!(decode_color_f32(&path, 96, &cancel).unwrap().rgba.len(), 6);
}
#[test]
fn jpeg_uses_actual_decoded_8bit_samples_with_combined_workspace_admission() {
    let root = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
        .encode(
            &[19, 63, 128, 255, 130, 1].repeat(3),
            3,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let expected = image::load_from_memory(&bytes).unwrap().into_rgba8();
    let path = source(root.path(), "source.jpg", &bytes, 3, 2);
    let cancel = AtomicBool::new(false);
    assert!(decode_color_f32(&path, 119, &cancel)
        .unwrap_err()
        .resource_rejection()
        .is_some());
    let actual = decode_color_f32(&path, 1024 * 1024, &cancel).unwrap();
    assert_eq!(actual.source_precision, ChannelPrecision::Unorm8);
    assert_eq!(
        actual.rgba,
        expected
            .pixels()
            .map(|v| v.0.map(|v| f32::from(v) / 255.))
            .collect::<Vec<_>>()
    );
}
