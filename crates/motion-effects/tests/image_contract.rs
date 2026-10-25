use motion_effects::image_contract::*;
use motion_effects::{AlphaMode, WorkingSpace};

#[test]
fn color_alpha_and_sampled_working_space_cannot_be_narrowed_to_scalar_data() {
    for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
        let linear = ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha,
            },
            ChannelPrecision::Unorm8,
        );
        let gamma = ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Srgb,
                alpha,
            },
            ChannelPrecision::Unorm8,
        );
        assert_eq!(
            linear.preferred_storage().unwrap(),
            ImageStorage::Rgba8UnormSrgb
        );
        assert_eq!(gamma.preferred_storage().unwrap(), ImageStorage::Rgba8Unorm);
        // Even a grayscale-colored image retains its color transfer and alpha.
        assert_eq!(linear.preferred_storage().unwrap().bytes_per_pixel(), 4);
        assert!(linear
            .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                if storage == ImageStorage::R8Unorm {
                    ImageUsage::EFFECT_TARGET
                } else {
                    ImageUsage::default()
                }
            })
            .is_err());
        assert_eq!(
            serde_json::from_str::<ImageContract>(&serde_json::to_string(&linear).unwrap())
                .unwrap(),
            linear
        );
    }
}

#[test]
fn numeric_channels_never_select_srgb_storage_or_a_color_alpha_contract() {
    let cases = [
        (
            DataChannels::Scalar,
            ChannelPrecision::Unorm8,
            ImageStorage::R8Unorm,
        ),
        (
            DataChannels::Vector2,
            ChannelPrecision::Unorm8,
            ImageStorage::Rg8Unorm,
        ),
        (
            DataChannels::Vector3,
            ChannelPrecision::Unorm8,
            ImageStorage::Rgba8Unorm,
        ),
        (
            DataChannels::Scalar,
            ChannelPrecision::Float16,
            ImageStorage::R16Float,
        ),
        (
            DataChannels::Vector2,
            ChannelPrecision::Float16,
            ImageStorage::Rg16Float,
        ),
        (
            DataChannels::Vector3,
            ChannelPrecision::Float16,
            ImageStorage::Rgba16Float,
        ),
        (
            DataChannels::Scalar,
            ChannelPrecision::Float32,
            ImageStorage::R32Float,
        ),
        (
            DataChannels::Vector2,
            ChannelPrecision::Float32,
            ImageStorage::Rg32Float,
        ),
        (
            DataChannels::Vector4,
            ChannelPrecision::Float32,
            ImageStorage::Rgba32Float,
        ),
    ];
    for (channels, precision, expected) in cases {
        let contract = ImageContract::new(ImageMeaning::Data { channels }, precision);
        assert_eq!(contract.preferred_storage().unwrap(), expected);
        assert_ne!(expected, ImageStorage::Rgba8UnormSrgb);
    }
}

#[test]
fn fallback_only_pads_channels_and_preserves_storage_precision() {
    for (meaning, precision, padded) in [
        (
            ImageMeaning::Coverage {},
            ChannelPrecision::Unorm8,
            ImageStorage::Rgba8Unorm,
        ),
        (
            ImageMeaning::Data {
                channels: DataChannels::Vector2,
            },
            ChannelPrecision::Float16,
            ImageStorage::Rgba16Float,
        ),
        (
            ImageMeaning::Data {
                channels: DataChannels::Scalar,
            },
            ChannelPrecision::Float32,
            ImageStorage::Rgba32Float,
        ),
    ] {
        let contract = ImageContract::new(meaning, precision);
        let chosen = contract
            .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                if storage == padded {
                    ImageUsage::EFFECT_TARGET
                } else {
                    ImageUsage::default()
                }
            })
            .unwrap();
        assert_eq!(chosen, padded);
    }
    let float = ImageContract::new(
        ImageMeaning::Data {
            channels: DataChannels::Scalar,
        },
        ChannelPrecision::Float16,
    );
    assert!(
        float
            .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                if storage == ImageStorage::R8Unorm || storage == ImageStorage::R32Float {
                    ImageUsage::EFFECT_TARGET
                } else {
                    ImageUsage::default()
                }
            })
            .is_err(),
        "neither quantization nor a different float rounding boundary is a padding fallback"
    );
}

