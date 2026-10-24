//! Enabled-device capabilities for semantic image storage. Adapter extensions
//! are usable only when the corresponding device features were enabled.
use motion_effects::image_contract::{
    ChannelPrecision, ImageContract, ImageFormatError, ImageStorage, ImageUsage,
};
use std::collections::{BTreeMap, BTreeSet};

/// Linear premultiplied working color. Only UNORM8 uses an automatic transfer
/// to preserve more useful shadow steps in its limited storage range.
pub const fn color_storage(precision: ChannelPrecision) -> ImageStorage {
    match precision {
        ChannelPrecision::Unorm8 => ImageStorage::Rgba8UnormSrgb,
        ChannelPrecision::Unorm16 => ImageStorage::Rgba16Unorm,
        ChannelPrecision::Float16 => ImageStorage::Rgba16Float,
        ChannelPrecision::Float32 => ImageStorage::Rgba32Float,
    }
}

/// Vector MSAA and group composition accumulate linear samples directly.
pub const fn raster_storage(precision: ChannelPrecision) -> ImageStorage {
    match precision {
        ChannelPrecision::Unorm8 => ImageStorage::Rgba8Unorm,
        _ => color_storage(precision),
    }
}

const STORAGES: [ImageStorage; 13] = [
    ImageStorage::R8Unorm,
    ImageStorage::Rg8Unorm,
    ImageStorage::Rgba8Unorm,
    ImageStorage::Rgba8UnormSrgb,
    ImageStorage::R16Unorm,
    ImageStorage::Rg16Unorm,
    ImageStorage::Rgba16Unorm,
    ImageStorage::R16Float,
    ImageStorage::Rg16Float,
    ImageStorage::Rgba16Float,
    ImageStorage::R32Float,
    ImageStorage::Rg32Float,
    ImageStorage::Rgba32Float,
];

pub const fn native_format(storage: ImageStorage) -> wgpu::TextureFormat {
    match storage {
        ImageStorage::R8Unorm => wgpu::TextureFormat::R8Unorm,
        ImageStorage::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
        ImageStorage::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        ImageStorage::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        ImageStorage::R16Unorm => wgpu::TextureFormat::R16Unorm,
        ImageStorage::Rg16Unorm => wgpu::TextureFormat::Rg16Unorm,
        ImageStorage::Rgba16Unorm => wgpu::TextureFormat::Rgba16Unorm,
        ImageStorage::R16Float => wgpu::TextureFormat::R16Float,
        ImageStorage::Rg16Float => wgpu::TextureFormat::Rg16Float,
        ImageStorage::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        ImageStorage::R32Float => wgpu::TextureFormat::R32Float,
        ImageStorage::Rg32Float => wgpu::TextureFormat::Rg32Float,
        ImageStorage::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
    }
}

/// Only supported color targets have a working precision. A numeric/depth or
/// otherwise unsupported target must not silently select the 8-bit mode.
pub const fn color_precision(format: wgpu::TextureFormat) -> Option<ChannelPrecision> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb => Some(ChannelPrecision::Unorm8),
        wgpu::TextureFormat::Rgba16Unorm => Some(ChannelPrecision::Unorm16),
        wgpu::TextureFormat::Rgba16Float => Some(ChannelPrecision::Float16),
        wgpu::TextureFormat::Rgba32Float => Some(ChannelPrecision::Float32),
        _ => None,
    }
}

/// Features needed by the fixed-function filtering/blending working path.
/// Manual sampling/blending paths can negotiate their narrower actual usage.
pub fn working_features(precision: ChannelPrecision) -> wgpu::Features {
    match precision {
        ChannelPrecision::Unorm8 | ChannelPrecision::Float16 => wgpu::Features::empty(),
        ChannelPrecision::Unorm16 => {
            wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
                | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        }
        ChannelPrecision::Float32 => {
            wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE
        }
    }
}

pub fn capabilities(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    storage: ImageStorage,
) -> ImageUsage {
    let format = native_format(storage);
    let features = device.features();
    enabled_capabilities(
        storage,
        reported_features(adapter, format, features),
        features,
    )
}

fn reported_features(
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    features: wgpu::Features,
) -> wgpu::TextureFormatFeatures {
    if features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES) {
        adapter.get_texture_format_features(format)
    } else {
        format.guaranteed_format_features(features)
    }
}

