//! One bounded proxy per live instance, keyed by immutable decoded-frame identity.
use crate::video_frame::{DecodedFrame, PreparedVideoFrame, VideoPixels};
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
#[cfg(test)]
#[path = "preview_video/memory_tests.rs"]
mod memory_tests;
impl PreviewVideoCache {
    pub fn upload_prepared(
        &mut self, renderer: &mut Renderer, object: u64, source: u64,
        sample: &PreparedVideoFrame, edge: Option<u32>, standalone: bool,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<EncodedVideoUpload>, RenderError> {
        let Some((right, weight)) = &sample.next else {
            return self.upload(renderer, object, source, &sample.left, edge, standalone, encoder);
        };
        let edge = edge.map(|e| e.clamp(1, motion_render::image_resources::MAX_PREVIEW_EDGE));
        let sources = [(0, &sample.left), (1, right)].map(|(slot, frame)| {
            let size = edge.map_or((frame.width, frame.height), |e|
                motion_render::image_resources::Resolution::Preview(e).dimensions(frame.width, frame.height));
            (slot, frame, size)
        });
        // Preserve a needed neighbor in either direction before a missing source
        // replaces its old slot. A miss is filled below, never treated as a fault.
        for (slot, frame, size) in sources {
            renderer.reuse_video_blend_source(object, source, frame.pts, slot, size);
        }
        for (slot, frame, size) in sources {
            if !renderer.reuse_video_blend_source(object, source, frame.pts, slot, size) {
                // Each conversion must be submitted before the source-cache copy.
                self.upload(renderer, object, source, frame, edge, true, encoder)?;
                renderer.cache_video_blend_source(object, source, frame.pts, slot)?;
            }
        }
        renderer.blend_video_sources(object, source, sample.left.pts, right.pts, *weight)?;
        Ok(None)
    }
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
    /// Explicit metadata read: weak source identities do not own decoded pixels.
    pub fn memory_ledger(&self) -> crate::resource_memory::PixelMemorySnapshot {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(object, _)| **object);
        let mut capacity = 0u64;
        let resources: Vec<_> = entries.into_iter().map(|(object, entry)| {
            let (payload, reserved) = match &entry.proxy {
                Proxy::Yuv(frame) => (frame.bytes(), frame.y.capacity() + frame.uv.capacity()),
                Proxy::Rgba { pixels, .. } => (pixels.len(), pixels.capacity()),
            };
            capacity += reserved as u64;
            serde_json::json!({"object":object,"source":entry.source,
                "payloadBytes":payload,"capacityBytes":reserved})
        }).collect();
        crate::resource_memory::PixelMemorySnapshot {
            known_payload_bytes:self.bytes() as u64, known_capacity_bytes:capacity,
            details:serde_json::json!({"scope":"owned_preview_proxy_pixel_vectors",
            "resources":resources,"sourceReferences":"weak_non_owning",
            "excludes":["container_and_weak_reference_overhead"]}) }
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
