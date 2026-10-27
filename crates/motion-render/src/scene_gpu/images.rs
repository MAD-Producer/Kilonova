use super::{charge, invalid, unsupported, Result, SceneGpuError, SceneGpuLimits};
use crate::{
    AdmittedTexture, AdmittedTextureView, AllocationContext, RenderError, TextureCategory,
};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Image(usize, bool),
    Pixel([u8; 4], bool),
}
struct Texture {
    view: AdmittedTextureView,
    _texture: AdmittedTexture,
}
pub(super) struct Images {
    textures: Vec<Texture>,
    keys: HashMap<Key, usize>,
}
pub(super) struct ImageRequest {
    pub image: Option<usize>,
    pub color: bool,
    pub fallback: [u8; 4],
}
impl Images {
    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            keys: HashMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.textures.len()
    }
    pub fn collect(&self, ledger: &mut crate::TextureResourceLedger) {
        for (id, texture) in self.textures.iter().enumerate() {
            texture._texture.collect_texture_resources(
                ledger,
                &format!("scene/image/{id}"),
                crate::TextureState::Retained,
            );
        }
    }
    pub fn view(&self, id: usize) -> &wgpu::TextureView {
        &self.textures[id].view
    }
    pub fn get(
        &mut self,
        context: &AllocationContext,
        source: &motion_scene::Scene,
        request: ImageRequest,
        limits: SceneGpuLimits,
        bytes: &mut u64,
    ) -> Result<usize> {
        let ImageRequest {
            image,
            color,
            fallback,
        } = request;
        let key = image.map_or(Key::Pixel(fallback, color), |id| Key::Image(id, color));
        if let Some(&id) = self.keys.get(&key) {
            return Ok(id);
        }
        let (width, height, mut pixels, high) = if let Some(id) = image {
            let source = &source.images()[id];
            let format = match source.mime_type.as_str() {
                "image/png" => image::ImageFormat::Png,
                "image/jpeg" => image::ImageFormat::Jpeg,
                _ => return Err(unsupported(id, "unsupported encoded image type")),
            };
            let reader = || {
                image::ImageReader::with_format(
                    std::io::Cursor::new(&source.resource.bytes),
                    format,
                )
            };
            let decode_error = |cause| SceneGpuError::Image { image: id, cause };
            let (width, height) = reader().into_dimensions().map_err(decode_error)?;
            let edge = limits
                .max_image_edge
                .min(context.device().limits().max_texture_dimension_2d);
            if width == 0 || height == 0 || width > edge || height > edge {
                return Err(invalid(
                    "image",
                    id,
                    "texture dimensions exceed native/scene limits",
                ));
            }
            // Worst-case PNG16 decode, f32 conversion, serialized upload and the
            // next mip coexist. This conservative pixel allowance precedes decode.
            let decoded_required = u64::from(width)
                .checked_mul(u64::from(height))
                .and_then(|pixels| pixels.checked_mul(48))
                .ok_or_else(|| invalid("image", id, "decoded pixel capacity overflow"))?;
            if decoded_required > limits.decoded_image_bytes {
                return Err(RenderError::budget(
                    crate::ResourceBudgetScope::DecodedImages,
                    decoded_required,
                    limits.decoded_image_bytes,
                    format!("scene image={id} decoded pixels exceed transient allowance"),
                )
                .into());
            }
            let mut reader = reader();
            let mut decode_limits = image::Limits::default();
            decode_limits.max_alloc = Some(limits.decoded_image_bytes / 2);
            reader.limits(decode_limits);
            let decoded = reader.decode().map_err(decode_error)?;
            let kind = decoded.color();
            let high = kind.bits_per_pixel() > u16::from(kind.channel_count()) * 8;
            let pixels = decoded.into_rgba32f().into_raw();
            (width, height, pixels, high)
        } else {
            (1, 1, fallback.map(|v| f32::from(v) / 255.).to_vec(), false)
        };
        if color {
            for p in pixels.chunks_exact_mut(4) {
                for v in &mut p[..3] {
                    *v = crate::renderer::srgb_to_linear(*v);
                }
            }
        }
        let format = if high {
            if !context
                .device()
                .features()
                .contains(wgpu::Features::FLOAT32_FILTERABLE)
            {
                return Err(unsupported(
                    image.unwrap_or(0),
                    "16-bit image requires filterable Float32 storage; no precision downgrade",
                ));
            }
            wgpu::TextureFormat::Rgba32Float
        } else if color {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let unit = if high { 16u64 } else { 4 };
        let mut levels = 1;
        let mut w = width;
        let mut h = height;
        let mut cost = u64::from(w) * u64::from(h) * unit;
        while w > 1 || h > 1 {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            levels += 1;
            cost = cost
                .checked_add(u64::from(w) * u64::from(h) * unit)
                .ok_or_else(|| invalid("image", 0, "mip capacity overflow"))?;
        }
        charge(bytes, cost, limits)?;
        let texture = context.texture_in(
            &wgpu::TextureDescriptor {
                label: Some("scene material image and linear mips"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureCategory::StaticImages,
        )?;
        let mut w = width;
        let mut h = height;
        for level in 0..levels {
            if high {
                context.write_texture_mip(
                    &texture,
                    level,
                    bytemuck::cast_slice(&pixels),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 16),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                )?;
            } else {
                let encoded: Vec<u8> = pixels
                    .chunks_exact(4)
                    .flat_map(|p| {
                        std::array::from_fn::<_, 4, _>(|i| {
                            let v = if color && i < 3 {
                                crate::renderer::linear_to_srgb(p[i])
                            } else {
                                p[i]
                            };
                            (v.clamp(0., 1.) * 255.).round() as u8
                        })
                    })
                    .collect();
                context.write_texture_mip(
                    &texture,
                    level,
                    &encoded,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                )?;
            }
            if level + 1 < levels {
                let next = mip(&pixels, w, h);
                pixels = next;
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
        }
        let id = self.textures.len();
        self.textures.push(Texture {
            view: texture.view()?,
            _texture: texture,
        });
        self.keys.insert(key, id);
        Ok(id)
    }
}

/// Exact area integration includes the odd last row/column, unlike a 2x2 crop.
fn mip(source: &[f32], width: u32, height: u32) -> Vec<f32> {
    let w = (width / 2).max(1);
    let h = (height / 2).max(1);
    let mut out = vec![0.; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let x0 = f64::from(x) * f64::from(width) / f64::from(w);
            let x1 = f64::from(x + 1) * f64::from(width) / f64::from(w);
            let y0 = f64::from(y) * f64::from(height) / f64::from(h);
            let y1 = f64::from(y + 1) * f64::from(height) / f64::from(h);
            let mut sum = [0f64; 4];
            let mut weight = 0.;
            for sy in y0.floor() as u32..y1.ceil() as u32 {
                for sx in x0.floor() as u32..x1.ceil() as u32 {
                    let area = (x1.min(f64::from(sx + 1)) - x0.max(f64::from(sx)))
                        * (y1.min(f64::from(sy + 1)) - y0.max(f64::from(sy)));
                    let index = (sy as usize * width as usize + sx as usize) * 4;
                    for i in 0..4 {
                        sum[i] += f64::from(source[index + i]) * area;
                    }
                    weight += area;
                }
            }
            let index = (y as usize * w as usize + x as usize) * 4;
            for i in 0..4 {
                out[index + i] = (sum[i] / weight) as f32;
            }
        }
    }
    out
}
