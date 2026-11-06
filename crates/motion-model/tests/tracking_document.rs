use motion_model::{
    tracking::{asset, colmap, decode, document::*, encode, plate, pose},
    TrackingFormat,
};
use std::io::{self, Cursor, Read};

const CAMERAS: &[u8] =
    b"# original spelling\r\n7 OPENCV 800 600 700.000 710 400 300 0.01 -0.003 0.002 -0.001\r\n";
const IMAGES: &[u8] =
    "# independently authored 文本\r\n83 1 0 0 0 -1 -2 -3 7 frame001.png\r\n\r\n".as_bytes();
fn source() -> colmap::Source {
    colmap::Source {
        id: "plate".into(),
        sha256: [0x51; 32],
        extent: plate::Extent::new(800, 600).unwrap(),
        ticks_per_second: 90_000,
        nominal_rate: asset::Rate::new(30_000, 1001).unwrap(),
        pts: vec![-3003, 0, 3004],
    }
}
fn frames() -> Vec<colmap::Frame> {
    (0..3)
        .map(|i| colmap::Frame {
            source_index: i,
            image_name: format!("frame{:03}.png", i + 1),
            confidence: 0.8,
        })
        .collect()
}
fn options() -> colmap::Options {
    colmap::Options {
        track_id: "original camera".into(),
        world_basis: pose::Basis::RIGHT_UP_BACK,
        source_meters_per_unit: 0.01,
        target_meters_per_unit: 0.1,
        target_origin: [1., 2., 3.],
    }
}
fn bytes() -> Vec<u8> {
    colmap_to_vec(CAMERAS, IMAGES, source(), &frames(), options()).unwrap()
}
fn value() -> serde_json::Value {
    serde_json::from_slice(&bytes()).unwrap()
}

#[test]
fn explicit_envelope_retains_original_text_mapping_options_and_exact_source_clock() {
    let bytes = bytes();
    let result = colmap_from_slice(&bytes).unwrap();
    assert_eq!(result.converted.originals.cameras(), CAMERAS);
    assert_eq!(result.converted.originals.images(), IMAGES);
    assert_eq!(
        result
            .frames
            .iter()
            .map(|f| f.image_name.as_str())
            .collect::<Vec<_>>(),
        ["frame001.png", "frame002.png", "frame003.png"]
    );
    assert_eq!(
        result.converted.options.world_basis,
        pose::Basis::RIGHT_UP_BACK
    );
    assert_eq!(result.converted.options.source_meters_per_unit, 0.01);
    assert_eq!(result.converted.options.target_origin, [1., 2., 3.]);
    assert_eq!(result.converted.asset.source_pts, [-3003, 0, 3004]);
    assert_eq!(result.converted.asset.nominal_rate.ratio(), [30000, 1001]);
    let decoded = from_reader(Cursor::new(&bytes)).unwrap();
    assert_eq!(decoded.format, TrackingFormat::ColmapCameraTextV1);
    assert_eq!(decoded.asset, result.converted.asset);
}

#[test]
fn ordinary_canonical_document_keeps_its_contract_without_a_discriminator() {
    let normalized = from_slice(&bytes()).unwrap().asset;
    let original = encode::to_vec(&normalized).unwrap();
    let actual = from_slice(&original).unwrap();
    assert_eq!(actual.format, TrackingFormat::MotionTrackingJsonV1);
    assert_eq!(actual.asset, decode::from_slice(&original).unwrap());
    assert!(colmap_from_slice(&original).is_err());
}

#[test]
fn vendor_metadata_is_required_and_never_guessed_or_defaulted() {
    for key in ["cameras", "images", "source", "frames", "options"] {
        let mut v = value();
        v.as_object_mut().unwrap().remove(key);
        assert!(
            from_slice(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{key}"
        );
    }
    for key in [
        "world_basis",
        "source_meters_per_unit",
        "target_meters_per_unit",
        "target_origin",
    ] {
        let mut v = value();
        v["options"].as_object_mut().unwrap().remove(key);
        assert!(
            from_slice(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut v = value();
    v["frames"][0].as_object_mut().unwrap().remove("confidence");
    assert!(from_slice(&serde_json::to_vec(&v).unwrap()).is_err());
    v = value();
    v["format"] = "future-unapproved-format".into();
    assert!(matches!(
        from_slice(&serde_json::to_vec(&v).unwrap()),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn list_bounds_apply_while_decoding_not_only_after_allocating_the_whole_list() {
    let mut v = value();
    v["frames"] = serde_json::json!((0..=colmap::MAX_FRAMES)
        .map(|i| serde_json::json!({
        "source_index":i,"image_name":format!("{i}.png"),"confidence":0.8}))
        .collect::<Vec<_>>());
    let error = from_slice(&serde_json::to_vec(&v).unwrap()).err().unwrap();
    assert!(error.to_string().contains("sequence limit"));
    v = value();
    v["source"]["pts"] = serde_json::json!(vec![0; colmap::MAX_FRAMES + 1]);
    assert!(from_slice(&serde_json::to_vec(&v).unwrap())
        .err()
        .unwrap()
        .to_string()
        .contains("sequence limit"));
}

#[test]
fn original_conversion_causes_and_locations_survive_document_admission() {
    let mut v = value();
    v["images"] = "83 0 0 0 0 0 0 0 7 frame001.png\n\n".into();
    let error = from_slice(&serde_json::to_vec(&v).unwrap()).err().unwrap();
    assert!(std::error::Error::source(&error).is_some());
    assert!(matches!(
        error,
        Error::Colmap(colmap::Error {
            input: Some(colmap::Input::Images),
            line: Some(1),
            cause: colmap::Cause::Pose(_),
            ..
        })
    ));
    v = value();
    v["options"]["world_basis"] = serde_json::json!([1, 1, 3]);
    assert!(matches!(
        from_slice(&serde_json::to_vec(&v).unwrap()),
        Err(Error::Colmap(_))
    ));
}

#[test]
fn invalid_json_fields_and_format_values_never_echo_user_payloads() {
    let secret = "secret-not-for-logs";
    let mut v = value();
    v["options"][secret] = secret.into();
    let error = from_slice(&serde_json::to_vec(&v).unwrap()).err().unwrap();
    assert!(!format!("{error:?} {error}").contains(secret));
    v = value();
    v["format"] = secret.into();
    assert!(!from_slice(&serde_json::to_vec(&v).unwrap())
        .err()
        .unwrap()
        .to_string()
        .contains(secret));
    let text = String::from_utf8(bytes()).unwrap();
    let duplicate = text.replacen('{', "{\"format\":\"duplicate\",", 1);
    assert!(from_slice(duplicate.as_bytes()).is_err());
}

#[test]
fn envelope_input_and_escaped_output_remain_bounded() {
    assert!(matches!(
        from_slice(&vec![b' '; decode::MAX_JSON_BYTES + 1]),
        Err(Error::Canonical(decode::Error::InputLimit))
    ));
    let mut cameras = b"#".to_vec();
    cameras.extend(vec![1; decode::MAX_JSON_BYTES / 6]);
    cameras.extend(b"\n");
    cameras.extend(CAMERAS);
    assert!(matches!(
        colmap_to_vec(&cameras, IMAGES, source(), &frames(), options()),
        Err(Error::Encode(encode::Error::OutputLimit))
    ));
}

#[test]
fn reader_failure_retains_the_original_io_cause() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled original reader failure",
            ))
        }
    }
    let error = from_reader(Broken).err().unwrap();
    assert!(matches!(error, Error::Canonical(decode::Error::Read(_))));
    assert!(error
        .to_string()
        .contains("controlled original reader failure"));
}
