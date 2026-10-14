//! Pure ImageReader metadata and RGB fallback adaptation, tested without NDK.
use motion_core::VideoAsset;
use motion_render::{rgb_video_to_rgba, VideoPlane, VideoRgbFormat};

pub(crate) fn validate_decoder(
    asset: &VideoAsset,
    timestamps: &[u64],
    reader_kind: usize,
) -> Result<(), String> {
    asset.validate().map_err(|error| error.to_string())?;
    if reader_kind > 3 || timestamps.is_empty() {
        return Err("invalid video decoder format or index".into());
    }
    Ok(())
}

pub(crate) fn crop_origin(
    asset: &VideoAsset,
    source_size: [i32; 2],
    crop: [i32; 4],
) -> Result<[u32; 2], String> {
    asset.validate().map_err(|error| error.to_string())?;
    let [left, top, right, bottom] = crop;
    if left < 0
        || top < 0
        || right > source_size[0]
        || bottom > source_size[1]
        || right.checked_sub(left) != Some(asset.width as i32)
        || bottom.checked_sub(top) != Some(asset.height as i32)
    {
        return Err("decoded crop differs from source metadata".into());
    }
    Ok([left as u32, top as u32])
}

pub(crate) fn rgb(
    asset: &VideoAsset,
    source_size: [i32; 2],
    crop: [i32; 4],
    format: VideoRgbFormat,
    plane: VideoPlane<'_>,
) -> Result<Vec<u8>, String> {
    let origin = crop_origin(asset, source_size, crop)?;
    rgb_video_to_rgba(
        [asset.width, asset.height],
        asset.rotation,
        origin,
        [source_size[0] as u32, source_size[1] as u32],
        format,
        plane,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_render::{ChromaLayout, Yuv420Frame};

    fn asset(rotation: u32) -> VideoAsset {
        let (display_width, display_height) = if rotation % 180 == 0 { (3, 2) } else { (2, 3) };
        VideoAsset {
            id: 1,
            path: "assets/video.mp4".into(),
            bytes: 1,
            mime: "video/avc".into(),
            track: 0,
            width: 3,
            height: 2,
            rotation,
            display_width,
            display_height,
            video_start_us: 0,
            video_end_us: 1_000_000,
            duration_us: 1_000_000,
            frame_count: 1,
            variable_frame_rate: false,
            nominal_frame_rate: 1.,
            color_standard: 1,
            color_range: 1,
            audio_asset: None,
        }
    }

    #[test]
    fn rgb_fallback_rotations_match_literal_goldens_and_yuv_export() {
        let shades = [10, 20, 30, 40, 50, 60];
        let pixels: Vec<_> = shades
            .into_iter()
            .flat_map(|value| [value, value, value, 7])
            .collect();
        for (rotation, order) in [
            (0, [10, 20, 30, 40, 50, 60]),
            (90, [40, 10, 50, 20, 60, 30]),
            (180, [60, 50, 40, 30, 20, 10]),
            (270, [30, 60, 20, 50, 10, 40]),
        ] {
            let expected: Vec<_> = order
                .into_iter()
                .flat_map(|value| [value, value, value, 255])
                .collect();
            let actual = rgb(
                &asset(rotation),
                [3, 2],
                [0, 0, 3, 2],
                VideoRgbFormat::Rgba8888,
                VideoPlane {
                    data: &pixels,
                    row_stride: 12,
                    pixel_stride: 4,
                },
            )
            .unwrap();
            assert_eq!(actual, expected, "rotation {rotation}");
            let yuv = Yuv420Frame {
                width: 3,
                height: 2,
                rotation,
                standard: 1,
                range: 1,
                phase: [0, 0],
                y: shades.to_vec(),
                uv: vec![128; 4],
                chroma_layout: ChromaLayout::Uv,
            };
            assert_eq!(actual, yuv.to_rgba().unwrap());
        }
    }

    #[test]
    fn padded_offset_rgb565_plane_keeps_crop_colors_and_opaque_alpha() {
        let mut source = vec![0; 28];
        for (at, value) in [
            (10, 0xf800u16),
            (14, 0x07e0),
            (18, 0x001f),
            (22, 0xffff),
            (26, 0),
        ] {
            source[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut video = asset(90);
        video.width = 2;
        video.height = 2;
        video.display_width = 2;
        video.display_height = 2;
        let actual = rgb(
            &video,
            [3, 3],
            [1, 1, 3, 3],
            VideoRgbFormat::Rgb565,
            VideoPlane {
                data: &source,
                row_stride: 4,
                pixel_stride: 2,
            },
        );
        // This crop would cross each row's declared extent and must refuse.
        assert!(actual.is_err());
        let mut source = vec![0; 38];
        for (at, value) in [(16, 0xf800u16), (20, 0x07e0), (28, 0x001f), (32, 0xffff)] {
            source[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        let actual = rgb(
            &video,
            [3, 3],
            [1, 1, 3, 3],
            VideoRgbFormat::Rgb565,
            VideoPlane {
                data: &source,
                row_stride: 12,
                pixel_stride: 4,
            },
        )
        .unwrap();
        assert_eq!(
            actual,
            [0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 255, 255, 0, 255, 0, 255]
        );
    }

    #[test]
    fn invalid_asset_rotation_display_size_and_extent_refuse_before_pixel_access() {
        let plane = || VideoPlane {
            data: &[],
            row_stride: 12,
            pixel_stride: 4,
        };
        for rotation in [1, 45, 89, 91, 181, 271, 360, u32::MAX] {
            let mut video = asset(0);
            video.rotation = rotation;
            assert!(validate_decoder(&video, &[0], 0).is_err());
            assert!(rgb(
                &video,
                [3, 2],
                [0, 0, 3, 2],
                VideoRgbFormat::Rgba8888,
                plane()
            )
            .is_err());
        }
        for dimensions in [[0, 2], [3, 0], [u32::MAX, u32::MAX], [8192, 8192]] {
            let mut video = asset(0);
            [video.display_width, video.display_height] = dimensions;
            assert!(validate_decoder(&video, &[0], 0).is_err());
            assert!(rgb(
                &video,
                [3, 2],
                [0, 0, 3, 2],
                VideoRgbFormat::Rgba8888,
                plane()
            )
            .is_err());
            [video.width, video.height] = dimensions;
            assert!(validate_decoder(&video, &[0], 0).is_err());
            assert!(rgb(
                &video,
                [3, 2],
                [0, 0, 3, 2],
                VideoRgbFormat::Rgba8888,
                plane()
            )
            .is_err());
        }
        assert!(validate_decoder(&asset(0), &[], 0).is_err());
        assert!(validate_decoder(&asset(0), &[0], 4).is_err());
        for reader_kind in 0..=3 {
            validate_decoder(&asset(0), &[0], reader_kind).unwrap();
        }
    }

    #[test]
    fn crop_bounds_and_arithmetic_overflow_are_errors() {
        let video = asset(0);
        for (source, crop) in [
            ([3, 2], [-1, 0, 2, 2]),
            ([3, 2], [0, -1, 3, 1]),
            ([3, 2], [1, 0, 4, 2]),
            ([3, 2], [0, 1, 3, 3]),
            ([3, 2], [0, 0, 2, 2]),
            ([-1, 2], [0, 0, 3, 2]),
            ([i32::MAX, i32::MAX], [i32::MIN, 0, i32::MAX, 2]),
            ([i32::MAX, i32::MAX], [0, i32::MIN, 3, i32::MAX]),
        ] {
            assert!(
                crop_origin(&video, source, crop).is_err(),
                "{source:?} {crop:?}"
            );
        }
        for (row_stride, pixel_stride) in
            [(0, 4), (12, 0), (12, 3), (usize::MAX, 4), (12, usize::MAX)]
        {
            assert!(rgb(
                &video,
                [3, 2],
                [0, 0, 3, 2],
                VideoRgbFormat::Rgba8888,
                VideoPlane {
                    data: &[0; 24],
                    row_stride,
                    pixel_stride
                }
            )
            .is_err());
        }
        assert!(rgb(
            &video,
            [3, 2],
            [0, 0, 3, 2],
            VideoRgbFormat::Rgba8888,
            VideoPlane {
                data: &[0; 23],
                row_stride: 12,
                pixel_stride: 4
            }
        )
        .is_err());
    }

    #[test]
    fn shared_rgb_geometry_rejects_adversarial_dimensions_rotation_and_crop_without_allocating() {
        for width in [0, 1, 3, 8192, u32::MAX] {
            for height in [0, 1, 2, 8192, u32::MAX] {
                for rotation in [0, 90, 180, 270, 45, u32::MAX] {
                    for crop in [[0, 0], [u32::MAX, 0], [0, u32::MAX]] {
                        assert!(rgb_video_to_rgba(
                            [width, height],
                            rotation,
                            crop,
                            [u32::MAX; 2],
                            VideoRgbFormat::Rgba8888,
                            VideoPlane {
                                data: &[],
                                row_stride: usize::MAX,
                                pixel_stride: usize::MAX
                            }
                        )
                        .is_err());
                    }
                }
            }
        }
    }
}
