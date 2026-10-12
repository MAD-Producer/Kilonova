//! Grid-fitted grayscale glyphs from the installed Windows font mapper.
//! Glyph outlines/metrics: https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getglyphoutlinew
use std::collections::HashMap;
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Default)]
struct Metrics {
    width: u32,
    height: u32,
    origin: Point,
    advance_x: i16,
    advance_y: i16,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Fixed {
    fraction: u16,
    value: i16,
}
#[repr(C)]
struct Matrix {
    xx: Fixed,
    xy: Fixed,
    yx: Fixed,
    yy: Fixed,
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: isize) -> isize;
    fn DeleteDC(dc: isize) -> i32;
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        name: *const u16,
    ) -> isize;
    fn SelectObject(dc: isize, object: isize) -> isize;
    fn DeleteObject(object: isize) -> i32;
    fn GetGlyphOutlineW(
        dc: isize,
        glyph: u32,
        format: u32,
        metrics: *mut Metrics,
        size: u32,
        buffer: *mut u8,
        matrix: *const Matrix,
    ) -> u32;
}
pub(super) struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub offset: [f32; 2],
    pub coverage: Vec<u8>,
}
pub(super) struct Rasterizer {
    dc: isize,
    original: isize,
    family: Vec<u16>,
    fonts: HashMap<i32, isize>,
}
impl Rasterizer {
    pub fn new(family: &str) -> Option<Self> {
        let dc = unsafe { CreateCompatibleDC(0) };
        (dc != 0).then(|| Self {
            dc,
            original: 0,
            family: family.encode_utf16().chain([0]).collect(),
            fonts: HashMap::new(),
        })
    }
    pub fn glyph(&mut self, index: u16, pixels: f32) -> Option<Bitmap> {
        if !pixels.is_finite() || pixels < 1. || pixels > 512. {
            return None;
        }
        let pixels = pixels.round().max(1.) as i32;
        let font = *self.fonts.entry(pixels).or_insert_with(|| unsafe {
            CreateFontW(
                -pixels,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                4,
                0,
                4,
                0,
                self.family.as_ptr(),
            )
        });
        if font == 0 {
            return None;
        }
        let previous = unsafe { SelectObject(self.dc, font) };
        if previous == 0 || previous == -1 {
            return None;
        }
        if self.original == 0 {
            self.original = previous;
        }
        let one = Fixed {
            fraction: 0,
            value: 1,
        };
        let zero = Fixed {
            fraction: 0,
            value: 0,
        };
        let matrix = Matrix {
            xx: one,
            xy: zero,
            yx: zero,
            yy: one,
        };
        let mut metrics = Metrics::default();
        let size = unsafe {
            GetGlyphOutlineW(
                self.dc,
                index as u32,
                6 | 0x80,
                &mut metrics,
                0,
                std::ptr::null_mut(),
                &matrix,
            )
        };
        if size == u32::MAX || size > 1_048_576 {
            return None;
        }
        if metrics.width == 0 || metrics.height == 0 {
            return Some(Bitmap {
                width: 0,
                height: 0,
                offset: [0.; 2],
                coverage: vec![],
            });
        }
        let stride = (metrics.width as usize + 3) & !3;
        if stride.checked_mul(metrics.height as usize)? > size as usize {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if unsafe {
            GetGlyphOutlineW(
                self.dc,
                index as u32,
                6 | 0x80,
                &mut metrics,
                size,
                data.as_mut_ptr(),
                &matrix,
            )
        } == u32::MAX
        {
            return None;
        }
        let coverage = (0..metrics.height as usize)
            .flat_map(|y| (0..metrics.width as usize).map(move |x| (y, x)))
            .map(|(y, x)| ((data[y * stride + x].min(64) as u16 * 255 + 32) / 64) as u8)
            .collect();
        Some(Bitmap {
            width: metrics.width,
            height: metrics.height,
            offset: [metrics.origin.x as f32, -(metrics.origin.y as f32)],
            coverage,
        })
    }
}
impl Drop for Rasterizer {
    fn drop(&mut self) {
        unsafe {
            if self.original != 0 {
                SelectObject(self.dc, self.original);
            }
            for font in self.fonts.values() {
                if *font != 0 {
                    DeleteObject(*font);
                }
            }
            DeleteDC(self.dc);
        }
    }
}
