use motion_render::{Renderer, Yuv420Frame};
use std::{borrow::Cow, sync::Arc};

/// Immutable required PTS pair. Raw decoder APIs still expose source frames;
/// composition preview/capture and frozen composition readers use this sample.
#[derive(Clone)]
pub struct PreparedVideoFrame {
    pub left: Arc<DecodedFrame>,
    pub next: Option<(Arc<DecodedFrame>, f32)>,
}
impl PreparedVideoFrame {
    pub fn upload(&self, renderer: &mut Renderer, object: u64, source: u64) -> Result<(), String> {
        let Some((right, weight)) = &self.next else { return self.left.upload(renderer, object, source); };
        // Preserve either previously required neighbor before overwriting a slot.
        // Cache misses here are normal and are filled in the following pass.
        for (slot, frame) in [(0, &self.left), (1, right)] {
            renderer.reuse_video_blend_source(object, source, frame.pts, slot, (frame.width, frame.height));
        }
        for (slot, frame) in [(0, &self.left), (1, right)] {
            if !renderer.reuse_video_blend_source(object, source, frame.pts, slot, (frame.width, frame.height)) {
                frame.upload(renderer, object, source)?;
                renderer.cache_video_blend_source(object, source, frame.pts, slot).map_err(|e| e.to_string())?;
            }
        }
        renderer.blend_video_sources(object, source, self.left.pts, right.pts, *weight).map_err(|e| e.to_string())
    }

    /// Materialize only for the explicit CPU frozen-reader protocol. Interactive
    /// preview and desktop capture use GPU blending, never this pixel loop.
    pub fn decoded(&self) -> Result<Arc<DecodedFrame>, String> {
        let Some((right, weight)) = &self.next else { return Ok(self.left.clone()); };
        if self.left.width != right.width || self.left.height != right.height
            || self.left.pts >= right.pts
            || !weight.is_finite() || !(0.0..=1.0).contains(weight) {
            return Err("adjacent video dimensions, PTS or blend weight invalid".into());
        }
        let left = self.left.rgba()?;
        let right_pixels = right.rgba()?;
        let bytes = u64::from(self.left.width) * u64::from(self.left.height) * 4;
        if left.len() as u64 != bytes || right_pixels.len() as u64 != bytes {
            return Err("adjacent video pixel length mismatch".into());
        }
        let linear = |v: u8| { let s = f32::from(v) / 255.; if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) } };
        let srgb = |v: f32| { let s = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1. / 2.4) - 0.055 }; (s * 255.).round() as u8 };
        let mut pixels = Vec::with_capacity(left.len());
        for (a, b) in left.chunks_exact(4).zip(right_pixels.chunks_exact(4)) {
            for c in 0..3 { pixels.push(srgb(linear(a[c]) * (1. - weight) + linear(b[c]) * weight)); }
            pixels.push((f32::from(a[3]) * (1. - weight) + f32::from(b[3]) * weight).round() as u8);
        }
        Ok(Arc::new(DecodedFrame {
            pixels: VideoPixels::Rgba(pixels), pts: self.left.pts, end: self.left.end,
            width: self.left.width, height: self.left.height,
            decode_us: self.left.decode_us.saturating_add(right.decode_us),
            codec_us: self.left.codec_us.saturating_add(right.codec_us),
            transfer_us: self.left.transfer_us.saturating_add(right.transfer_us),
            pack_us: self.left.pack_us.saturating_add(right.pack_us),
            source_transfer: "linear-frame-blend", decoder_name: self.left.decoder_name.clone(),
        }))
    }
}

pub enum VideoPixels {
    Yuv(Yuv420Frame),
    Rgba(Vec<u8>),
}
pub struct DecodedFrame {
    pub pixels: VideoPixels,
    pub pts: u64,
    pub end: u64,
    pub width: u32,
    pub height: u32,
    pub decode_us: u64,
    pub codec_us: u64,
    pub transfer_us: u64,
    pub pack_us: u64,
    pub source_transfer: &'static str,
    pub decoder_name: String,
}
impl DecodedFrame {
    pub fn encode_upload(
        &self, renderer: &mut Renderer, object: u64, source: u64,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<motion_render::EncodedVideoUpload>, String> {
        match &self.pixels {
            VideoPixels::Yuv(f) => renderer.encode_video_yuv(object, source, self.pts, f, encoder),
            VideoPixels::Rgba(p) => renderer
                .upload_video_frame(object, source, self.pts, self.width, self.height, p)
                .map(|_| None),
        }.map_err(|e| e.to_string())
    }
    pub fn bytes(&self) -> usize {
        match &self.pixels {
            VideoPixels::Yuv(f) => f.bytes(),
            VideoPixels::Rgba(p) => p.len(),
        }
    }
    pub fn rgba(&self) -> Result<Cow<'_, [u8]>, String> {
        match &self.pixels {
            VideoPixels::Yuv(f) => f.to_rgba().map(Cow::Owned).map_err(|e| e.to_string()),
            VideoPixels::Rgba(p) => Ok(Cow::Borrowed(p)),
        }
    }
    pub fn upload(&self, renderer: &mut Renderer, object: u64, source: u64) -> Result<(), String> {
        match &self.pixels {
            VideoPixels::Yuv(f) => renderer.upload_video_yuv(object, source, self.pts, f),
            VideoPixels::Rgba(p) => {
                renderer.upload_video_frame(object, source, self.pts, self.width, self.height, p)
            }
        }
        .map_err(|e| e.to_string())
    }
}
