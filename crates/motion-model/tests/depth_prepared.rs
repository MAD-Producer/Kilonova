use motion_model::depth::{DepthGeometry, PreparedDepthInput};

#[test]
fn prepared_depth_identity_has_fixed_digest_extent_and_oriented_crop_validation() {
    let mut geometry = DepthGeometry {
        source_size: [4, 3],
        crop: [1, 0, 2, 3],
        quarter_turns: 1,
        mirror_x: true,
    };
    let identity = PreparedDepthInput {
        rgb_size: [3, 2],
        rgb_sha256: [42; 32],
    };
    identity.validate(&geometry).unwrap();
    let bytes = serde_json::to_vec(&identity).unwrap();
    assert_eq!(
        serde_json::from_slice::<PreparedDepthInput>(&bytes).unwrap(),
        identity
    );
    for length in [0, 31, 33] {
        let value = serde_json::json!({"rgb_size":[3,2],"rgb_sha256":vec![42; length]});
        assert!(serde_json::from_value::<PreparedDepthInput>(value).is_err());
    }
    geometry.quarter_turns = 0;
    assert!(identity.validate(&geometry).is_err());
    geometry.source_size = [5000, 2];
    geometry.crop = [0, 0, 5000, 2];
    assert!(PreparedDepthInput {
        rgb_size: [5000, 2],
        ..identity
    }
    .validate(&geometry)
    .is_err());
}