fn enabled_capabilities(
    storage: ImageStorage,
    reported: wgpu::TextureFormatFeatures,
    features: wgpu::Features,
) -> ImageUsage {
    if matches!(
        storage,
        ImageStorage::R16Unorm | ImageStorage::Rg16Unorm | ImageStorage::Rgba16Unorm
    ) && !features.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
    {
        return ImageUsage::default();
    }
    let float32 = matches!(
        storage,
        ImageStorage::R32Float | ImageStorage::Rg32Float | ImageStorage::Rgba32Float
    );
    let usage = reported.allowed_usages;
    ImageUsage {
        sampled: usage.contains(wgpu::TextureUsages::TEXTURE_BINDING),
        linear_filter: reported
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
            && (!float32 || features.contains(wgpu::Features::FLOAT32_FILTERABLE)),
        render: usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT),
        blend: reported
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            && (!float32 || features.contains(wgpu::Features::FLOAT32_BLENDABLE)),
        copy_src: usage.contains(wgpu::TextureUsages::COPY_SRC),
        copy_dst: usage.contains(wgpu::TextureUsages::COPY_DST),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("image storage {storage:?} lacks enabled device capabilities {missing:?}")]
pub struct UnsupportedStorage {
    pub storage: ImageStorage,
    pub missing: ImageUsage,
}

#[derive(Debug, thiserror::Error)]
#[error("image storage {storage:?} lacks enabled {samples}-sample attachment and resolve support")]
pub struct UnsupportedSamples {
    pub storage: ImageStorage,
    pub samples: u32,
}