#[test]
fn filter_blend_and_copy_failures_are_reported_separately_without_inventing_support() {
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Straight,
        },
        ChannelPrecision::Float16,
    );
    let available = ImageUsage {
        sampled: true,
        render: true,
        ..Default::default()
    };
    let error = contract
        .negotiate(ImageUsage::EFFECT_TARGET, |_| available)
        .unwrap_err();
    let ImageFormatError::Unsupported { missing, .. } = error else {
        panic!("expected capability refusal")
    };
    assert_eq!(missing.len(), 1);
    let required = missing[0].1;
    assert!(!required.sampled && !required.render);
    assert!(required.linear_filter && required.blend && required.copy_src && required.copy_dst);
    assert!(matches!(
        contract.negotiate(
            ImageUsage {
                linear_filter: true,
                ..Default::default()
            },
            |_| available
        ),
        Err(ImageFormatError::InvalidUsage(_))
    ));
    assert!(matches!(
        contract.negotiate(
            ImageUsage {
                blend: true,
                ..Default::default()
            },
            |_| available
        ),
        Err(ImageFormatError::InvalidUsage(_))
    ));
}

#[test]
fn unsupported_versions_and_unknown_fields_cannot_silently_select_a_format() {
    let contract = ImageContract::new(ImageMeaning::Coverage {}, ChannelPrecision::Unorm8);
    assert_eq!(contract.version(), IMAGE_CONTRACT_VERSION);
    let mut value = serde_json::to_value(contract).unwrap();
    value["version"] = 2.into();
    let future: ImageContract = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        future.preferred_storage(),
        Err(ImageFormatError::UnsupportedVersion(2))
    );
    assert_eq!(
        future.negotiate(ImageUsage::default(), |_| ImageUsage::EFFECT_TARGET),
        Err(ImageFormatError::UnsupportedVersion(2))
    );
    value["version"] = 1.into();
    value["silentFallback"] = true.into();
    assert!(serde_json::from_value::<ImageContract>(value).is_err());
}

#[test]
fn storage_capacity_rejects_empty_and_overflowing_extents() {
    assert_eq!(
        ImageStorage::Rgba16Float.capacity(1920, 1080),
        Some(1920 * 1080 * 8)
    );
    assert_eq!(
        ImageStorage::R16Float.capacity(1920, 1080),
        Some(1920 * 1080 * 2)
    );
    assert_eq!(ImageStorage::Rgba32Float.capacity(u32::MAX, u32::MAX), None);
    assert_eq!(ImageStorage::R8Unorm.capacity(0, 1), None);
    assert_eq!(ImageStorage::R8Unorm.capacity(1, 0), None);
}

