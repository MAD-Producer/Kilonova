//! Owned SDR YUV pixels. Preview uploads these planes; RGBA conversion is only
//! used by thumbnail/JNI export consumers that explicitly request CPU pixels.
use crate::{renderer::Result, RenderError};

pub struct VideoPlane<'a> {
    pub data: &'a [u8],
    pub row_stride: usize,
    pub pixel_stride: usize,
}

/// Android RGB fallback formats; output remains opaque SDR RGBA.
#[derive(Clone, Copy, Debug)]
pub enum VideoRgbFormat {
    Rgba8888,
    Rgb565,
}

struct PixelLayout {
    width: u32,
    height: u32,
    rotation: u32,
    display_width: u32,
    bytes: usize,
}
impl PixelLayout {
    fn new(width: u32, height: u32, rotation: u32) -> Result<Self> {
        if width == 0 || height == 0 || width > 8192 || height > 8192
            || u64::from(width) * u64::from(height) > 16 * 1024 * 1024
            || !matches!(rotation, 0 | 90 | 180 | 270)
        {
            return Err(RenderError::Invalid("invalid video pixel geometry".into()));
        }
        let bytes = (width as usize).checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| RenderError::Invalid("video RGBA size overflow".into()))?;
        Ok(Self { width, height, rotation,
            display_width: if rotation % 180 == 0 { width } else { height }, bytes })
    }
    // Only called with coordinates from the validated source rectangle.
    fn offset(&self, x: u32, y: u32) -> usize {
        let (dx, dy) = match self.rotation {
            90 => (self.height - 1 - y, x),
            180 => (self.width - 1 - x, self.height - 1 - y),
            270 => (y, self.width - 1 - x),
            _ => (x, y),
        };
        (dy as usize * self.display_width as usize + dx as usize) * 4
    }
}

/// Pack a cropped RGB plane with the same rotation mapping as YUV export.
/// Source and plane extents are checked before allocating the destination.
pub fn rgb_video_to_rgba(
    size: [u32; 2], rotation: u32, crop: [u32; 2], source_size: [u32; 2],
    format: VideoRgbFormat, plane: VideoPlane<'_>,
) -> Result<Vec<u8>> {
    let layout = PixelLayout::new(size[0], size[1], rotation)?;
    let invalid = || RenderError::Invalid("invalid RGB crop or plane extent".into());
    let right = crop[0].checked_add(size[0]).ok_or_else(invalid)?;
    let bottom = crop[1].checked_add(size[1]).ok_or_else(invalid)?;
    let pixel_bytes = match format { VideoRgbFormat::Rgba8888 => 4, VideoRgbFormat::Rgb565 => 2 };
    if right > source_size[0] || bottom > source_size[1] || plane.row_stride == 0
        || plane.pixel_stride < pixel_bytes
    { return Err(invalid()); }
    let row_end = (right as usize - 1).checked_mul(plane.pixel_stride)
        .and_then(|last| last.checked_add(pixel_bytes)).ok_or_else(invalid)?;
    let end = (bottom as usize - 1).checked_mul(plane.row_stride)
        .and_then(|last| last.checked_add(row_end)).ok_or_else(invalid)?;
    if row_end > plane.row_stride || end > plane.data.len() { return Err(invalid()); }
    let mut rgba = vec![0; layout.bytes];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let at = (y as usize + crop[1] as usize) * plane.row_stride
                + (x as usize + crop[0] as usize) * plane.pixel_stride;
            let rgb = match format {
                VideoRgbFormat::Rgb565 => {
                    let value = u16::from_le_bytes([plane.data[at], plane.data[at + 1]]);
                    [(((value >> 11) & 31) * 255 / 31) as u8,
                     (((value >> 5) & 63) * 255 / 63) as u8,
                     ((value & 31) * 255 / 31) as u8, 255]
                }
                VideoRgbFormat::Rgba8888 => [plane.data[at], plane.data[at + 1], plane.data[at + 2], 255],
            };
            let target = layout.offset(x, y);
            rgba[target..target + 4].copy_from_slice(&rgb);
        }
    }
    Ok(rgba)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChromaLayout {
    Uv = 0,
    Vu = 1,
    /// Each packed row contains all U samples, followed by all V samples.
    PlanarRows = 2,
}

fn plane_row<'a>(p: &VideoPlane<'a>, x: usize, y: usize, width: usize) -> Result<&'a [u8]> {
    let start = y
        .checked_mul(p.row_stride)
        .and_then(|v| x.checked_mul(p.pixel_stride).and_then(|x| v.checked_add(x)))
        .ok_or_else(|| RenderError::Invalid("video plane offset overflow".into()))?;
    let end = (width - 1)
        .checked_mul(p.pixel_stride)
        .and_then(|span| start.checked_add(span)?.checked_add(1))
        .ok_or_else(|| RenderError::Invalid("video plane offset overflow".into()))?;
    p.data
        .get(start..end)
        .ok_or_else(|| RenderError::Invalid("video plane too short".into()))
}

