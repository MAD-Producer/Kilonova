//! Immutable, bounded sRGB UI images. Text coverage has a separate shader and atlas.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
pub const MAX_BITMAPS: usize = 256;

#[derive(Debug)]
pub struct Bitmap {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl Bitmap {
    /// sRGB RGBA8, premultiplied in linear space, matching host image resources.
    /// Producers decode/resize off the drawing thread.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Arc<Self>, PaintError> {
        if width == 0
            || height == 0
            || width > 128
            || height > 128
            || u64::from(width) * u64::from(height) * 4 != pixels.len() as u64
        {
            return Err(invalid("expected RGBA8 image with dimensions 1..=128"));
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| invalid("bitmap identity exhausted"))?;
        Ok(Arc::new(Self {
            id,
            width,
            height,
            pixels,
        }))
    }
    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }
}
fn invalid(message: &str) -> PaintError {
    PaintError::from_source(
        "bitmap",
        std::io::Error::new(std::io::ErrorKind::InvalidInput, message),
        None,
    )
}
#[derive(Clone, Debug)]
pub struct ImageQuad {
    pub bitmap: Arc<Bitmap>,
    pub area: Rect,
    pub uv: [f32; 4],
}
impl ImageQuad {
    pub fn new(bitmap: Arc<Bitmap>, area: Rect) -> Self {
        Self {
            bitmap,
            area,
            uv: [0., 0., 1., 1.],
        }
    }
    pub(super) fn clipped(&self, clip: Rect) -> Option<Self> {
        let min = [
            self.area.min[0].max(clip.min[0]),
            self.area.min[1].max(clip.min[1]),
        ];
        let max = [
            self.area.max[0].min(clip.max[0]),
            self.area.max[1].min(clip.max[1]),
        ];
        if max[0] <= min[0] || max[1] <= min[1] {
            return None;
        }
        let mut uv = self.uv;
        for axis in 0..2 {
            let extent = self.area.max[axis] - self.area.min[axis];
            let delta = self.uv[axis + 2] - self.uv[axis];
            uv[axis] += delta * (min[axis] - self.area.min[axis]) / extent;
            uv[axis + 2] -= delta * (self.area.max[axis] - max[axis]) / extent;
        }
        Some(Self {
            bitmap: self.bitmap.clone(),
            area: Rect { min, max },
            uv,
        })
    }
    pub(super) fn vertices(&self, srgb_target: bool) -> impl Iterator<Item = GlyphVertex> {
        let [u, v, r, b] = self.uv;
        let min = self.area.min;
        let max = self.area.max;
        [
            (min, [u, v]),
            ([max[0], min[1]], [r, v]),
            (max, [r, b]),
            (min, [u, v]),
            (max, [r, b]),
            ([min[0], max[1]], [u, b]),
        ]
        .into_iter()
        .map(move |(position, uv)| GlyphVertex {
            position,
            uv,
            color: [if srgb_target { 1. } else { 0. }, 0., 0., 1.],
        })
    }
}
pub(super) struct Entry {
    _texture: memory::Texture,
    pub bind: wgpu::BindGroup,
    used: u64,
}
#[derive(Default)]
pub(super) struct Cache {
    pub entries: BTreeMap<u64, Entry>,
    pub pipeline: Option<wgpu::RenderPipeline>,
    tick: u64,
}
impl Painter {
    pub(super) fn prepare_bitmaps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &PaintList,
    ) -> Result<(), PaintError> {
        if list.images.is_empty() {
            return Ok(());
        }
        let needed = list
            .images
            .iter()
            .map(|image| image.bitmap.id)
            .collect::<BTreeSet<_>>();
        // At most 16 MiB of native image storage per window; in-flight retention
        // remains charged by the host allocation context after eviction.
        if needed.len() > MAX_BITMAPS {
            return Err(invalid("more than 256 visible UI bitmaps"));
        }
        self.bitmaps.tick = self.bitmaps.tick.saturating_add(1);
        let tick = self.bitmaps.tick;
        if self.bitmaps.pipeline.is_none() {
            self.bitmaps.pipeline = Some(bitmap_pipeline(
                device,
                &self.glyph_layout,
                self.target_format,
            ));
        }
        for image in &list.images {
            let bitmap = &image.bitmap;
            if let Some(entry) = self.bitmaps.entries.get_mut(&bitmap.id) {
                entry.used = tick;
                continue;
            }
            if self.bitmaps.entries.len() == MAX_BITMAPS {
                if let Some(id) = self
                    .bitmaps
                    .entries
                    .iter()
                    .filter(|(id, _)| !needed.contains(id))
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(id, _)| *id)
                {
                    self.bitmaps.entries.remove(&id);
                }
            }
            let texture = memory::Texture::new(
                device,
                self.admission.as_ref(),
                &wgpu::TextureDescriptor {
                    label: Some("UI bitmap"),
                    size: wgpu::Extent3d {
                        width: bitmap.width,
                        height: bitmap.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
            )
            .map_err(|failure| failure.with_operation("bitmap allocation"))?;
            texture
                .write(queue, &bitmap.pixels, bitmap.width, bitmap.height)
                .map_err(|failure| failure.with_operation("bitmap upload"))?;
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("UI bitmap bind"),
                layout: &self.glyph_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.glyph_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.bitmaps.entries.insert(
                bitmap.id,
                Entry {
                    _texture: texture,
                    bind,
                    used: tick,
                },
            );
        }
        Ok(())
    }
}
pub(super) fn id(image: &ImageQuad) -> u64 {
    image.bitmap.id
}
fn bitmap_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("UI bitmap shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/panel.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI bitmap pipeline"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UI bitmaps"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("glyph_vs"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GlyphVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x4],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("bitmap_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_clip_append_and_text_retention_keep_uvs_and_creation_order() {
        assert!(Bitmap::new(0, 1, vec![]).is_err());
        assert!(Bitmap::new(129, 1, vec![0; 129 * 4]).is_err());
        assert!(Bitmap::new(1, 1, vec![0; 3]).is_err());
        let bitmap = Bitmap::new(1, 1, vec![40, 80, 160, 255]).unwrap();
        let mut list = PaintList::default();
        rect(
            &mut list,
            Rect::new(0., 0., 12., 8.),
            Color([0., 0., 0., 1.]),
        );
        list.push_image(ImageQuad::new(bitmap.clone(), Rect::new(0., 0., 8., 8.)));
        let mut child = list.child();
        child.push_image(ImageQuad::new(bitmap, Rect::new(20., 0., 8., 8.)));
        list.append(child);
        list.clip_to(Rect::new(4., 0., 8., 8.));
        assert_eq!(list.order, vec![PaintOp::Shape(0), PaintOp::Image(0)]);
        assert_eq!(list.images[0].uv, [0.5, 0., 1., 1.]);
        list.retain_glyphs(|_| false);
        assert_eq!(list.order, vec![PaintOp::Shape(0), PaintOp::Image(0)]);
        list.clear();
        assert!(list.images.is_empty() && list.order.is_empty());
    }
    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires gpu-tests")]
    fn real_rgba_images_preserve_color_alpha_clipping_order_and_bounded_native_cache() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let mut painter = Painter::new(&device, format);
            let bitmap = Bitmap::new(1, 1, vec![50, 100, 150, 255]).unwrap();
            let mut list = PaintList::default();
            list.push_image(ImageQuad::new(bitmap.clone(), Rect::new(0., 0., 16., 8.)));
            rect(
                &mut list,
                Rect::new(8., 0., 4., 8.),
                Color([0., 0., 1., 1.]),
            );
            let rgba =
                super::super::bindings_tests::capture(&device, &queue, &mut painter, &list, 2);
            for (x, expected) in [
                (4, [50u8, 100, 150, 255]),
                (20, [0, 0, 255, 255]),
                (28, [50, 100, 150, 255]),
            ] {
                for (actual, expected) in rgba[(4 * 32 + x) * 4..(4 * 32 + x + 1) * 4]
                    .iter()
                    .zip(expected)
                {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "{format:?} {actual} != {expected}"
                    );
                }
            }
            let bind = painter.bitmaps.entries[&bitmap.id].bind.clone();
            super::super::bindings_tests::capture(&device, &queue, &mut painter, &list, 1);
            assert_eq!(
                painter.bitmaps.entries[&bitmap.id].bind, bind,
                "healthy repaint reuploaded bitmap"
            );
            list.clear();
            list.push_image(ImageQuad::new(
                Bitmap::new(1, 1, vec![188, 0, 0, 128]).unwrap(),
                Rect::new(0., 0., 16., 8.),
            ));
            let rgba =
                super::super::bindings_tests::capture(&device, &queue, &mut painter, &list, 1);
            let expected = if format.is_srgb() { 188 } else { 128 };
            assert!(rgba[0].abs_diff(expected) <= 2);
            assert_eq!(rgba[3], 128);
            for _ in 0..MAX_BITMAPS + 6 {
                list.clear();
                list.push_image(ImageQuad::new(
                    Bitmap::new(1, 1, vec![25, 70, 130, 255]).unwrap(),
                    Rect::new(0., 0., 16., 8.),
                ));
                painter.prepare_bitmaps(&device, &queue, &list).unwrap();
            }
            assert_eq!(painter.bitmaps.entries.len(), MAX_BITMAPS);
            list.clear();
            for _ in 0..MAX_BITMAPS + 1 {
                list.push_image(ImageQuad::new(
                    Bitmap::new(1, 1, vec![0; 4]).unwrap(),
                    Rect::new(0., 0., 1., 1.),
                ));
            }
            assert!(painter
                .prepare_bitmaps(&device, &queue, &list)
                .unwrap_err()
                .to_string()
                .contains("256 visible"));
        }
    }
}
