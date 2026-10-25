use motion_effects::image_contract::{
    ChannelPrecision as P, ImageContract, ImageFormatError, ImageMeaning as M, ImageStorage as S,
    ImageUsage,
};
use std::cell::Cell;

#[test]
fn named_numeric_meanings_select_narrow_linear_storage_without_alpha_semantics() {
    for (precision, scalar) in [
        (P::Unorm8, S::R8Unorm),
        (P::Unorm16, S::R16Unorm),
        (P::Float16, S::R16Float),
        (P::Float32, S::R32Float),
    ] {
        for meaning in [
            M::Coverage {},
            M::RelativeDepth {},
            M::Weight { normalized: true },
        ] {
            assert_eq!(
                ImageContract::new(meaning, precision)
                    .preferred_storage()
                    .unwrap(),
                scalar
            );
        }
    }
    for (precision, scalar, normal) in [
        (P::Float16, S::R16Float, S::Rgba16Float),
        (P::Float32, S::R32Float, S::Rgba32Float),
    ] {
        for meaning in [M::LinearDepth {}, M::Weight { normalized: false }] {
            assert_eq!(
                ImageContract::new(meaning, precision)
                    .preferred_storage()
                    .unwrap(),
                scalar
            );
        }
        assert_eq!(
            ImageContract::new(M::Normal {}, precision)
                .preferred_storage()
                .unwrap(),
            normal
        );
    }
}

#[test]
fn unbounded_weights_distance_and_signed_normals_refuse_unorm_before_capability_queries() {
    for precision in [P::Unorm8, P::Unorm16] {
        for meaning in [
            M::LinearDepth {},
            M::Normal {},
            M::Weight { normalized: false },
        ] {
            let contract = ImageContract::new(meaning, precision);
            let queries = Cell::new(0);
            let error = contract
                .negotiate(ImageUsage::EFFECT_TARGET, |_| {
                    queries.set(queries.get() + 1);
                    ImageUsage::EFFECT_TARGET
                })
                .unwrap_err();
            assert_eq!(
                error,
                ImageFormatError::InvalidPrecision { meaning, precision }
            );
            assert_eq!(queries.get(), 0);
        }
    }
}

#[test]
fn named_semantics_round_trip_and_require_explicit_weight_range_without_color_fields() {
    for (meaning, tag) in [
        (M::Coverage {}, "coverage"),
        (M::Weight { normalized: false }, "weight"),
        (M::RelativeDepth {}, "relative_depth"),
        (M::LinearDepth {}, "linear_depth"),
        (M::Normal {}, "normal"),
    ] {
        let contract = ImageContract::new(meaning, P::Float32);
        let mut encoded = serde_json::to_value(contract).unwrap();
        assert_eq!(encoded["meaning"]["kind"], tag);
        assert_eq!(
            serde_json::from_value::<ImageContract>(encoded.clone()).unwrap(),
            contract
        );
        encoded["meaning"]["alpha"] = serde_json::json!("premultiplied");
        assert!(serde_json::from_value::<ImageContract>(encoded).is_err());
    }
    let mut encoded = serde_json::to_value(ImageContract::new(
        M::Weight { normalized: true },
        P::Float16,
    ))
    .unwrap();
    encoded["meaning"]
        .as_object_mut()
        .unwrap()
        .remove("normalized");
    assert!(serde_json::from_value::<ImageContract>(encoded).is_err());
}

#[test]
fn numeric_fallback_only_pads_ignored_channels_at_the_same_precision() {
    let contract = ImageContract::new(M::LinearDepth {}, P::Float16);
    assert_eq!(
        contract
            .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                if storage == S::Rgba16Float {
                    ImageUsage::EFFECT_TARGET
                } else {
                    ImageUsage::default()
                }
            })
            .unwrap(),
        S::Rgba16Float
    );
    let normal = ImageContract::new(M::Normal {}, P::Float32);
    let error = normal
        .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
            if storage == S::Rg32Float || storage == S::Rgba16Float {
                ImageUsage::EFFECT_TARGET
            } else {
                ImageUsage::default()
            }
        })
        .unwrap_err();
    let ImageFormatError::Unsupported { contract, missing } = error else {
        panic!("unexpected rejection")
    };
    assert_eq!(contract, normal);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].0, S::Rgba32Float);
}