/// One bounded snapshot per device. Healthy frame preparation never queries
/// the adapter again; at most thirteen storage entries are retained.
pub struct ImageCapabilities {
    available: BTreeMap<ImageStorage, ImageUsage>,
    msaa4: BTreeSet<ImageStorage>,
}
impl ImageCapabilities {
    pub fn query(adapter: &wgpu::Adapter, device: &wgpu::Device) -> Self {
        let mut available = BTreeMap::new();
        let mut msaa4 = BTreeSet::new();
        for storage in STORAGES {
            let reported = reported_features(adapter, native_format(storage), device.features());
            let usage = enabled_capabilities(storage, reported, device.features());
            if usage.render
                && reported.flags.sample_count_supported(4)
                && reported
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
            {
                msaa4.insert(storage);
            }
            available.insert(storage, usage);
        }
        Self { available, msaa4 }
    }
    pub fn available(&self, storage: ImageStorage) -> ImageUsage {
        self.available.get(&storage).copied().unwrap_or_default()
    }
    pub fn negotiate(
        &self,
        contract: ImageContract,
        usage: ImageUsage,
    ) -> Result<ImageStorage, ImageFormatError> {
        contract.negotiate(usage, |storage| self.available(storage))
    }
    pub fn require(
        &self,
        storage: ImageStorage,
        usage: ImageUsage,
    ) -> Result<(), UnsupportedStorage> {
        let missing = usage.missing_from(self.available(storage));
        if missing == ImageUsage::default() {
            Ok(())
        } else {
            Err(UnsupportedStorage { storage, missing })
        }
    }
    pub fn require_msaa4(&self, storage: ImageStorage) -> Result<(), UnsupportedSamples> {
        if self.msaa4.contains(&storage) {
            Ok(())
        } else {
            Err(UnsupportedSamples {
                storage,
                samples: 4,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_support_does_not_imply_four_sample_resolve() {
        let mut snapshot = ImageCapabilities {
            available: BTreeMap::from([(ImageStorage::Rgba32Float, ImageUsage::EFFECT_TARGET)]),
            msaa4: BTreeSet::new(),
        };
        snapshot
            .require(ImageStorage::Rgba32Float, ImageUsage::EFFECT_TARGET)
            .unwrap();
        let error = snapshot
            .require_msaa4(ImageStorage::Rgba32Float)
            .unwrap_err();
        assert_eq!(error.storage, ImageStorage::Rgba32Float);
        assert_eq!(error.samples, 4);
        snapshot.msaa4.insert(ImageStorage::Rgba32Float);
        snapshot.require_msaa4(ImageStorage::Rgba32Float).unwrap();
        assert!(snapshot.require_msaa4(ImageStorage::Rgba16Float).is_err());
    }

    #[test]
    fn native_storage_sizes_and_color_precision_are_explicit() {
        for storage in STORAGES {
            assert_eq!(
                native_format(storage).block_copy_size(None).map(u64::from),
                Some(storage.bytes_per_pixel())
            );
        }
        for (format, expected) in [
            (
                wgpu::TextureFormat::Rgba8UnormSrgb,
                ChannelPrecision::Unorm8,
            ),
            (wgpu::TextureFormat::Rgba16Unorm, ChannelPrecision::Unorm16),
            (wgpu::TextureFormat::Rgba16Float, ChannelPrecision::Float16),
            (wgpu::TextureFormat::Rgba32Float, ChannelPrecision::Float32),
        ] {
            assert_eq!(color_precision(format), Some(expected));
        }
        assert_eq!(color_precision(wgpu::TextureFormat::R32Float), None);
        assert_eq!(color_precision(wgpu::TextureFormat::Depth32Float), None);
    }

    #[test]
    fn adapter_reports_cannot_enable_float32_filtering_or_blending() {
        let reported = wgpu::TextureFormatFeatures {
            allowed_usages: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            flags: wgpu::TextureFormatFeatureFlags::FILTERABLE
                | wgpu::TextureFormatFeatureFlags::BLENDABLE,
        };
        for storage in [
            ImageStorage::R32Float,
            ImageStorage::Rg32Float,
            ImageStorage::Rgba32Float,
        ] {
            let disabled = enabled_capabilities(storage, reported, wgpu::Features::empty());
            assert!(disabled.sampled && disabled.render);
            assert!(!disabled.linear_filter && !disabled.blend);
            let filter =
                enabled_capabilities(storage, reported, wgpu::Features::FLOAT32_FILTERABLE);
            assert!(filter.linear_filter && !filter.blend);
            let blend = enabled_capabilities(storage, reported, wgpu::Features::FLOAT32_BLENDABLE);
            assert!(!blend.linear_filter && blend.blend);
        }
    }

    #[test]
    fn norm16_reports_require_enabled_features_and_actual_attachment_support() {
        let reported = wgpu::TextureFormatFeatures {
            allowed_usages: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            flags: wgpu::TextureFormatFeatureFlags::FILTERABLE
                | wgpu::TextureFormatFeatureFlags::BLENDABLE,
        };
        for storage in [
            ImageStorage::R16Unorm,
            ImageStorage::Rg16Unorm,
            ImageStorage::Rgba16Unorm,
        ] {
            for features in [
                wgpu::Features::empty(),
                wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ] {
                assert_eq!(
                    enabled_capabilities(storage, reported, features),
                    ImageUsage::default()
                );
            }
            let norm = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM;
            assert_eq!(
                enabled_capabilities(storage, reported, norm),
                ImageUsage::EFFECT_TARGET
            );
            let guaranteed = native_format(storage).guaranteed_format_features(norm);
            assert!(!enabled_capabilities(storage, guaranteed, norm).render);
        }
        assert_eq!(
            working_features(ChannelPrecision::Unorm16),
            wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
                | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        );
    }

    #[test]
    fn missing_storage_is_explicit_and_exact_usage_is_required() {
        let snapshot = ImageCapabilities {
            msaa4: BTreeSet::new(),
            available: BTreeMap::from([(
                ImageStorage::Rgba32Float,
                ImageUsage {
                    sampled: true,
                    render: true,
                    ..Default::default()
                },
            )]),
        };
        let missing = snapshot
            .require(ImageStorage::Rgba32Float, ImageUsage::EFFECT_TARGET)
            .unwrap_err();
        assert_eq!(missing.storage, ImageStorage::Rgba32Float);
        assert!(
            missing.missing.linear_filter
                && missing.missing.blend
                && missing.missing.copy_src
                && missing.missing.copy_dst
        );
        assert!(!missing.missing.sampled && !missing.missing.render);
        let absent = snapshot
            .require(ImageStorage::R16Float, ImageUsage::EFFECT_TARGET)
            .unwrap_err();
        assert_eq!(absent.missing, ImageUsage::EFFECT_TARGET);
    }
}