#[derive(Debug)]
pub struct Yuv420Frame {
    pub width: u32,
    pub height: u32,
    pub rotation: u32,
    pub standard: u32,
    pub range: u32,
    /// Preserve chroma phase for crops starting on an odd source pixel.
    pub phase: [u32; 2],
    pub y: Vec<u8>,
    pub chroma_layout: ChromaLayout,
    /// Packed chroma rows; native UV/VU or separate U/V rows need no shuffling.
    pub uv: Vec<u8>,
}

impl Yuv420Frame {
    pub fn pack(
        width: u32,
        height: u32,
        crop: [u32; 2],
        rotation: u32,
        standard: u32,
        range: u32,
        planes: [VideoPlane<'_>; 3],
    ) -> Result<Self> {
        let [yp, up, vp] = planes;
        let adjacent =
            up.pixel_stride == 2 && vp.pixel_stride == 2 && up.row_stride == vp.row_stride;
        let native_uv = adjacent && up.data.as_ptr().wrapping_add(1) == vp.data.as_ptr();
        let native_vu = adjacent && vp.data.as_ptr().wrapping_add(1) == up.data.as_ptr();
        let chroma_layout = if up.pixel_stride == 1 && vp.pixel_stride == 1 {
            ChromaLayout::PlanarRows
        } else if native_vu {
            ChromaLayout::Vu
        } else {
            ChromaLayout::Uv
        };
        let mut frame = Self {
            width,
            height,
            rotation,
            standard,
            range,
            phase: [crop[0] % 2, crop[1] % 2],
            y: Vec::new(),
            uv: Vec::new(),
            chroma_layout,
        };
        frame.validate_metadata()?;
        let (cw, ch) = frame.chroma_size();
        for (p, x, width) in [
            (&yp, crop[0] as usize, width as usize),
            (&up, crop[0] as usize / 2, cw as usize),
            (&vp, crop[0] as usize / 2, cw as usize),
        ] {
            if p.row_stride == 0 || p.pixel_stride == 0 {
                return Err(RenderError::Invalid("invalid video plane stride".into()));
            }
            let row_end = x
                .checked_add(width - 1)
                .and_then(|v| v.checked_mul(p.pixel_stride))
                .and_then(|v| v.checked_add(1));
            if row_end.is_none_or(|end| end > p.row_stride) {
                return Err(RenderError::Invalid(
                    "video crop exceeds plane row stride".into(),
                ));
            }
        }
        frame.y.reserve(width as usize * height as usize);
        for row in 0..height as usize {
            let y = (crop[1] as usize)
                .checked_add(row)
                .ok_or_else(|| RenderError::Invalid("video plane offset overflow".into()))?;
            let pixels = plane_row(&yp, crop[0] as usize, y, width as usize)?;
            if yp.pixel_stride == 1 {
                frame.y.extend_from_slice(pixels);
            } else {
                frame
                    .y
                    .extend(pixels.iter().step_by(yp.pixel_stride).copied());
            }
        }
        frame.uv.reserve(cw as usize * ch as usize * 2);
        for row in 0..ch as usize {
            let x = crop[0] as usize / 2;
            let y = crop[1] as usize / 2 + row;
            let u = plane_row(&up, x, y, cw as usize)?;
            let v = plane_row(&vp, x, y, cw as usize)?;
            if chroma_layout == ChromaLayout::PlanarRows {
                frame.uv.extend_from_slice(u);
                frame.uv.extend_from_slice(v);
            } else if native_uv {
                // Android plane spans can omit the last byte belonging to the
                // other plane. Copy the valid span, then append that sample.
                frame.uv.extend_from_slice(u);
                frame.uv.push(*v.last().unwrap());
            } else if native_vu {
                frame.uv.extend_from_slice(v);
                frame.uv.push(*u.last().unwrap());
            } else {
                for (u, v) in u
                    .iter()
                    .step_by(up.pixel_stride)
                    .zip(v.iter().step_by(vp.pixel_stride))
                {
                    frame.uv.push(*u);
                    frame.uv.push(*v);
                }
            }
        }
        Ok(frame)
    }
    fn validate_metadata(&self) -> Result<()> {
        if PixelLayout::new(self.width, self.height, self.rotation).is_err()
            || !matches!(self.standard, 1 | 2 | 4)
            || !matches!(self.range, 1 | 2)
            || self.phase.iter().any(|v| *v > 1)
        {
            return Err(RenderError::Invalid(
                "invalid SDR YUV frame metadata".into(),
            ));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_metadata()?;
        let (cw, ch) = self.chroma_size();
        if self.y.len() as u64 != u64::from(self.width) * u64::from(self.height)
            || self.uv.len() as u64 != u64::from(cw) * u64::from(ch) * 2
        {
            return Err(RenderError::Invalid(
                "YUV frame plane length mismatch".into(),
            ));
        }
        Ok(())
    }
    pub fn chroma_size(&self) -> (u32, u32) {
        (
            (self.width + self.phase[0]).div_ceil(2),
            (self.height + self.phase[1]).div_ceil(2),
        )
    }
    pub fn display_size(&self) -> (u32, u32) {
        if self.rotation % 180 == 0 {
            (self.width, self.height)
        } else {
            (self.height, self.width)
        }
    }
    pub fn bytes(&self) -> usize {
        self.y.len() + self.uv.len()
    }
    /// Preview-only nearest proxy in native YUV space; rotation/color metadata
    /// remain intact and crop chroma phase is absorbed into the new UV plane.
    pub fn preview_proxy(&self, max_edge: u32) -> Result<Option<Self>> {
        self.validate()?;
        let (w, h) = crate::image_resources::Resolution::Preview(max_edge).dimensions(self.width, self.height);
        if (w, h) == (self.width, self.height) { return Ok(None); }
        let (cw, _) = self.chroma_size();
        let mut proxy = Self { width: w, height: h, rotation: self.rotation, standard: self.standard,
            range: self.range, phase: [0, 0], y: Vec::with_capacity((w * h) as usize),
            chroma_layout: ChromaLayout::Uv, uv: Vec::with_capacity((w.div_ceil(2) * h.div_ceil(2) * 2) as usize) };
        let map = |p: u32, source: u32, target: u32| ((u64::from(p) * u64::from(source)) / u64::from(target)).min(u64::from(source - 1)) as u32;
        for yy in 0..h {
            for xx in 0..w {
                proxy.y.push(self.y[(map(yy, self.height, h) * self.width + map(xx, self.width, w)) as usize]);
            }
        }
        for yy in 0..h.div_ceil(2) {
            for xx in 0..w.div_ceil(2) {
                let sx = (map(xx * 2, self.width, w) + self.phase[0]) / 2;
                let sy = (map(yy * 2, self.height, h) + self.phase[1]) / 2;
                let row = (sy * cw * 2) as usize;
                let x = sx as usize;
                let (u, v) = match self.chroma_layout {
                    ChromaLayout::Uv => (self.uv[row + x * 2], self.uv[row + x * 2 + 1]),
                    ChromaLayout::Vu => (self.uv[row + x * 2 + 1], self.uv[row + x * 2]),
                    ChromaLayout::PlanarRows => (self.uv[row + x], self.uv[row + cw as usize + x]),
                };
                proxy.uv.extend_from_slice(&[u, v]);
            }
        }
        proxy.validate()?;
        Ok(Some(proxy))
    }
    pub fn to_rgba(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let layout = PixelLayout::new(self.width, self.height, self.rotation)?;
        let (cw, _) = self.chroma_size();
        let mut rgba = vec![0; layout.bytes];
        for y in 0..self.height {
            for x in 0..self.width {
                let yy = i32::from(self.y[(y * self.width + x) as usize]);
                let cx = ((x + self.phase[0]) / 2) as usize;
                let row = ((y + self.phase[1]) / 2 * cw * 2) as usize;
                let (u, v) = match self.chroma_layout {
                    ChromaLayout::Uv => (self.uv[row + cx * 2], self.uv[row + cx * 2 + 1]),
                    ChromaLayout::Vu => (self.uv[row + cx * 2 + 1], self.uv[row + cx * 2]),
                    ChromaLayout::PlanarRows => {
                        (self.uv[row + cx], self.uv[row + cw as usize + cx])
                    }
                };
                let u = i32::from(u) - 128;
                let v = i32::from(v) - 128;
                let (r, g, b) = match (self.range == 1, self.standard == 1) {
                    (true, true) => (
                        256 * yy + 403 * v,
                        256 * yy - 48 * u - 120 * v,
                        256 * yy + 475 * u,
                    ),
                    (true, false) => (
                        256 * yy + 359 * v,
                        256 * yy - 88 * u - 183 * v,
                        256 * yy + 454 * u,
                    ),
                    (false, true) => {
                        let c = 298 * (yy - 16);
                        (c + 459 * v, c - 55 * u - 136 * v, c + 541 * u)
                    }
                    (false, false) => {
                        let c = 298 * (yy - 16);
                        (c + 409 * v, c - 100 * u - 208 * v, c + 516 * u)
                    }
                };
                let at = layout.offset(x, y);
                rgba[at..at + 4].copy_from_slice(&[
                    ((r + 128) >> 8).clamp(0, 255) as u8,
                    ((g + 128) >> 8).clamp(0, 255) as u8,
                    ((b + 128) >> 8).clamp(0, 255) as u8,
                    255,
                ]);
            }
        }
        Ok(rgba)
    }
}
