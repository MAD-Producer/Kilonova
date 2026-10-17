//! Validate native plane geometry before ffmpeg-next constructs a Rust slice.
use ffmpeg_next::{self as av, frame};
use motion_host::Result;

/// Only used with the supported software pixel formats. A positive stride and
/// the entire `stride * plane_height` span must fit its live AVBufferRef: that
/// is the span ffmpeg-next's data/data_mut methods expose, including padding.
pub(crate) fn checked_stride(
    frame: &frame::Video,
    index: usize,
    row_bytes: usize,
) -> Result<usize> {
    // SAFETY: Video owns a live AVFrame. Read its signed metadata, without
    // constructing a slice from any native data pointer.
    let native = unsafe { frame.as_ptr().as_ref() }.ok_or("missing native video frame")?;
    if native.width <= 0
        || native.height <= 0
        || native.width as u32 > motion_core::MAX_VIDEO_DIMENSION
        || native.height as u32 > motion_core::MAX_VIDEO_DIMENSION
    {
        return Err("invalid native video dimensions".into());
    }
    let signed_stride = *native
        .linesize
        .get(index)
        .ok_or_else(|| format!("video plane {index}: plane index outside native frame"))?;
    let stride = usize::try_from(signed_stride)
        .ok()
        .filter(|stride| *stride > 0)
        .ok_or_else(|| format!("video plane {index}: invalid row stride {signed_stride}"))?;
    if index >= frame.planes() || row_bytes == 0 || stride < row_bytes {
        return Err(format!(
            "video plane {index}: row stride is shorter than its pixels"
        ));
    }
    let pointer = native.data[index];
    if pointer.is_null() {
        return Err(format!("video plane {index}: missing pixel data"));
    }
    let bytes = stride
        .checked_mul(frame.plane_height(index) as usize)
        .filter(|bytes| *bytes > 0 && *bytes <= isize::MAX as usize)
        .ok_or_else(|| format!("video plane {index}: plane byte span overflow"))?;
    // SAFETY: The AVFrame is still borrowed and index is bounded by its plane
    // count and eight metadata slots. This API returns a borrowed buffer ref;
    // it does not construct a pixel slice or transfer ownership.
    let buffer =
        unsafe { av::ffi::av_frame_get_plane_buffer(frame.as_ptr(), index as i32).as_ref() }
            .ok_or_else(|| format!("video plane {index}: missing backing buffer"))?;
    let start = pointer as usize;
    let base = buffer.data as usize;
    let end = start.checked_add(bytes);
    let buffer_end = base.checked_add(buffer.size);
    if buffer.data.is_null()
        || start < base
        || !matches!((end, buffer_end), (Some(end), Some(limit)) if end <= limit)
    {
        return Err(format!(
            "video plane {index}: plane byte span exceeds backing buffer"
        ));
    }
    Ok(stride)
}

#[cfg(test)]
mod tests {
    use super::*;
    use av::format::Pixel;

    #[test]
    fn padded_native_planes_are_valid_for_planar_and_interleaved_chroma() {
        for pixel in [Pixel::YUV420P, Pixel::YUVJ420P, Pixel::NV12, Pixel::NV21] {
            let frame = frame::Video::new(pixel, 18, 10);
            assert!(checked_stride(&frame, 0, 18).unwrap() >= 18);
            if matches!(pixel, Pixel::NV12 | Pixel::NV21) {
                assert!(checked_stride(&frame, 1, 18).unwrap() >= 18);
            } else {
                assert!(checked_stride(&frame, 1, 9).unwrap() >= 9);
                assert!(checked_stride(&frame, 2, 9).unwrap() >= 9);
            }
        }
    }

    #[test]
    fn negative_zero_short_and_oversized_strides_fail_without_slicing_pixels() {
        let mut frame = frame::Video::new(Pixel::YUV420P, 18, 10);
        for index in 0..3 {
            let row_bytes = if index == 0 { 18 } else { 9 };
            // SAFETY: Only mutate owned AVFrame metadata, restoring it before
            // inspecting results. Never construct a slice with a bad layout.
            let native = unsafe { &mut *frame.as_mut_ptr() };
            let original = native.linesize[index];
            for stride in [-1, i32::MIN, 0, row_bytes as i32 - 1, i32::MAX] {
                unsafe {
                    (*frame.as_mut_ptr()).linesize[index] = stride;
                }
                let result = checked_stride(&frame, index, row_bytes);
                unsafe {
                    (*frame.as_mut_ptr()).linesize[index] = original;
                }
                let error = result.unwrap_err();
                assert!(error.contains(&format!("video plane {index}:")), "{error}");
            }
        }
    }

    #[test]
    fn null_and_out_of_buffer_plane_addresses_are_rejected() {
        let mut frame = frame::Video::new(Pixel::YUV420P, 18, 10);
        for index in 0..3 {
            // SAFETY: The buffer remains owned by frame. Move only the plane
            // address inside that allocation, or to null; no pixel is accessed.
            let native = unsafe { &mut *frame.as_mut_ptr() };
            let original = native.data[index];
            let buffer =
                unsafe { &*av::ffi::av_frame_get_plane_buffer(frame.as_ptr(), index as i32) };
            let tail = buffer.data.wrapping_add(buffer.size - 1);
            for pointer in [std::ptr::null_mut(), tail] {
                unsafe {
                    (*frame.as_mut_ptr()).data[index] = pointer;
                }
                let result = checked_stride(&frame, index, if index == 0 { 18 } else { 9 });
                unsafe {
                    (*frame.as_mut_ptr()).data[index] = original;
                }
                assert!(result.is_err(), "plane {index} pointer {pointer:p}");
            }
        }
    }

    #[test]
    fn invalid_dimensions_missing_planes_and_unbacked_data_are_rejected() {
        let mut frame = frame::Video::new(Pixel::RGBA, 18, 10);
        assert!(checked_stride(&frame, 1, 18).is_err());
        assert!(checked_stride(&frame, usize::MAX, 18).is_err());
        assert!(checked_stride(&frame, 0, 0).is_err());
        // SAFETY: Keep and restore each native field while the allocation and
        // buffer reference remain live; invalid metadata never reaches data().
        for dimension in [0, -1, i32::MAX] {
            unsafe {
                (*frame.as_mut_ptr()).height = dimension;
            }
            let result = checked_stride(&frame, 0, 72);
            unsafe {
                (*frame.as_mut_ptr()).height = 10;
            }
            assert!(result.unwrap_err().contains("dimensions"));
        }
        let buffer = unsafe { (*frame.as_ptr()).buf[0] };
        unsafe {
            (*frame.as_mut_ptr()).buf[0] = std::ptr::null_mut();
        }
        let result = checked_stride(&frame, 0, 72);
        unsafe {
            (*frame.as_mut_ptr()).buf[0] = buffer;
        }
        assert!(result.unwrap_err().contains("backing buffer"));
        assert!(checked_stride(&frame, 0, 72).is_ok());
    }
}
