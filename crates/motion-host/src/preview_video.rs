//! One bounded proxy per live instance, keyed by immutable decoded-frame identity.
use crate::video_frame::{DecodedFrame, VideoPixels};
use motion_render::{EncodedVideoUpload, RenderError, Renderer, Scene, Yuv420Frame};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

enum Proxy {
    Yuv(Yuv420Frame),
    Rgba {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
}
struct Entry {
    frame: Weak<DecodedFrame>,
    edge: u32,
    source: u64,
    proxy: Proxy,
}
#[derive(Default)]
pub struct PreviewVideoCache {
    entries: HashMap<u64, Entry>,
}
impl PreviewVideoCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn retain(&mut self, scene: &Scene) {
        let layers = scene.video_layers();
        self.entries
            .retain(|id, _| layers.iter().any(|l| l.id == *id));
    }
    pub fn bytes(&self) -> usize {
        self.entries
            .values()
            .map(|e| match &e.proxy {
                Proxy::Yuv(f) => f.bytes(),
                Proxy::Rgba { pixels, .. } => pixels.len(),
            })
            .sum()
    }
    pub fn upload(
        &mut self,
        renderer: &mut Renderer,
        object: u64,
        source: u64,
        frame: &Arc<DecodedFrame>,
        edge: Option<u32>,
        standalone: bool,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<EncodedVideoUpload>, RenderError> {
        let edge = edge.map(|e| e.clamp(1, motion_render::image_resources::MAX_PREVIEW_EDGE));
        if edge.is_none_or(|e| frame.width.max(frame.height) <= e) {
            self.entries.remove(&object);
            return upload_pixels(
                renderer,
                object,
                source,
                frame.pts,
                &frame.pixels,
                frame.width,
                frame.height,
                standalone,
                encoder,
            );
        }
        let edge = edge.unwrap();
        let identity = Arc::downgrade(frame);
        if self
            .entries
            .get(&object)
            .is_none_or(|e| e.edge != edge || e.source != source || !e.frame.ptr_eq(&identity))
        {
            self.entries.remove(&object);
            let proxy = match &frame.pixels {
                VideoPixels::Yuv(f) => Proxy::Yuv(f.preview_proxy(edge)?.ok_or_else(|| {
                    RenderError::Invalid("decoded video dimensions disagree".into())
                })?),
                VideoPixels::Rgba(p) => {
                    if frame.width == 0
                        || frame.height == 0
                        || u64::from(frame.width) * u64::from(frame.height) * 4 != p.len() as u64
                    {
                        return Err(RenderError::Invalid("video pixel length mismatch".into()));
                    }
                    let (w, h) = motion_render::image_resources::Resolution::Preview(edge)
                        .dimensions(frame.width, frame.height);
                    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
                    for y in 0..h {
                        for x in 0..w {
                            let sx = u64::from(x) * u64::from(frame.width) / u64::from(w);
                            let sy = u64::from(y) * u64::from(frame.height) / u64::from(h);
                            let at = ((sy * u64::from(frame.width) + sx) * 4) as usize;
                            pixels.extend_from_slice(&p[at..at + 4]);
                        }
                    }
                    Proxy::Rgba {
                        width: w,
                        height: h,
                        pixels,
                    }
                }
            };
            self.entries.insert(
                object,
                Entry {
                    frame: identity,
                    edge,
                    source,
                    proxy,
                },
            );
        }
        match &self.entries[&object].proxy {
            Proxy::Yuv(f) => {
                upload_yuv(renderer, object, source, frame.pts, f, standalone, encoder)
            }
            Proxy::Rgba {
                width,
                height,
                pixels,
            } => renderer
                .upload_video_frame(object, source, frame.pts, *width, *height, pixels)
                .map(|_| None),
        }
    }
}
fn upload_yuv(
    renderer: &mut Renderer,
    object: u64,
    source: u64,
    pts: u64,
    f: &Yuv420Frame,
    standalone: bool,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<Option<EncodedVideoUpload>, RenderError> {
    if standalone {
        renderer
            .upload_video_yuv(object, source, pts, f)
            .map(|_| None)
    } else {
        renderer.encode_video_yuv(object, source, pts, f, encoder)
    }
}
fn upload_pixels(
    renderer: &mut Renderer,
    object: u64,
    source: u64,
    pts: u64,
    pixels: &VideoPixels,
    width: u32,
    height: u32,
    standalone: bool,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<Option<EncodedVideoUpload>, RenderError> {
    match pixels {
        VideoPixels::Yuv(f) => upload_yuv(renderer, object, source, pts, f, standalone, encoder),
        VideoPixels::Rgba(p) => renderer
            .upload_video_frame(object, source, pts, width, height, p)
            .map(|_| None),
    }
}
