use motion_effects::color_lut::*;
use motion_effects::AlphaMode;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Reference {
    version: String,
    tolerance: f32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    cube: String,
    sha256: String,
    interpolation: LutInterpolation,
    alpha: AlphaMode,
    reference_path: String,
    original_file_expected: Vec<[f32; 4]>,
    input: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn cube(name: &str) -> &'static str {
    match name {
        "one-d.cube" => include_str!("color_lut/one-d.cube"),
        "three-d.cube" => include_str!("color_lut/three-d.cube"),
        _ => panic!("unexpected fixture"),
    }
}

#[test]
fn cube_transforms_match_explicit_numerical_references_and_round_trip() {
    let reference: Reference =
        serde_json::from_str(include_str!("color_lut/reference.json")).unwrap();
    assert_eq!(reference.version, "2.6.0");
    assert_eq!(reference.cases.len(), 10);
    assert_eq!(reference.tolerance, 0.00001);
    for case in reference.cases {
        if case.interpolation == LutInterpolation::Nearest {
            assert_eq!(case.reference_path, "nearest_grid_distance");
            assert_ne!(case.expected, case.original_file_expected);
        } else {
            assert_eq!(case.reference_path, "cube_file");
            assert_eq!(case.expected, case.original_file_expected);
        }
        let source = cube(&case.cube);
        assert_eq!(
            format!("{:x}", Sha256::digest(source.as_bytes())),
            case.sha256
        );
        let lut = ColorLut::from_cube(source, case.interpolation, LutPurpose::Look).unwrap();
        let restored =
            ColorLut::from_cube(&lut.to_cube(), case.interpolation, LutPurpose::Look).unwrap();
        assert_eq!(restored.key(), lut.key());
        assert_eq!(restored.descriptor(), lut.descriptor());
        assert_eq!(restored.values(), lut.values());
        let mut actual = case.input.clone();
        lut.apply_rgba(&mut actual, case.alpha).unwrap();
        assert_eq!(actual.len(), 12);
        assert_eq!(actual.len(), case.expected.len());
        for (pixel, (got, want)) in actual.iter().zip(&case.expected).enumerate() {
            for channel in 0..4 {
                assert!(
                    (got[channel] - want[channel]).abs() <= reference.tolerance,
                    "{} {:?} {:?} pixel={pixel} channel={channel} got={got:?} expected={want:?}",
                    case.cube,
                    case.interpolation,
                    case.alpha
                );
            }
            assert_eq!(got[3].to_bits(), case.input[pixel][3].to_bits());
        }
    }
}

#[test]
fn rejected_pixels_preserve_the_entire_input_and_explicit_recovery_works() {
    let lut = ColorLut::from_cube(
        cube("one-d.cube"),
        LutInterpolation::Linear,
        LutPurpose::Input,
    )
    .unwrap();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut pixels = [[0.1, 0.2, 0.3, 1.], [0.4, invalid, 0.6, 1.]];
        let before = pixels.map(|p| p.map(f32::to_bits));
        assert_eq!(
            lut.apply_rgba(&mut pixels, AlphaMode::Straight),
            Err(LutError::Pixel {
                pixel: 1,
                channel: 1
            })
        );
        assert_eq!(pixels.map(|p| p.map(f32::to_bits)), before);
    }
    for alpha in [-1., 1.01] {
        let mut pixels = [[0.1, 0.2, 0.3, 1.], [0.4, 0.5, 0.6, alpha]];
        let before = pixels;
        assert_eq!(
            lut.apply_rgba(&mut pixels, AlphaMode::Premultiplied),
            Err(LutError::Alpha { pixel: 1 })
        );
        assert_eq!(pixels, before);
    }
    let mut healthy = [[0.1, 0.2, 0.3, 1.]];
    lut.apply_rgba(&mut healthy, AlphaMode::Straight).unwrap();
    assert_ne!(healthy, [[0.1, 0.2, 0.3, 1.]]);
}

#[test]
fn cube_admission_is_bounded_and_never_guesses_dimensions_or_interpolation() {
    for text in [
        "",
        "LUT_3D_SIZE 4294967295",
        "LUT_1D_SIZE 1",
        "LUT_3D_SIZE 66",
        "LUT_1D_SIZE 2\nLUT_3D_SIZE 2",
        "LUT_1D_SIZE 2\nNaN 0 0\n1 1 1",
        "LUT_1D_SIZE 2\n0 0 0",
        "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n2 2 2",
        "LUT_1D_SIZE 2\nDOMAIN_MIN 1 0 0\nDOMAIN_MAX 0 1 1\n0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MIN 0 0 0\n0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\nLUT_1D_INPUT_RANGE 0 1\n0 0 0\n1 1 1",
    ] {
        assert!(
            ColorLut::from_cube(text, LutInterpolation::Linear, LutPurpose::Input).is_err(),
            "{text}"
        );
    }
    assert!(ColorLut::from_cube(
        cube("one-d.cube"),
        LutInterpolation::Tetrahedral,
        LutPurpose::Look
    )
    .is_err());
    let huge = " ".repeat(MAX_CUBE_BYTES + 1);
    assert!(matches!(
        ColorLut::from_cube(&huge, LutInterpolation::Linear, LutPurpose::Look),
        Err(LutError::Capacity { .. })
    ));
}

#[test]
fn transform_identity_includes_semantics_domain_samples_and_interpolation() {
    let original = ColorLut::from_cube(
        cube("three-d.cube"),
        LutInterpolation::Linear,
        LutPurpose::Look,
    )
    .unwrap();
    let descriptor = original.descriptor();
    for changed in [
        LutDescriptor {
            purpose: LutPurpose::Display,
            ..descriptor
        },
        LutDescriptor {
            interpolation: LutInterpolation::Tetrahedral,
            ..descriptor
        },
        LutDescriptor {
            domain_min: [-2.; 3],
            ..descriptor
        },
    ] {
        assert_ne!(
            ColorLut::new(changed, original.values().to_vec())
                .unwrap()
                .key(),
            original.key()
        );
    }
    let mut values = original.values().to_vec();
    values[0][0] += 0.01;
    assert_ne!(
        ColorLut::new(descriptor, values).unwrap().key(),
        original.key()
    );
    let mut values = original.values().to_vec();
    values[2][1] = f32::INFINITY;
    assert!(matches!(
        ColorLut::new(descriptor, values),
        Err(LutError::Entry {
            entry: 2,
            channel: 1
        })
    ));
}