#[test]
fn unorm16_is_fixed_point_for_color_coverage_and_numeric_channels() {
    for (meaning, expected) in [
        (ImageMeaning::Coverage {}, ImageStorage::R16Unorm),
        (
            ImageMeaning::Data {
                channels: DataChannels::Scalar,
            },
            ImageStorage::R16Unorm,
        ),
        (
            ImageMeaning::Data {
                channels: DataChannels::Vector2,
            },
            ImageStorage::Rg16Unorm,
        ),
        (
            ImageMeaning::Data {
                channels: DataChannels::Vector3,
            },
            ImageStorage::Rgba16Unorm,
        ),
        (
            ImageMeaning::Data {
                channels: DataChannels::Vector4,
            },
            ImageStorage::Rgba16Unorm,
        ),
    ]
    .into_iter()
    .chain(
        [WorkingSpace::Linear, WorkingSpace::Srgb]
            .into_iter()
            .flat_map(|space| {
                [AlphaMode::Straight, AlphaMode::Premultiplied].map(move |alpha| {
                    (
                        ImageMeaning::Color { space, alpha },
                        ImageStorage::Rgba16Unorm,
                    )
                })
            }),
    ) {
        let contract = ImageContract::new(meaning, ChannelPrecision::Unorm16);
        assert_eq!(contract.preferred_storage().unwrap(), expected);
        let encoded = serde_json::to_value(contract).unwrap();
        assert_eq!(encoded["precision"], "unorm16");
        assert_eq!(
            serde_json::from_value::<ImageContract>(encoded).unwrap(),
            contract
        );
    }
    assert_eq!(
        coverage_storage(ChannelPrecision::Unorm16),
        ImageStorage::R16Unorm
    );
    for (storage, bpp) in [
        (ImageStorage::R16Unorm, 2),
        (ImageStorage::Rg16Unorm, 4),
        (ImageStorage::Rgba16Unorm, 8),
    ] {
        assert_eq!(storage.bytes_per_pixel(), bpp);
        assert_eq!(storage.capacity(17, 3), Some(17 * 3 * bpp));
        assert_eq!(storage.capacity(0, 3), None);
    }
    assert_eq!(ImageStorage::Rgba16Unorm.capacity(u32::MAX, u32::MAX), None);
}

#[test]
fn all_four_precisions_preserve_color_alpha_and_coverage_without_fixed_pool_slots() {
    for (precision, color, coverage, color_bytes, coverage_bytes) in [
        (
            ChannelPrecision::Unorm8,
            ImageStorage::Rgba8UnormSrgb,
            ImageStorage::R8Unorm,
            4,
            1,
        ),
        (
            ChannelPrecision::Unorm16,
            ImageStorage::Rgba16Unorm,
            ImageStorage::R16Unorm,
            8,
            2,
        ),
        (
            ChannelPrecision::Float16,
            ImageStorage::Rgba16Float,
            ImageStorage::R16Float,
            8,
            2,
        ),
        (
            ChannelPrecision::Float32,
            ImageStorage::Rgba32Float,
            ImageStorage::R32Float,
            16,
            4,
        ),
    ] {
        for alpha in [AlphaMode::Straight, AlphaMode::Premultiplied] {
            let contract = ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha,
                },
                precision,
            );
            assert_eq!(contract.preferred_storage().unwrap(), color);
            assert_eq!(color.capacity(19, 7), Some(19 * 7 * color_bytes));
        }
        assert_eq!(coverage_storage(precision), coverage);
        assert_eq!(
            ImageContract::new(ImageMeaning::Coverage {}, precision)
                .preferred_storage()
                .unwrap(),
            coverage
        );
        assert_eq!(coverage.capacity(19, 7), Some(19 * 7 * coverage_bytes));
    }
}

#[test]
fn unorm16_padding_never_changes_fixed_point_rounding_to_float_or_eight_bit() {
    for channels in [DataChannels::Scalar, DataChannels::Vector2] {
        let contract =
            ImageContract::new(ImageMeaning::Data { channels }, ChannelPrecision::Unorm16);
        assert_eq!(
            contract
                .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                    if storage == ImageStorage::Rgba16Unorm {
                        ImageUsage::EFFECT_TARGET
                    } else {
                        ImageUsage::default()
                    }
                })
                .unwrap(),
            ImageStorage::Rgba16Unorm
        );
        let error = contract
            .negotiate(ImageUsage::EFFECT_TARGET, |storage| {
                if matches!(
                    storage,
                    ImageStorage::Rgba8Unorm
                        | ImageStorage::Rgba16Float
                        | ImageStorage::Rgba32Float
                ) {
                    ImageUsage::EFFECT_TARGET
                } else {
                    ImageUsage::default()
                }
            })
            .unwrap_err();
        let ImageFormatError::Unsupported { missing, .. } = error else {
            panic!("expected refusal")
        };
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[1].0, ImageStorage::Rgba16Unorm);
    }
}
